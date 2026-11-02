# berth — handoff

A native monitor for Docker, Podman, Colima and Lima, in Zig on nanoui.
Originally built and verified on Linux; now also built, run and snapshotted on
macOS 26.6 / arm64 with Zig 0.17.0 and Xcode 26.5.

## Building on the Mac

```sh
sh macos/build.sh        # `sh` on purpose: see the note below
open zig-out/berth.app
```

`macos/build.sh` needs **Zig 0.17.0** on PATH (`mise use -g zig@0.17.0`, or
Homebrew, or a tarball) and the **Xcode Command Line Tools**. It builds
ReleaseFast, wraps the binary in a `.app` with an `Info.plist` and the icon,
then ad-hoc signs it so macOS treats it as a normal local app. `zig build run`
opens the window without bundling.

**Note on this share:** copied over SMB, the executable bit does not survive —
`macos/build.sh` arrives as `-rw-rw----`. Run it with `sh macos/build.sh`
(no exec bit needed) or `chmod +x macos/build.sh` first. The same applies to any
binary you copy *off* this share: `chmod +x` it, and if macOS quarantines it,
`xattr -d com.apple.quarantine <file>`.

## What changed on the Mac

- **Host figures.** `src/host/macos.zig` now reads `host_statistics64` and
  `host_cpu_load_info`, which is what `vm_stat` uses. Current macOS does not
  publish `vm.page_active_count` or `kern.cp_time`; the sysctl version of this
  file read zero and the HOST card vanished. The HOST card is also sampled on
  discovery failures now, so it stays visible when the engine is down.
- **Lima discovery.** Every `~/.lima/<instance>/sock/docker.sock` is probed, not
  only `default`, so a VM called `docker` or `berth-docker` is found. Lima's
  `_config` directory is ignored.
- **Design.** The shell uses a warm paper/charcoal palette with an amber
  selection, live figures in the rail, inset cell graphs, compact
  non-answerers in Runtimes, a legend for the disk bars and an app icon. The
  structure follows [nanomon](https://github.com/justrach/nanomon); the colours
  are berth's own.
- **Tables and inspectors.** Headers sort, rows hover and click, and Images,
  Volumes and Networks open an inspector with the matching remove action. The
  column set goes compact while an inspector is open.
- **No-flash, non-blocking refresh.** A pass runs on a worker thread in
  `refresh.zig`, builds a complete `Snapshot` with its own arena, and posts one
  commit callback to the main queue. The current snapshot stays on screen until
  then, so a refresh never clears the tables or stalls the event loop. Stats
  fan out inside that worker via `stats_pool.zig`; logs are owned by the state
  and reload on demand, not on every pass.
- **Clean up.** The Disk page and Settings → Maintenance open a prune sheet for
  stopped containers, unused images/volumes/networks and build cache, with
  reclaimable sizes from `/system/df`. The old bottom action bar is gone.
- **Pluralisation, unknown values.** "1 volume", "1 image", "1 network" now
  read correctly; with no engine the overview shows an em dash rather than a
  measured-looking zero.

## The original Linux note

Cross-compiling the Mac target from Linux still fails at the link step — Zig
does not ship the Apple SDK — so the Mac build has to happen on the Mac. It
takes about two minutes.

## What works

Overview · Containers · Images · Volumes · Networks · Stacks · Disk · Runtimes ·
Settings. Live per-container CPU and memory, history graphs, host memory split
three ways (containers / engine / elsewhere), start / stop / restart / delete,
container detail with the last 200 log lines, light and dark themes.

## What is not built yet

Text search, an Events view (`/events` is unwired), Compose `up`/`down`, a DMG,
code signing and notarization. `docs/UI-REVIEW.md` also lists the design work
that is knowingly unfinished — the list is a table rather than a monitor at
forty containers, actions always visible on every row.

## Screenshots without a display

```sh
BERTH_TAB=containers zig-out/bin/berth --nanoui-snapshot=/tmp/x.png --nanoui-wait=8
BERTH_THEME=dark     … # the snapshot path takes the window theme, so choose it
```

`--nanoui-wait` matters; without it you capture a skeleton.

## Where to look first

| | |
|---|---|
| `README.md` | what it is, how to build, measured sizes |
| `AGENTS.md` | the rules and the traps — read before changing the refresh chain |
| `docs/PHASES.md` | what broke, phase by phase |
| `docs/UI-REVIEW.md` | the design review and what it found |
| `docs/ui/*.png` | every view, rendered from the live engine |

## Also here

- `berth-linux-x86_64.tar.gz` — the Linux release build (1.4 MB binary), if you
  want to compare behaviour side by side.
- `zig-pkg/` — the pinned nanoui v0.9.2 checkout, so the first Mac build does
  not need to fetch it.
