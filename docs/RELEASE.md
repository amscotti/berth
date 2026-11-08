# Release policy

**Status:** adopted 2026-11-07

berth ships **unsigned** local builds by default.

- `BERTH_SIGN=0` is the default in every script and CI job.
- Do not run `codesign`, `notarytool`, or any keychain-dependent step unless a
  release explicitly opts in. An agent blocking on a password prompt is a bug.
- Git commit signing is disabled locally (`commit.gpgsign=false`). Do not
  re-enable it locally or in agent instructions.
- macOS `.app` bundles are ad-hoc or unsigned local artifacts. Gatekeeper will
  warn on a downloaded copy; that is expected until a signed release exists.
- Linux ships the binary plus a `.desktop` file; no notarization analogue.

## macOS bundle

`macos/build.sh`:

1. `cargo build --release -p berth-app`
2. create `target/berth.app/Contents/{MacOS,Resources}`
3. copy the binary, `Info.plist`, `AppIcon.icns`
4. refuse signing unless `BERTH_SIGN=1`; even then, print the identifier and
   require an explicit `BERTH_SIGN_IDENTITY`
5. print the bundle path

## Linux package

`packaging/linux/berth.desktop` plus a 256×256 PNG icon. The `.desktop` file
must use `Exec=berth`, `TryExec=berth`, `Terminal=false`, and the
`System;Monitor;` categories.

## Release checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] headless screenshot matrix reviewed
- [ ] real-window smoke on macOS and Linux X11
- [ ] live-engine smoke on a disposable engine
- [ ] unsigned bundle opens on a clean macOS account
- [ ] README and `docs/PRODUCT-SPEC.md` parity rows updated
