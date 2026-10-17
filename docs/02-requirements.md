# 02 — Software Requirements Specification

| | |
| --- | --- |
| Product | Sheshnag (शेषनाग), a three-layer security watcher |
| Version | Target v2.0, baseline v1.0 |
| Source | `Sheshnag — Requirements and Architecture.md`, current code, gap analysis `01-gap-analysis.md` |
| Status | Draft for owner review. Implementation status in section 10 |

Requirement keywords: **shall** is mandatory, **should** is desirable. Status values: `Done` is built and tested, `Partial` is built with known gaps, `Planned` is not built.
Every requirement has an ID that the test plan and test cases cite.

## 1. Purpose and scope

Sheshnag watches a computer for signs of malicious persistence and hostile activity. It then warns the person (Layer 1, Sakshi), can destroy confirmed malicious artifacts on that computer (Layer 2, Agni), and protects the machine and the person from the consequences of that destruction (Layer 3, Kavach).

**In scope:** the local machine and the user account that runs Sheshnag.
**Out of scope (non-goals):**
- Any action against a remote system, including counter-attacks or "hack back".
- Network-wide or fleet management.
- Antivirus-style signature scanning of all files.

## 2. Stakeholders and users

| Role | Need |
| --- | --- |
| Owner / author | A protocol that is faithful to the three-layer idea, safe, and publishable |
| Everyday user | Clear warnings, few false alarms, nothing destroyed without understanding |
| Security-minded user | Evidence, logs, and a way to review and trust |
| Contributor | Clear architecture, tests, and a build that works on their OS |

## 3. Layer 1 — Sakshi (the witness): functional requirements

| ID | Requirement | Status |
| --- | --- | --- |
| FR-S-01 | The system shall record a baseline of launch agents and daemons, shell start-up files, crontab and login items. | Done |
| FR-S-02 | The system shall report launch agents that are new, changed or removed relative to the baseline. New and changed are findings. Removed is logged only. | Done |
| FR-S-03 | The system shall flag agents whose command hides activity (base64, osascript, curl, wget, `/tmp/`, nohup, inline python or perl, eval). | Done in the Rust engine. The legacy bash script still has SHE-D-001 and SHE-D-002 |
| FR-S-04 | The system shall flag labels that imitate Apple (`com.Apple.*`, or `com.apple.*` in the user's own folder). | Done |
| FR-S-05 | The system shall flag miner-like processes by name and any user process above 300 percent CPU, except an allow-list of build tools. | Done |
| FR-S-06 | The system shall flag pasted-style commands (`curl \| sh`, `base64 -d`, `osascript`) in shell start-up files and any crontab change. | Done |
| FR-S-07 | The system shall flag new login items. | Done |
| FR-S-08 | In scheduled mode the system shall raise one desktop notification per scan that has new findings, and none for repeats. | Done |
| FR-S-09 | The user shall be able to mark a file as trusted until its content changes. | Partial. See Q-5 |
| FR-S-10 | The system shall write its state to a per-user folder, including `findings.now`, `findings.prev` and a timestamped activity log. | Done |
| FR-S-11 | The user shall be able to install and remove the scheduled monitor (every 10 minutes). | Done |
| FR-S-12 | The user shall be able to quarantine a launch agent by label. The plist is moved, not deleted. | Done in the Rust engine |
| FR-S-13 | The command-line contract shall remain stable: exit 0 for no findings, 1 for findings, 2 for usage errors. | Done |
| FR-S-14 | Findings should carry a stable ID, a severity (info, warning, critical), a confidence level and the evidence that triggered them. | Planned |
| FR-S-15 | Quarantine shall stop processes by exact match on the agent's own process, never by an unanchored pattern. | Done in the Rust engine |
| FR-S-16 | Findings for agents signed by a verified vendor (for example Adobe, Microsoft) should be downgraded or suppressed after the user trusts the vendor once. | Planned |
| FR-S-17 | The same unchanged finding should be logged at most once per day, with a counter. | Planned |
| FR-S-18 | The engine shall provide machine-readable output (`--json`). | Planned |
| FR-S-19 | The warning shall state that crossing the limit leads to consequences at the next layer, addressed to both the person and the intruder. | Planned |

## 4. Layer 2 — Agni (the fire): functional requirements

These requirements are **proposed** because the RA document leaves the logic undesigned. They are implemented (section 10) but still need the owner's approval (decisions D-1 and D-2).

| ID | Requirement |
| --- | --- |
| FR-A-01 | Agni shall run only when it receives a valid **order** naming the exact targets to destroy. |
| FR-A-02 | Agni shall have exactly one behaviour: destroy the targets in the order. It shall have no modes, options or retry heuristics. |
| FR-A-03 | An order shall be created only by an explicit user confirmation of a specific finding (v2.0). An opt-in pre-authorised policy may create orders later. |
| FR-A-04 | Agni shall act only on local artifacts: launch agents or services, their payload files, and the processes they run. |
| FR-A-05 | Agni shall never open a network connection and shall never act on a remote system. |
| FR-A-06 | Agni shall refuse any target that Kavach has not cleared (see FR-K-01). |
| FR-A-07 | Agni shall report every destroyed target and every failure to the activity log. |
| FR-A-08 | The ugliness is by design: no progress animation, no confirmation inside Agni, no partial modes. The nuance lives in the order and in Kavach. |

## 5. Layer 3 — Kavach (the shield): functional requirements

Proposed, pending decision D-3. Implemented, see section 10.

| ID | Requirement |
| --- | --- |
| FR-K-01 | Before Agni runs, Kavach shall **clear** every target against a deny-list of protected paths and labels (operating system, the user's documents, Sheshnag's own files) and refuse any target that matches. |
| FR-K-02 | Before Agni runs, Kavach shall seal a copy of each target and its metadata in an evidence vault, so destruction is recoverable. |
| FR-K-03 | Kavach shall bound the blast radius: a maximum number of targets per order, and a circuit breaker that blocks further orders after repeated Agni runs in a short period. |
| FR-K-04 | Kavach shall verify the integrity of Sheshnag's own binaries, scripts and state, and refuse to arm Agni if they changed unexpectedly. |
| FR-K-05 | Kavach shall run Agni with the least privilege needed and with network egress disabled where the platform allows. |
| FR-K-06 | After Agni runs, Kavach shall re-scan to confirm the targets are gone and that nothing was re-armed, and report the result. |
| FR-K-07 | Kavach shall provide a restore operation that returns a sealed target to its original place. |
| FR-K-08 | Kavach shall defend against reflection: a finding or target whose name, path or content was crafted to point Agni at something valuable shall be caught by FR-K-01 and FR-K-03, never trusted from the target's own metadata. |

## 6. Cross-cutting functional requirements

### 6.1 Dashboard (GUI)

| ID | Requirement | Status |
| --- | --- | --- |
| FR-U-01 | The dashboard shall show the three layers and what each one does in this build, without overstating capability. | Done |
| FR-U-02 | The dashboard shall show current status: baseline present, monitor on or off, finding count. | Done |
| FR-U-03 | The dashboard shall never run a scan on launch or install monitoring without a user action. | Done |
| FR-U-04 | The dashboard shall offer Dark, Light (off-white) and System themes, and remember the choice. | Done |
| FR-U-05 | The dashboard shall use the layer names Sakshi, Agni and Kavach, with Outer Eyes and Inner Eyes as subtitles. | Done |
| FR-U-06 | The dashboard shall show a finding's evidence, severity and the action available to the user. | Planned |
| FR-U-07 | The dashboard should provide an Agni order flow with a plain-language summary of what will be destroyed and what was sealed. | Done |

### 6.2 Platform support

| ID | Requirement | Status |
| --- | --- | --- |
| FR-P-01 | The system shall run on macOS. | Done |
| FR-P-02 | The system shall run on Linux, covering systemd user units, XDG autostart, cron and shell start-up files. | Implemented, not yet run on a real Linux machine |
| FR-P-03 | The system shall run on Windows, covering Run keys, Scheduled Tasks, Startup folder and services. | Implemented, not yet run on a real Windows machine |
| FR-P-04 | Platform-specific code shall sit behind one interface so detectors are shared. | Done |

## 7. Non-functional requirements

| ID | Category | Requirement |
| --- | --- | --- |
| NFR-SAFE-01 | Safety | Layer 1 shall not modify, move, delete or kill anything unless the user invokes a manual tool. |
| NFR-SAFE-02 | Safety | No destructive action shall run automatically in v2.0. |
| NFR-SAFE-03 | Safety | Every destructive action shall be preceded by a sealed backup (FR-K-02). |
| NFR-SEC-01 | Security | Sheshnag shall not require administrator rights for Layer 1. |
| NFR-SEC-02 | Security | Test seams shall not widen what a normal user can make Sheshnag read or change. |
| NFR-PRIV-01 | Privacy | All data stays on the machine. No telemetry, no network calls. |
| NFR-PERF-01 | Performance | A scan should finish in under 5 seconds on a machine with 100 launch agents. |
| NFR-PERF-02 | Performance | The idle dashboard should use less than 2 percent of one CPU core. Currently it repaints about 25 times a second, which needs review. |
| NFR-REL-01 | Reliability | A failing check shall not stop the other checks, and the failure shall be logged. |
| NFR-REL-02 | Reliability | Detection results shall be deterministic for the same machine state. |
| NFR-UX-01 | Usability | A first-time user shall be told what to do next at every state. |
| NFR-UX-02 | Usability | Text shall meet WCAG AA contrast in both Dark and Light themes. |
| NFR-MNT-01 | Maintainability | Detection logic shall have automated tests, run in a hermetic sandbox. |
| NFR-PORT-01 | Portability | Platform code shall be isolated, and CI shall build on all three platforms. |
| NFR-DOC-01 | Documentation | Architecture, requirements, design and test documents shall be updated in the same change as the behaviour they describe. |

## 8. Constraints and assumptions

- Language for the core is Rust. The bash engine is transitional.
- The GUI toolkit is egui/eframe (assumed, decision D-5).
- Sheshnag runs as the logged-in user. System-wide folders may need elevated rights to change, so v1 reads them but does not write to them.
- A quiet result is never a guarantee of safety. This statement appears in the product.

## 9. Traceability

The full requirement-to-test matrix is in `06-test-plan.md`, section 7.

## 10. Implementation status (2026-10-16)

| Requirement | Status | Where | Verified by |
| --- | --- | --- | --- |
| FR-A-01 order required | Done | `kavach::Order`, `Agni::execute(ClearedOrder)` | Rust tests `kavach.rs`, compile-fail doctest |
| FR-A-02 one behaviour | Done | `agni.rs` has a single code path | Review |
| FR-A-03 only a person creates an order | Done | UI dialog, CLI `--confirm`, unconfirmed orders refused | `empty_oversized_and_unconfirmed_orders_are_refused`, UI test for the dialog |
| FR-A-04 local artifacts only | Done | `Platform::destroyable`, user-scope only | `machine_wide_units_are_never_destroyed`, `machine_wide_registry_keys_are_never_touched` |
| FR-A-05 no network | Done by construction | Agni and Kavach make no network calls and the crate has no network dependency | Review |
| FR-A-06 only cleared targets | Done, enforced by the compiler | `ClearedOrder` has private fields | Compile-fail doctest |
| FR-A-07 everything logged | Done | `Agni: destroyed ...` log lines | Review |
| FR-A-08 deliberately plain | Done | No modes, no progress, no prompts inside Agni | Review |
| FR-K-01 deny-list and scope | Done | `file_destroyable`, protected locations per platform | `a_symlink_cannot_be_used...`, `protected_locations_are_refused...` |
| FR-K-02 seal before destroy | Done | `Kavach::clear`, vault with checksum | `destroy_seals_a_copy...` |
| FR-K-03 blast radius, circuit breaker | Done | 10 targets per order, 3 orders per hour | `the_circuit_breaker...`, `empty_oversized...` |
| FR-K-04 integrity of Sheshnag itself | **Partial** | State folder symlink check only. Binary and script hash check is not built. | Planned |
| FR-K-05 least privilege, no network egress | **Partial** | Runs as the user, makes no network calls. OS-level egress blocking is not built. | Planned |
| FR-K-06 verify after Agni | Done | `Kavach::verify`, reports "still present" | `a_process_that_survives...` |
| FR-K-07 restore | Done | `Kavach::restore`, checksum checked, never overwrites | `restore_*` tests |
| FR-K-08 reflection and lures | Done | A target must be flagged by a fresh scan, symlinks and outside-root paths refused | `only_flagged_targets_are_cleared`, symlink test |
| FR-S-03, 12, 15 | Done in the Rust engine | `engine.rs`, `patterns.rs` | gtest against the Rust CLI, Rust tests |
| FR-P-02 Linux | Implemented | `platform/linux.rs` | 16 Rust tests with fakes, compiles for Linux. Not run on a real machine. |
| FR-P-03 Windows | Implemented | `platform/windows.rs` | 10 Rust tests with fakes, parser unit tests, compiles for Windows. Not run on a real machine. |
| FR-U-05, FR-U-07 | Done | Dashboard | UI tests |
| FR-S-14, 16, 17, 18, 19 | Still planned | | |
