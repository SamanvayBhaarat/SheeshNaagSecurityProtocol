# 01 — Gap Analysis: Requirements and Architecture Document vs Current State

| | |
| --- | --- |
| Compared | `Sheshnag — Requirements and Architecture.md` (the "RA document") against the code in this repository |
| Code reviewed | `Src/main.rs` (egui dashboard), `Src/sheshnag.sh` (detection engine, v0.2), `README.md`, `Logs/SheshNaag.log` |
| Date | 2026-10-16 |
| Status | Draft for owner review. Updated 2026-10-16 after implementation, see section 5 |

## 1. Summary (at the time of the analysis, before implementation; see section 5 for what changed)

The RA document describes Sheshnag as a **cross-platform, Rust-core** security watcher with a **three-layer protocol**. The repository today is a **macOS-only** tool whose **detection engine is a bash script**, wrapped by a **native Rust (egui) dashboard**. Layer 1 works and has been confirmed on a real Mac. Layers 2 and 3 do not exist, and the RA document itself says their logic is undesigned.

| Area | Documented | Actual | Gap |
| --- | --- | --- | --- |
| Layer 1 Sakshi | Built, watch-only | Built, watch-only (with one destructive manual tool) | Small |
| Layer 2 Agni | Design intent | Not built, no design | Expected, needs design |
| Layer 3 Kavach | Design intent | Not built, no design | Expected, needs design |
| Platforms | Mac, Linux, Windows | macOS only | **Large** |
| Core language | Rust | Bash engine, Rust only as GUI and launcher | **Large** |
| GUI | "Built in VS Code" | egui/eframe native window | Wording only, confirm toolkit |
| Log path | `~/.sheshnag.log` | `~/.sheshnag/log` | Small |
| Tests | Not mentioned | None existed before this review | Closed in part (59 gtest cases added) |

## 2. Detailed comparison

### 2.1 Matches the document

| ID | RA statement | Evidence in code |
| --- | --- | --- |
| M-1 | Detection is live | `check_agents`, `check_processes`, `check_rc_and_cron`, `check_login_items` in `sheshnag.sh` |
| M-2 | Baseline recording | `baseline()` snapshots agents, shell start-up files, cron, login items |
| M-3 | Scheduled monitoring every 10 minutes via LaunchAgent | `install()` writes a plist with `StartInterval` 600 |
| M-4 | On-demand "vibe check" | `dekho` command, "Run vibe check" button |
| M-5 | Activity log of findings | `~/.sheshnag/log`, shown in the dashboard |
| M-6 | Quarantine a launch agent | `jaago` unloads and moves the plist to `~/.sheshnag/quarantine` |
| M-7 | Mark a file as trusted | `theek` records path and hash in `baseline/accepted` |
| M-8 | Three panels Outer Eyes / Agni / Inner Eyes | Three protocol cards in the dashboard |
| M-9 | Real-world confirmation, 33 agents, 5 Adobe changes caught, nothing altered | `Logs/SheshNaag.log` shows the five CHANGED findings and the line "Nothing was changed" |

### 2.2 Mismatches that need a decision or a fix

| ID | Gap | Why it matters | Severity | Action |
| --- | --- | --- | --- | --- |
| G-1 | **Platforms.** The document says Mac, Linux, Windows. The engine relies on `launchctl`, `osascript`, `plutil`, `~/Library/LaunchAgents`, `/Library/LaunchDaemons` and `/bin/bash`. | Linux and Windows users cannot run it. | High | A-3.x |
| G-2 | **Core language.** The document says "Core: Rust" and that Rust "replaced" the bash version. In fact the Rust app writes the bash script to disk and runs `/bin/bash script action`. Bash is still the engine. | Cross-platform support, testing, type safety and performance all depend on the engine moving to Rust. | High | A-2.x |
| G-3 | **Layer naming and meaning.** The document names the layers Sakshi, Agni, Kavach and defines Layer 3 as protection first, cleanup second. The dashboard and README call them Outer Eyes, Agni, Inner Eyes and describe Layer 3 as "cleanup, restore". | Users and contributors get two different mental models. | Medium | A-1.7 |
| G-4 | **Log path.** The document says `~/.sheshnag.log`. The code uses `~/.sheshnag/log`. | Documentation error, or an intended change that was never made. | Low | A-1.7 |
| G-5 | **Watch-only claim vs `jaago`.** Layer 1 is "watch only", but `jaago` runs `pkill -9 -f <label>`. The label is an unanchored pattern, so it can kill unrelated processes. | This is destructive behaviour without the safeguards the document wants Kavach to provide. | Medium | A-1.4 |
| G-6 | **Layer 2 and Layer 3 have no design.** The document parks "detection-to-destruction-to-protection logic". | Blocks the product's core promise. | High (strategic) | A-4.x, A-5.x |
| G-7 | **Version labels.** Script header says v0.2, dashboard says v1.0, document says v1.0. | Confusing release story. | Low | A-1.7 |
| G-8 | **"GUI built in VS Code".** VS Code is an editor, not a GUI toolkit. The app uses egui/eframe. | Needs confirmation before it is written into the architecture. | Low | Decision D-5 |
| G-9 | **"Palanhar OS".** The document says the app is "for Palanhar OS" and also "Mac, Linux, Windows". The relationship is not defined. | Determines packaging and platform scope. | Medium | Decision D-4 |

### 2.3 Detection quality findings (not in the document)

These came from reading the script, from `Logs/SheshNaag.log`, and from the new automated tests.

| ID | Finding | Evidence | Type | Action |
| --- | --- | --- | --- | --- |
| Q-1 | **Defect SHE-D-001.** An inline interpreter passed as separate arguments (`["python3","-c","..."]`) is not flagged. The detector scans the JSON form of `ProgramArguments`, where arguments are separated by `","`, so the pattern `python[0-9.]* -c` cannot match. Same for `perl -e`. | Disabled test `FlagsInlinePythonPassedAsSeparateArguments` | False negative | A-1.1 |
| Q-2 | **Defect SHE-D-002.** `plutil` escapes slashes in JSON (`\/tmp\/payload`), so the `/tmp/` pattern never matches inside `ProgramArguments`. It only matches the plain `Program` key. | Disabled test `FlagsATmpPathInProgramArguments`, passing test `FlagsAProgramKeyThatPointsIntoTmp` | False negative | A-1.1 |
| Q-3 | **False positives on vendor agents.** Microsoft Entra and Adobe agents are flagged as "runs hidden commands (/tmp/)" on every scan. Adobe agents show as CHANGED after every vendor update. | `Logs/SheshNaag.log`, 2026-10-15 | False positive | A-1.2 |
| Q-4 | **Log noise.** Because of Q-3, the same two findings were logged about every 10 minutes for a day. | Same log | Usability | A-1.3 |
| Q-5 | **Accepted-file list only suppresses hidden-command findings.** A trusted file that is later changed by its vendor is reported as CHANGED until the whole baseline is re-recorded. | `accepted()` is only consulted in the content check | Usability | A-1.2 |
| Q-6 | **No machine-readable output.** The dashboard parses text lines that begin with `!!`. | `read_optional_lines` in `main.rs` | Maintainability | A-1.5 |
| Q-7 | **Unanchored kill.** See G-5. | Test `CurrentlyKillsByUnanchoredLabelPattern` documents it | Safety | A-1.4 |
| Q-8 | **`rc` file change is only a note.** A modified `.zshrc` is logged but not flagged unless it contains a pasted-style command. | Test `NotesAChangedStartupFileWithoutFlaggingBenignEdits` | Design choice, confirm | A-1.6 |

### 2.4 Added since the document was written

| ID | Item |
| --- | --- |
| N-1 | Dashboard redesign: gradient hero, peacock palette, Dark / Light (off-white) / System themes, preference saved to `~/.sheshnag/theme` |
| N-2 | Test seam `SHESHNAG_AGENT_DIRS` in the script, so tests never read real system folders |
| N-3 | gtest acceptance suite under `qa/gtest`, 59 tests, 2 disabled for known defects |
| N-4 | This documentation set |

## 3. Risks raised by the gap

1. **Agni as written is dangerous if automated.** "Does exactly one thing: destroy, no second-guessing" combined with the false-positive rate in Q-3 means an automatic Agni would have destroyed Adobe and Microsoft components on a real Mac within a day. See requirement `NFR-SAFE-02` and decision D-2.
2. **Reflection scenario is undefined.** The document says an attacker could "send that fire back" to the person's machine. Without a stated threat model Kavach cannot be designed or tested. See decision D-3.
3. **Hack-back is out of scope.** Any destructive action aimed at a remote system is legally risky and is not required by the document. This plan treats Agni as acting only on artifacts on the user's own machine. See decision D-7.

## 4. Decisions needed from the owner

| ID | Question | Recommendation |
| --- | --- | --- |
| D-1 | What exactly does Agni "destroy"? | Local persistence artifacts and processes that Sakshi identified: launch agents or services, their payload files, and the processes they run. Nothing else. |
| D-2 | Who "calls" Agni? | Only an explicit, per-incident confirmation by the user in v2.0. Pre-authorised automatic mode is a later, opt-in feature after the false-positive rate is measured. |
| D-3 | What is the threat Kavach defends against? | Three cases: destruction of the wrong target (false positive), attacker-crafted lures that make Agni destroy something valuable, and attacker persistence that survives or re-arms after Agni runs. Write this as a threat model before building. |
| D-4 | What is Palanhar OS relative to Mac, Linux, Windows? | Treat it as the product family name unless told otherwise. |
| D-5 | Is egui the intended GUI toolkit? | Yes, keep it. It is cross-platform and already in use. |
| D-6 | Pursue the patent filing? | Out of scope for engineering. Note that public release of the protocol or code can affect patentability in some jurisdictions, so decide before open-sourcing. |
| D-7 | Confirm that Sheshnag never acts against remote systems. | Yes. Make it a stated non-goal. |

## 5. Update after implementation (2026-10-16)

The engine was moved to Rust, Linux and Windows adapters were written, and Agni and Kavach were built on the design proposed in this document. What changed for each gap:

| ID | Gap | Now |
| --- | --- | --- |
| G-1 | Platforms | **Implemented, not yet verified on real machines.** Linux and Windows adapters exist. The whole workspace compiles for both targets and the adapters are tested on macOS with fake command output. A native run on real Linux and Windows machines is still needed. |
| G-2 | Core language | **Closed.** The engine is the `sheshnag-core` Rust crate. The dashboard calls it directly. The bash script is kept only as a legacy reference and as the oracle for tests. |
| G-3 | Layer naming | **Closed.** The dashboard says Sakshi, Agni and Kavach, with the document's definitions. |
| G-4 | Log path | **Open, document fix.** The code uses `~/.sheshnag/log` on every platform. Update the Requirements and Architecture document, which says `~/.sheshnag.log`. |
| G-5 | `jaago` kills by pattern | **Closed.** Quarantine stops the job through the service manager and never kills by name pattern. |
| G-6 | Agni and Kavach undesigned | **Built on the proposed design, awaiting owner approval** (decisions D-1 to D-3 below). |
| G-7 | Version labels | **Closed.** Everything is 1.0.0. |
| G-8 | "GUI built in VS Code" | **Closed.** The GUI is egui/eframe. Update the document wording. |
| G-9 | "Palanhar OS" | **Open.** Still needs the owner's answer. |
| Q-1 | Inline interpreter as separate arguments missed | **Fixed in the Rust engine.** Still present in the bash script. |
| Q-2 | `/tmp/` missed inside `ProgramArguments` | **Fixed in the Rust engine.** Still present in the bash script. |
| Q-3 | False positives on vendor agents | **Open.** Vendor trust (FR-S-16) is not built. Use "trust a file" meanwhile. |
| Q-4 | Log noise from repeated findings | **Open** (FR-S-17). |
| Q-5 | Trusting a file does not stop CHANGED findings | **Open.** |
| Q-6 | No machine-readable output | **Open** (FR-S-18). |
| Q-7 | Unanchored kill | **Closed** (same as G-5). |
| Q-8 | Changed start-up file is only a note | **Open, decision needed.** |
| Q-9 | New: the bash script also reported a changed agent as "gone" | **Fixed in the Rust engine.** Removed is now reported only for entries that are really gone. |

### What was decided while building (please review)

These are the owner decisions D-1 to D-3 answered with the recommendations from section 4, so that Agni and Kavach could exist. Nothing is automatic.

- **D-1, what Agni destroys:** startup entries in user-owned locations (a launch agent, a systemd user unit or autostart entry, a user Run key or Startup item) and flagged processes of the same user. Never machine-wide entries, never files elsewhere.
- **D-2, who calls Agni:** only a person, by confirming a specific target in the dashboard or by passing `--confirm` on the command line. There is no automatic mode.
- **D-3, what Kavach defends against:** destroying the wrong thing (a target must still be flagged by a fresh scan, and must sit in a user startup location outside every protected path), crafted lures (symlinks and path tricks are refused), repeated orders (a circuit breaker allows three per hour), and loss (a sealed copy is kept in the vault before anything is destroyed).
- **D-7, remote systems:** Agni never opens a network connection and never acts on another computer.

### Still open

Vendor trust, log de-duplication, JSON output, a native test run on real Linux and Windows machines, the integrity check of Sheshnag's own binary (FR-K-04, only the state folder check exists), egress blocking for Agni (FR-K-05), and the owner's answers to D-4 and D-6.
