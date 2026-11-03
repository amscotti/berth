# A UI/UX review, done properly

Every view, both themes, judged against the design contract rather than against
the code. Nine defects; all fixed. Screenshots at the end of each row are the
evidence, in `docs/ui/`.

## The Tier 1 pass, with vision

The search, ports, pause/resume, health, Inspect, metrics and copy-run work was
then walked again at 960×600, 1280×820 and 1440×900, in both themes:

- Search moved into the page header on all four lists, with a clear button and
  a "Clear search" action on the empty result. Counts only say "N of M shown"
  when something is actually hidden.
- The container name and image share one two-line cell, which bought the ports
  column back. Network/disk/pids/uptime each gained a grouped slot; Ports is
  back in the table and sorted host-side.
- Status says health when there is one. A stopped row shows one dash per
  unknown figure, not a column of stacked dashes.
- The inspector is now title + close, a Facts/Inspect row, then content. Facts
  labels share a column; ports read `18080→80` and are clickable; started-at
  and restart count come from inspect; the JSON view has Copy and Refresh.
- At 1120 points the rail folds to icons; the inspector is 430 or 360; table
  densities are full, medium, dense, compact and tiny, chosen by the content
  width. Actions stay pinned right and the name column takes spare width.
- The panel pages scroll at 600 points tall, and the Settings prose is held to
  a readable measure. The window opens at 1280×760, and a theme switch keeps
  the size the user chose.
- "Updated" and "Started" are local wall-clock times, not UTC. First launch is
  a skeleton, not "0 images": discovery probes its candidates in parallel, and
  engine metadata is read once at discovery, so a metadata hiccup cannot throw
  away a good container list.

Evidence: `overview.png`, `containers-light.png`, `containers-dark.png`,
`detail-light.png`, `inspect-light.png`, `containers-960.png`,
`detail-960.png`, `containers-1440.png`, `settings-light.png`.


## What the surface is

This is a **Monitor** surface: the user is watching state change. Density and
glanceability beat decoration, numbers outrank prose, and no composition should
need explaining. Rows read *what is it · is it healthy · how much · what can I
do about it* in that order, left to right.

## Slop audit (lower is better)

| Tell | Fired? |
|---|---|
| Tech gradient, glassmorphism, blob art | no |
| Generic indigo accent by default | no — the accent is burnt amber on warm paper, not a default blue or violet |
| Equal-weight feature-tile grid | partly — the Overview is four equal cards, but they carry live graphs, not copy |
| Accent rail / icon topper / center stack | no |
| Monument stat | no — large numbers are the point on a monitor |
| Wrong surface | no |

**Score: 2/10**, and both fired tells are the cards' sameness rather than
tell-tale decoration. Left as is deliberately: four equal graphs is the right
composition when the four series genuinely rank equally.

## Defects found, and what was done

**1. The Disk view's explanation card was clipped off the window.**
`defer row.end()` closed the metrics row *after* the note, so the note became a
sixth item in a horizontal row and was squeezed to 96pt at the right edge. The
third time this exact trap has bitten; the row now ends explicitly, with a
comment saying why. The reclaimed space holds a four-part breakdown strip, so
the view answers "how is it split", not only "how much".

**2. Images: the STATUS column drifted 30pt from its header.**
A `pill` sizes itself to its word; the header slot was fixed at 120. Every
intrinsic-width widget shifts everything after it, so the fix is the fixed-slot
`pillCell` — the same fix the containers table already needed.

**3. The API's vocabulary was leaking into the interface.**
`<none>:<none>` for an untagged image and `null` for a network with no driver.
Now "untagged" and an em dash. An interface that speaks only in the API's
dialect asks the user to learn the dialect.

**4. Published ports ran into the action buttons.**
`8080→80, 808…` collided with Stop. The ports column now takes the width the
PROJECT column no longer needs.

**5. A column of em dashes.**
PROJECT only exists for Compose groupings, so it was absent-value noise on every
plain container. It is now hidden unless something is actually grouped, which
buys the ports column 90pt.

**6. The same binding listed twice.**
Docker reports a port once per address family, so a container published on IPv4
and IPv6 read `8080→80, 8080→80`. Deduplicated on the public/private pair, and
the "+N more" count now counts real ports instead of raw entries.

**7. Dark mode had never actually been looked at.**
The earlier "dark" screenshots were light: the snapshot path takes the app's
window theme, so `--nanoui-dark` was overridden by the app's own `themeFor`.
`BERTH_THEME=dark|light` now exists as an explicit override — it wins for one
run, which is what a screenshot needs and also what someone whose desktop does
not report a theme needs.

**8. The dark theme's unlit graph grid was louder than the data.**
A 45% track that reads as a quiet baseline on white reads as noise on
`#1c1c1e`. The unlit alpha is now chosen from the background's brightness.

**9. Empty states were inconsistent in tone.**
Now every one names the state and the next action:
*No volumes — create one above, or run a container that declares one*,
*None match this filter — All shows every container*.

## What is still wrong, knowingly

- **The container list is a table, not a monitor.** With forty containers it
  will read as a spreadsheet. A compact/comfortable density switch is the fix,
  and it is not built.
- **No text search.** Filters only. Typing a name is the fastest way to find a
  container and it does not exist yet.
- **Actions are always visible on every row.** Right for three containers,
  noisy for thirty. A row-level menu is the usual answer; it costs a click and
  buys calm.
- **The Overview's four cards are equal weight**, which is honest but flat:
  nothing says which figure matters most right now.
- **Column headers and cells land within a pixel or two**, not exactly, because
  the pill inset is per-widget. It reads as deliberate inset rather than drift,
  which is why it was left.

## Method

Snapshots were taken of all nine views through the headless path
(`BERTH_TAB=… --nanoui-snapshot=…`), in both themes, plus one run with no engine
reachable to exercise the error state. Each was read as an image, not inferred
from code. The defects above are the ones that survived that reading; the fixes
were re-snapshotted and re-read.

## Second pass: nanomon as the reference

The first pass kept the platform's cool grey and one blue accent. A later pass
used [nanomon](https://github.com/justrach/nanomon), the same author's activity
monitor, as the structural reference:

- **One warm surface, one accent.** Paper (`#f7f3ea`) and charcoal (`#17140f`)
  with burnt amber for selection; sage and rust carry state. The palette is
  derived from the framework theme each frame, so System / Light / Dark still
  follows the desktop live.
- **The rail monitors.** Each page shows the figure it is about (running/total
  containers, image and volume counts, store size, answering engines) and the
  bottom line says *Live · every 5 s* plus the engine's name. Group labels
  split Resources from Engine.
- **Cards look like instruments.** A tinted glyph, a coloured label, one large
  tabular figure, a context note ("of 4 cores", "of 3 containers", "reclaimable
  4.0 KiB") and the graph in a recessed well.
- **Runtimes leads with the answer.** Reachable engines get full cards; the
  sockets that did not answer collapse into one compact card instead of a
  screenful of red.
- **Bars fill their track.** The host split and the disk strip use proportional
  leaves, so they are correct at any window width; the disk bar gained a
  colour legend.
- **The dock icon is the rail's cube.** `macos/make-icon.py` draws it;
  `macos/build.sh` copies `berth.icns` into the bundle.
- **The tables are tables.** Headers sort (with an arrow and a coloured title),
  rows have hover and selection, and Images, Volumes and Networks open the
  same right-hand inspector as Containers. With the inspector open they switch
  to compact columns instead of running under it.
- **Refresh neither blanks nor blocks the page.** The pass runs on a worker
  thread, builds into its own arena, and posts one commit back to the main
  queue; the rail says "Refreshing…" while it is in flight. The list, graphs
  and disk figures stay readable and the window stays responsive throughout.
- **Clean up is a first-class action.** The Disk page and Settings →
  Maintenance open a sheet of prune categories with their reclaimable sizes,
  defaulting to stopped containers, unused images and build cache; volumes and
  networks are opt-in because they outlive their containers. The old bottom
  action bar was removed; its refresh and cleanup actions moved here.

## Initial-review follow-up

The first release review found eight behaviours that did not match the
intent. They are fixed, and the evidence is in `runtimes-light.png`,
`runtimes-open-light.png`, `settings-menu-light.png`, `confirm-light.png`,
`prune-light.png`, and `volumes-light.png`.

- **Volume sizes.** `/volumes` omits usage; the app now fills each row and the
  inspector from `/system/df`, and retains the last good df for one timed-out
  pass without keeping a pointer into a retired arena.
- **Engine discovery.** Only sockets that exist are probed; Docker Desktop,
  OrbStack, Rancher Desktop, Colima, Lima, and the current Podman machine
  layouts are included. Aliases to one daemon are deduplicated by the engine's
  own ID. A failed scan retries in two seconds, and a chosen or typed socket is
  probed first on the next launch.
- **Runtimes.** The rail badge is the number answering, not a total. The page
  groups sockets by engine kind; "Show N sockets" expands the paths. A custom
  socket field validates before saving and has a Forget action.
- **Choosing an engine.** Settings has a chooser listing the answering engines,
  plus a link to the full socket list. Appearance is left, Engine right, and
  both cards stretch to the same height.
- **Dialog buttons.** Confirm/Clean up and Cancel now share one size and
  baseline; the old code nested Cancel inside Confirm because of deferred ends.
- **CPU.** The overview history is normalised to the same "of N cores" share as
  the headline figure; the sparkline is labelled `recent`. A value that is
  current-zero with past dots now reads as history, not a mismatch.
- **Rail status.** No busy text, no Rediscover button, no width change every
  five seconds: an amber dot and a constant "Refresh · every 5 s", with the
  updated clock as the indicator. Rescan lives on Runtimes.
- **Truthful figures.** Engine switches clear the previous engine's history;
  failed stats stay `—`; pending confirmations are closed when the engine
  changes; the UI client adopts the engine's API version, so actions and
  Inspect work on current Docker releases.
## Tier 2, Wave 2

The monitor gained a memory and the engine gained controls, without giving up
the no-flicker contract:

- **Events.** `/events` is polled as an interval on the refresh worker, deduped
  into a 300-row ring keyed by the engine's own timestamps, and shown in a new
  Events view with search and density-scaled columns. Exits, OOMs and restarts
  toast once; the first backfill never replays as news. A failed interval is
  retried, not skipped.
- **Engine lifecycle.** Colima, Lima and Podman machines can be started,
  stopped and restarted from Runtimes. Commands run as background jobs with
  captured output, a timeout, and a single retry-safe callback; Lima and Podman
  restarts are stop-then-start against the instance that was clicked.
- **Bulk actions.** Container checkboxes borrow the name column's width, and
  start/stop/delete run on a worker with one confirmation naming the count and
  an honest "N of M" result.
- **Logs.** Follow now fetches the tail on the refresh worker, timestamps are
  local, the pane has search and copy, and a stalled engine costs one pass
  rather than a frozen window.

Evidence: `events-light.png`, `events-960.png`, `bulk-light.png`,
`logs-follow-light.png`, and `runtimes-open-light.png`.

## Tier 2, Wave 3

The creation card and the tray shipped, then every view was walked again at
1280×760 in both themes, at 960×600 dark (icon rail), and on Images with the
inspector open — which forces the card's stacked layout. The tray itself is
menu-bar chrome, not part of these snapshots.

- **Creation card.** Wide layout (Image/Name, Ports/Volumes, Env+Run) at 1280
  and 960; stacked full-width fields with a right-aligned Run when the
  inspector narrows the page. Placeholders, hint line and both buttons line up
  with the card's edges in both themes. Enter submits like the buttons.
- **Events.** The pass caught a real one: some rows rendered TYPE as "er".
  `/events` was parsed with slices aliasing the response body, which the
  worker frees immediately after parsing — the ring then showed whatever
  reused the bytes (a 7-byte allocation left the tail of "container").
  Parsing now allocates its strings (`.allocate = .alloc_always`), with a test
  that poisons the body before reading the fields. Re-shot: all rows correct.
- **Inspector.** The image inspector's Run… pill and Delete align, and Run…
  prefills a dangling image's Id rather than the word "untagged".
- **Overview, Containers, Disk, Runtimes, Settings.** Unchanged by the wave;
  both themes, the 960 rail, and the responsive column set still hold.

Evidence: `images-light.png`, `images-dark.png`, `images-960.png`,
`events-light.png`, `events-dark.png`.

## Tier 3, wave 1

Walked at 1280 in both themes and at 960×600 with the inspector open — the
472-point content width is the real narrow case, not the window minimum.

- **Volume and network creation.** The page-header action sits beside the
  search box at 1280 and 960, clear button included; the dialog is one name,
  a hint line and Cancel/Create on one baseline. Created both resources
  through the dialog against the live engine; duplicate volumes are refused
  with the volume's name before any request, since Docker's create is
  idempotent.
- **Stacks.** Start all / Stop all appear only when they would do something.
  Stop all confirms with the project and a live count ("1 container in
  berth-w4-shop will be stopped" — the plural bug the first pass caught is
  fixed). At 960 with the inspector the working directory collapses and the
  project name gets the room; both buttons stay inside the card.
- **Terminal.** The container inspector's Terminal action opened Terminal.app
  with `docker -H unix://… exec -it berth-demo-web sh`; verified the window's
  own title carries the socket-pinned command.
- **Host bar.** The engine segment now shows the engine's host processes
  (≈45–93 MiB of `limactl` here) and the caption says "engine procs". The
  segment deliberately excludes the VM's guest RAM: the containers are
  measured inside it, and counting the VM again would double-count them.
  Values checked against `ps` directly.
- **Narrow layouts** for Volumes, Networks and Stacks with the inspector open
  are in `volumes-960.png`, `networks-960.png` and `stacks-960.png`.

Evidence: `volumes-light.png`, `volumes-dialog-light.png`,
`networks-light.png`, `networks-dialog-dark.png`, `stacks-light.png`,
`stacks-960.png`, `detail-terminal.png`, `overview.png`, `overview-dark.png`.
