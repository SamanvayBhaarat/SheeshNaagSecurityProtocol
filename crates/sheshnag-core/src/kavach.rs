//! Kavach, the shield. It protects the machine and the person from Agni's own fire.
//!
//! Before Agni runs, Kavach decides whether each target may be destroyed (it must still be
//! flagged by a fresh scan, live in a user-owned location, and stay out of every protected
//! path), then seals a copy in the vault. After Agni runs, Kavach verifies the result and can
//! restore anything it sealed.
//!
//! [`ClearedOrder`] has private fields and no public constructor. Agni accepts nothing else,
//! so Agni cannot run on a target Kavach did not clear.
use crate::agni::{Agni, TargetResult};
use crate::engine::Engine;
use crate::model::Target;
use crate::platform::is_protected_process_name;
use crate::util::{epoch_millis, epoch_secs, sha256_hex, stamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::PathBuf;

pub const MAX_TARGETS: usize = 10;
pub const BREAKER_LIMIT: usize = 3;
pub const BREAKER_WINDOW_SECS: u64 = 3600;

#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub targets: Vec<Target>,
    /// Who confirmed it, for example `user-ui` or `user-cli`. An order never exists without one.
    pub confirmed_by: String,
}

impl Order {
    pub fn new(targets: Vec<Target>, confirmed_by: &str) -> Self {
        Self { id: format!("o{}", epoch_millis()), targets, confirmed_by: confirmed_by.to_owned() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Empty,
    TooMany { count: usize, max: usize },
    CircuitOpen,
    NotFlagged(String),
    Unsafe(String),
    StateUnsafe(String),
    SealFailed(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Empty => write!(f, "Kavach refused: the order names no targets."),
            Refusal::TooMany { count, max } => write!(f, "Kavach refused: {count} targets is above the limit of {max} per order."),
            Refusal::CircuitOpen => write!(f, "Kavach refused: too many orders in the last hour. Review what happened, then try again later."),
            Refusal::NotFlagged(t) => write!(f, "Kavach refused: {t} is not flagged by a fresh scan."),
            Refusal::Unsafe(why) => write!(f, "Kavach refused: {why}"),
            Refusal::StateUnsafe(why) => write!(f, "Kavach refused: Sheshnag's own state looks tampered with ({why})."),
            Refusal::SealFailed(why) => write!(f, "Kavach refused: could not seal a backup first ({why}). Nothing was changed."),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMeta {
    pub vault_id: String,
    pub order_id: String,
    pub kind: String,
    pub id: String,
    pub label: String,
    pub command: String,
    pub sha256: String,
    pub size: usize,
    pub sealed_at: String,
    pub confirmed_by: String,
}

pub struct SealedTarget {
    vault_id: String,
    target: Target,
}

impl SealedTarget {
    pub fn vault_id(&self) -> &str {
        &self.vault_id
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
}

/// An order Kavach has cleared and sealed. Only this module can create one.
///
/// ```compile_fail
/// // Agni cannot be handed an order that Kavach did not clear: the fields are private.
/// let _ = sheshnag_core::kavach::ClearedOrder { order_id: String::new(), items: Vec::new() };
/// ```
pub struct ClearedOrder {
    order_id: String,
    items: Vec<SealedTarget>,
}

impl ClearedOrder {
    pub fn order_id(&self) -> &str {
        &self.order_id
    }
    pub fn items(&self) -> &[SealedTarget] {
        &self.items
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub order_id: String,
    pub destroyed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub still_present: Vec<String>,
    pub vault_ids: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.failed.is_empty() && self.still_present.is_empty()
    }

    pub fn text(&self) -> String {
        let mut s = format!("Order {}: {} destroyed", self.order_id, self.destroyed.len());
        if !self.failed.is_empty() {
            s.push_str(&format!(", {} failed", self.failed.len()));
        }
        if !self.still_present.is_empty() {
            s.push_str(&format!(", {} came back or survived", self.still_present.len()));
        }
        s.push('.');
        for d in &self.destroyed {
            s.push_str(&format!("\n  destroyed  {d}"));
        }
        for (t, why) in &self.failed {
            s.push_str(&format!("\n  failed     {t}: {why}"));
        }
        for t in &self.still_present {
            s.push_str(&format!("\n  PRESENT    {t} (re-armed or still running)"));
        }
        if !self.vault_ids.is_empty() {
            s.push_str("\nSealed copies are in the vault. Restore with: sheshnag kavach restore <id>");
            for v in &self.vault_ids {
                s.push_str(&format!("\n  {v}"));
            }
        }
        s
    }
}

pub struct Kavach<'a> {
    engine: &'a Engine,
}

impl<'a> Kavach<'a> {
    pub fn new(engine: &'a Engine) -> Self {
        Self { engine }
    }

    fn orders_dir(&self) -> PathBuf {
        self.engine.state_dir().join("orders")
    }

    fn vault_dir(&self) -> PathBuf {
        self.engine.state_dir().join("vault")
    }

    fn recent_orders(&self) -> usize {
        let now = epoch_secs();
        fs::read_to_string(self.orders_dir().join("history"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_whitespace().next()?.parse::<u64>().ok())
            .filter(|t| now.saturating_sub(*t) < BREAKER_WINDOW_SECS)
            .count()
    }

    /// Everything Kavach checks before anything is sealed or destroyed. Changes nothing.
    pub fn check(&self, order: &Order) -> Result<(), Refusal> {
        if order.targets.is_empty() {
            return Err(Refusal::Empty);
        }
        if order.targets.len() > MAX_TARGETS {
            return Err(Refusal::TooMany { count: order.targets.len(), max: MAX_TARGETS });
        }
        if order.confirmed_by.trim().is_empty() {
            return Err(Refusal::Unsafe("the order has no confirmation from a person".to_owned()));
        }
        if self.recent_orders() >= BREAKER_LIMIT {
            return Err(Refusal::CircuitOpen);
        }
        if let Ok(meta) = fs::symlink_metadata(self.engine.state_dir()) {
            if meta.file_type().is_symlink() {
                return Err(Refusal::StateUnsafe("the state folder is a symbolic link".to_owned()));
            }
        }

        // A target must be flagged right now. Nothing from a stale list or from the target's own
        // metadata is trusted, which blocks crafted lures.
        let flagged: BTreeSet<String> =
            self.engine.scan_findings().into_iter().filter_map(|f| f.target).map(|t| t.key()).collect();
        let platform = &self.engine.platform;
        let entries = platform.persistence();
        let procs = platform.processes();
        let me = platform.current_user();

        for t in &order.targets {
            if !flagged.contains(&t.key()) {
                return Err(Refusal::NotFlagged(t.describe()));
            }
            match t {
                Target::Persistence { id, .. } => {
                    if let Some(e) = entries.iter().find(|e| &e.id == id) {
                        if platform.is_own(e) {
                            return Err(Refusal::Unsafe(format!("{id} belongs to Sheshnag itself")));
                        }
                    }
                    platform.destroyable(id).map_err(Refusal::Unsafe)?;
                }
                Target::Process { pid, command } => {
                    if *pid == std::process::id() {
                        return Err(Refusal::Unsafe("a process cannot be Sheshnag itself".to_owned()));
                    }
                    let Some(p) = procs.iter().find(|p| p.pid == *pid) else {
                        return Err(Refusal::Unsafe(format!("process {pid} is no longer running")));
                    };
                    if format!("{} {}", p.comm, p.args).trim() != command.trim() {
                        return Err(Refusal::Unsafe(format!("process {pid} is no longer the one that was flagged")));
                    }
                    if p.user != me {
                        return Err(Refusal::Unsafe(format!("process {pid} belongs to another user")));
                    }
                    if is_protected_process_name(&p.comm) {
                        return Err(Refusal::Unsafe(format!("{} is a protected system process", p.comm)));
                    }
                }
            }
        }
        Ok(())
    }

    /// Check, then seal a copy of every target in the vault.
    pub fn clear(&self, order: &Order) -> Result<ClearedOrder, Refusal> {
        self.check(order)?;
        let mut items = Vec::new();
        let order_dir = self.vault_dir().join(&order.id);
        for (i, t) in order.targets.iter().enumerate() {
            let dir = order_dir.join(i.to_string());
            fs::create_dir_all(&dir).map_err(|e| Refusal::SealFailed(e.to_string()))?;
            let (kind, id, label, command, data) = match t {
                Target::Persistence { id, label } => {
                    let data = self.engine.platform.export(id).map_err(Refusal::SealFailed)?;
                    ("persistence", id.clone(), label.clone(), String::new(), data)
                }
                Target::Process { pid, command } => ("process", format!("pid:{pid}"), String::new(), command.clone(), command.clone().into_bytes()),
            };
            fs::write(dir.join("data"), &data).map_err(|e| Refusal::SealFailed(e.to_string()))?;
            let meta = VaultMeta {
                vault_id: format!("{}/{}", order.id, i),
                order_id: order.id.clone(),
                kind: kind.to_owned(),
                id,
                label,
                command,
                sha256: sha256_hex(&data),
                size: data.len(),
                sealed_at: stamp(),
                confirmed_by: order.confirmed_by.clone(),
            };
            let json = serde_json::to_string_pretty(&meta).map_err(|e| Refusal::SealFailed(e.to_string()))?;
            fs::write(dir.join("meta.json"), json).map_err(|e| Refusal::SealFailed(e.to_string()))?;
            items.push(SealedTarget { vault_id: meta.vault_id, target: t.clone() });
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(self.vault_dir(), fs::Permissions::from_mode(0o700));
        }
        let _ = fs::create_dir_all(self.orders_dir());
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.orders_dir().join("history")) {
            use std::io::Write;
            let _ = writeln!(f, "{} {}", epoch_secs(), order.id);
        }
        self.engine.log(&format!("Kavach: cleared order {} and sealed {} target(s)", order.id, items.len()));
        Ok(ClearedOrder { order_id: order.id.clone(), items })
    }

    /// Re-scan after Agni ran and report what is gone and what came back.
    pub fn verify(&self, cleared: &ClearedOrder, results: &[TargetResult]) -> Report {
        let entries = self.engine.platform.persistence();
        let procs = self.engine.platform.processes();
        let mut report = Report {
            order_id: cleared.order_id().to_owned(),
            destroyed: Vec::new(),
            failed: Vec::new(),
            still_present: Vec::new(),
            vault_ids: cleared.items().iter().map(|i| i.vault_id().to_owned()).collect(),
        };
        for r in results {
            let desc = r.target.describe();
            match &r.outcome {
                Err(why) => report.failed.push((desc, why.clone())),
                Ok(()) => {
                    let back = match &r.target {
                        Target::Persistence { id, .. } => entries.iter().any(|e| &e.id == id),
                        Target::Process { pid, command } => {
                            procs.iter().any(|p| p.pid == *pid && format!("{} {}", p.comm, p.args).trim() == command.trim())
                        }
                    };
                    if back {
                        report.still_present.push(desc);
                    } else {
                        report.destroyed.push(desc);
                    }
                }
            }
        }
        let _ = fs::create_dir_all(self.orders_dir());
        let _ = fs::write(self.orders_dir().join(format!("{}.txt", report.order_id)), report.text());
        self.engine.log(&format!(
            "Kavach: order {} verified, {} destroyed, {} failed, {} still present",
            report.order_id,
            report.destroyed.len(),
            report.failed.len(),
            report.still_present.len()
        ));
        report
    }

    pub fn vault(&self) -> Vec<VaultMeta> {
        let mut out = Vec::new();
        let Ok(orders) = fs::read_dir(self.vault_dir()) else { return out };
        for order in orders.filter_map(|e| e.ok()) {
            let Ok(items) = fs::read_dir(order.path()) else { continue };
            for item in items.filter_map(|e| e.ok()) {
                if let Ok(text) = fs::read_to_string(item.path().join("meta.json")) {
                    if let Ok(meta) = serde_json::from_str::<VaultMeta>(&text) {
                        out.push(meta);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.vault_id.cmp(&b.vault_id));
        out
    }

    /// Put a sealed target back exactly as it was. Only the person asks for this.
    pub fn restore(&self, vault_id: &str) -> Result<String, String> {
        if vault_id.contains("..") {
            return Err("invalid vault id".to_owned());
        }
        let dir = self.vault_dir().join(vault_id);
        let meta: VaultMeta = serde_json::from_str(&fs::read_to_string(dir.join("meta.json")).map_err(|e| format!("{vault_id}: {e}"))?)
            .map_err(|e| e.to_string())?;
        if meta.kind != "persistence" {
            return Err("A process cannot be restored, only startup entries can.".to_owned());
        }
        let data = fs::read(dir.join("data")).map_err(|e| e.to_string())?;
        if sha256_hex(&data) != meta.sha256 {
            return Err("The sealed copy no longer matches its checksum, so it was not restored.".to_owned());
        }
        if self.engine.platform.persistence().iter().any(|e| e.id == meta.id) {
            return Err(format!("{} already exists, nothing was overwritten.", meta.id));
        }
        self.engine.platform.import(&meta.id, &data)?;
        self.engine.log(&format!("Kavach: restored {}", meta.id));
        Ok(format!("Restored {}", meta.id))
    }
}

/// The whole protocol for one confirmed order: Kavach clears and seals, Agni destroys, Kavach verifies.
pub fn destroy(engine: &Engine, targets: Vec<Target>, confirmed_by: &str) -> Result<Report, Refusal> {
    let kavach = Kavach::new(engine);
    let order = Order::new(targets, confirmed_by);
    let cleared = kavach.clear(&order)?;
    let results = Agni::execute(engine, &cleared);
    Ok(kavach.verify(&cleared, &results))
}
