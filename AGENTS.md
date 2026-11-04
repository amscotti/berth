# Working in berth

Notes for whoever — or whatever — picks this up next. The short version: this
is a Zig 0.17 app built on nanoui, it monitors container runtimes, and most of
the rules below exist because ignoring them cost a day.

## Commands

```sh
zig build                 # debug binary at zig-out/bin/berth
zig build run             # build and open the window
zig build test            # no engine, no display needed
zig build lines           # the 600-line rule; part of every phase's gate
zig build dist            # ReleaseFast + tarball in zig-out/dist
```

**Every phase ends with all four gates green**: build, test, lines, and a
snapshot of the view you touched.

## Seeing the UI without a display

```sh
BERTH_TAB=overview   zig-out/bin/berth --nanoui-snapshot=/tmp/a.png --nanoui-wait=12
BERTH_TAB=containers BERTH_FILTER=running ...
BERTH_TAB=containers BERTH_SELECT=<name> ...
BERTH_TAB=containers BERTH_ACT=stop:<name> ...
BERTH_SIZE=960x600 BERTH_THEME=dark BERTH_TAB=overview ...
BERTH_SHEET=prune BERTH_THEME=light BERTH_SIZE=960x600 ...
BERTH_SHEET='confirm:stop:<name>' BERTH_TAB=containers ...
BERTH_SHEET=engine-stop BERTH_TAB=runtimes ...
```

`BERTH_SIZE` is `WIDTHxHEIGHT` in content points. `BERTH_THEME=light|dark`
overrides the saved appearance for one run. `BERTH_SHEET` opens a sheet for a
still frame: `prune`, `create-volume`, `create-network`, `create-error`,
`form-errors`, `bulk`, `bulk-all`, `confirm:stop:<name>`,
`confirm:delete:<name>`, `engine-stop` or `engine-restart`. Values that need a
row from the engine wait for the first snapshot.

`--nanoui-wait` is not optional in practice: it runs the event loop for that
many seconds first. A snapshot without it catches a skeleton. Then **look at the
PNG** — do not assume a layout change worked because it compiled.

Docker on this box is `sudo -n docker …`; the user is not in the `docker` group.

## How the app is shaped

- **One refresh chain.** `State.refresh()` → version/info → containers → stats →
  images → volumes → networks → df → `.done`. Each step is a callback; nothing
  polls. The 5 s timer re-arms itself from the *end* of the chain.
- **Actions run at the end of the chain**, after `s.step = .done; s.sample();`.
  Running one inside a step lets the tick retire the refresh that started it —
  that bug showed as "0 running · 0 total" with the row gone.
- **The frame arena owns every string.** User-visible text goes through
  `c.print`; a stack `bufPrint` buffer dies before paint and renders as `aaa`.
- **Arenas rotate.** The refresh arena from last cycle stays alive one more
  cycle, because the frame can hold slices into it. Freeing it at the end of the
  refresh segfaults in `memcpy` about a second later.
- **Selection is re-resolved by id** every refresh, never held as a pointer.

## nanoui rules that are not obvious

- Inside `ui.list`, rows must be `ui.row(c, .{…})` … `ui.end(c)`. A bare
  `ui.div` row floats above its slot.
- **Never** `defer x.end()` inside a `for` or `inline for` over repeated items —
  the boxes nest. Use a runtime `for` and close explicitly. This has caused
  three separate layout bugs, including one where a metrics row swallowed the
  card below it.
- On a **column**, `itemsCenter()` centres horizontally and `justifyCenter()`
  centres vertically. It is the reverse of the CSS reflex.
- `Div.grow()` takes **no** argument. It is a flag, not a weight. Proportional
  segments need explicit widths.
- A widget's intrinsic width shifts every later column. Status pills go through
  `widgets.pillCell` with a fixed slot.
- `std.Io.Dir.openDirAbsolute(io, path, opts)` is namespaced, not a method on
  `cwd()`.

## Zig 0.17 traps hit here

- `std.process.Child.init` is gone: `std.process.spawn(io, .{ .argv = … })`, then
  `child.wait(io)`. `std.process.run(gpa, io, …)` is blocking.
- `std.Io.net.UnixAddress` with `init`/`connect(io)`/`listen(io)`.
- **procfs reports `st_size` 0.** `readFileAlloc` and the buffered `Io` reader
  both return an **empty string, no error**. Read `/proc` with a `posix.read`
  loop until EOF. This cost hours and produced two "successful" empty reads.

## Data rules

- `/stats?stream=false` waits **two collection cycles** (~2 s per container)
  unless `one-shot=true`, and `one-shot` empties `precpu_stats`, which makes CPU%
  fall back to a lifetime average. Never use it.
- Unknown is not zero: a stopped container shows `—`, never `0`.
- Memory is a working set: `usage − inactive_file` (`total_inactive_file`, then
  `cache`, as fallbacks).
- Graphs use fixed, honest scales. Auto-scaling made 900 KiB of 9.6 GiB look
  full.
- The host bar is three segments — containers, engine, elsewhere. Containerd
  shims are excluded from "engine" or per-container memory counts twice.
- List answers are capped at **5,000 entries per store** (containers, images,
  volumes, networks, build cache, and the same arrays inside `/system/df`), and
  parsed JSON deeper than 64 levels is refused. This is a deliberate
  availability limit, not an empty or deleted store: the refresh reports
  “refusing to parse (availability limit — retry when the engine has fewer)”.
  A refused list aborts that pass, so the previous list snapshot stays on
  screen. `/system/df` is fetched on its own ~30 s cadence; its last good
  document is kept with an age/error note and owns a separate arena, so
  snapshot rotation cannot dangle it. Mutations and Refresh now invalidate it.
- Copy-run redaction is best-effort: credential tokens (including plural,
  camelCase and `PASSCODE`/`PASSPHRASE`/`PASSWD` forms) and URL userinfo are
  replaced with `<redacted>`, and the toast says “some env values redacted”
  rather than claiming every secret was found.

## Rules of the codebase

600 lines per file, no C shims, no phase names in code or comments, and
comments explain *why* rather than restating the line. `docs/PHASES.md` records
what broke and why, phase by phase — read the phase you are about to touch.
