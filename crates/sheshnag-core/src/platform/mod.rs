//! Everything that touches an operating system sits behind [`Platform`].
//! The detectors, Kavach and Agni are shared across macOS, Linux and Windows.
use crate::model::{Entry, Proc};
use std::path::{Path, PathBuf};

pub mod linux;
pub mod macos;
pub mod windows;

pub trait Platform: Send + Sync {
    fn name(&self) -> &'static str;
    /// What an entry is called in messages, for example "launch agent".
    fn noun(&self) -> &'static str;
    fn schedule_name(&self) -> &'static str;
    fn home(&self) -> &Path;
    fn current_user(&self) -> String;

    // ---- observe (Sakshi) ----
    fn persistence(&self) -> Vec<Entry>;
    fn impersonation(&self, entry: &Entry) -> Option<String>;
    fn is_own(&self, entry: &Entry) -> bool;
    fn extra_hiders(&self) -> &'static str {
        ""
    }
    fn extra_pasted(&self) -> &'static str {
        ""
    }
    /// Lower-case process names that may legitimately use a lot of CPU.
    fn cpu_allowlist(&self) -> &'static [&'static str];
    fn processes(&self) -> Vec<Proc>;
    fn startup_files(&self) -> Vec<PathBuf>;
    fn schedule(&self) -> Vec<String>;
    fn login_items(&self) -> Vec<String>;
    fn notify(&self, title: &str, subtitle: &str, body: &str);

    // ---- scheduled monitor ----
    fn monitor_installed(&self) -> bool;
    fn install_monitor(&self, exe: &Path, state_dir: &Path) -> Result<(), String>;
    fn remove_monitor(&self) -> Result<(), String>;

    // ---- act (manual tools, Kavach, Agni) ----
    /// Stop an entry from running now and from starting again, without deleting it.
    fn stop(&self, id: &str, label: &str) -> Result<(), String>;
    fn remove(&self, id: &str) -> Result<(), String>;
    fn export(&self, id: &str) -> Result<Vec<u8>, String>;
    fn import(&self, id: &str, data: &[u8]) -> Result<(), String>;
    fn kill(&self, pid: u32) -> Result<(), String>;
    /// Whether Agni may ever touch this entry. Ok means it is in a user-owned location.
    fn destroyable(&self, id: &str) -> Result<(), String>;
}

/// The platform this build runs on.
pub fn native() -> Box<dyn Platform> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        let dirs = std::env::var("SHESHNAG_AGENT_DIRS")
            .ok()
            .map(|v| v.split_whitespace().map(PathBuf::from).collect());
        Box::new(macos::MacOs::new(home, dirs))
    } else if cfg!(target_os = "windows") {
        Box::new(windows::Windows::new(home))
    } else {
        Box::new(linux::Linux::new(home))
    }
}

// ---- shared helpers for file-based adapters ----

pub(crate) fn list_files(dir: &Path, accept: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && accept(p)).collect(),
        Err(_) => Vec::new(),
    };
    out.sort();
    out
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Shared safety check for file-based targets: inside an allowed root, outside every protected
/// path, not a symlink, and a regular file. Paths are resolved first, so `..` and symlinks cannot
/// smuggle a target out of its folder.
pub(crate) fn file_destroyable(id: &str, roots: &[PathBuf], protected: &[PathBuf]) -> Result<(), String> {
    let path = Path::new(id);
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{id}: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{id} is a symbolic link"));
    }
    if !meta.is_file() {
        return Err(format!("{id} is not a regular file"));
    }
    let real = std::fs::canonicalize(path).map_err(|e| format!("{id}: {e}"))?;
    for p in protected {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if real.starts_with(&p) {
            return Err(format!("{id} is inside the protected location {}", p.display()));
        }
    }
    let inside = roots.iter().any(|r| {
        let r = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
        real.starts_with(&r)
    });
    if !inside {
        return Err(format!("{id} is outside the user startup locations Agni may act on"));
    }
    Ok(())
}

pub(crate) fn write_creating(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Parse `ps -Ao pid,pcpu,user,comm,args` output (header line first).
pub(crate) fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let cpu = it.next()?.to_owned();
            let user = it.next()?.to_owned();
            let comm = it.next()?.to_owned();
            let args = it.collect::<Vec<_>>().join(" ");
            Some(Proc { pid, cpu_text: cpu, user, comm, args })
        })
        .collect()
}

pub(crate) fn sorted_lines(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.lines().map(str::to_owned).filter(|l| !l.is_empty()).collect();
    v.sort();
    v
}

pub(crate) fn user_from(runner: &dyn crate::runner::Runner) -> String {
    let out = runner.run("id", &["-un"]);
    if out.ok() && !out.stdout.trim().is_empty() {
        return out.stdout.trim().to_owned();
    }
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default()
}

pub(crate) const SHARED_PROTECTED_PROCESSES: &[&str] = &[
    "launchd", "systemd", "init", "kernel_task", "windowserver", "loginwindow", "finder", "dock",
    "sshd", "explorer.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe",
    "smss.exe", "system", "dbus-daemon", "networkmanager", "gnome-shell", "kwin",
];

pub(crate) fn is_protected_process_name(name: &str) -> bool {
    let n = crate::util::basename(name).to_lowercase();
    SHARED_PROTECTED_PROCESSES.contains(&n.as_str())
}

pub(crate) fn user_protected_dirs(home: &Path) -> Vec<PathBuf> {
    [
        "Documents", "Desktop", "Downloads", "Pictures", "Movies", "Music", "Videos", ".ssh", ".gnupg",
        ".sheshnag", "Library/Keychains",
    ]
    .iter()
    .map(|d| home.join(d))
    .collect()
}
