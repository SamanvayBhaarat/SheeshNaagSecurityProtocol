//! Command-line front end. The same code serves the `sheshnag` binary and the dashboard's
//! `sheshnag-ui <command>` mode, which the scheduled monitor uses.
use crate::engine::Engine;
use crate::kavach::{self, Kavach, Order};
use crate::model::Target;
use crate::platform;

pub const COMMANDS: [&str; 8] = ["baseline", "dekho", "install", "uninstall", "jaago", "theek", "agni", "kavach"];

pub fn is_command(arg: &str) -> bool {
    COMMANDS.contains(&arg)
}

const USAGE: &str = "Sheshnag: three layers, one protocol.

  sheshnag baseline                  record what is on this computer now as trusted
  sheshnag dekho [quiet]             look once: print anything new or suspicious
  sheshnag install | uninstall       look every 10 minutes and raise a notification when something is off
  sheshnag theek <path|id>           this one is fine: stop flagging it until it changes
  sheshnag jaago <label>             quarantine a startup entry by label, on your word only

  sheshnag agni plan                 list what Agni could destroy right now
  sheshnag agni destroy <target>...  dry run: Kavach checks the targets and shows what would happen
  sheshnag agni destroy --confirm <target>...
                                     Kavach seals a backup, Agni destroys, Kavach verifies
  sheshnag kavach vault              list sealed backups
  sheshnag kavach restore <id>       put a sealed startup entry back

Exit codes: 0 nothing found, 1 findings or a refusal, 2 usage error.";

fn print(out: &str) {
    print!("{out}");
}

pub fn run(args: &[String]) -> i32 {
    run_with(&Engine::new(platform::native()), args)
}

pub fn run_with(engine: &Engine, args: &[String]) -> i32 {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[args.len().min(1)..];
    match cmd {
        "baseline" => finish(engine.baseline()),
        "dekho" => finish(engine.dekho(rest.first().map(String::as_str) == Some("quiet"))),
        "install" => match std::env::current_exe() {
            Ok(exe) => finish(engine.install(&exe)),
            Err(e) => {
                eprintln!("sheshnag: cannot find its own path: {e}");
                1
            }
        },
        "uninstall" => finish(engine.uninstall()),
        "theek" => match rest.first() {
            Some(p) => finish(engine.theek(p)),
            None => missing("path"),
        },
        "jaago" => match rest.first() {
            Some(l) => finish(engine.jaago(l)),
            None => missing("label"),
        },
        "agni" => agni(engine, rest),
        "kavach" => kavach_cmd(engine, rest),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}

fn finish(o: crate::engine::Outcome) -> i32 {
    print(&o.output);
    o.code
}

fn missing(what: &str) -> i32 {
    eprintln!("sheshnag: a {what} is required.");
    2
}

fn agni(engine: &Engine, rest: &[String]) -> i32 {
    match rest.first().map(String::as_str) {
        Some("plan") => {
            let c = engine.candidates();
            if c.is_empty() {
                println!("Nothing is flagged that Agni could act on.");
            }
            for t in &c {
                println!("{}\n    {}", t.key(), t.describe());
            }
            0
        }
        Some("destroy") => destroy(engine, &rest[1..]),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}

fn destroy(engine: &Engine, rest: &[String]) -> i32 {
    let confirm = rest.iter().any(|a| a == "--confirm");
    let keys: Vec<&String> = rest.iter().filter(|a| *a != "--confirm").collect();
    if keys.is_empty() {
        return missing("target");
    }
    let candidates = engine.candidates();
    let targets: Vec<Target> = keys
        .iter()
        .map(|k| {
            candidates.iter().find(|t| &t.key() == *k).cloned().unwrap_or_else(|| match k.strip_prefix("pid:") {
                Some(n) => Target::Process { pid: n.parse().unwrap_or(0), command: String::new() },
                None => Target::Persistence { id: (*k).clone(), label: String::new() },
            })
        })
        .collect();

    if !confirm {
        return match Kavach::new(engine).check(&Order::new(targets.clone(), "dry-run")) {
            Ok(()) => {
                println!("Dry run. Kavach would clear {} target(s):", targets.len());
                for t in &targets {
                    println!("  {}", t.describe());
                }
                println!("Nothing was changed. Add --confirm to seal backups and destroy them.");
                0
            }
            Err(r) => {
                println!("{r}");
                1
            }
        };
    }
    match kavach::destroy(engine, targets, "user-cli") {
        Ok(report) => {
            println!("{}", report.text());
            if report.ok() { 0 } else { 1 }
        }
        Err(r) => {
            println!("{r}");
            1
        }
    }
}

fn kavach_cmd(engine: &Engine, rest: &[String]) -> i32 {
    let k = Kavach::new(engine);
    match rest.first().map(String::as_str) {
        Some("vault") => {
            let items = k.vault();
            if items.is_empty() {
                println!("The vault is empty.");
            }
            for m in items {
                println!("{}  {}  sealed {}", m.vault_id, m.id, m.sealed_at);
            }
            0
        }
        Some("restore") => match rest.get(1) {
            Some(id) => match k.restore(id) {
                Ok(msg) => {
                    println!("{msg}");
                    0
                }
                Err(e) => {
                    println!("{e}");
                    1
                }
            },
            None => missing("vault id"),
        },
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}
