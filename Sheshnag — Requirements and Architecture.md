# Sheshnag (शेषनाग)

Cross-platform security watcher app for Palanhar OS, v1.0 — built in Rust with a VS Code-built GUI, running the three-layer Sheshnag protocol.

## Requirements — the three-layer protocol

### Layer 1 — Sakshi (साक्षी), the witness

- Detect, protect, warn.
- Only watches and speaks; takes no destructive action itself.
- The warning is addressed to both the attacker and the person — crossing the limit brings consequence in the next stage.

### Layer 2 — Agni (अग्नि), the fire

- Triggers once called; does exactly one thing: destroy.
- No nuance, no second-guessing, no other mode.
- Deliberately "ugly" by design — once the limit is crossed, nothing else happens except destruction.

### Layer 3 — Kavach (कवच), the shield

- Primary job is not cleanup but protection — of itself, the machine, and the person — from the destruction Agni causes.
- Reasoning: when Agni's fire hits an attacker, the attacker could redirect or reflect that destructive force back onto the person's own system ("send that fire back to your home").
- Exists as a firewall against the person's own weapon turning back on them.
- Cleanup and restore come second to this protective role.

## Architecture — current state (v1.0)

| Area | Detail |
| --- | --- |
| Platforms | Mac, Linux, Windows |
| Core | Rust |
| GUI | Built in VS Code; labeled "Sesh Nag protocol" with three panels — Outer Eyes / Agni / Inner Eyes |
| Built today | Layer 1 (Sakshi) only |
| Agni, Kavach | Design intent only, not yet built |

**What is live today (Layer 1 / Sakshi):**

- Detection
- Baseline recording — a trusted snapshot of system state to compare against
- Scheduled monitoring — background check every 10 minutes via a LaunchAgent
- On-demand checks ("vibe check" / manual scan)
- Activity log of findings (`~/.sheshnag.log`)
- Manual tools: quarantine a launch agent (unloads it, moves its plist to quarantine); mark a file as trusted ("chill") to stop re-flagging it

## History

- An earlier bash-script version (`sheshnag.sh`, with baseline/dekho/install/jaago/theek commands) came first, at the same Layer-1-only stage, running as a launchd agent.
- The current Rust + GUI version (v1.0) replaced it, still at Layer 1 only.

## Open / deferred

- The exact detection-to-destruction-to-protection logic for Agni and Kavach is undesigned — parked for a future session.
- A possible patent filing on the three-layer protocol itself (separate from the app) has been raised but not pursued.

## Real-world confirmation

Running live on a real Mac. Baseline: 33 launch agents recorded. Caught a real event — 5 Adobe-related launch agent changes (agsservice, acc.installer.v2, ccxprocess, AdobeDesktopService, AdobeCreativeCloud) — detected and reported correctly, nothing altered automatically, matching Layer 1's watch-only design.
