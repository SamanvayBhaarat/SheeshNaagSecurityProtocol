//! Sakshi, the witness: detection and the manual tools. It reports and never acts on its own.
//!
//! State lives in `~/.sheshnag/` in the same layout the original script used, so an existing
//! baseline, log and findings keep working.
use crate::model::{Entry, Finding, Proc, Target};
use crate::patterns;
use crate::platform::Platform;
use crate::util::{basename, file_hash16, stamp};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const CPU_MAX: i64 = 300;

pub struct Outcome {
    pub code: i32,
    pub output: String,
}

enum Item {
    Note(String),
    Flag(Finding),
}

pub struct Engine {
    pub platform: Box<dyn Platform>,
    state: PathBuf,
}

fn write_lines(path: &Path, lines: &[String]) {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    let _ = fs::write(path, text);
}

fn parse_tsv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect()
}

impl Engine {
    pub fn new(platform: Box<dyn Platform>) -> Self {
        let state = platform.home().join(".sheshnag");
        Self { platform, state }
    }

    pub fn state_dir(&self) -> &Path {
        &self.state
    }

    pub fn base_dir(&self) -> PathBuf {
        self.state.join("baseline")
    }

    pub fn log_path(&self) -> PathBuf {
        self.state.join("log")
    }

    fn ensure(&self) {
        let _ = fs::create_dir_all(self.base_dir());
    }

    fn read_base(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.base_dir().join(name)).ok()
    }

    /// Append to the activity log and to the text the caller will print.
    fn note(&self, out: &mut String, msg: &str) {
        self.log_only(msg);
        out.push_str(msg);
        out.push('\n');
    }

    /// Append one line to the activity log.
    pub fn log(&self, msg: &str) {
        self.log_only(msg);
    }

    fn log_only(&self, msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.log_path()) {
            let _ = writeln!(f, "{}  {}", stamp(), msg);
        }
    }

    fn startup_snapshot(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .platform
            .startup_files()
            .iter()
            .filter(|p| p.is_file())
            .filter_map(|p| file_hash16(p).map(|h| format!("{}\t{}", p.display(), h)))
            .collect();
        v.sort();
        v
    }

    fn agent_snapshot(&self) -> Vec<String> {
        let mut v: Vec<String> = self.platform.persistence().iter().map(|e| format!("{}\t{}", e.id, e.fingerprint)).collect();
        v.sort();
        v
    }

    fn accepted(&self, id: &str, fingerprint: &str) -> bool {
        let wanted = format!("{id}\t{fingerprint}");
        self.read_base("accepted").map(|t| t.lines().any(|l| l == wanted)).unwrap_or(false)
    }

    // ------------------------------------------------------------------ baseline

    pub fn baseline(&self) -> Outcome {
        self.ensure();
        let mut out = String::new();
        let agents = self.agent_snapshot();
        let mut login = self.platform.login_items();
        login.sort();
        write_lines(&self.base_dir().join("agents"), &agents);
        write_lines(&self.base_dir().join("rc"), &self.startup_snapshot());
        write_lines(&self.base_dir().join("cron"), &self.platform.schedule());
        write_lines(&self.base_dir().join("login"), &login);
        let _ = fs::write(self.base_dir().join("when"), format!("{}\n", stamp()));
        self.note(&mut out, &format!("baseline recorded: {} agents, {} login items", agents.len(), login.len()));
        Outcome { code: 0, output: out }
    }

    // ------------------------------------------------------------------ checks

    fn check_agents(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let noun = self.platform.noun();
        let Some(base_text) = self.read_base("agents") else {
            items.push(Item::Note("no baseline yet: run  sheshnag baseline".to_owned()));
            return items;
        };
        let base = parse_tsv(&base_text);
        let entries = self.platform.persistence();
        let now: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.id.as_str(), e)).collect();

        for (id, e) in &now {
            let target = Some(Target::Persistence { id: (*id).to_owned(), label: e.label.clone() });
            match base.get(*id) {
                Some(old) if *old == e.fingerprint => {}
                Some(_) => items.push(Item::Flag(Finding { text: format!("{noun} CHANGED: {id}"), target })),
                None => items.push(Item::Flag(Finding { text: format!("{noun} NEW: {id}"), target })),
            }
        }
        for id in base.keys() {
            if !now.contains_key(id.as_str()) {
                items.push(Item::Note(format!("{noun} gone: {id}")));
            }
        }

        for e in &entries {
            if self.platform.is_own(e) {
                continue;
            }
            let target = Some(Target::Persistence { id: e.id.clone(), label: e.label.clone() });
            if let Some(why) = self.platform.impersonation(e) {
                items.push(Item::Flag(Finding { text: format!("{why}: {}", e.id), target: target.clone() }));
            }
            if self.accepted(&e.id, &e.fingerprint) {
                continue;
            }
            if let Some(tokens) = patterns::hidden_tokens(&e.command, self.platform.extra_hiders()) {
                items.push(Item::Flag(Finding {
                    text: format!("{noun} runs hidden commands ({tokens}): {}", e.id),
                    target,
                }));
            }
        }
        items
    }

    fn cpu_allowed(&self, p: &Proc) -> bool {
        let name = basename(&p.comm).to_lowercase();
        let name = name.trim_end_matches(".exe");
        self.platform.cpu_allowlist().iter().any(|a| name == *a || (a.len() > 3 && name.starts_with(a)))
    }

    fn check_processes(&self) -> Vec<Item> {
        let me = self.platform.current_user();
        let own_pid = std::process::id();
        let mut items = Vec::new();
        for p in self.platform.processes() {
            if p.pid == own_pid {
                continue;
            }
            let target = Some(Target::Process { pid: p.pid, command: format!("{} {}", p.comm, p.args).trim().to_owned() });
            if patterns::is_miner(&p.comm, &p.args) {
                items.push(Item::Flag(Finding {
                    text: format!("miner-like process {} ({}) at {}% CPU: {}", p.pid, p.comm, p.cpu_text, p.args),
                    target,
                }));
                continue;
            }
            if p.cpu_whole() > CPU_MAX && p.user == me && !self.cpu_allowed(&p) {
                items.push(Item::Flag(Finding {
                    text: format!("process {} ({}) is using {}% CPU for user {}", p.pid, p.comm, p.cpu_text, p.user),
                    target,
                }));
            }
        }
        items
    }

    fn check_rc_and_cron(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let Some(base_rc) = self.read_base("rc") else { return items };
        let base = parse_tsv(&base_rc);
        let now = parse_tsv(&self.startup_snapshot().join("\n"));
        let mut changed: BTreeSet<&String> = BTreeSet::new();
        for (k, v) in &now {
            if base.get(k) != Some(v) {
                changed.insert(k);
            }
        }
        for k in base.keys() {
            if !now.contains_key(k) {
                changed.insert(k);
            }
        }
        for f in changed {
            items.push(Item::Note(format!("shell start-up file changed: {f}")));
        }

        for f in self.platform.startup_files() {
            let Ok(text) = fs::read_to_string(&f) else { continue };
            let hit = text
                .lines()
                .enumerate()
                .find(|(_, l)| patterns::is_pasted_command(l, self.platform.extra_pasted()));
            if let Some((n, line)) = hit {
                items.push(Item::Flag(Finding {
                    text: format!("shell start-up file runs a pasted-style command: {}  ({}:{})", f.display(), n + 1, line),
                    target: None,
                }));
            }
        }

        let base_cron: Vec<String> =
            self.read_base("cron").map(|t| t.lines().map(str::to_owned).collect()).unwrap_or_default();
        let now_cron = self.platform.schedule();
        if base_cron != now_cron {
            let mut diff: Vec<String> = Vec::new();
            diff.extend(base_cron.iter().filter(|l| !now_cron.contains(l)).map(|l| format!("< {l}")));
            diff.extend(now_cron.iter().filter(|l| !base_cron.contains(l)).map(|l| format!("> {l}")));
            let shown: String = diff.iter().take(3).map(|d| format!("{d} ")).collect();
            items.push(Item::Flag(Finding {
                text: format!("{} changed: {}", self.platform.schedule_name(), shown),
                target: None,
            }));
        }
        items
    }

    fn check_login_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        let Some(base_text) = self.read_base("login") else { return items };
        let base: BTreeSet<&str> = base_text.lines().collect();
        for item in self.platform.login_items() {
            if !item.is_empty() && !base.contains(item.as_str()) {
                items.push(Item::Flag(Finding { text: format!("new Login Item: {item}"), target: None }));
            }
        }
        items
    }

    fn run_checks(&self) -> Vec<Item> {
        let mut all = self.check_agents();
        all.extend(self.check_processes());
        all.extend(self.check_rc_and_cron());
        all.extend(self.check_login_items());
        all
    }

    /// Run every check without writing anything. Kavach uses this to confirm a target is still flagged.
    pub fn scan_findings(&self) -> Vec<Finding> {
        self.run_checks()
            .into_iter()
            .filter_map(|i| match i {
                Item::Flag(f) => Some(f),
                Item::Note(_) => None,
            })
            .collect()
    }

    /// Things Agni could destroy, from a fresh scan. One entry per target.
    pub fn candidates(&self) -> Vec<Target> {
        let mut seen = BTreeSet::new();
        self.scan_findings()
            .into_iter()
            .filter_map(|f| f.target)
            .filter(|t| seen.insert(t.key()))
            .collect()
    }

    // ------------------------------------------------------------------ dekho (look once)

    pub fn dekho(&self, quiet: bool) -> Outcome {
        self.dekho_with_targets(quiet).0
    }

    /// Like [`Engine::dekho`], and also returns what Agni could act on, from the same pass.
    pub fn dekho_with_targets(&self, quiet: bool) -> (Outcome, Vec<Target>) {
        self.ensure();
        let mut out = String::new();
        let now_path = self.state.join("findings.now");
        let prev_path = self.state.join("findings.prev");
        match fs::read_to_string(&now_path) {
            Ok(t) => {
                let _ = fs::write(&prev_path, t);
            }
            Err(_) => {
                let _ = fs::write(&prev_path, "");
            }
        }

        let mut flagged: Vec<String> = Vec::new();
        let mut targets: Vec<Target> = Vec::new();
        for item in self.run_checks() {
            match item {
                Item::Note(n) => self.log_only(&n),
                Item::Flag(f) => {
                    let line = format!("!! {}", f.text);
                    self.log_only(&line);
                    flagged.push(line);
                    if let Some(t) = f.target {
                        if !targets.iter().any(|x| x.key() == t.key()) {
                            targets.push(t);
                        }
                    }
                }
            }
        }
        write_lines(&now_path, &flagged);
        let n = flagged.len();

        if n > 0 {
            for l in &flagged {
                out.push_str(l);
                out.push('\n');
            }
            let prev: BTreeSet<String> =
                fs::read_to_string(&prev_path).unwrap_or_default().lines().map(str::to_owned).collect();
            let new = flagged.iter().filter(|l| !prev.contains(*l)).count();
            self.note(&mut out, &format!("Sheshnag: {n} thing(s) need your eyes ({new} new since the last look). Nothing was changed."));
            if quiet && new > 0 {
                self.platform.notify(
                    "Sheshnag",
                    "Something new is on this computer",
                    &format!("{new} new finding(s). Open Sheshnag to review."),
                );
            }
            return (Outcome { code: 1, output: out }, targets);
        }

        if !quiet {
            match self.read_base("agents") {
                Some(t) => {
                    let count = t.lines().count();
                    self.note(&mut out, &format!("Sheshnag: all quiet. {count} agents as recorded, no miner, no new login item."));
                }
                None => self.note(&mut out, "Sheshnag: nothing suspicious seen. Record the baseline next:  sheshnag baseline"),
            }
        }
        (Outcome { code: 0, output: out }, targets)
    }

    // ------------------------------------------------------------------ manual tools

    /// "This one is fine": stop flagging an entry until its content changes.
    pub fn theek(&self, target: &str) -> Outcome {
        self.ensure();
        let mut out = String::new();
        let fingerprint = if Path::new(target).is_file() {
            file_hash16(Path::new(target))
        } else {
            self.platform.persistence().into_iter().find(|e| e.id == target).map(|e| e.fingerprint)
        };
        let Some(fp) = fingerprint else {
            self.note(&mut out, &format!("no such file: {target}"));
            return Outcome { code: 1, output: out };
        };
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.base_dir().join("accepted")) {
            let _ = writeln!(f, "{target}\t{fp}");
        }
        self.note(&mut out, &format!("accepted as fine until it changes: {target}"));
        Outcome { code: 0, output: out }
    }

    /// Quarantine by label: stop it, then move it aside and keep it as evidence.
    pub fn jaago(&self, label: &str) -> Outcome {
        self.ensure();
        let mut out = String::new();
        let quarantine = self.state.join("quarantine");
        let _ = fs::create_dir_all(&quarantine);

        let wanted_stem = Path::new(label).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let entries: Vec<Entry> = self
            .platform
            .persistence()
            .into_iter()
            .filter(|e| e.label == label || e.label == wanted_stem || Path::new(&e.label).file_stem().map(|s| s.to_string_lossy() == label).unwrap_or(false))
            .collect();

        let first_id = entries.first().map(|e| e.id.clone()).unwrap_or_default();
        let _ = self.platform.stop(&first_id, label);

        for e in &entries {
            if let Some(file) = &e.file {
                let dest = quarantine.join(file.file_name().unwrap_or_default());
                let moved = fs::rename(file, &dest).or_else(|_| fs::copy(file, &dest).and_then(|_| fs::remove_file(file)));
                match moved {
                    Ok(()) => self.note(&mut out, &format!("quarantined {} -> {}/", e.id, quarantine.display())),
                    Err(err) => self.note(&mut out, &format!("could not move {}: {err}", e.id)),
                }
            } else {
                match self.platform.export(&e.id) {
                    Ok(data) => {
                        let _ = fs::write(quarantine.join(format!("{}.entry.txt", e.label)), data);
                        match self.platform.remove(&e.id) {
                            Ok(()) => self.note(&mut out, &format!("quarantined {} -> {}/", e.id, quarantine.display())),
                            Err(err) => self.note(&mut out, &format!("could not remove {}: {err}", e.id)),
                        }
                    }
                    Err(err) => self.note(&mut out, &format!("could not read {}: {err}", e.id)),
                }
            }
        }
        self.note(&mut out, &format!("jaago done for {label}; if it was a system entry, run again as administrator"));
        Outcome { code: 0, output: out }
    }

    // ------------------------------------------------------------------ scheduled monitor

    pub fn install(&self, exe: &Path) -> Outcome {
        self.ensure();
        let mut out = String::new();
        if let Err(e) = self.platform.install_monitor(exe, &self.state) {
            self.note(&mut out, &format!("could not install: {e}"));
            return Outcome { code: 1, output: out };
        }
        self.note(&mut out, &format!("installed: looks every 10 minutes; log at {}", self.log_path().display()));
        // The monitor is itself startup content, so record it as trusted.
        out.push_str(&self.baseline().output);
        Outcome { code: 0, output: out }
    }

    pub fn uninstall(&self) -> Outcome {
        self.ensure();
        let mut out = String::new();
        let _ = self.platform.remove_monitor();
        self.note(&mut out, "uninstalled");
        Outcome { code: 0, output: out }
    }
}
