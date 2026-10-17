use super::*;
use crate::model::Scope;
use crate::runner::{Runner, SystemRunner};
use crate::util::file_hash16;
use std::sync::Arc;

pub const LABEL: &str = "in.palanhar.sheshnag";

pub struct MacOs {
    home: PathBuf,
    /// Directories to scan, each tagged with its scope.
    dirs: Vec<(PathBuf, Scope)>,
    runner: Arc<dyn Runner>,
}

impl MacOs {
    /// `dir_override` replaces the three standard folders. Folders under `home` count as user scope.
    pub fn new(home: PathBuf, dir_override: Option<Vec<PathBuf>>) -> Self {
        Self::with_runner(home, dir_override, Arc::new(SystemRunner))
    }

    pub fn with_runner(home: PathBuf, dir_override: Option<Vec<PathBuf>>, runner: Arc<dyn Runner>) -> Self {
        let dirs = match dir_override {
            Some(list) => list
                .into_iter()
                .map(|d| {
                    let scope = if d.starts_with(&home) { Scope::User } else { Scope::System };
                    (d, scope)
                })
                .collect(),
            None => vec![
                (home.join("Library/LaunchAgents"), Scope::User),
                (PathBuf::from("/Library/LaunchAgents"), Scope::System),
                (PathBuf::from("/Library/LaunchDaemons"), Scope::System),
            ],
        };
        Self { home, dirs, runner }
    }

    fn uid(&self) -> String {
        let out = self.runner.run("id", &["-u"]);
        if out.ok() { out.stdout.trim().to_owned() } else { "501".to_owned() }
    }

    fn own_plist(&self) -> PathBuf {
        self.home.join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
    }

    /// Program and ProgramArguments as one string. Falls back to the raw text if it is not a plist.
    fn command_of(path: &Path) -> String {
        if let Ok(value) = plist::Value::from_file(path) {
            if let Some(dict) = value.as_dictionary() {
                let mut parts: Vec<String> = Vec::new();
                if let Some(p) = dict.get("Program").and_then(|v| v.as_string()) {
                    parts.push(p.to_owned());
                }
                if let Some(args) = dict.get("ProgramArguments").and_then(|v| v.as_array()) {
                    parts.extend(args.iter().filter_map(|a| a.as_string().map(str::to_owned)));
                }
                return parts.join(" ");
            }
        }
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

impl Platform for MacOs {
    fn name(&self) -> &'static str {
        "macOS"
    }
    fn noun(&self) -> &'static str {
        "launch agent"
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
        for (dir, scope) in &self.dirs {
            for path in list_files(dir, |p| p.extension().map(|e| e == "plist").unwrap_or(false)) {
                let Some(fingerprint) = file_hash16(&path) else { continue };
                out.push(Entry {
                    id: path.to_string_lossy().into_owned(),
                    label: file_stem(&path),
                    command: Self::command_of(&path),
                    fingerprint,
                    scope: *scope,
                    file: Some(path),
                });
            }
        }
        out
    }

    fn impersonation(&self, e: &Entry) -> Option<String> {
        let name = e.file.as_deref().map(file_name).unwrap_or_default();
        if name.starts_with("com.Apple.") {
            Some("fake Apple label (capital A)".to_owned())
        } else if name.starts_with("com.apple.") && e.scope == Scope::User {
            Some("Apple-named agent in your own LaunchAgents, Apple never puts one there".to_owned())
        } else {
            None
        }
    }

    fn is_own(&self, e: &Entry) -> bool {
        e.id.contains(LABEL)
    }

    fn cpu_allowlist(&self) -> &'static [&'static str] {
        &["xcode", "clang", "rustc", "cargo", "ffmpeg", "final cut", "compressor", "ollama"]
    }

    fn processes(&self) -> Vec<Proc> {
        parse_ps(&self.runner.run("ps", &["-Ao", "pid,pcpu,user,comm,args"]).stdout)
    }

    fn startup_files(&self) -> Vec<PathBuf> {
        [".zshrc", ".zprofile", ".zshenv", ".bash_profile", ".bashrc", ".profile"]
            .iter()
            .map(|f| self.home.join(f))
            .collect()
    }

    fn schedule(&self) -> Vec<String> {
        sorted_lines(&self.runner.run("crontab", &["-l"]).stdout)
    }

    fn login_items(&self) -> Vec<String> {
        let out = self.runner.run(
            "osascript",
            &["-e", "tell application \"System Events\" to get the name of every login item"],
        );
        let mut items: Vec<String> =
            out.stdout.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        items.sort();
        items
    }

    fn notify(&self, title: &str, subtitle: &str, body: &str) {
        let clean = |s: &str| s.replace('"', "'").replace('\\', "");
        let script = format!(
            "display notification \"{}\" with title \"{}\" subtitle \"{}\"",
            clean(body),
            clean(title),
            clean(subtitle)
        );
        self.runner.run("osascript", &["-e", &script]);
    }

    fn monitor_installed(&self) -> bool {
        self.own_plist().is_file()
    }

    fn install_monitor(&self, exe: &Path, state_dir: &Path) -> Result<(), String> {
        let exe_text = exe.to_string_lossy();
        if exe_text.starts_with("/Volumes/") {
            return Err("Move Sheshnag to Applications before turning on auto-watch, so the schedule does not point at a disk image.".to_owned());
        }
        let last_run = state_dir.join("last-run.txt");
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\"><dict>\n\
  <key>Label</key><string>{LABEL}</string>\n\
  <key>ProgramArguments</key><array><string>{}</string><string>dekho</string><string>quiet</string></array>\n\
  <key>StartInterval</key><integer>600</integer>\n\
  <key>RunAtLoad</key><true/>\n\
  <key>StandardOutPath</key><string>{}</string>\n\
  <key>StandardErrorPath</key><string>{}</string>\n\
</dict></plist>\n",
            xml_escape(&exe_text),
            xml_escape(&last_run.to_string_lossy()),
            xml_escape(&last_run.to_string_lossy()),
        );
        let path = self.own_plist();
        write_creating(&path, plist.as_bytes())?;
        let domain = format!("gui/{}", self.uid());
        self.runner.run("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
        let out = self.runner.run("launchctl", &["bootstrap", &domain, &path.to_string_lossy()]);
        if out.ok() { Ok(()) } else { Err(format!("launchctl bootstrap failed: {}", out.stderr.trim())) }
    }

    fn remove_monitor(&self) -> Result<(), String> {
        self.runner.run("launchctl", &["bootout", &format!("gui/{}/{LABEL}", self.uid())]);
        let _ = std::fs::remove_file(self.own_plist());
        Ok(())
    }

    fn stop(&self, _id: &str, label: &str) -> Result<(), String> {
        // Booting out the job also ends the process it started, so no pattern-based kill is needed.
        self.runner.run("launchctl", &["bootout", &format!("gui/{}/{label}", self.uid())]);
        Ok(())
    }

    fn remove(&self, id: &str) -> Result<(), String> {
        std::fs::remove_file(id).map_err(|e| format!("{id}: {e}"))
    }

    fn export(&self, id: &str) -> Result<Vec<u8>, String> {
        std::fs::read(id).map_err(|e| format!("{id}: {e}"))
    }

    fn import(&self, id: &str, data: &[u8]) -> Result<(), String> {
        write_creating(Path::new(id), data)
    }

    fn kill(&self, pid: u32) -> Result<(), String> {
        let out = self.runner.run("kill", &["-9", &pid.to_string()]);
        if out.ok() { Ok(()) } else { Err(format!("kill {pid}: {}", out.stderr.trim())) }
    }

    fn destroyable(&self, id: &str) -> Result<(), String> {
        let roots: Vec<PathBuf> =
            self.dirs.iter().filter(|(_, s)| *s == Scope::User).map(|(d, _)| d.clone()).collect();
        let mut protected: Vec<PathBuf> =
            ["/System", "/usr", "/bin", "/sbin", "/Library", "/Applications", "/private/etc"]
                .iter()
                .map(PathBuf::from)
                .collect();
        protected.extend(user_protected_dirs(&self.home));
        file_destroyable(id, &roots, &protected)
    }
}
