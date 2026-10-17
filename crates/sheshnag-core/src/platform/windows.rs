use super::*;
use crate::model::Scope;
use crate::runner::{Runner, SystemRunner};
use crate::util::{basename, hash16};
use regex::Regex;
use std::sync::Arc;

const TASK_NAME: &str = "Sheshnag";

const RUN_KEYS: [(&str, Scope); 4] = [
    (r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", Scope::User),
    (r"HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce", Scope::User),
    (r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run", Scope::System),
    (r"HKLM\Software\Microsoft\Windows\CurrentVersion\RunOnce", Scope::System),
];

const PROCESS_SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue'; $cpu=@{}; \
Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | ForEach-Object { $cpu[[int]$_.IDProcess]=$_.PercentProcessorTime }; \
Get-CimInstance Win32_Process | ForEach-Object { [pscustomobject]@{Pid=$_.ProcessId;Cpu=$cpu[[int]$_.ProcessId];Name=$_.Name;Cmd=$_.CommandLine;Path=$_.ExecutablePath} } | ConvertTo-Csv -NoTypeInformation";

const TASK_SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue'; Get-ScheduledTask | Where-Object { $_.TaskPath -notlike '\\Microsoft\\*' } | ForEach-Object { $a = ($_.Actions | ForEach-Object { $_.Execute + ' ' + $_.Arguments }) -join ' ; '; $_.TaskPath + $_.TaskName + ' -> ' + $a }";

pub struct Windows {
    home: PathBuf,
    appdata: PathBuf,
    programdata: PathBuf,
    runner: Arc<dyn Runner>,
}

impl Windows {
    pub fn new(home: PathBuf) -> Self {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Roaming"));
        let programdata =
            std::env::var_os("PROGRAMDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Self::with_parts(home, appdata, programdata, Arc::new(SystemRunner))
    }

    pub fn with_parts(home: PathBuf, appdata: PathBuf, programdata: PathBuf, runner: Arc<dyn Runner>) -> Self {
        Self { home, appdata, programdata, runner }
    }

    fn user_startup(&self) -> PathBuf {
        self.appdata.join("Microsoft/Windows/Start Menu/Programs/Startup")
    }

    fn system_startup(&self) -> PathBuf {
        self.programdata.join("Microsoft/Windows/Start Menu/Programs/StartUp")
    }

    fn powershell(&self, script: &str) -> crate::runner::CmdOut {
        self.runner.run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])
    }

    fn split_id(id: &str) -> Option<(&str, &str)> {
        id.split_once("::")
    }
}

/// One value from `reg query` output: name, type and data.
pub fn parse_reg_query(text: &str) -> Vec<(String, String, String)> {
    let re = Regex::new(r"^\s{4}(.+?)\s{4}(REG_[A-Z_]+)(?:\s{4}(.*))?$").unwrap();
    text.lines()
        .filter_map(|line| {
            let c = re.captures(line.trim_end_matches('\r'))?;
            Some((c[1].to_owned(), c[2].to_owned(), c.get(3).map(|m| m.as_str().to_owned()).unwrap_or_default()))
        })
        .collect()
}

/// Split one CSV line, honouring quotes and doubled quotes.
pub fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Rows of `Pid,Cpu,Name,Cmd,Path` from the PowerShell process query.
pub fn parse_processes_csv(text: &str, user: &str) -> Vec<Proc> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f = parse_csv_line(line.trim_end_matches('\r'));
            if f.len() < 5 {
                return None;
            }
            let pid: u32 = f[0].trim().parse().ok()?;
            let name = f[2].clone();
            let lower = name.to_lowercase();
            if pid == 0 || pid == 4 || ["idle", "system", "_total", "system idle process"].iter().any(|n| lower.starts_with(n)) {
                return None;
            }
            let cpu = if f[1].trim().is_empty() { "0".to_owned() } else { f[1].trim().to_owned() };
            let args = if f[3].is_empty() { f[4].clone() } else { f[3].clone() };
            Some(Proc { pid, cpu_text: cpu, user: user.to_owned(), comm: name, args })
        })
        .collect()
}

impl Platform for Windows {
    fn name(&self) -> &'static str {
        "Windows"
    }
    fn noun(&self) -> &'static str {
        "startup entry"
    }
    fn schedule_name(&self) -> &'static str {
        "scheduled tasks"
    }
    fn home(&self) -> &Path {
        &self.home
    }
    fn current_user(&self) -> String {
        std::env::var("USERNAME").unwrap_or_default()
    }

    fn persistence(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        for (key, scope) in RUN_KEYS {
            let listing = self.runner.run("reg", &["query", key]);
            for (name, _ty, data) in parse_reg_query(&listing.stdout) {
                out.push(Entry {
                    id: format!("{key}::{name}"),
                    label: name,
                    fingerprint: hash16(data.as_bytes()),
                    command: data,
                    scope,
                    file: None,
                });
            }
        }
        for (dir, scope) in [(self.user_startup(), Scope::User), (self.system_startup(), Scope::System)] {
            for path in list_files(&dir, |p| !file_name(p).eq_ignore_ascii_case("desktop.ini")) {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                let mut command = path.to_string_lossy().into_owned();
                if ["bat", "cmd", "ps1", "vbs", "js"].contains(&ext.as_str()) {
                    command.push(' ');
                    command.push_str(&String::from_utf8_lossy(&bytes));
                }
                out.push(Entry {
                    id: path.to_string_lossy().into_owned(),
                    label: file_name(&path),
                    command,
                    fingerprint: hash16(&bytes),
                    scope,
                    file: Some(path),
                });
            }
        }
        out
    }

    fn impersonation(&self, e: &Entry) -> Option<String> {
        let first = e.command.trim().trim_start_matches('"');
        let exe = basename(first.split('"').next().unwrap_or(first).split(' ').next().unwrap_or(first)).to_lowercase();
        let lower_cmd = e.command.to_lowercase();
        let system_names = ["svchost.exe", "lsass.exe", "csrss.exe", "winlogon.exe", "explorer.exe", "services.exe", "smss.exe", "wininit.exe", "taskhost.exe"];
        if system_names.contains(&exe.as_str()) && !lower_cmd.contains(r"\windows\") {
            return Some("System-process name running from outside the Windows folder".to_owned());
        }
        let label = e.label.to_lowercase();
        let user_folder = lower_cmd.contains(r"\appdata\") || lower_cmd.contains(r"\temp\") || lower_cmd.contains(r"\users\public\");
        if e.scope == Scope::User && (label.starts_with("microsoft") || label.starts_with("windows")) && user_folder {
            return Some("Microsoft-looking entry that runs from a user folder".to_owned());
        }
        None
    }

    fn is_own(&self, e: &Entry) -> bool {
        e.label.eq_ignore_ascii_case(TASK_NAME)
    }

    fn extra_hiders(&self) -> &'static str {
        r"-e(nc|ncodedcommand)\b|-w(indowstyle)? +hidden|-nop\b|mshta|certutil[^|]*-urlcache|bitsadmin|regsvr32[^|]*/i:http|rundll32[^|]*javascript|invoke-expression|\biex\b|frombase64string|downloadstring|downloadfile|\\appdata\\local\\temp|%temp%|\\users\\public\\"
    }

    fn extra_pasted(&self) -> &'static str {
        r"\biex\b|invoke-expression|downloadstring|-encodedcommand|frombase64string"
    }

    fn cpu_allowlist(&self) -> &'static [&'static str] {
        &["msbuild", "cl", "link", "devenv", "rustc", "cargo", "ffmpeg", "ollama", "msmpeng", "searchindexer", "vctip"]
    }

    fn processes(&self) -> Vec<Proc> {
        parse_processes_csv(&self.powershell(PROCESS_SCRIPT).stdout, &self.current_user())
    }

    fn startup_files(&self) -> Vec<PathBuf> {
        let docs = self.home.join("Documents");
        vec![
            docs.join("WindowsPowerShell/profile.ps1"),
            docs.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
            docs.join("PowerShell/profile.ps1"),
            docs.join("PowerShell/Microsoft.PowerShell_profile.ps1"),
        ]
    }

    fn schedule(&self) -> Vec<String> {
        sorted_lines(&self.powershell(TASK_SCRIPT).stdout.replace('\r', ""))
    }

    fn login_items(&self) -> Vec<String> {
        // The Startup folder and Run keys are already covered as startup entries.
        Vec::new()
    }

    fn notify(&self, title: &str, subtitle: &str, body: &str) {
        let clean = |s: &str| s.replace('\'', " ");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \
$n = New-Object System.Windows.Forms.NotifyIcon; $n.Icon = [System.Drawing.SystemIcons]::Warning; $n.Visible = $true; \
$n.ShowBalloonTip(8000, '{}: {}', '{}', [System.Windows.Forms.ToolTipIcon]::Warning); Start-Sleep -Seconds 9; $n.Dispose()",
            clean(title),
            clean(subtitle),
            clean(body)
        );
        self.runner.spawn("powershell", &["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]);
    }

    fn monitor_installed(&self) -> bool {
        self.runner.run("schtasks", &["/Query", "/TN", TASK_NAME]).ok()
    }

    fn install_monitor(&self, exe: &Path, _state_dir: &Path) -> Result<(), String> {
        let action = format!("\"{}\" dekho quiet", exe.to_string_lossy());
        let out = self
            .runner
            .run("schtasks", &["/Create", "/TN", TASK_NAME, "/TR", &action, "/SC", "MINUTE", "/MO", "10", "/F"]);
        if out.ok() { Ok(()) } else { Err(format!("schtasks failed: {}", out.stderr.trim())) }
    }

    fn remove_monitor(&self) -> Result<(), String> {
        self.runner.run("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"]);
        Ok(())
    }

    fn stop(&self, _id: &str, _label: &str) -> Result<(), String> {
        // Run keys and Startup items do not keep a running job. Their process is handled as a target.
        Ok(())
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        if let Some((key, name)) = Self::split_id(id) {
            let out = self.runner.run("reg", &["delete", key, "/v", name, "/f"]);
            return if out.ok() { Ok(()) } else { Err(format!("reg delete failed: {}", out.stderr.trim())) };
        }
        std::fs::remove_file(id).map_err(|e| format!("{id}: {e}"))
    }

    fn export(&self, id: &str) -> Result<Vec<u8>, String> {
        if let Some((key, name)) = Self::split_id(id) {
            let out = self.runner.run("reg", &["query", key, "/v", name]);
            return parse_reg_query(&out.stdout)
                .into_iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, ty, data)| format!("{ty}\t{data}").into_bytes())
                .ok_or_else(|| format!("{id}: value not found"));
        }
        std::fs::read(id).map_err(|e| format!("{id}: {e}"))
    }

    fn import(&self, id: &str, data: &[u8]) -> Result<(), String> {
        if let Some((key, name)) = Self::split_id(id) {
            let text = String::from_utf8_lossy(data);
            let (ty, value) = text.split_once('\t').unwrap_or(("REG_SZ", &text));
            let out = self.runner.run("reg", &["add", key, "/v", name, "/t", ty, "/d", value, "/f"]);
            return if out.ok() { Ok(()) } else { Err(format!("reg add failed: {}", out.stderr.trim())) };
        }
        write_creating(Path::new(id), data)
    }

    fn kill(&self, pid: u32) -> Result<(), String> {
        let out = self.runner.run("taskkill", &["/F", "/PID", &pid.to_string()]);
        if out.ok() { Ok(()) } else { Err(format!("taskkill {pid}: {}", out.stderr.trim())) }
    }

    fn destroyable(&self, id: &str) -> Result<(), String> {
        if let Some((key, _)) = Self::split_id(id) {
            let allowed = RUN_KEYS.iter().any(|(k, s)| *s == Scope::User && k.eq_ignore_ascii_case(key));
            return if allowed {
                Ok(())
            } else {
                Err(format!("{key} is machine-wide or not a startup key Agni may act on"))
            };
        }
        let roots = vec![self.user_startup()];
        let mut protected: Vec<PathBuf> = [r"C:\Windows", r"C:\Program Files", r"C:\Program Files (x86)"]
            .iter()
            .map(PathBuf::from)
            .collect();
        protected.extend(user_protected_dirs(&self.home));
        file_destroyable(id, &roots, &protected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REG: &str = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    OneDrive    REG_SZ    \"C:\\Program Files\\OneDrive.exe\" /background\r\n    My App    REG_EXPAND_SZ    %APPDATA%\\app.exe\r\n\r\n";

    #[test]
    fn registry_listing_is_parsed_including_names_with_spaces() {
        let v = parse_reg_query(REG);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "OneDrive");
        assert_eq!(v[0].2, "\"C:\\Program Files\\OneDrive.exe\" /background");
        assert_eq!(v[1].0, "My App");
        assert_eq!(v[1].1, "REG_EXPAND_SZ");
    }

    #[test]
    fn csv_handles_quotes_commas_and_doubled_quotes() {
        assert_eq!(parse_csv_line(r#""1","a,b","say ""hi""","""#), vec!["1", "a,b", "say \"hi\"", ""]);
    }

    #[test]
    fn process_csv_skips_idle_and_system_and_fills_blank_cpu() {
        let csv = "\"Pid\",\"Cpu\",\"Name\",\"Cmd\",\"Path\"\r\n\"0\",\"800\",\"System Idle Process\",\"\",\"\"\r\n\"4\",\"1\",\"System\",\"\",\"\"\r\n\"88\",\"\",\"app.exe\",\"C:\\app.exe --x\",\"C:\\app.exe\"\r\n\"99\",\"450\",\"xmrig.exe\",\"\",\"C:\\t\\xmrig.exe\"\r\n";
        let p = parse_processes_csv(csv, "me");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].cpu_text, "0");
        assert_eq!(p[0].args, "C:\\app.exe --x");
        assert_eq!(p[1].args, "C:\\t\\xmrig.exe");
        assert_eq!(p[1].user, "me");
    }
}
