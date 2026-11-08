# berth

A native monitor for container runtimes — Docker, Podman, Colima and Lima — on
macOS and Linux. One window, no webview, no daemon of its own, no telemetry.

berth reads the engine's HTTP API over its Unix socket and answers three
questions without a terminal:

- **What is running?** Containers with CPU, memory, ports, state and Compose
  project, plus the last 500 log lines for any one of them.
- **What does it cost?** Container CPU and memory over time, the host's memory
  split into containers, engine and everything else, and the engine's image,
  container, volume and build-cache stores.
- **What can I clean up?** Stopped containers, unused images and volumes,
  unused networks and build cache, with the reclaimable total shown before
  anything is removed.

## Status

This repository is the **Rust + GPUI rewrite** of the unreleased Zig/nanoui
app. Ten views, live engine reads, actions, creation, cleanup, events, stacks,
bulk selection, an inspector with logs/inspect, a tray title and light/dark
appearance are implemented and tested. The live path was verified against
Docker 29.9.2 on macOS.

```sh
cargo run -p berth-app
cargo run -p berth-app --example live_probe    # discovery + read a real engine
cargo run -p berth-app --example live_actions  # disposable action smoke
cargo run -p berth-app --example perf          # 100-container frame budget
cargo run -p berth-app --example screenshots   # full visual matrix
```

`docs/PARITY-CHECKLIST.md` is the honest status, including the remaining gaps
(tray reopen after window close, automated xa11y/driver E2E, Linux runtime
verification). The old Zig app remains the reference implementation and visual
oracle (`docs/reference-ui/`).

## What works

Overview · Containers · Images · Volumes · Networks · Events · Stacks · Disk ·
Runtimes · Settings. Live per-container CPU and memory, history graphs, host
memory split three ways, sortable tables, an inspector with Facts / Config /
Runtime / Inspect tabs, image layer history, logs, terminal, clone-to-run,
start / stop / restart / pause / resume / delete, bulk actions, Compose
start/stop, image pull and run, volume and network creation, prune, engine
machine start/stop/restart, a macOS tray title, and light/dark themes.

## Building

Needs Rust 1.98 (the pinned toolchain) and, on macOS, the Xcode Command Line
Tools.

```sh
cargo run -p berth-app        # build and open the window
cargo test --workspace        # no engine, no display
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Linux needs the GPUI runtime dependencies: `libfontconfig`, `libfreetype`,
`libwayland`, `libxkbcommon`, `libx11-xcb`, `libssl`, `libzstd`, `libasound`,
and a Vulkan-capable driver.

## Documentation

| | |
|---|---|
| `docs/GPUI-REBUILD.md` | plan, decisions, milestones |
| `docs/PRODUCT-SPEC.md` | behavior, visual contract, endpoints |
| `docs/GPUI-VERSION.md` | pinned GPUI commit and upgrade policy |
| `docs/TESTING.md` | test layers, driver, screenshots |
| `docs/PARITY-BACKLOG.md` | feature matrix and next work |
| `docs/PARITY-CHECKLIST.md` | verified parity and known gaps |
| `docs/ACCESSIBILITY.md` | roles, labels and keyboard paths |
| `docs/RELEASE.md` | unsigned build policy |
| `AGENTS.md` | rules for working in this repo |

## License

MIT
