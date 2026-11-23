# berth fix backlog

**Status:** living document
**Created:** 2026-11-23
**Baseline:** `ebe5f59`
**Source:** the code and UX review of 2026-11-23
**Next:** T1
**Purpose:** every finding from that review as checkable work, in the order to
do it. Anyone, or any sub-agent, should be able to take the next unticked row
and know what "done" means without re-reading the review.

Related: `docs/PARITY-BACKLOG.md` (what parity meant), `docs/PRODUCT-SPEC.md`
(behaviour and visual contract), `docs/TESTING.md` (how each item is proven),
`TODO.md` (the rewrite's remaining gaps).

---

## How to use this

- Work top-down. A phase's exit condition is the gate before the next phase.
- Tick a row by changing `- [ ]` to `- [x]`, then move **Next** at the top of
  the file. Keep IDs stable, so a commit or a test can say "fixes B4".
- Each row reads: **ID** (size) what to do. `where`. Done when (level): the
  proof. Depends and Feeds name rows that must land first, or that wait on this
  one.
- A row is done when its proof exists at the stated level and every gate is
  green: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`, `./tools/lines.py`. If a person can see
  the change, regenerate and read the screenshot in `docs/ui/`, and update
  `docs/PRODUCT-SPEC.md`.
- Levels and sizes are `docs/PARITY-BACKLOG.md` §1's: **U** unit, **E**
  engine/mock, **G** GPUI in-process, **H** headless render, **X** real-window
  E2E, **L** live engine, **M** manual. **S** an evening, **M** a day or two,
  **L** a week. A size is a guess; re-size a row when you pick it up.
- Paths: `ui/…`, `app_*.rs`, `jobs/…` and `state.rs` are under
  `crates/berth-app/src/`; the other crates are written `berth-engine/src/…`.
  Line numbers are as of the baseline and will drift.

## Phases at a glance

| Phase | Scope | Items | Exit condition |
|---|---|---|---|
| 0 | Test scaffolding | 3 | A test run leaves nothing in the temp dir; the mock engine serves the refresh, mutation and detail tests |
| 1 | Behaviour and honest numbers | 24 | Every bug has a test that failed first; logs don't blink; no figure is clipped |
| 2 | Component kit | 9 | No ad-hoc button; every control has hover, pressed, disabled and focus; sizes and washes come from `berth-ui` |
| 3 | Architecture and code | 15 | State lives in the entities `GPUI-REBUILD.md` §2.2 draws; one `DataTable`; 5,000 rows render inside the frame budget |
| 4 | UI completion | 23 | Overview matches the reference's structure; every state is designed; the main flows work from the keyboard alone |
| 5 | Engine and scale | 6 | Scan and pass times are bounded and stated; long operations show progress and don't lock the slot |
| 6 | Tests, CI, packaging | 9 | CI fails on a coverage drop, a visual regression or a slow frame; a release runs the app's tests |
| 7 | Documentation | 6 | The docs describe the app that ships |
| — | Verify on a live system | 4 | Four manual checks, each with a recorded result |
| — | Also found while drafting | 12 | Optional; take each with the phase it names |

111 rows: 69 S, 39 M, 3 L. Added up literally in the legend's units that is
several months for one person, so treat the phases, not the total, as the plan.
Phases 2 and 4 touch the views. Phase 5 and most of Phase 1's engine rows
(B8–B15) don't, and can run beside them in the Engine workstream.

---

## Phase 0 — Test scaffolding

Phase 1's fixes need these to be provable. Small, and every later test benefits.

- [ ] **T1** (S) Give every test socket and temp directory a cleanup guard. The test modules in `berth-engine/src/transport.rs:406`, `client.rs:417`, `discovery.rs:363`; `jobs/refresh.rs:196`; `jobs/detail.rs:83`. One RAII helper that removes its path on drop. Done when (U): the guard removes its own path in a test, and a full `cargo test --workspace` leaves no new `berth-*` entry in the temp dir. On this machine the baseline left 528 sockets and 36 directories behind.
- [ ] **T2** (M) Build the scripted mock engine that `docs/TESTING.md` §4 describes, in a new `berth-engine/src/mock.rs`, and replace the five hand-written one-shot servers. It answers by method and path from a queue, can chunk, trickle, delay or hang, and records requests. Done when (E): `refresh::load`, `mutation::run` and `detail::load` each run against it end to end and assert on the request log. Feeds B7, B8, B9, E1, E2, E5, E6, T4.
- [ ] **T3** (S) Add GPUI helpers for driving the app in tests: open each sheet, select a row, hold a mutation or a pass in flight, wait on a condition (no sleep), read back tab, selection, sheet and toast. Done when (G): the tests for B4 and B5 are written with them. Feeds B4, B5, B19, B20, B23.

## Phase 1 — Behaviour and honest numbers

B1–B18 are the review's "fix first" table, in its order; B19–B24 are its
smaller ones. Each row should fail a test before it passes one.

- [ ] **B1** (S) Follow mode: keep the last logs on screen while a poll is in flight, and actually follow the tail. `ui/inspector.rs:493–521`, `app_detail.rs:11–63`. Show "Reading…" only when there is no text yet; give the log well a `ScrollHandle`; scroll to the end after each follow commit; leave the offset alone once the user scrolls up. The Inspect well (`ui/inspector.rs:127–134`) and the Facts "Started · Reading…" row (`:178–179`) blank the same way. Done when (G): with a follow poll pending the well still shows the previous text and its offset is at the end.
- [ ] **B2** (S) Stop clipping CPU at 960 wide. `berth-ui/src/layout.rs` (the CPU column is 46–50 pt), `ui/table_cells.rs:103–112`. Widen the column to fit a five-character value such as `99.9%`, or measure it with the text system; give spare width to numeric columns before the name column's cap. Done when (H): `99.7%` and `1235%` render unclipped at Medium density; `containers-light-960x600.png` is regenerated and read.
- [ ] **B3** (S) Make text fields follow the app's Appearance setting. `ui/input.rs:194`, `app_actions.rs:190–212`. Pass the current tokens into `InputState` (updated when the theme changes) or set the window appearance to match the override. Done when (H): with the system dark and the app set to Light, the fields render light.
- [ ] **B4** (S) Stop a sheet from closing as if it ran. `app_forms.rs:118, 156, 291–303`. Have `start_mutation` report whether it accepted the request; set `create_in_flight` and `prune_in_flight` only then; close a sheet only on the outcome of its own request. Done when (G): a refused prune leaves the sheet open, and an unrelated success doesn't close it. Depends T3.
- [ ] **B5** (S) Make `mutating()` mean a mutation, not a refresh. `app_forms.rs:319–321`, `ui/sheets.rs:118–125`. Drop `refresh_task` from it; while a mutation runs, show Confirm disabled and keep the sheet open. Done when (G): the "already running" note never appears during a refresh pass. Depends T3. U1 gives the disabled look its final form.
- [ ] **B6** (S) Stripe rows by their visible position. `ui/table_cells.rs:26–49`. Pass the visible position from `ui/table.rs` into `container_row`; this also removes the per-row linear search for the index. Done when (G): a sorted or filtered table alternates evenly.
- [ ] **B7** (M) Keep the engine live across a rescan, and its history when it is the same engine. `app_actions.rs:246–265`, `app_engine.rs:104–136`, `state.rs:149–167`. Don't drop the client when a rescan starts; in `adopt_runtimes` compare identities and reset history, df and events only on a real change. Done when (E/G): a rescan against the same daemon keeps `History::len()` and the last df, and the clock keeps advancing during the scan. Depends T2.
- [ ] **B8** (S) Move the events cursor to the engine's clock. `app_engine.rs:272`, `jobs/refresh.rs:82–90`. Use the newest `time_nano` already in the ring minus one second, not the host's `last_update`; the existing dedupe covers the overlap. Done when (E): with the mock's clock five seconds behind the host, no event is missed across two passes. Depends T2.
- [ ] **B9** (S) Make prune survive a slow or failing step. `berth-engine/src/client.rs:303–313`, `jobs/mutation.rs:269–291`. Give prune a 60–120 s budget; run every step even when one fails; return per-step results so the toast can say "Reclaimed 1.2 GiB · build cache: <error>". Done when (E): a failing middle step doesn't stop the rest, and the outcome names the bytes already freed. Depends T2.
- [ ] **B10** (S) Bound machine commands. `jobs/mutation.rs:337–360`. Run `colima`, `limactl` and `podman` with null stdin and a five-minute limit, kill on expiry, and say "timed out; the machine may still be starting". Done when (E): a stand-in command that never exits (make the program path injectable for the test) returns an error within the budget.
- [ ] **B11** (S) Never draw a memory graph against a ceiling of 1. `ui/views/overview_cards.rs:111–114`, `ui/graph.rs:52`. Take an `Option<f32>` ceiling; with no `mem_total`, or `info_read` false, draw no series and say the ceiling is unknown. Done when (H): with `info_read = false` the memory well is empty, not full.
- [ ] **B12** (S) Redact URL userinfo. `berth-engine/src/detail.rs:200–219, 392–432`. Redact values of the form `scheme://user:password@host` in the env list and the run command, and report it through the existing `run_redacted` flag. Done when (U): `DATABASE_URL=postgres://admin:hunter2@db/app` leaves the crate redacted in both.
- [ ] **B13** (S) Show the real inspect document. `app_render.rs:302–304`, `jobs/detail.rs:36–70`, `app_detail.rs:55–61`. Keep the raw body and pretty-print it with `serde_json`; use the typed DTO only for Facts and Config. Done when (E): the Inspect text shows a `HostConfig` field the DTO doesn't model, such as `Privileged`.
- [ ] **B14** (S) Roll byte figures to the next unit. `berth-core/src/format.rs:8–22`. Round first, then re-check the ladder. Done when (U): `bytes(1_048_575)` is `"1.0 MiB"`.
- [ ] **B15** (S) Stop adding unknown memory as zero. `berth-engine/src/stats.rs:178–188`. Require `mem_known`; expose "all memory known" the way `all_sampled` does, and feed it to the partial-sample label (U14). Done when (U): a sample with `mem_known = false` reports partial, not a smaller sum.
- [ ] **B16** (M) Offer only the actions a state allows. `ui/inspector.rs:318–428`, `ui/table_cells.rs:442–528`, `ui/views/runtimes.rs:225–242`. One table-driven `actions_for(state)`: paused gives Resume and Stop; restarting gives Stop; created and exited give Start; removing gives none; a machine that is answering gives Stop and Restart, one that isn't gives Start. Use it in the row, the inspector and the runtime card. Done when (G): a test walks every state.
- [ ] **B17** (S) Keep redacted keys when cloning. `app_detail.rs:196–220`. Carry them as `KEY=` and mark the Env field "2 values to fill in" instead of dropping them. Done when (G): a clone's env list contains every original key.
- [ ] **B18** (M) Say which disk figures are missing, and where the disk is. `ui/views/disk.rs:86`, `berth-engine/src/df.rs:112–171`. Return `Option<i64>` per store and render `—` when a store is unmeasured or unsupported (build cache on Podman); use the VM sentence only for VM-backed kinds (Lima, Colima, Docker Desktop, OrbStack, Rancher Desktop, Podman machine). Done when (H): a Podman fixture shows `—`; a native Linux fixture has no VM sentence.
- [ ] **B19** (S) Clear a selection properly when a filter chip is clicked. `ui/table.rs:127–134`. Call `clear_selection(cx)` instead of nulling two fields, so the detail job is cancelled. Done when (G): after a chip click `detail_task` is `None`.
- [ ] **B20** (S) Drop a stale detail completion cleanly. `app_detail.rs:41–47, 65–71`. Clear `detail_task` before the generation and id check. Done when (G): a stale completion leaves no finished task handle behind.
- [ ] **B21** (S) Show the row you jump to. `app_detail.rs:169–175`. Clear the containers search when jumping from Events or another inspector. Done when (G): a row hidden by the current search is visible after the jump.
- [ ] **B22** (S) Sort ports as numbers. `app_actions.rs:328`. Done when (U): 8080 sorts before 10000.
- [ ] **B23** (S) Queue a rescan requested during a scan. `app_engine.rs:82–102`. Remember it and run it after `adopt_runtimes`. Done when (G): two requests produce two scans, in order.
- [ ] **B24** (S) Backfill events on the first pass. `jobs/refresh.rs:85`, `app_engine.rs:272`. Ask for the last five minutes without raising toasts. Done when (E): the first pass returns the engine's recent events; the no-toast-on-first-pass rule still holds.

## Phase 2 — Component kit

The root cause behind most of Phase 4. Doing it first means the later UI work
lands on shared parts instead of adding more one-offs.

- [ ] **U1** (L) One `Button` element. New `ui/kit/`; replaces `small_action`, `verb_button`, `delete_button`, `button`, `small`, `action_pill`, `command_button`, the page action, `primary_button`, `secondary_button`, and the two `small_button`s. Variants Primary, Secondary, Ghost, Danger, Danger-wash; heights 24 and 28; hover, pressed, disabled and focus built in; one rule for the accessible name. Done when (H): every page screenshot is regenerated and read, and a grep finds no `Role::Button` div outside `ui/kit/`. Feeds U2, U3, U5, U9, B5.
- [ ] **U2** (S) Make destructive controls danger-coloured, as the spec and the reference have them. `ui/sheets.rs:105–117, 306–317`; `ui/views/stacks.rs:95–116`; `ui/views/runtimes.rs:256–287`. Danger fill for Confirm and Clean up; danger wash for Stop all and machine Stop. Done when (H): `sheet-confirm-delete.png` and `sheet-prune.png` show danger fills. Depends U1.
- [ ] **U3** (S) One `Chip`, and a selected state that reads as selected. `ui/table.rs:102–146`, `ui/views/settings.rs:42–79`, `ui/inspect_shared.rs:11–109`. Selected means accent wash, accent text and medium weight, as the Appearance chips already do. Use it for the filter chips, Appearance, the inspector tabs and Follow/Times. Done when (H): on Containers, "All" reads as the active filter. Depends U1.
- [ ] **U4** (M) Cut the type scale to seven steps and use nothing else. `berth-ui/src/typography.rs`; every `text_size(px(…))` under `ui/`. Keep 9.5 (meta), 11 (label, pill), 12.5 (secondary), 13 (body), 15 (card and sheet title), 22 (page title) and 26 (metric). One card-title size: it is 12.5 on Overview, 13 on Runtimes and Images, 15 on Settings and Disk today. Done when (M): a grep for `text_size(px(` under `ui/` finds none outside the scale.
- [ ] **U5** (M) Turn every wash, radius and control size into a token. `berth-ui/src/theme.rs`, `berth-ui/src/layout.rs`; uses across `ui/`. Named values for selected 0.10, pill 0.14, danger 0.12, zebra 0.35, backdrop 0.38 light and 0.58 dark (as the spec has it), and the rest; radii 4, 6, 7, 10 and 14; control heights 24 and 28. Done when (U): the contrast tests cover the new washes; (M) a grep for `.alpha(0.` outside `berth-ui` is empty. Depends U1.
- [ ] **U6** (M) One `Card`, with equal heights in a row. New; replaces the card bodies in `ui/views/overview*.rs`, `settings.rs`, `disk.rs`, `runtimes.rs`, `images.rs` and `stacks.rs`. Title, optional tinted icon chip, optional right-hand slot; one padding and radius; siblings stretch to the tallest (`ui/views/settings.rs:164–181` uses `items_start` today). Done when (H): Settings' two-column row shares a bottom edge. Depends U4, U5.
- [ ] **U7** (S) One `Checkbox`. `ui/sheets.rs:321–375`, `ui/table_cells.rs:60–97`. A drawn check instead of the `✓` text glyph, a larger hit area, a checked state for assistive tech. Done when (G/H): the prune sheet and the bulk column use it. Depends U1.
- [ ] **U8** (S) Choose the monospace font per platform. The 16 `font_family("Menlo")` sites in `ui/inspector.rs`, `ui/inspect_tabs.rs`, `ui/inspect_objects.rs`, `ui/views/runtimes.rs`, `ui/views/stacks.rs` and `ui/views/settings.rs`. One `berth_ui::typography::MONO_FAMILY`: Menlo on macOS, a named Linux mono with a fallback. Done when (V3): logs are monospaced on Linux.
- [ ] **U9** (M) Make every control focusable, with a visible focus ring. `ui/kit/`, `ui/table_cells.rs`, `ui/rail.rs`; today only `app_render.rs:270` and `ui/input.rs:218` call `track_focus`. Buttons, chips, checkboxes, rail items and rows take focus and answer Enter and Space; add a focus-ring token and a sensible Tab order. Done when (G): a test tabs from the root through search, chips, header and first row. Depends U1. Feeds U25, V4.

## Phase 3 — Architecture and code

- [ ] **A1** (L) Split `BerthApp` along the lines `docs/GPUI-REBUILD.md` §2.2 already draws. `app.rs:193–261`. A `ViewState` for tab, filter, searches, sorts, selection and detail generation, bulk, sheets and toasts; an `Inspector` that owns logs, inspect, its task and generation; a `ListState` per table. Each a `Render` child, so an edit re-renders its own subtree. Done when (G): the existing tests still pass (facade methods may remain) and no field in `BerthApp` belongs to another entity. Feeds A2, A4, U24.
- [ ] **A2** (M) Replace parallel booleans with `enum Sheet`. `app.rs:224, 233–238`; `app_forms.rs:72–85, 134–145`. `Sheet::Confirm`, `::Prune` and `::Create` take over from `pending`, `prune_open`, `create_kind` and `creation_open`, so two sheets can't be open at once by construction. Done when (G): no opener resets another sheet's flag. Depends A1.
- [ ] **A3** (M) Type the pending action. `app.rs:285`, `app_forms.rs:197–239`, `app_bulk.rs:36–51, 91–125`. `Pending::Container`, `::Batch` and `::Engine`, plus a `ContainerAction` enum, replace the `"BULK"`, `"ENGINE"`, method, path and verb strings. Done when (M): those string comparisons are gone. Depends A2.
- [ ] **A4** (M) `enum MutationSlot { Idle, Running { kind, since, progress } }`. `app_forms.rs:264–321`. It replaces `mutation_task.is_some()`, `create_in_flight` and `prune_in_flight`, names the running operation in a refusal, and carries the progress Phase 4 displays. Done when (G): a second request is refused with the running kind named. Depends A1. Feeds U23.
- [ ] **A5** (S) Keep fixtures and screenshot hooks out of release builds. `app_render.rs:17–138`, `state.rs:170–447`, `app.rs:299–320`, `crates/berth-app/Cargo.toml:11`. Move them behind a feature (the declared but empty `test-driver` is the natural one) or `cfg(test)`. Done when (M): a release build contains no `fixture_snapshot`, and `--features test-driver` does something or is removed.
- [ ] **A6** (S) Delete dead code. `berth-core/src/history.rs` (`clear_engine`, `ceiling`, `peak`); `ui/graph.rs:13, 89`; `host.rs:82–125`; `ui/views/disk.rs:235–236`; `jobs/refresh.rs:192–193`; the stale `#[allow(dead_code)]` on four used methods in `ui/input.rs:56–76`; the icons `cpu`, `memory`, `plus` and `sparkles` in `assets.rs` (`xmark` goes to X1); `selected: Option<usize>` and the `stats_complete` field in `state.rs`; the second `short_id`, `section_heading`, `empty_note`, `title` and `muted`. Done when (M): clippy is green with no `allow(dead_code)` outside the tray's non-macOS stub.
- [ ] **A7** (M) Compute list work once per commit. `app_actions.rs:287–334`, `ui/views/images.rs:126–131`, `ui/views/events.rs:15–21`, `ui/views/stacks.rs:30`. Cache visible rows per snapshot, query, sort and filter; precompute image use counts per snapshot; stop re-sorting events each frame. Done when (H): `examples/perf.rs` holds its budget with 1,000 rows (T7). Feeds A8.
- [ ] **A8** (L) One `DataTable` for all five tables. New `ui/data_table.rs`; replaces `ui/table.rs`, `ui/table_cells.rs` and the table code in `ui/views/{images,volumes,networks,events}.rs`. A column spec (key, title, width, alignment, sort key, cell), density from `layout::Density`, selection, striping, hover, the bulk column, and `uniform_list` for virtualisation. Done when (H): the five tables re-render with the same structure, and 5,000 rows fit the frame budget (T7). Closes the virtualised-tables gap in `TODO.md`. Depends A1, A7, U1.
- [ ] **A9** (S) Keep the host sample out of the snapshot. `state.rs:36, 96–104`, `app_engine.rs:138–153`. `HostSample` belongs beside the snapshot, as the plan draws it; stop deep-cloning the snapshot to set it. Done when (G): no `(*app.snapshot).clone()` in the host path.
- [ ] **A10** (S) Track the active engine by identity, not index. `state.rs:109`, `app_actions.rs:214–224`, `app_engine.rs:119–127`, `ui/views/settings.rs:132`, `ui/views/runtimes.rs:140`. Done when (G): a rescan that reorders runtimes keeps the same engine active.
- [ ] **A11** (S) Give the theme an enum. `berth-ui/src/theme.rs:94–226`. Derived colours key on `Theme::Light` or `Theme::Dark`, not on comparing a colour with `dark().background`; make `mix()` blend in linear space, or fix its comment. Done when (M): no such comparison remains.
- [ ] **A12** (M) Unify the engine crate's duplicates. One `Dialect` (`berth-engine/src/prune.rs:6–10` against `discovery.rs:42–47`); one `Mount` and one inspect `HostConfig` (`dto.rs:106–139` against `detail.rs:66–91`); rename `Lifetime` (`detail.rs:104`) to `InspectFacts`. Done when (M): the crate builds with one of each.
- [ ] **A13** (M) Make "Copy run command" reproduce the container. `berth-engine/src/detail.rs:318–337`. Add `--restart`, `--network`, `--user`, `-w`, `--entrypoint`, `--label` and the command, quoted as today. Done when (U): a golden command for the fixture container.
- [ ] **A14** (M) Write settings atomically, report failures, and remember more. `settings.rs:60–66`, `app_actions.rs:202–210, 233–241`, `app_forms.rs`. Write to a temp file and rename; toast on failure; one `save_settings` helper instead of three copies; persist window bounds and position. Done when (U): an interrupted write leaves the old file; (G) the window reopens at its last size.
- [ ] **A15** (S) One source for create-name validation. `app_forms.rs:98–112` against `:342`; `jobs/mutation.rs:204–267`. `submit_create` calls `create_name_error`; `container_body` returns a real error instead of `Err(())`. Done when (U): one validation function.

## Phase 4 — UI completion

Depends on Phases 2 and 3. Take U10 first: it settles what the spec should say
before the views change.

- [ ] **U10** (S) Write the UI decisions into the spec before building them. `docs/PRODUCT-SPEC.md` §2.1, §2.2, §3.4. Four graph cards (recommended) or the merged card; toasts as a floating stack (recommended) or in the rail; confirmations as modal sheets or as the reference's banner. Done when (M): the spec describes the intended end state, and `docs/PARITY-CHECKLIST.md` stops claiming Overview parity early. Feeds U11, U24, D1.
- [ ] **U11** (M) Overview: four graph cards and a host sparkline. `ui/views/overview.rs:74–128`, `ui/views/overview_cards.rs:14–119`, `ui/views/overview_disk.rs`. CPU, Memory, Running and Disk cards from the series `History` already records and nothing draws; a Host card with `HostCpu` and `HostMem`; the spec's card anatomy (tinted chip, coloured label, figure, context note, well); network, disk and PIDs as a strip. Done when (H): `overview-light.png` and `overview-dark.png` match `docs/reference-ui/overview.png` in structure. Depends U6.
- [ ] **U12** (S) Fold the healthy status band into the header. `ui/views/overview.rs:131–250`. When healthy, a dot and "updated HH:MM:SS" beside the subtitle; the full band only for warning, not answering and reading, with a button ("Open Runtimes", "Retry now"). Done when (H): the Overview fits 760 pt without scrolling. Depends U6.
- [ ] **U13** (M) Make a low reading look low, not empty. `ui/graph.rs:30–86`. Draw the reference lattice the Zig app had, label the ceiling ("100%", "of 3.8 GiB") and mark the peak; keep the fixed ceiling. Done when (H): a 5 MiB series on a 3.8 GiB ceiling is legible as near the floor.
- [ ] **U14** (S) Say how partial a partial sample is. `ui/views/overview.rs:32–57`, `ui/views/overview_cards.rs:74–88`. "partial sample · 49 of 50" in place of a dash beside a label. Done when (H): a partial fixture reads correctly. Depends B15.
- [ ] **U15** (S) Render unknown engine facts as `—`. `ui/views/overview_cards.rs:380–421`. An empty driver, root, OS or cgroup string is unknown, not blank. Done when (H): a runtime with `info_read = false` shows no half-empty line.
- [ ] **U16** (M) Show rates on the Overview and label totals in the table. `ui/views/overview_cards.rs:121–158`, `ui/table_cells.rs:125–160`, `berth-core/src/history.rs`. Network and disk as bytes per second from consecutive samples; the table's cumulative columns titled as totals. Done when (U): the rate from two known samples is exact.
- [ ] **U17** (S) Give inspector actions a hierarchy. `ui/inspector.rs:309–429`. Pause or Resume, Restart and Stop as verbs; Terminal, Copy run and Clone under one overflow menu; Delete apart, in danger wash. Done when (H): `inspector-container.png` is regenerated and read. Depends U1, B16.
- [ ] **U18** (S) Make Facts scroll. `ui/inspector.rs:140–307`. A scrollable facts region above a log well with a minimum height. Done when (H): at 960×600 the logs are reachable.
- [ ] **U19** (S) Let long facts be read whole. `ui/inspect_shared.rs:111–135`, `ui/inspect_tabs.rs`. Wrap, or truncate in the middle with a tooltip; click to copy IDs, paths and commands. Done when (H): a long command is fully readable.
- [ ] **U20** (S) Logs: highlight search matches and add a wrap toggle. `ui/inspector.rs:487–521`. Done when (H): a logs screenshot with one highlighted match. Depends B1.
- [ ] **U21** (M) Lighten row actions and collapse them when space is tight. `ui/table_cells.rs:442–588`; the 154 pt actions column in `berth-ui/src/layout.rs`. Ghost-size pills; below Medium density one overflow menu; only state-allowed actions (B16). Done when (H): the 960-wide screenshots with and without the inspector. Depends A8, B16.
- [ ] **U22** (M) Empty and loading states that offer the next step. `ui/table_cells.rs:193–214` and each view's empty state. A button where one helps ("Clear search", "Open Runtimes", "Retry now", "Pull an image"); skeleton rows on the first read; engine errors wrap instead of truncating. Done when (H): empty, warning and first-read fixtures are in the screenshot set (C2). Depends U1.
- [ ] **U23** (M) Show what is in flight. `ui/table_cells.rs`, `ui/inspector.rs`, `ui/views/images.rs`, `ui/views/runtimes.rs`, `ui/sheets.rs`. The affected row says "Stopping…"; a pull reports layers done ("Pulling nginx · 3/8"); a machine op says "Starting lima-berth-docker…"; prune names its step. Only the affected controls are disabled. Done when (G/H): each state has a fixture. Depends A4, E4, B10.
- [ ] **U24** (M) A toast queue that can't shout over itself. `app_forms.rs:323–339`, `app_engine.rs:294–306`, `ui/toast.rs`, `berth-engine/src/events.rs:76–89`. Up to three stacked; repeats coalesced ("web restarted 4× in the last minute"); a routine `exit 0` is a notice, not an error; placed so it doesn't cover the inspector. Done when (G): five notices show three and the rest follow; a crash loop yields one toast. Depends A1, U10.
- [ ] **U25** (M) Complete the keyboard map. `berth-commands/src/keymap.rs:34–58`, `app_actions.rs:96–188`. `⌘,` for Settings; `⌘W` closes the inspector, then the window (today "Window ▸ Close" is bound to `Dismiss`); Enter opens the selected row in every table; Space toggles the bulk checkbox; `⌘[` closes the inspector; Up and Down select in every table. Done when (U): each keystroke resolves to its action through `Keymap::load_bindings` (T5). Depends U9, T5.
- [ ] **U26** (S) Complete the menus. `berth-commands/src/menus.rs:8–28`. berth ▸ About and Settings…; Edit ▸ Undo, Cut, Copy, Paste, Select All; View ▸ Appearance ▸ System, Light, Dark, and Refresh; Window ▸ Minimize, Zoom, Close Inspector; Help ▸ README, Keyboard shortcuts. Done when (M): each item works on macOS. Depends U25.
- [ ] **U27** (S) Fix the narrow rail. `ui/rail.rs:194–270`. The status is a dot only; every icon item has a tooltip with its label and figure; nothing wraps or truncates. Done when (H): the 960×600 screenshots.
- [ ] **U28** (M) Make the Runtimes page answer its own questions. `ui/views/runtimes.rs`, `app_actions.rs:246–260`. One button row for Connect and Rescan; the typed path is validated inline (it exists and answers `/version`) with the error beside the field; a custom socket is listed even when it isn't answering; a "Forget" action; a kind icon and tint per card. Done when (G/H): a bad path shows an inline error and its own card.
- [ ] **U29** (M) Stacks and Events. `ui/views/stacks.rs`, `ui/views/events.rs`. Stacks: a column header row (service, container, image, ports, CPU · memory, state) and a selected state for the open row. Events: type filter chips, Pause, colour by action family (start and create; die, stop and destroy; oom and unhealthy), Clear. Done when (H): both pages, light and dark. Depends U3.
- [ ] **U30** (M) Settings worth returning to. `ui/views/settings.rs`, `settings.rs:28–33`. Terminal app, refresh interval, landing page, launch at login, keep running in the tray; an About card with version and commit, links to the repository and licence, and "Copy diagnostics" (engine kinds and versions, socket paths, OS, GPUI revision). Done when (U): the settings round-trip test covers every new field; (H) the page. Depends A14.
- [ ] **U31** (S) Open on Overview. `app.rs:414–416`, where a comment still says this was the intent. Done when (G): a fresh launch lands on Overview, or on the landing page from U30.
- [ ] **U32** (S) Open a shell in the user's terminal, and reap what you spawn. `terminal.rs:24–68`. Honour the setting from U30 on macOS (today it is always Terminal.app); honour `$TERMINAL`, `$TERM_PROGRAM` or `xdg-terminal-exec` on Linux, and add kitty, alacritty, wezterm and foot; wait on spawned children so none are left as zombies. Done when (V3): one launch on each platform leaves no zombie behind. Depends U30.

## Phase 5 — Engine and scale

No views here, so this can run beside Phases 2–4 in the Engine workstream.

- [ ] **E1** (S) Probe candidates in parallel, as the spec and the plan say. `berth-engine/src/discovery.rs:342–351`. Concurrent probes under one scan deadline. Done when (E): with two of five candidates hanging, the scan returns in about one probe timeout, not two. Depends T2.
- [ ] **E2** (S) Replace static chunks with a bounded stats pool. `jobs/refresh.rs:132–185`. Sixty-four workers pulling from one queue, so a slow request delays only its own worker and a pass doesn't spawn 64 threads. Done when (E): 65 containers where one request is slow and the rest are quick finish in about the slow one's time, not the slow one plus its chunk-mates. Depends T2.
- [ ] **E3** (M) State and bound the pass time at scale. `jobs/refresh.rs:61–124`, `ui/rail.rs:210`, `docs/PRODUCT-SPEC.md` §2.2. A pass costs about `ceil(running / 64) × 2 s`; record that, decide between a higher cap and sampling a subset per pass, and make the rail's "every 5 s" honest (it is nearer 7). Done when (E): pass times for 100, 500 and 1,000 containers are measured on the mock, and the rule is in the spec.
- [ ] **E4** (M) Stream pull progress, and stop it from owning the mutation slot. `berth-engine/src/client.rs:275–289`, `jobs/mutation.rs` (`Request::Pull`), `app_forms.rs:20–40`. Read the progress lines as they arrive (layers done and total); let a pull run beside container actions; when the budget runs out say "still pulling" and confirm from the image list. Done when (E): a trickle of progress lines updates the reported progress; (G) a stop works while a pull runs. Feeds U23.
- [ ] **E5** (M) Run bulk actions concurrently, and let them be cancelled. `jobs/mutation.rs:149–168`, `app_bulk.rs`. Up to eight in flight; a cancel flag checked between items; "Stopped 12 of 30 · cancelled". Done when (E): cancelling after three leaves the rest untouched. Depends T2.
- [ ] **E6** (S) Fetch `/system/df` alongside the lists. `jobs/refresh.rs:67–96`. Start it with the other calls and keep every failure rule. Done when (E): with a slow df the pass takes about the df's time, not the sum. Depends T2.

## Phase 6 — Tests, CI, packaging

- [ ] **T4** (M) Write the mock-engine scenarios `docs/TESTING.md` §4 lists and the code lacks. Each core failure aborts the pass; a df failure keeps the old df; an events failure keeps the ring; a stats failure marks the pass partial; a stale generation is dropped; a second mutation is refused. Done when (E/G): each is a named test. Depends T2. B7 and B8 add the rescan and cursor cases.
- [ ] **T5** (S) Replace the keymap test that only compares constants with themselves. `berth-commands/src/keymap.rs:60–71`. Done when (U): removing a binding fails the test. Feeds U25.
- [ ] **T6** (M) Assert that no table cell is clipped. A headless test in `crates/berth-app` that measures each column's widest value with the real text system, for every density. Done when (H): it fails on a deliberately narrow column; it would have caught B2.
- [ ] **T7** (S) Give the perf example a gate with teeth. `crates/berth-app/examples/perf.rs:28` fails only above two seconds; the measured frame is 13.9 ms. Fail above one frame budget (say 33 ms); measure 100, 500 and 1,000 rows. Done when (H): the example fails when a frame exceeds the budget. Feeds A7, A8.
- [ ] **C1** (S) Run the coverage gate in CI. `.github/workflows/gpui.yml`, `scripts/coverage.sh`. The 85% line gate named in `AGENTS.md` isn't enforced anywhere. Done when (M): a workflow job runs it and publishes the report.
- [ ] **C2** (M) Make the screenshots a gate. `.github/workflows/gpui.yml:73–93`, `crates/berth-app/examples/screenshots.rs`. Today the job only uploads PNGs. Extend the fixtures (empty, warning, first read, partial sample, bulk mode, 100 rows, narrow rail with the inspector, dark sheets and inspector), add T6's assertions and a perceptual diff against `docs/ui/` with a tolerance, and fail the pull request on a difference. Done when (M): a deliberate regression fails CI.
- [ ] **C3** (S) Run the app's tests before a release. `.github/workflows/release.yml:59–84`. The macOS job builds and packages without running clippy or `cargo test -p berth-app`. Done when (M): both run before packaging.
- [ ] **C4** (S) Fix `Info.plist`. `macos/Info.plist:19–20`, `macos/build.sh:22–24`. `CFBundleVersion` is a git SHA; Apple wants up to three integers. Use a build number; add `LSApplicationCategoryType` and `CFBundleDevelopmentRegion`. Done when (M): `plutil -lint` passes and the value is numeric.
- [ ] **C5** (S) Make the Linux package installable. `packaging/linux/berth.desktop`, `scripts/package-linux.sh`, `README.md`. Add `StartupWMClass=dev.berth.app` so the window joins its launcher; ship an `install.sh` for the binary, desktop entry and icon; document the steps. Done when (V3): on Linux the window shows berth's icon and name.

## Phase 7 — Documentation

- [ ] **D1** (S) Bring `docs/PRODUCT-SPEC.md` back in line with the app, using U10's decisions. §2.1 and §3.4: inspector tabs, Overview layout, toast placement, confirmation style, destructive controls. §9.3: the engine-stop sentence (X3). §3.4: the search clear button (X1). Done when (M): no row describes a UI that doesn't exist. Depends U10, U17, U24, X1, X3.
- [ ] **D2** (S) Set `docs/PARITY-BACKLOG.md` to the truth. `:86–122` still says `todo` for nearly every shipped feature, against its own checkpoint at `:63–69`. Give every row its real status and point §5 at this file as the queue. Done when (M): no shipped feature is marked `todo`.
- [ ] **D3** (S) Update `docs/GPUI-REBUILD.md`. `:3` says "M0 in progress"; the milestone boxes at `:221–336` are all unticked, while `:402–412` says M0–M7 are done. Tick what shipped and mark waived gates with a reason (M3's "overview structure matches `overview.png`"). Done when (M): the status line matches the file's own ending.
- [ ] **D4** (S) Settle the version statement. `TODO.md:33` says the version stays `0.0.0` until parity; `Cargo.toml` is `0.1.0`. Say which is right. Done when (M): the two agree.
- [ ] **D5** (S) Rewrite the module docs that still describe the M0 shell. `crates/berth-app/src/lib.rs:1–6`, `app.rs:1–5`, `ui/inspector.rs:1–2`, `berth-core/src/lib.rs:1–2`; the card-title examples in `berth-ui/src/typography.rs:7–8`; "GitHub Docker Engine API" in `berth-engine/src/lib.rs:3`. Done when (M): each describes what the module does now.
- [ ] **D6** (S) Take the personal path out of the fixture. `state.rs:422` embeds `/Users/ascotti/…`, which is visible in the published screenshots. Use a neutral path and regenerate `docs/ui/*.png`. Done when (M): no screenshot shows a real home directory.

## Verify on a live system

The review couldn't settle these from the code alone. Each is a manual check
with a recorded result.

- [ ] **V1** (M) Tray "Open" on a closed window. `crates/berth-app/src/lib.rs:99–112`, `app_engine.rs:40–54`. Closing minimises; Open only activates. On macOS, close the window and choose Open from the tray. If the window stays in the Dock, restore it on Open, or hide instead of minimise if the pinned GPUI allows. Done when (M): the window is back on screen. Closes the first gap in `TODO.md`.
- [ ] **V2** (M) The host split on a vz-backed Lima machine. `host.rs:13–72`. Compare "engine helpers" with Activity Monitor or `ps` for `limactl hostagent` and `qemu-system`, on one vz and one qemu machine, and confirm guest RAM isn't counted under both "containers" and "engine helpers". Done when (M): both sets of numbers are written down, or a fix is filed here.
- [ ] **V3** (M) Linux runtime, on X11 and Wayland. Launch; monospace where it belongs (U8); no tray, so the last window quits; the window joins its launcher (C5); a terminal opens without leaving a zombie (U32). Done when (M): each is recorded. Closes the Linux gap in `TODO.md`.
- [ ] **V4** (M) A screen-reader pass. `docs/ACCESSIBILITY.md` already makes a VoiceOver pass a release gate; do it after U9, and with Orca on Linux. Rail, rows, inspector and sheets are read with a role and a name. Done when (M): findings are filed here.

## Also found while drafting

Not in the review. Each was checked against the baseline while turning the
review into rows. Take each with the phase it names.

- [ ] **X1** (S) Search fields have no clear button, though the spec (§3.4) says one appears when there is text, and `xmark.svg` is bundled for it. `ui/input.rs`, `ui/table.rs:69–100`. Done when (G/H): the button appears only with text and clears the field. Take with Phase 2. Feeds D1, A6.
- [ ] **X2** (M) Text fields don't behave like text fields. `ui/input.rs`. A click always puts the caret at the end (`:169`); there is no drag-selection; long text doesn't scroll to the caret; there is no undo; the IME candidate position is guessed from an average character width (`:397`). Use the text system's real glyph positions. Done when (G): a click mid-word puts the caret there; a drag selects. Take with Phase 2.
- [ ] **X3** (S) Confirmations should name what they cost, as the spec asks (§2.2). `ui/sheets.rs:65–126`. Engine stop: "N running containers on <name> will stop with the engine"; image delete: "used by N containers"; container delete: running (so force-stopped) and its stack. Done when (H): the three sheets. Take with Phase 4. Feeds D1.
- [ ] **X4** (S) Image inspector: "Run…" is offered for dangling images and fills the form with the word `untagged`; an image with several tags shows only its first. `ui/inspect_objects.rs:139–151`, `berth-engine/src/dto.rs:301`. Done when (G): no Run… on a dangling image; every tag is visible. Take with Phase 4.
- [ ] **X5** (M) Raw Inspect for images, volumes and networks, and the facts people look up. `ui/inspect_objects.rs`, `berth-engine/src/dto.rs:347`. Only containers have an Inspect tab; the network inspector shows no subnet or gateway. Done when (H): each of the four inspectors has an Inspect tab, and a network shows its subnet. Take with Phase 4. Depends B13.
- [ ] **X6** (S) Form validation polish. `berth-core/src/image_ref.rs`, `validation.rs`. `KEY=` is rejected though Docker accepts it; `8080:80,` produces "Port  needs…" with nothing named; `nginx::` and `NGINX` pass validation and fail at the engine. Done when (U): each case is a test. Take with Phase 1.
- [ ] **X7** (S) The prune sheet should count as well as weigh. `ui/sheets.rs:182–319`. "Stopped containers · 3 · 1.2 MiB" instead of "0 B"; Clean up disabled when nothing is selected. Done when (H): the sheet. Take with Phase 4. Depends U1.
- [ ] **X8** (S) "Copy .env" writes the literal text `<redacted>` as a value. `ui/inspect_tabs.rs:228–241`. Leave those keys empty and say so in the toast. Done when (G): the copied text contains no `<redacted>`. Take with Phase 4.
- [ ] **X9** (S) Truncated lists should say so. `ui/inspect_tabs.rs:368, 446`, `jobs/detail.rs:57`. Processes stop at 40 and layer changes at 80, silently. Done when (H): "showing 40 of 112". Take with Phase 4.
- [ ] **X10** (S) The terminal always runs `sh`. `terminal.rs:15–22`. Fall back to `bash`, or let the shell be chosen. Done when (U): the command builder honours a shell argument. Take with U32.
- [ ] **X11** (M) Optional: discover Docker contexts. `berth-engine/src/discovery.rs:77–81` reads a `unix://` `DOCKER_HOST` and the known paths; it doesn't read `~/.docker/contexts`, and says nothing about a `tcp://` host. Done when (U): a context's socket is a candidate. Take with Phase 5.
- [ ] **X12** (S) Optional: let the Disk page's figures lead somewhere. `ui/views/disk.rs:36–47`. Each store links to its view, and the page shows the disk history that is recorded but not drawn. Done when (H): the page. Take with Phase 4. Depends U11.

## Carried over from `TODO.md`

These stay where they are; this is only where they meet this list.

| `TODO.md` gap | Meets |
|---|---|
| Reopen a closed window from the tray | V1 |
| Feature-gated test driver and xa11y E2E | T3 is the in-process stand-in; `tests/e2e/README.md` is unchanged |
| Linux X11/Wayland smoke | V3 |
| Virtualised tables | A8 |
| Version until parity | D4 |
| Windows compile-only; DMG and notarization | Out of scope, as before |

## Not on this list

Checked during the review and found correct, so nobody needs to look again:

- Search is case-insensitive in every table.
- The sheet backdrop does cover the rail.
- The rail figures are upright; the italic look was an artifact of downscaling.
- The engine chooser is in Settings whenever more than one engine is reachable.

Still deliberate non-goals, as in `docs/GPUI-REBUILD.md` §6: Compose `up` and
`down`, streaming logs, an embedded terminal, Notification Centre, DMG and
notarization, Windows.
