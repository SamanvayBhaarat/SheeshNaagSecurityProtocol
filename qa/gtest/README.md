# Sheshnag acceptance tests (GoogleTest)

Black-box tests that run the real engine (`Src/sheshnag.sh` under `/bin/bash`, or the Rust `sheshnag` CLI) inside a private sandbox.
Nothing on the machine running the tests is read or changed.

## Run

```sh
cmake -S qa/gtest -B build/qa -G Ninja
cmake --build build/qa
ctest --test-dir build/qa --output-on-failure
```

Needs CMake 3.20+, a C++17 compiler, and network access on the first configure (GoogleTest is fetched, pinned by SHA-256).

## Two engines, one suite

By default the suite runs the legacy bash script. Set `SHESHNAG_BIN` to run the Rust CLI instead:

```sh
cargo build -p sheshnag-cli
SHESHNAG_BIN=$PWD/target/debug/sheshnag ctest --test-dir build/qa --output-on-failure
```

Tests that only make sense for one engine call `GTEST_SKIP()` on the other.

## How the sandbox works

| Concern | Handling |
| --- | --- |
| `HOME` | A fresh temp directory per test, deleted in `TearDown` |
| LaunchAgent folders | `SHESHNAG_AGENT_DIRS` points at three folders inside the sandbox |
| `launchctl`, `osascript`, `crontab`, `ps`, `pkill` | Recording stubs placed first on `PATH`; calls are written to a log the test can read |
| Time, network, randomness | Not used |

## Files

| File | Purpose |
| --- | --- |
| `test_support.h/.cc` | The `SheshnagTest` fixture: sandbox, stubs, `Run()`, file helpers |
| `baseline_test.cc` | FR-S-01 |
| `detection_agents_test.cc` | FR-S-02, 03, 04, 09, including the table-driven hidden-command cases |
| `detection_system_test.cc` | FR-S-05 to 08, 10 |
| `actions_test.cc` | FR-S-09, 11, 12 and the watch-only safety test |
| `cli_contract_test.cc` | FR-S-13 |

## Writing a new test

1. Name it `Suite.Behaviour`, so a failure explains itself.
2. Arrange with fixture helpers, call `Run()` once, then assert. Keep logic out of the test.
3. Use `ASSERT_*` for preconditions and `EXPECT_*` for checks.
4. If it exposes a defect, add it as `DISABLED_...` with the defect ID in a comment and list it in `docs/05-action-plan.md`.
5. Cite the requirement ID in the file header comment.

## Skipped tests

The two defect tests (SHE-D-001, SHE-D-002) now run against the Rust engine and skip for the bash script.

See `docs/06-test-plan.md` for strategy, traceability and planned tests.
