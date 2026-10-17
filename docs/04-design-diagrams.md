# 04 — Design Diagrams

Behavioural and structural diagrams that complement `03-architecture.md`. All are Mermaid.
"Today" means built. "Target" means proposed.

**Update 2026-10-16:** the scan engine is now the Rust `sheshnag-core` library instead of the bash script, and it reproduces the flows below (the dashboard calls the library directly, and the scheduled monitor runs `sheshnag-ui dekho quiet`). The Agni with Kavach sequence, the finding life cycle, the Agni arming states and the core class model are **implemented**. One difference: the circuit breaker does not need a manual reset, it clears itself after an hour.

## 1. Use cases

```mermaid
flowchart LR
    person([Person])
    person --> uc1["Record baseline"]
    person --> uc2["Run vibe check"]
    person --> uc3["Turn auto-watch on or off"]
    person --> uc4["Review findings"]
    person --> uc5["Trust a file"]
    person --> uc6["Quarantine a launch agent"]
    person --> uc7["Switch theme"]
    person --> uc8["Confirm an Agni order (target)"]
    person --> uc9["Restore from the vault (target)"]
```

## 2. Sequence — "Run vibe check" (today)

```mermaid
sequenceDiagram
    actor P as Person
    participant UI as Dashboard
    participant T as Worker thread
    participant S as sheshnag.sh
    participant OS as macOS tools
    participant F as ~/.sheshnag
    P->>UI: click Run vibe check
    UI->>UI: busy = true, buttons disabled
    UI->>T: spawn(run_script_command "dekho")
    T->>S: /bin/bash sheshnag.sh dekho
    S->>OS: ps, plutil, crontab, osascript
    OS-->>S: snapshots
    S->>F: append log, write findings.now
    S-->>T: exit 0 or 1 + text
    T-->>UI: ActionResult via channel
    UI->>F: refresh_data()
    UI-->>P: hero shows all quiet or N sus things
```

## 3. Sequence — scheduled monitoring (today)

```mermaid
sequenceDiagram
    participant L as launchd
    participant S as sheshnag.sh dekho quiet
    participant F as ~/.sheshnag
    participant N as Notification Center
    loop every 600 s
        L->>S: run
        S->>F: copy findings.now to findings.prev
        S->>S: run all checks
        S->>F: write findings.now
        alt new findings exist
            S->>N: one notification, count of new findings
        else none new
            S-->>L: silent
        end
    end
```

## 4. Sequence — Agni order with Kavach (target)

```mermaid
sequenceDiagram
    actor P as Person
    participant UI as Dashboard
    participant K as Kavach
    participant V as Vault
    participant A as Agni
    participant S as Sakshi
    S-->>UI: Finding F (severity critical)
    UI-->>P: show evidence and what will be destroyed
    P->>UI: confirm order for F
    UI->>K: Order(targets)
    K->>K: deny-list check, blast-radius check, integrity check
    alt any check fails
        K-->>UI: Refused(reason)
        UI-->>P: nothing was changed
    else all pass
        K->>V: seal copy and metadata
        K->>A: ClearedOrder
        A->>A: destroy targets, no modes
        A-->>K: Result(destroyed, failed)
        K->>S: re-scan
        S-->>K: findings after
        K-->>UI: Report(verified or re-armed)
        UI-->>P: report with Restore option
    end
```

## 5. State machine — dashboard mood (today)

```mermaid
stateDiagram-v2
    [*] --> Setup
    Setup: No baseline
    Clean: All quiet
    Sus: N findings
    Busy: Working
    Setup --> Busy: Run vibe check
    Setup --> Clean: Lock in baseline and no findings
    Clean --> Busy: Run vibe check
    Sus --> Busy: Run vibe check
    Busy --> Clean: scan finished, 0 findings
    Busy --> Sus: scan finished, findings
    Busy --> Setup: no baseline on disk
```

## 6. State machine — a finding's life (target)

```mermaid
stateDiagram-v2
    [*] --> New
    New --> Seen: next scan still present
    Seen --> Seen: repeat (count + 1, no new notification)
    New --> Trusted: user trusts file or vendor
    Seen --> Trusted: user trusts
    New --> Ordered: user confirms Agni order
    Seen --> Ordered: user confirms
    Ordered --> Refused: Kavach refuses
    Ordered --> Destroyed: Agni executes
    Refused --> Seen
    Destroyed --> Verified: re-scan clean
    Destroyed --> Rearmed: re-scan finds it again
    Rearmed --> New
    New --> Resolved: no longer present
    Seen --> Resolved: no longer present
    Verified --> [*]
    Resolved --> [*]
    Trusted --> Seen: content changes
```

## 7. State machine — Agni arming (target)

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Cleared: Kavach clears an order
    Cleared --> Executing: Agni starts
    Executing --> Done: all targets handled
    Done --> Idle
    Idle --> Locked: circuit breaker trips
    Locked --> Idle: user resets after review
```

## 8. Class diagram — core model (target)

```mermaid
classDiagram
    class Snapshot {
      +persistence: Vec~Entry~
      +processes: Vec~Proc~
      +startup_files: Vec~FileHash~
      +schedule: Vec~Line~
      +login_items: Vec~String~
    }
    class Baseline {
      +snapshot: Snapshot
      +accepted: Vec~FileHash~
    }
    class Detector {
      <<trait>>
      +id() str
      +run(Snapshot, Baseline) Vec~Finding~
    }
    class Finding {
      +id: FindingId
      +kind: Kind
      +severity: Severity
      +confidence: f32
      +subject: Subject
      +evidence: String
      +first_seen
      +last_seen
      +count: u32
    }
    class Platform {
      <<trait>>
      +snapshot() Snapshot
      +notify(String)
      +install_monitor()
      +remove_monitor()
      +stop_process(Pid)
    }
    class Order {
      +targets: Vec~Target~
      +confirmed_by: User
    }
    class ClearedOrder {
      +targets: Vec~Target~
      +vault_ids: Vec~VaultId~
    }
    class Kavach {
      +clear(Order) Result~ClearedOrder~
      +verify(ClearedOrder) Report
      +restore(VaultId)
    }
    class Agni {
      +execute(ClearedOrder) Result
    }
    Detector --> Finding
    Detector ..> Snapshot
    Detector ..> Baseline
    Platform --> Snapshot
    Kavach --> Order
    Kavach --> ClearedOrder
    Agni --> ClearedOrder
```

## 9. Detection flow — one scan (today)

```mermaid
flowchart TD
    start([dekho]) --> hasbase{"baseline exists?"}
    hasbase -- no --> hint["print: record the baseline next"] --> done0([exit 0])
    hasbase -- yes --> a["check_agents: new, changed, gone, Apple label, hidden commands"]
    a --> b["check_processes: miner names, CPU above 300 percent"]
    b --> c["check_rc_and_cron: start-up file notes and pasted commands, crontab diff"]
    c --> d["check_login_items: new items"]
    d --> w["write findings.now with lines starting !!"]
    w --> any{"any findings?"}
    any -- no --> q0([exit 0])
    any -- yes --> n{"quiet mode and new ones?"}
    n -- yes --> notify["one notification"] --> q1([exit 1])
    n -- no --> q1
```

## 10. Data model — state files

```mermaid
erDiagram
    BASELINE ||--o{ AGENT_ENTRY : records
    BASELINE ||--o{ RC_ENTRY : records
    BASELINE ||--o{ LOGIN_ITEM : records
    BASELINE ||--o{ ACCEPTED_FILE : trusts
    SCAN ||--o{ FINDING : produces
    FINDING ||--o| ORDER : "may lead to"
    ORDER ||--o{ VAULT_ENTRY : seals
    AGENT_ENTRY {
        string path
        string hash16
    }
    ACCEPTED_FILE {
        string path
        string hash16
    }
    FINDING {
        string id
        string kind
        string severity
        string evidence
    }
    ORDER {
        string id
        string confirmed_by
        datetime at
    }
    VAULT_ENTRY {
        string id
        string original_path
        string sha256
    }
```

## 11. Dashboard layout (today)

```mermaid
flowchart TB
    h["Header: logo, name, version, theme switch, status"]
    hero["Hero: mood headline, orb, Run vibe check"]
    layers["Three protocol cards: 01 Outer Eyes, 02 Agni, 03 Inner Eyes"]
    setup["Two cards: baseline, auto-watch"]
    findings["Findings list"]
    tools["Two cards: quarantine, trust file"]
    last["Last action terminal"]
    activity["Activity terminal"]
    h --> hero --> layers --> setup --> findings --> tools --> last --> activity
```
