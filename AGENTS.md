# berth — working notes for agents and people

A native monitor for container runtimes (Docker, Podman, Colima, Lima) in Zig
0.17 with nanoui. macOS and Linux. Read `docs/PHASES.md` for where the work
stands.

## Hard rules

- **600 lines per file.** `zig build lines` enforces it.
- **No C shims.** `extern` declarations and libc calls only; no `@cImport`.
- **Zig 0.17 APIs.** `std.Io` for I/O, `std.process.spawn`, `Context.print` for
  frame-allocated strings, `std.Io.net.UnixAddress` for sockets.
- **Nothing polls.** Data arrives from the engine; the window redraws when a
  snapshot changes.
- **UI on the main thread.** `App.after(0, …)` for the refresh chain,
  `App.onMain` for anything posted from elsewhere.
- **No phase names in code.** Comments cite behaviour, not plan phases.

## Shape

```
src/main.zig        app, window, BERTH_TAB
src/app.zig         State, refresh machine, actions
src/engine/         http_uds (protocol), api (types), discovery (sockets)
src/ui/shell.zig    rail, status bar, page dispatch
src/ui/widgets.zig  row anatomy, cells, pills, empty states
src/ui/format.zig   the only number/time formatter
src/ui/views/       lists (4 views), panels (4 views), stacks
```

## Checking work

```sh
zig build test                      # protocol, JSON, formatting
zig build lines                     # the 600-line rule
sudo ./zig-out/bin/berth --nanoui-snapshot=/tmp/s.png --nanoui-wait=2
BERTH_TAB=images sudo -E ./zig-out/bin/berth --nanoui-snapshot=/tmp/i.png --nanoui-wait=2
```

`sudo` is only needed because the socket belongs to the `docker` group on this
machine; the app needs no privileges of its own.
