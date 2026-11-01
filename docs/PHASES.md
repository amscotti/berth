# Phase reviews

Written after each phase, as the brief asks: what was verified, what broke, and
whether anything found would block the next phase.

## P0 — the protocol spike — PASSED

**Goal.** Prove the three unknowns before building on them: an HTTP/1.1 client
over a Unix socket, `fdwatch` on a socket, and spawning a child process.

**Verified, not assumed**
- `zig build test` — 13 tests, 0 failures.
- `zig build` produces a single binary; `zig build lines` reports 12 files, all
  within the 600-line rule.
- The app reads a **live Docker 29.1.3 engine** at `/var/run/docker.sock` and
  renders real numbers: 25 images, 4 CPUs, 9.6 GiB, `overlayfs`,
  `Ubuntu 24.04.5 LTS · linux/x86_64 · cgroups 2`.
- `--nanoui-snapshot=… --nanoui-wait=2` renders a 1180×760 @2x PNG with no
  window, so every screen is reviewable in CI.

**What broke, and what it taught**

| Issue | Cause | Fix |
|---|---|---|
| Every formatted number rendered as garbage (`aaa`) | text formatted into stack buffers that died before the frame was painted | all formatting goes through `Context.print`, which allocates in the frame arena |
| Nav rail laid out horizontally, rows collapsed | `defer box.end()` closes at scope exit, so later siblings became *children* | close boxes explicitly where siblings follow |
| `/info` returned `{"message":"page not found"}` | two bugs stacked: the API version was written through a pointer to a temporary, and `v` was dropped when adopting `1.52` | pointer into the state; adopt as `v{s}` |
| Nothing answered at all | this user is not in the `docker` group | run the harness under `sudo` for now; the app itself needs the group |
| `std.posix.write`/`getenv`/`getuid`/`time.timestamp` missing | Zig 0.17 moved them behind `std.Io` and `std.process` | `Io.File.writeStreamingAll`, `Io.net.UnixAddress`, libc externs for `getenv`/`getuid`/`time` |

**Gate:** the UDS client is ~250 lines plus tests — well under the 450-line
budget — so the helper-process fallback is not needed. **No blockers.**

## P1 — read-only monitoring — IN PROGRESS

**Working, verified against the live engine:** Overview (engine card, container
and image counts, disk breakdown), Images (repository, short ID, size, age,
ready/dangling), plus Containers, Volumes, Networks, Stacks, Disk, Runtimes and
Settings rendering from the same snapshot.

**Container stats: done and verified.** CPU comes from `stream=false *without*
`one-shot`, which waits two collection cycles precisely so that `precpu_stats`
is populated; memory is the working set (`usage − inactive_file`, with
`total_inactive_file` and `cache` as fallbacks). Against a container spinning a
busy loop, berth read **102%** while `docker stats` read **103%** — different
sampling instants, same number. A stopped container shows `—`, never `0%`,
because unknown and idle are not the same thing.

**Known gaps, in priority order**
1. **Container detail** — logs, inspect, mounts, ports.
3. **Search and filtering** — the toolbar exists but has no field yet.
4. **Events** — the `/events` stream is not read yet; the Events view does not exist.
5. **Host and VM attribution** — no `/proc` or `libproc` sampler yet, so the
   Overview shows engine totals rather than host-versus-VM layers.
6. **Actions** — start/stop/delete are wired to the API but nothing has been run
   against a live container yet (this engine has none), so they are unverified.

**Judgement:** none of these block the next phase; they are features, not
foundations. The foundation — engine discovery, protocol, typed API, shell,
formatter, view dispatch — is real and exercised.

**Verified against a live container** (`debian:bookworm-slim`, started for the
test and removed afterwards): the Containers view lists it with name, image,
state pill, project, ports and reachable action buttons; Images lists six images
with tags, short IDs, sizes, ages and dangling/ready state.

**What broke in P1, and the lesson**

| Issue | Cause | Fix |
|---|---|---|
| A container row rendered *vertically*, with the name and image missing | `const name_cell = ui.div(...); defer name_cell.end();` — the defer fires at the end of the loop body, so every following sibling became a **child of the name column** | one cell per line, no deferred box that siblings fall into |
| Cells sat centred instead of left-aligned | `itemsCenter()` on a **column** centres horizontally; vertical centring is `justifyCenter()` | `column().justifyCenter().itemsStart()` for cells, `itemsEnd()` for numbers |
| Two edits appeared to apply but changed nothing | a scripted string replace that did not match, reported as success | use the `patch` tool, which fails loudly, and re-read the region after editing |
| Columns drifted left of their headers, growing with each column | the status pill is as wide as its word, so an intrinsic-width child shifts every sibling after it | `pillCell` gives it a fixed slot; a header/row contract only holds if every cell honours its declared width |

That last one is the most expensive lesson of the phase: a silent no-op replace
sent me chasing a layout bug that had already been fixed on paper. Verify the
diff, not the intent.

## Appearance — light, dark, and the override

**Done and verified three ways**, each with a rendered snapshot:

| Case | Result |
|---|---|
| No settings file, `GTK_THEME=Adwaita:dark` | opens **dark** (sampled `#2c2c2e`) — system detection works |
| No settings file, no dark GTK theme | opens **light** — the fallback when nothing can be detected |
| `settings.json` with `"appearance":"dark"` | opens **dark**, overriding the desktop |

- `Settings → Appearance` offers **System / Light / Dark**, with the current one highlighted; the choice is written to `<data dir>/berth/settings.json` and read at startup.
- The theme is a **window-creation** option, so a change reopens the window with the new theme after the current frame (never during it, which would close the window being painted). State survives; only scroll position is lost.
- macOS passes no theme in System mode, so the framework follows the desktop **live**; Linux has no appearance notification here, so `GTK_THEME` is read once at window creation. This asymmetry is deliberate and documented in `themeFor`.

## P2 — actions — VERIFIED

Start, stop and delete were taken on faith until now. They are proven against a
live container, through the app's own code path (the same function the buttons
call), with the engine's own state checked before and after:

| Verb | before | after |
|---|---|---|
| `stop` | `running` | `exited` |
| `start` | `exited` | `running` |
| `delete` | present | gone |

The UI agrees: a stopped container reads `exited`, its CPU and memory show `—`
rather than `0`, and its row offers **Start** instead of **Stop**. A toast names
what happened.

The confirmation dialog the buttons raise is now actually rendered — it existed
as a function that nothing called, which would have made Delete silent.

**A re-entrancy bug this surfaced.** Running the action from inside the
containers step meant `act()` started a refresh, and then the step being
interrupted retired it — the list came back empty and the row vanished. Actions
now run at the *end* of the refresh chain, where a new refresh cannot be
clobbered. The lesson generalises: **a callback that starts work must not run
inside another callback that is still writing state.**

## Overview graphs — following nanomon

nanomon (same framework, same author) draws its charts as **cell graphs**: a
grid of 4–5pt dots where each column is a slice of time and each dot is one
step of height, placed with `c.leaf(.{ .absolute = true, … })` rather than one
nested box per bar. That is the technique adopted here, with berth's own theme
tokens instead of nanomon's palette, so the graphs follow light and dark.

- **History**: `ui/history.zig`, 120 samples of container CPU, memory, running
  count and engine disk. A shift on overflow — 480 bytes moved every few
  seconds costs nothing next to ring-index arithmetic on every read.
- **Sampling**: one point per completed refresh, and a 5-second `App.after`
  timer re-arms itself from the end of each refresh. Nothing polls in the
  frame loop; the timer only asks for new data.
- **Scales are honest, not auto**: CPU is a share of the engine's cores,
  memory is against `MemTotal` (so 900 KiB of 9.6 GiB reads as a thin line, not
  a full bar), counts scale to twice the peak, disk to 1.6×. Auto-scaling a
  flat series makes it look saturated, which is a lie of the most convincing
  kind.

**The bug this surfaced** was older than the feature: `resetData()` freed the
request arena on every refresh while the frame still held slices into it
(container names, logs, the selection). It had been latent — the graphs simply
made refreshes frequent enough to hit it, and it arrived as a segfault in
`memcpy` inside a text write. Fixed with **arena rotation**: the previous arena
is kept for one cycle and freed at the next refresh, and the selection is
re-resolved into the new arena by id.

## Cross-platform notes for the Mac

- Every OS difference is behind `builtin.os.tag` or a constant; nothing Linux-only
  is baked into a code path. No `tray`, no `GTK` calls of our own.
- The only libc externs are `getenv`, `getuid`, `time` — all present on Darwin.
- macOS-specific work still to do: VM attribution (`proc_pid_rusage`,
  `phys_footprint`), Docker Desktop's process subtree, and Metal rendering
  (handled by the framework, untested here).
- `std.Io.net.UnixAddress` is the same API on both platforms, so the client
  needs no porting.

## Next
P1 completion (stats, detail, search) → P2 (actions, verified against real
containers) → P3 (VM context, multi-runtime switching) → P4 (logs, packaging).
