# Sheshnag

Sheshnag is a security watcher for macOS, Linux and Windows, written in Rust,
with a native dashboard. It runs the three-layer protocol:

| Layer | Role |
| --- | --- |
| **Sakshi**, the witness | Detects, protects and warns. It only watches and speaks. |
| **Agni**, the fire | Destroys a confirmed target and nothing else. No modes. |
| **Kavach**, the shield | Clears every target first, seals a backup, verifies the result, and can restore. |

Nothing is destroyed without a person confirming a specific target. Agni acts only
on startup entries and processes in the user's own account, never on machine-wide
entries and never on another computer. The dashboard does not open at login, scan
on launch, or install background monitoring unless you ask.

## Platform support

| Platform | Status |
| --- | --- |
| macOS | Built, tested and verified on a real Mac. |
| Linux | Implemented (systemd user units, autostart, cron, shell files). Compiles and is tested with simulated commands. **Not yet run on a real Linux machine.** |
| Windows | Implemented (Run keys, Startup folder, Scheduled Tasks, PowerShell profiles). Compiles and is tested with simulated commands. **Not yet run on a real Windows machine.** |

## Run the dashboard

```sh
cargo run --release
```

## What the dashboard does

- **Run vibe check** scans once and lists what is new, changed or suspicious.
- **Lock in baseline** records the current state as trusted.
- **Auto-watch** installs or removes a scheduled check every 10 minutes
  (LaunchAgent, systemd user timer, or Scheduled Task).
- **Agni and Kavach:** each flagged startup entry or process offers
  *Review and destroy*. A dialog names exactly what will be destroyed. Kavach
  seals a backup first. Sealed startup entries can be restored from the vault.
- **Quarantine** moves a startup entry aside by name. **Trust it** stops flagging a
  file until its contents change.
- **Themes:** System, Dark and Light. The choice is stored in `~/.sheshnag/theme`.

## Command line

The `sheshnag` binary (build it with `cargo build -p sheshnag-cli`) and the
dashboard program accept the same commands.

```sh
sheshnag baseline                  # record the trusted state
sheshnag dekho                     # look once; exit 0 clean, 1 findings
sheshnag install | uninstall       # scheduled check every 10 minutes
sheshnag theek <path|id>           # stop flagging this until it changes
sheshnag jaago <label>             # quarantine a startup entry

sheshnag agni plan                 # what Agni could destroy right now
sheshnag agni destroy <target>...  # dry run: Kavach checks and shows what would happen
sheshnag agni destroy --confirm <target>...
sheshnag kavach vault              # list sealed backups
sheshnag kavach restore <id>       # put a sealed startup entry back
```

State lives in `~/.sheshnag` (`%USERPROFILE%\.sheshnag` on Windows). The original
bash script, `Src/sheshnag.sh`, is kept only as a behaviour reference for the tests.

## Layout

| Path | Contents |
| --- | --- |
| `Src/main.rs` | Dashboard (egui). Also runs the command line when given a command. |
| `crates/sheshnag-core` | Engine: Sakshi detectors, Kavach, Agni, and the macOS, Linux and Windows adapters. |
| `crates/sheshnag-cli` | The `sheshnag` command-line binary. |
| `qa/gtest` | GoogleTest acceptance suite that runs against either engine. |
| `docs/` | Requirements, architecture, design diagrams, action plan, test plan. |
| `packaging/` | macOS DMG, Linux `.deb` and tarball, Windows installer, release workflow. |

## Tests

```sh
cargo test --workspace                       # Rust unit, adapter, Kavach and UI tests

cmake -S qa/gtest -B build/qa -G Ninja && cmake --build build/qa
ctest --test-dir build/qa --output-on-failure                                   # legacy bash engine
cargo build -p sheshnag-cli
SHESHNAG_BIN=$PWD/target/debug/sheshnag ctest --test-dir build/qa --output-on-failure   # Rust engine
```

See [`docs/`](docs/README.md) for the full documentation and
[`qa/gtest`](qa/gtest/README.md) for the test suite.

## Install

Download from the releases page: a DMG for macOS, a `.deb` or `.tar.gz` for Linux,
and a setup `.exe` for Windows. See [`packaging/README.md`](packaging/README.md).
