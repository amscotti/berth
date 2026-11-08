# berth parity checklist

**Status:** verified against the live Docker 29.9.2 engine on macOS, 2026-11-07.
The Zig app remains the behavior oracle in `../code/berth`.

| Feature | Status | Evidence |
|---|---|---|
| Engine discovery (Docker Desktop, OrbStack, Rancher, dockerd, Colima, Lima, Podman) | ✅ | `discovery` unit tests + live probe found Lima |
| HTTP/1.1 over Unix socket, chunked, total deadline | ✅ | 7 transport tests, including a trickle |
| API version negotiation | ✅ | `client` test adopts `v1.56` |
| Containers list, names, images, state, ports | ✅ | live probe + fixture |
| Container stats: CPU two-sample, working set, net/disk, PIDs | ✅ | stats unit tests + live probe |
| Images, volumes, networks, `/system/df` | ✅ | engine tests + live probe |
| Events interval, merge, dedupe, notices | ✅ | event tests + feed merge |
| Compose grouping, Start all / Stop all | ✅ | projects tests + stacks UI |
| Overview cards, cell graphs, host split, engine facts, disk bar | ✅ | screenshots |
| Disk page, stores, reclaimable, legend | ✅ | screenshots + df tests |
| Runtimes page, custom socket, rescan, machine commands | ✅ | UI + command planning |
| Settings, appearance, engine chooser, maintenance | ✅ | UI + settings round-trip test |
| Containers search/filter/sort, row actions, inspector | ✅ | GPUI tests + screenshots |
| Images search/sort, pull/run card, delete | ✅ | UI + validation tests |
| Volumes/networks search/sort, create, remove | ✅ | UI + create name tests |
| Sortable tables (containers, images, volumes, networks) | ✅ | header sort state + screenshot |
| Inspector Facts / Config / Runtime / Inspect tabs | ✅ | `inspector-*.png` |
| Config: command, workdir, networks, mounts, redacted env | ✅ | `inspector-config.png` |
| Runtime: process list and writable-layer changes | ✅ | `inspector-runtime.png` |
| Image layer history, used-by, clone-to-run | ✅ | `inspector-image.png` |
| Inspector facts for 4 kinds | ✅ | screenshots |
| Logs tail, follow state, local times, search, copy | ✅ | detail job test |
| Raw inspect JSON tab | ✅ | detail job test |
| Start/stop/restart/pause/resume/delete | ✅ | live actions example |
| Bulk select/all/none, start/stop/delete, confirm | ✅ | UI + bulk code |
| Container terminal and open published port | ✅ | command tests; terminal launch manual |
| Prune sheet with Docker/Podman differences | ✅ | prune tests + screenshots |
| Engine machine start/stop/restart | ✅ | command planning |
| Light/dark/system appearance, remembered | ✅ | theme tests + screenshots |
| Settings and socket persistence | ✅ | settings round-trip test |
| Keyboard Cmd/Ctrl+1…9, Cmd/Ctrl+F | ✅ | keymap test |
| Tray title, Open/Quit menu | ✅ | app runs with tray |
| Icons, bundled SVG set | ✅ | screenshots |
| 100-container render | ✅ | `examples/perf.rs`: 13.9 ms/frame |
| macOS unsigned `.app` | ✅ | `macos/build.sh` produced `target/Berth.app` |

## Known gaps (honest)

1. **Tray reopen after the window is closed.** The tray title and menu exist, and
   the app uses `QuitMode::Explicit` on macOS so closing the last window keeps
   the process alive. Reopening a closed GPUI window from the tray is not wired;
   quit and relaunch until that lands. The window's `on_window_should_close`
   path needs a platform hide/show adapter.
2. **Automated real-window E2E.** There is no feature-gated test driver or
   xa11y suite in this checkout. Accessibility roles/labels are present, but
   the screen-reader tree is not asserted. `tests/e2e/README.md` records the
   intended contract.
3. **Virtualized tables.** Tables render visible rows directly. A 100-container
   frame is 13.9 ms, so this is not a problem at the tested scale, but
   `uniform_list` is not used yet.
4. **Linux was not run here.** It compiles through the platform split and has a
   `.desktop` file; CI installs the GPUI deps. X11/Wayland behavior is unverified
   on this machine.
5. **DMG, Developer ID signing, notarization** are deliberately out of scope.
6. **Compose `up`/`down`, streaming logs, embedded terminal, Notification Centre**
   are the same deliberate non-goals as the Zig app.

## Live verification commands

```sh
cargo run -p berth-app --example live_probe   # discovery + containers + stats + logs
cargo run -p berth-app --example live_actions # create/start/restart/pause/stop/delete a disposable container
cargo run -p berth-app --example perf         # 100-container frame budget
cargo run -p berth-app --example screenshots  # full view/theme/size matrix
```
