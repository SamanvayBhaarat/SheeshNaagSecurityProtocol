# Packaging and release

| Platform | Output | Built by | Status |
| --- | --- | --- | --- |
| macOS 11+ (Apple Silicon and Intel) | `Sheshnag-<version>-macos-universal.dmg` | `packaging/macos/build-macos.sh` | Built and verified locally. Ad-hoc signed, not notarized. |
| Linux (Debian/Ubuntu) | `sheshnag_<version>_<arch>.deb`, `.tar.gz` with `install.sh` | `packaging/linux/build-linux.sh` | Script written, not yet run. Builds in CI. |
| Windows 10+ | `Sheshnag-<version>-windows-setup.exe`, portable `.zip` | `packaging/windows/build-windows.ps1` | Script written, not yet run. Builds in CI. |

## Important limits of this release

- **macOS is the only platform verified on a real machine.** Linux and Windows are implemented and compile, and their logic is tested with simulated commands. They have not been run on real Linux or Windows machines yet, so treat those builds as beta.
- **macOS builds are not notarized.** Without a Developer ID certificate, Gatekeeper asks users to right-click the app and choose Open once.
- **Windows:** the app is built as a windowed program, so running it with a command (for example the scheduled check) prints nothing to a console. Use the `sheshnag` command-line binary from `cargo build -p sheshnag-cli` when you want console output.

## Build locally

```sh
packaging/macos/build-macos.sh        # on a Mac; needs rsvg-convert (brew install librsvg)
packaging/linux/build-linux.sh        # on Linux; see the header for apt packages
pwsh packaging/windows/build-windows.ps1   # on Windows; needs Inno Setup
```

Artifacts and SHA-256 checksums are written to `dist/`.

## Release through CI

Push a version tag. `.github/workflows/release.yml` runs the Rust tests on all three platforms and the acceptance suite against the Rust engine on macOS, builds all three platforms on native runners, and attaches the files to a draft GitHub release for review.

```sh
git tag v1.0.0 && git push origin v1.0.0
```

## Notarized macOS build

```sh
CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
NOTARY_PROFILE=my-notary-profile \
packaging/macos/build-macos.sh
```

Create the profile once with `xcrun notarytool store-credentials`.

## Before every release

1. Bump `version` in `Cargo.toml`.
2. Run `cargo test --workspace` and the acceptance suite (`qa/gtest`).
3. Update the changelog and `docs/`.
