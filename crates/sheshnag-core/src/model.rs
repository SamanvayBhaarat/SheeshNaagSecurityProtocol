use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Lives in the current user's own folders or registry hive.
    User,
    /// Machine-wide. Sheshnag reads it but Agni never changes it.
    System,
}

/// One thing that can start code by itself: a launch agent, systemd unit, Run key, and so on.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Stable identity: a file path, or `KEY::NAME` for a registry value.
    pub id: String,
    pub label: String,
    /// What it runs, as one plain string with the arguments joined by spaces.
    pub command: String,
    /// Changes whenever the entry's content changes.
    pub fingerprint: String,
    pub scope: Scope,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub cpu_text: String,
    pub user: String,
    pub comm: String,
    pub args: String,
}

impl Proc {
    pub fn cpu_whole(&self) -> i64 {
        self.cpu_text.split('.').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }
}

/// Something Agni could destroy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Persistence { id: String, label: String },
    Process { pid: u32, command: String },
}

impl Target {
    /// The text a person types to name this target on the command line.
    pub fn key(&self) -> String {
        match self {
            Target::Persistence { id, .. } => id.clone(),
            Target::Process { pid, .. } => format!("pid:{pid}"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Target::Persistence { id, .. } => format!("startup entry {id}"),
            Target::Process { pid, command } => format!("process {pid} ({command})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    /// The message without the leading `!! `.
    pub text: String,
    pub target: Option<Target>,
}
