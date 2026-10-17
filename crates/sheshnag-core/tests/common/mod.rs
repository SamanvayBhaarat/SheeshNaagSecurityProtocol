#![allow(dead_code)]
use sheshnag_core::platform::{linux::Linux, windows::Windows};
use sheshnag_core::runner::{CmdOut, Runner};
use sheshnag_core::Engine;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A command runner that records calls and returns canned output (longest matching whole-word prefix wins).
#[derive(Default)]
pub struct FakeRunner {
    responses: Mutex<Vec<(String, CmdOut)>>,
    calls: Mutex<Vec<String>>,
    inputs: Mutex<Vec<String>>,
}

impl FakeRunner {
    pub fn new() -> Arc<Self> {
        let f = Arc::new(Self::default());
        f.set("id -un", 0, "tester\n");
        f.set("id -u", 0, "501\n");
        f
    }

    pub fn set(&self, prefix: &str, code: i32, stdout: &str) {
        let mut r = self.responses.lock().unwrap();
        r.retain(|(p, _)| p != prefix);
        r.push((prefix.to_owned(), CmdOut { code, stdout: stdout.to_owned(), stderr: String::new() }));
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub fn called(&self, needle: &str) -> bool {
        self.calls().iter().any(|c| c.contains(needle))
    }

    pub fn inputs(&self) -> Vec<String> {
        self.inputs.lock().unwrap().clone()
    }

    fn answer(&self, joined: &str) -> CmdOut {
        self.calls.lock().unwrap().push(joined.to_owned());
        let r = self.responses.lock().unwrap();
        r.iter()
            .filter(|(p, _)| joined == p.as_str() || joined.starts_with(&format!("{p} ")))
            .max_by_key(|(p, _)| p.len())
            .map(|(_, o)| o.clone())
            .unwrap_or_default()
    }
}

impl Runner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> CmdOut {
        self.answer(&format!("{program} {}", args.join(" ")))
    }
    fn run_input(&self, program: &str, args: &[&str], input: &str) -> CmdOut {
        self.inputs.lock().unwrap().push(input.to_owned());
        self.answer(&format!("{program} {}", args.join(" ")))
    }
    fn spawn(&self, program: &str, args: &[&str]) {
        self.answer(&format!("{program} {}", args.join(" ")));
    }
}

pub fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub struct LinuxEnv {
    pub tmp: tempfile::TempDir,
    pub home: PathBuf,
    pub root: PathBuf,
    pub fake: Arc<FakeRunner>,
    pub engine: Engine,
}

impl LinuxEnv {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let root = tmp.path().join("root");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let fake = FakeRunner::new();
        let runner: Arc<dyn Runner> = fake.clone();
        let engine = Engine::new(Box::new(Linux::with_parts(home.clone(), root.clone(), runner)));
        Self { tmp, home, root, fake, engine }
    }

    pub fn unit(&self, name: &str, exec: &str) -> PathBuf {
        let p = self.home.join(".config/systemd/user").join(name);
        write(&p, &format!("[Service]\nExecStart={exec}\n"));
        p
    }

    pub fn findings(&self) -> Vec<String> {
        self.engine.scan_findings().into_iter().map(|f| f.text).collect()
    }

    pub fn has_finding(&self, needle: &str) -> bool {
        self.findings().iter().any(|f| f.contains(needle))
    }
}

pub struct WindowsEnv {
    pub tmp: tempfile::TempDir,
    pub fake: Arc<FakeRunner>,
    pub engine: Engine,
}

pub const RUN_HKCU: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_HKLM: &str = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run";

impl WindowsEnv {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let fake = FakeRunner::new();
        let runner: Arc<dyn Runner> = fake.clone();
        let engine = Engine::new(Box::new(Windows::with_parts(
            home,
            tmp.path().join("appdata"),
            tmp.path().join("programdata"),
            runner,
        )));
        Self { tmp, fake, engine }
    }

    /// Set what `reg query <key>` returns.
    pub fn registry(&self, key: &str, values: &[(&str, &str)]) {
        let mut text = format!("\r\nHKEY_{key}\r\n");
        for (name, data) in values {
            text.push_str(&format!("    {name}    REG_SZ    {data}\r\n"));
        }
        self.fake.set(&format!("reg query {key}"), 0, &text);
    }

    pub fn findings(&self) -> Vec<String> {
        self.engine.scan_findings().into_iter().map(|f| f.text).collect()
    }
}
