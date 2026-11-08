# berth product specification

The behavior and visual contract carried over from the unreleased Zig/nanoui app.
This is the "what must survive" document; the runtime plan is `docs/GPUI-REBUILD.md`.

Reference screenshots: `docs/reference-ui/`.

### 2.1 Views

| View | What it does today | Port priority |
|---|---|---|
| **Overview** | Four live graph cards (container CPU, memory, running count, disk), HOST card with three-way memory split (containers / engine helpers / elsewhere) + CPU%, engine facts, disk breakdown with legend | P0 |
| **Containers** | 44 pt rows, All/Running/Stopped chips, search, sortable columns, per-row Start/Stop/Restart/Delete, checkbox bulk mode, inspector | P0 |
| **Images** | Search/sort table, dangling/ready pills, collapsed "Get and run images" creation card (pull; run with image/name/ports/volumes/env), inspector | P0 |
| **Volumes** | Search/sort table with size/in-use from `/system/df`, New volume sheet, inspector/remove | P0 |
| **Networks** | Search/sort table with attached counts, New network sheet, inspector/remove (built-ins protected) | P0 |
| **Events** | 34 pt rows, newest first, search, exit/OOM/restart toasts | P1 |
| **Stacks** | Compose projects grouped by labels, members, Start all/Stop all via bulk worker | P1 |
| **Disk** | Store figures, proportional four-store bar + legend, purpose note, Clean up | P0 |
| **Runtimes** | Custom socket field (connect/forget), socket cards grouped by engine kind, expandable paths, start/stop/restart for Lima/Colima/Podman machine | P0 |
| **Settings** | About, Appearance (System/Light/Dark), Engine card + chooser, Maintenance (prune/refresh) | P0 |

### 2.2 Behavioural contract

These are not implementation details; they are what the app promises.

**Refresh**

- One pass every 5 seconds, re-armed **from the end** of the previous pass.
- The pass runs off the UI thread and builds a complete immutable snapshot.
  The last complete snapshot stays on screen until the new one commits. Tables
  never blink and the event loop never waits.
- A failed core call (containers/images/volumes/networks) aborts the pass and
  keeps the previous snapshot. A failed `/system/df` keeps the last good df with
  its age/error. A failed stats sample marks the pass partial; a failed
  `/events` keeps the old ring. A ping failure drops the client so the next tick
  rescans engines.
- `/system/df` runs on its own ~30 s cadence (six 5 s passes) and owns its own
  cache, outside snapshot rotation.
- Stats fan out across all running containers, capped at 64 concurrent
  requests. `/stats?stream=false` waits two collection cycles (~2 s); it is
  never used with `one-shot`.
- Selection is re-resolved by id after every snapshot swap.
- A click during a pass requests the next pass immediately.

**Data honesty**

- Unknown is not zero. A stopped or unsampled container shows `—`.
- CPU is the two-sample delta (same window as `docker stats`); memory is the
  working set (`usage − inactive_file`, then `total_inactive_file`, then `cache`).
- Container memory totals render "partial sample" unless every up container
  answered.
- Graphs use fixed, honest ceilings (CPU 100%, memory vs engine `MemTotal`,
  running vs fleet size, disk rounded 1/2/5 ceiling). Auto-scaling is a bug.
- Host memory is three segments, never double-counting containerd shims or
  guest RAM.
- List answers are capped at 5,000 entries per store; JSON deeper than 64 levels
  is refused with an availability-limit message, not shown as an empty store.
- Port bindings are deduplicated on public/private pair.
- API vocabulary is translated: `<none>:<none>` → "untagged", `null` driver →
  "—".

**Discovery**

- Probe candidates in parallel, deduplicate, prefer the remembered socket and
  `DOCKER_HOST`.
- Candidates: Docker Desktop (several layouts), OrbStack, Rancher Desktop,
  dockerd, Colima, every Lima instance, Podman 5 runtime sockets, Podman
  machines under XDG data, rootless Podman, then `/var/run/docker.sock`,
  `/run/podman/podman.sock`, `/var/run/podman/podman.sock`.
- Probe only paths that exist; negotiate the API version from `/version`.
- Docker identity = daemon id; Podman compat `/info` returns a fresh id per
  request, so Podman identity falls back to the socket path. A Docker→Podman
  dialect change is always an engine change.
- Failed scan retries in 2 s; a rescan requested during a scan runs after it.
- Runtimes and the previous client stay usable while a rescan is in flight.

**Actions**

- One engine mutation at a time (container action, bulk batch, pull/run, prune,
  engine machine command). A second request is refused with a toast, not queued.
- Every mutation completion refreshes and resumes any queued switch/rescan/pass.
- Destructive actions confirm first, naming the target and consequences.
- Container: start, stop, restart, pause, resume, delete.
- Bulk: select shown/all/none, start/stop/delete selected; Compose Start all /
  Stop all. Batched, cancellable, reports partial results.
- Image: pull a reference (bare name means `:latest`), run with name, ports,
  volumes, env; delete with force.
- Volume/network: create after local name validation; remove (built-in networks
  cannot be removed).
- Prune: stopped containers, unused images, unused volumes, unused networks,
  build cache; Podman has no build-cache route and its image/volume prune
  filters differ. Show reclaimable sizes before the first destructive click.
- Engine machines: start/stop/restart Colima, Lima, Podman machine. Lima/Podman
  restart is stop-then-start. Stopping/restarting confirms how many running
  containers will go down.
- Terminal: open the user's terminal with a socket-pinned `docker -H … exec -it
  <name> sh` / `podman --url …`. Quote both socket and name; bounded AppleScript
  wait on macOS.
- Published ports: open `http://host:port`, `https` for 443/8443; validate the
  engine-supplied host before building a URL.
- Copy run command with best-effort redaction of credential-like env values and
  URL userinfo.
- Logs: last 500 lines, follow-to-end, local timestamps, search, copy. Follow
  polls on the detail worker; true streaming is out of scope.

**Persistence and chrome**

- `settings.json` in the platform data dir (`~/Library/Application Support/berth`
  on macOS, `~/.local/share/berth` on Linux): `appearance` (`system|light|dark`)
  and last `socket`.
- Window default 1280×760, minimum 960×600; appearance switch keeps the size.
- macOS: closing the window leaves the tray monitor running; Open restores the
  window at the last size. No tray: last window quits.
- Keyboard: Cmd/Ctrl+1…9 switch the first nine views; Cmd/Ctrl+F focuses the
  current table's search; Esc closes a sheet or inspector; Enter submits.
- Toasts appear in the rail for ~4 s; errors are text, not colour alone.

### 2.3 Deliberate non-goals (carry over from the Zig app)

- Compose `up`/`down` through the Compose CLI.
- True streaming `docker logs --follow`.
- Embedded terminal output (open the user's terminal instead).
- macOS Notification Centre (in-app toasts only).
- DMG, Developer ID signing, notarization, Windows support.

---

## 3. Visual contract

The look is warm paper/charcoal with burnt amber; it follows nanomon's
structure but uses berth's own palette. The port must preserve the tokens, not
"something similar".

### 3.1 Palette

| Token | Light | Dark | Use |
|---|---|---|---|
| `background` | `#f7f3ea` | `#17140f` | page, recessed wells |
| `surface` | `#fffcf5` | `#211d16` | rail, cards, inspector |
| `surface_hover` | `#efe8da` | `#2d2820` | hover, secondary buttons, alt rows |
| `text` | `#1f1b14` | `#ece6d9` | primary text |
| `muted` | `#6f6759` | `#9b9282` | secondary text, labels |
| `border` | `#e1d8c7` | `#3a3327` | hairlines, card edges |
| `accent` | `#a35d14` | `#e3a44b` | selection, primary buttons |
| `accent_hover` | `#955716` | `#f2b664` | primary hover |
| `accent_text` | `#fffaf0` | `#17140f` | text on accent |
| `track` | `#e7dfd0` | `#332d23` | bar tracks, inactive checkboxes |
| `knob` | `#fffcf5` | `#ece6d9` | switches |
| `danger` | `#b3472d` | `#d06a4f` | destructive fill |
| `success` | `#5d7f45` | `#83a76d` | success fill |
| `shadow` | 12% | 40% | layers only; cards never float |

Derived, text-safe variants (must be implemented as functions, not reused
fills):

| Token | Light | Dark |
|---|---|---|
| `accent_fill` (bars/graphs) | `#b26a1e` | `#e3a44b` |
| `warning` (text) | `#7a4700` | `#ff9f0a` |
| `success_text` | `#3f5c2e` | `#83a76d` |
| `danger_text` | `#8f2f1c` | `#d4745a` |
| `accent_text_pill` | `#7d430f` | `#e3a44b` |
| `muted_text_pill` | `#5a5245` | `#9b9282` |
| `running_text` | `#7a4700` | `#d9a441` |
| `warning_fill` | `#ff9f0a` | `#ff9f0a` |
| `store_volume` | `#ff9f0a` | `#ff9f0a` |

Per-page tint (icon chip / rail glyph):

| Page | Light | Dark |
|---|---|---|
| Overview, Containers | accent | accent |
| Images, Volumes | `#9a6e1b` | `#d9a441` |
| Networks, Events | `#2f8f9d` | `#6fb0a8` |
| Stacks | `#7a4ea3` | `#bd80e4` |
| Disk, Runtimes | `#3f6079` | `#8a9bb0` |
| Settings | muted | muted |

Contrast rules from the Zig tests must be ported: on-accent text ≥ 4.5:1;
warning/success text ≥ 4.5:1 against a 14% wash of itself; danger text ≥ 4.7:1
on a 12% wash; every status pill on every row background.

### 3.2 Typography

- System UI font on macOS (`NSFont` today). Use GPUI's system font; do not
  invent a brand font. Bundling Inter is optional if Linux needs consistency.
- Tabular numerals (`font_features: "tnum"` or `FontFeatures`) on all figures,
  metrics, clocks, byte counts, PIDs, ports, table numbers.
- Monospace for IDs, socket paths, raw JSON and logs (`Menlo`/`SF Mono` with a
  generic fallback).
- Scale: page title 22 semibold; page subtitle 12.5 muted; card title 15
  semibold; card sub-label 12.5 semibold; metric 26 bold; section label 10.5
  semibold muted 75%; rail 13; table header 11 medium; table body 13; pills 11
  medium; small buttons 11.5; inspector label 11 muted; inspector value 11.5;
  logs 11.5 mono leading 1.4; "recent" 9.5.
- Truncation: end ellipsis by default, middle for paths/IDs. Numbers right
  aligned, text left aligned.

### 3.3 Layout

| | |
|---|---|
| Window | 1280×760 default, min 960×600 |
| Rail | 200 wide; 88 below 1120 (icons only); 14 vertical padding, 10 horizontal; 1 px right border |
| Rail item | 32 high (36 narrow), radius 7, gap 8; selected = accent fill + accent_text; icon chip 20 (24 narrow) at 14% tint |
| Main column | padding 20 left/right, 16 top, 12 bottom, gap 4 |
| Inspector | 430 at ≥1180, else 360; padding 14, gap 8 |
| Content width | `window − rail − 40 − inspector`, min 280 |
| Cards | radius 10, padding 14–16, 1 px border, surface; no shadow |
| Tables | container/image/volume/network rows 44; events rows 34; row padding 12; gap 10; radius 4; header 18 high with 1 px rule |
| Graph well | 70 high (overview), radius 8, background page colour, 1 px border, 8 inset; dots 4 pt on a 6 pt pitch |
| Sheets | max 620 wide, top ≈ centred minus 4%, backdrop 38% light / 58% dark |
| Empty/loading | skeleton rows for first read; empty state names the state and the next action |

Table density by content width: full ≥1040, medium ≥832, dense ≥656, compact
≥510, tiny below. Column sets and widths are in `src/ui/views/columns.zig` and
must be copied literally (including the 110 pt name-column growth cap).

Bulk mode borrows 34 pt from the name column for a checkbox; it does not widen
the row.

### 3.4 Components and interaction

- **Rail**: icon + label + live figure (running/total, counts, store bytes,
  answering engines). Group headings "Resources" and "Engine"; hairline
  dividers when narrow. Bottom status: dot + "Refresh · every 5 s", engine name,
  `updated HH:MM:SS`; errors replace the label with a warning and keep the
  clock. Whole status has a tooltip, is static text, not a button.
- **Page header**: 22 pt title, muted count/subtitle, search at right (190 pt,
  12.5 text), optional action pill (New volume/network) before search, clear
  button appears only when text exists.
- **Table**: sortable headers (accent + ▲/▼, 8 pt arrow), hover surface,
  selected accent 10% wash, alternating rows at 35% surface_hover, actions
  pinned right (Stop/Restart/Delete or Start), 24 pt pills.
- **Status pill**: word first, colour second; 20 pt high, 8 px horizontal,
  radius 10, 14% background wash; health overrides state when present.
- **Cards**: tinted chip, coloured label, one large tabular figure, a context
  note ("of 4 cores", "partial sample", "reclaimable 12.8 MiB"), graph in a
  recessed well.
- **Buttons**: primary = accent fill + accent_text; secondary = surface_hover;
  destructive = danger (or danger text + 14% wash for inspector actions);
  dialog buttons share 92×28 and one baseline.
- **Inspector**: title + close, Facts/Inspect pills, rule, content. Facts use a
  fixed 64 pt label column. Container: image/state/status/health/project/
  service/started/CPU/memory/network/disk/PIDs/restarts/ports; Pause/Resume,
  Terminal, Copy run command; logs header + Follow/Times/Copy/Search; log well.
  Image: repository/ID/size/created/status + Run…/Delete. Volume: name/driver/
  scope/mountpoint/size/in use + Remove. Network: name/ID/driver/scope/
  attached + Remove (not built-ins). Inspect: monospace JSON, Copy/Refresh.
- **Sheets**: one modal slot only. Confirm stop/delete, engine stop/restart,
  prune options, create volume/network, bulk confirm. A sheet owns the keyboard;
  background focus is cleared; backdrop consumes clicks and wheel.
- **States**: first read = skeleton; failed first pass = "Engine not answering"
  + help; later failure = keep stale data + rail warning; search no-match =
  "No results for “q”" + Clear search; empty engine = specific next action.
- **Motion**: none required. Use GPUI hover/pressed states only.

---


## 9. Mapping appendix

### 9.1 Zig module → Rust module

| Zig | Rust |
|---|---|
| `main.zig`, `boot.zig` | `main.rs`, `app.rs` |
| `app.zig` (`State`) | `app.rs` (`BerthApp` + `ViewState`) |
| `session.zig`, `refresh.zig` | `jobs/discovery.rs`, `jobs/refresh.rs` |
| `snapshot.zig` | `state/snapshot.rs` (owned structs, `Arc`) |
| `stats_pool.zig` | `engine/stats.rs` + refresh fan-out |
| `events_feed.zig`, `engine/events.zig` | `engine/events.rs` |
| `engine/df_cache.zig`, `engine/df.zig` | `engine/df.rs` |
| `actions.zig`, `bulk.zig`, `bulk_action.zig`, `bulk_batch.zig`, `cleanup.zig` | `jobs/mutation.rs` |
| `image_tasks.zig`, `image_refs.zig`, `image_validation.zig` | `jobs/image_tasks.rs`, `domain/image_ref.rs`, `domain/validation.rs` |
| `create.zig`, `engine/create.zig` | `ui/sheets.rs`, `engine/client.rs` |
| `detail_job.zig`, `logs.zig`, `engine/inspect.zig`, `engine/lifetime.zig` | `jobs/detail.rs`, `engine/logs.rs`, `engine/inspect.rs`, `domain/lifetime.rs` |
| `terminal.zig` | `platform/terminal.rs` (or `jobs/terminal.rs`) |
| `ops.zig`, `engine/command.zig` | `jobs/ops.rs` |
| `tray.zig` | `tray.rs` |
| `tabs.zig`, `settings.zig`, `sheets.zig`, `shortcuts.zig` | `state/view.rs`, `settings.rs`, `ui/sheets.rs`, `shortcuts.rs` |
| `engine/http_uds.zig` | `engine/transport.rs` |
| `engine/api.zig`, `engine/stats.zig`, `engine/volumes.zig` | `engine/dto.rs`, `engine/stats.rs` |
| `engine/discovery.zig`, `engine/socket_path.zig`, `engine/podman_config.zig` | `engine/discovery.rs` |
| `engine/prune.zig` | `engine/prune.rs` |
| `engine/guards.zig` | `engine/guards.rs` |
| `host/*` | `host/*` |
| `ui/shell.zig`, `ui/theme.zig`, `ui/widgets.zig`, `ui/layout.zig` | `ui/shell.rs`, `theme.rs`, `ui/widgets.rs`, layout constants |
| `ui/views/*` | `ui/views/*` |
| `ui/graph.zig`, `ui/history.zig` | `ui/graph.rs`, `domain/history.rs` |
| `ui/detail.zig` | `ui/detail.rs` |
| `ui/format.zig` | `domain/format.rs` |

### 9.2 Engine endpoints used today

```
GET  /version
GET  /info
GET  /containers/json?all=1
GET  /containers/{id}/stats?stream=false
GET  /containers/{id}/logs?stdout=1&stderr=1&tail=500[&timestamps=1]
GET  /containers/{id}/json
POST /containers/{id}/start|stop|restart|pause|unpause
DELETE /containers/{id}?force=1
GET  /images/json
POST /images/create?fromImage=…&tag=…
DELETE /images/{id}?force=1
GET  /volumes
POST /volumes/create
DELETE /volumes/{name}
POST /volumes/prune[?filters=…]
GET  /networks
POST /networks/create
DELETE /networks/{id}
POST /networks/prune
GET  /events?since=…&until=…
GET  /system/df
POST /containers/prune
POST /images/prune?filters=…
POST /build/prune            # Docker only
```

### 9.3 String and state inventory to lift verbatim

- Empty states: "No volumes — create one above, or run a container that
  declares one"; "None match this filter — All shows every container"; "No
  output yet. A container that has said nothing says nothing."
- First read: "Reading engine…", "no figures yet", "partial sample", "Engine
  not answering", "no engine found", "probing known sockets…".
- Disk: "What these numbers are", "Prune frees space inside the engine's VM…".
- Prune: "Stopped containers, unused images and build cache. Volumes and
  networks are opt-in; this cannot be undone."
- Engine stop: "N running containers on <name> will stop with the engine."
- Toast strings: "Run command copied · some env values redacted", "Logs
  copied", "Inspect JSON copied · truncated", "Terminal did not answer — check
  Automation permissions".
- Status labels: running, exited, paused, restarting, starting, dead, created,
  healthy, unhealthy, dangling, ready.

Keep `docs/ui/*.png` as the visual baseline and the Zig binary as the
behavioural baseline until the Rust app passes M7.

---

## 10. Decisions to confirm before M0

