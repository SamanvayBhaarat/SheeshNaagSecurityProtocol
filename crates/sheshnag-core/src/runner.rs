//! Running external programs, behind a trait so adapters can be tested with canned output.
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct CmdOut {
    /// Exit status, or -1 when the program could not be started.
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOut {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
}

pub trait Runner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> CmdOut;
    fn run_input(&self, program: &str, args: &[&str], input: &str) -> CmdOut;
    /// Start a program without waiting for it.
    fn spawn(&self, program: &str, args: &[&str]);
}

/// Runs real programs found on `PATH`.
pub struct SystemRunner;

fn collect(output: std::io::Result<std::process::Output>) -> CmdOut {
    match output {
        Ok(o) => CmdOut {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        },
        Err(e) => CmdOut { code: -1, stdout: String::new(), stderr: e.to_string() },
    }
}

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> CmdOut {
        collect(Command::new(program).args(args).stdin(Stdio::null()).output())
    }

    fn run_input(&self, program: &str, args: &[&str], input: &str) -> CmdOut {
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match child {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(input.as_bytes());
                }
                collect(child.wait_with_output())
            }
            Err(e) => CmdOut { code: -1, stdout: String::new(), stderr: e.to_string() },
        }
    }

    fn spawn(&self, program: &str, args: &[&str]) {
        let _ = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}
