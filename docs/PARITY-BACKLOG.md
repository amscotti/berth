# berth parity and free-time backlog

**Status:** living document
**Updated:** 2026-11-07
**Purpose:** know exactly what "parity" means, what is done, and the next small
piece of work when there is an hour — not a week.

Related: `docs/GPUI-REBUILD.md` (plan), `docs/PRODUCT-SPEC.md` (behavior and
visual contract), `docs/TESTING.md` (how each item is proven),
`docs/GPUI-VERSION.md` (pin).

---

## 1. How to use this

- Work top-down unless a dependency says otherwise.
- Every item names its **test level**: U unit, E engine/mock, G GPUI
  in-process, H headless render, X real-window E2E, L live engine, M manual.
- Sizes: **S** an evening, **M** a day or two, **L** a week (split before
  starting).
- A feature is not done until its tests pass at the stated level and this row is
  updated.
- Keep the Zig app runnable outside this repo as the behavior oracle until the
  Rust app passes the parity gate.

---

## 2. Definition of parity

A person can switch from the Zig app to the Rust app and monitor the same
engine without missing a feature:

1. Launch and see the last good snapshot without blanking; the rail says which
   engine and when it last updated.
2. Read the overview: container CPU/memory, running count, disk, host
   three-way split, engine facts, store breakdown.
3. Browse, search, sort and filter containers, images, volumes, networks and
   events.
4. Open an inspector for any container/image/volume/network; read facts, logs
   (follow/times/search/copy) and raw inspect JSON.
5. Start, stop, restart, pause, resume and delete a container; bulk start/stop/
   delete; Compose Start all/Stop all.
6. Pull and run an image; create a volume/network; remove one.
7. Prune stopped containers, unused images/volumes/networks and build cache,
   with reclaimable sizes first.
8. Start/stop/restart Colima, Lima or Podman machines.
9. Open a container's shell in the user's terminal; open a published port;
   copy the run command with redaction.
10. See exits, OOMs and restarts in Events and as toasts.
11. System/Light/Dark appearance, remembered between launches; last socket
    remembered.
12. The macOS tray shows "N up · CPU · mem" and reopens the window.
13. Keyboard: Cmd/Ctrl+1…9, Cmd/Ctrl+F, Esc, Enter.
14. Honest unknowns: `—`, partial-sample language, no invented zeros.
15. Accessibility: VoiceOver/Orca can read the chrome and the tables; every
    control has a role and name.

Not required: Compose `up`/`down`, streaming logs, embedded terminal,
Notification Centre, DMG/notarization, Windows.

---

## 2.2 Current checkpoint (2026-11-07)

Everything in §4 through M7 is implemented and verified except the items in
`docs/PARITY-CHECKLIST.md` "Known gaps": tray reopen after window close,
automated xa11y/test-driver E2E, Linux runtime smoke, and `uniform_list`
virtualization. The live Docker 29.9.2 probe, disposable action smoke, 100-
container perf frame and screenshot matrix are green.

## 3. Status legend

| Status | Meaning |
|---|---|
| `todo` | Not started |
| `wip` | In progress |
| `blocked` | Waiting on a decision/interface |
| `tested` | Implemented; tests pass at the stated level |
| `parity` | Matches the Zig behavior; covered by E2E/live |
| `dropped` | Deliberately not planned |

---

## 4. Parity matrix

| Feature | Zig reference | Rust status | Milestone | Tests |
|---|---|---|---|---|
| Workspace, pinned GPUI, unsigned build | — | `tested` | M0 | U |
| Theme tokens + contrast | `ui/theme.zig` | `tested` | M0 | U |
| Window + rail + page frame | `ui/shell.zig` | `tested` | M0/M2 | G/H |
| Static containers table | `views/lists.zig` | `tested` | M0 | H |
| Text input / search | nanoui field | `todo` | M0/M2 | G/X |
| Unix HTTP transport | `engine/http_uds.zig` | `todo` | M1 | U/E |
| API DTOs | `engine/api.zig` | `todo` | M1 | U/E |
| Stats math | `stats_pool.zig` | `todo` | M1 | U |
| Guards (5k/64-depth) | `engine/guards.zig` | `todo` | M1 | U/E |
| Discovery | `engine/discovery.zig` | `todo` | M1 | U/E/L |
| Refresh commit/failure | `refresh.zig` | `todo` | M6 | E/G/X/L |
| Overview graphs | `ui/views/panels.zig` | `todo` | M3 | U/H |
| Host sampling | `host/*` | `todo` | M3 | U/L |
| Disk page | `panels.zig` | `todo` | M3 | H/X |
| Runtimes page | `views/config.zig` | `todo` | M3 | G/H |
| Settings page | `views/config.zig` | `todo` | M3/M6 | G/H |
| Inspector facts | `ui/detail.zig` | `todo` | M4 | G/H/X |
| Logs | `logs.zig` | `todo` | M4 | E/G/X |
| Inspect JSON | `detail_job.zig` | `todo` | M4 | E/G |
| Container actions | `actions.zig` | `todo` | M4 | E/G/L |
| Terminal / open port | `terminal.zig`, `app.zig` | `todo` | M4 | U/X |
| Toasts | `ui/shell.zig` | `todo` | M4 | G |
| Events feed | `events_feed.zig` | `todo` | M5 | U/E/G |
| Stacks | `views/stacks.zig` | `todo` | M5 | U/G |
| Bulk | `bulk*.zig` | `todo` | M5 | E/G/L |
| Image pull/run | `image_tasks.zig` | `todo` | M5 | E/G/L |
| Volume/network create | `create.zig` | `todo` | M5 | E/G/L |
| Prune | `cleanup.zig`, `engine/prune.zig` | `todo` | M5 | E/G/L |
| Engine machine ops | `ops.zig` | `todo` | M5 | E/G/L |
| Persistence | `settings.zig` | `todo` | M6 | U/G |
| Shortcuts | `shortcuts.zig` | `todo` | M2/M6 | G/X |
| Tray | `tray.zig` | `todo` | M6 | X/M |
| Accessibility | nanoui roles | `todo` | M2/M6 | X/M |
| Headless screenshots | `docs/ui/` | `tested` | M0 | H |
| Packaging | `macos/build.sh` | `todo` | M7 | M |

---

## 5. Free-time queue

Work in this order. Each item leaves the repo green.

### Phase A — M0 foundation

1. **S** Pin GPUI; hello window at 1280×760 with tokens.
2. **S** Rail + page header + static table; headless PNG.
3. **S** `TextInput` spike: focus, caret, type, select, paste.
4. **S** UDS `/version` + `/containers/json` with a total deadline.
5. **S** Render live rows instead of the fixture.
6. **S** xa11y attaches; one row/button is discoverable.

### Phase B — M1 engine

7. **M** Transport parser + tests (chunked, truncation, trickle).
8. **M** DTOs + serde defaults; parse a captured live snapshot.
9. **S** `format` + `history` + contrast tests.
10. **M** Mock engine server; refresh success/failure scenarios.
11. **M** Discovery candidates + probe + identity rules.
12. **S** Guards: depth 64, 5,000-entry caps.

### Phase C — M2 shell and tables

13. **M** Production `TextInput` with IME.
14. **M** Shell chrome: rail states, status, toasts, sheet layer.
15. **M** Table primitives: header sort, cells, pills, density.
16. **M** Containers view wired to live data.
17. **S** Images/Volumes/Networks views.
18. **S** Keyboard map + focus.

### Phase D — M3 panels

19. **M** Cell graph canvas + fixed scales.
20. **M** Overview cards + host bar + engine card.
21. **S** Disk page + legend.
22. **M** Runtimes page + socket field.
23. **S** Settings page + appearance persistence.
24. **M** Host sampler verified against OS tools.

### Phase E — M4 inspector/actions

25. **M** Detail jobs + guards.
26. **M** Inspector facts for four kinds.
27. **M** Logs pane.
28. **M** Mutation slot + confirmations.
29. **S** Terminal + open port + copy run command.
30. **S** Live action verification.

### Phase F — M5 secondary

31. **M** Events feed.
32. **M** Stacks + bulk.
33. **M** Pull/run card.
34. **S** Create volume/network.
35. **M** Prune sheet.
36. **M** Engine machine ops.

### Phase G — M6/M7

37. **M** Discovery/refresh orchestration and queued work.
38. **S** Shortcuts/focus/sheets.
39. **M** Tray.
40. **M** Performance pass (100 containers).
41. **S** Packaging script.
42. **M** Screenshot matrix + parity review.
43. **M** CI + release notes.
