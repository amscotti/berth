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

1. `cargo build --release -p berth-app --locked`
2. create `target/Berth.app/Contents/{MacOS,Resources}`
3. copy the binary and icon; stamp `Info.plist` from `BERTH_VERSION` and the
   current commit
4. refuse signing unless `BERTH_SIGN=1`; even then, print the identifier and
   require an explicit `BERTH_SIGN_IDENTITY`
5. print the bundle path

## Linux package

`packaging/linux/berth.desktop` plus a 256×256 PNG icon. The `.desktop` file
must use `Exec=berth`, `TryExec=berth`, `Terminal=false`, and the
`System;Monitor;` categories.

## Tag-driven release

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`. The tag must match
the workspace version in `Cargo.toml`; the workflow fails otherwise.

```sh
git tag v0.1.0
git push origin v0.1.0
```

Artifacts:

- `berth-X.Y.Z-macos-arm64.zip` — unsigned `.app` bundle.
- `berth-X.Y.Z-linux-x86_64.tar.gz` — binary, `.desktop`, 256px icon, LICENSE
  and README.
- `SHA256SUMS`.

The release job is unsigned by design (`BERTH_SIGN=0`); there are no signing
secrets to configure. A future signed release adds Developer ID, hardened
runtime and notarization as a separate opt-in job.

## Release checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] headless screenshot matrix reviewed
- [ ] real-window smoke on macOS and Linux X11
- [ ] live-engine smoke on a disposable engine
- [ ] unsigned bundle opens on a clean macOS account
- [ ] README and `docs/PRODUCT-SPEC.md` parity rows updated
