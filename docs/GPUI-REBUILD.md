# berth on Rust + GPUI — rebuild plan

**Status:** M0 in progress
**Started:** 2026-11-07
**Source reference:** the unreleased Zig 0.17 / nanoui app (kept outside this
repository at `../code/berth` until parity)
**Process reference:** `../pilcrow` — the same product-preserving rewrite to
Rust + GPUI. Its workspace, pinned GPUI policy, test layers, unsigned release
policy and documentation split are the template here.
**Audience:** the integration lead and any sub-agent picking up a workstream.

---

## 0. Short version

berth is a native container monitor for Docker, Podman, Colima and Lima. It
reads the Docker Engine API over a Unix socket and turns it into one window:
what is running, what it costs, and what can be cleaned up. Two hard
constraints survive the rewrite:

1. **The window never waits on the engine.** A worker builds a complete
   immutable snapshot; the previous one stays on screen until one commit. Tables
   never blank, graphs never freeze, and a slow registry never blocks a frame.
2. **The numbers are honest.** Unknown is an em dash, not zero. Container CPU
   uses the two-sample window `docker stats` uses. Memory is a working set. A
   partial sample says "partial sample". Graphs use fixed ceilings, not
   auto-scaling.

This repository is a **new app**, not a migration. The previous product was
never released, so there is no legacy data to read and no compatibility shim.
We keep the berth name and the behavior/visual contract; everything else is
rebuilt.

**Confirmed decisions**

| Decision | Answer |
|---|---|
| Repository | This folder (`git_repos/berth`), fresh history |
| Runtime | Rust 1.98, edition 2024, GPUI pinned to one Zed commit |
| Runtime distribution | `gpui` + `gpui_platform` from Zed git, same rev (see `docs/GPUI-VERSION.md`) |
| Platforms | **macOS primary, Linux first-class.** Windows compiles only; not a v1 target. |
| Migration | None. Never read or modify the Zig app's settings or data. |
| Signing | Off by default (`BERTH_SIGN=0`): no codesign, no notarization, no keychain prompt. Git commit signing is disabled locally. |
| Visual bar | Same tokens and design language; not pixel-identical. Text shaping and SVG icons will differ slightly. |
| Accessibility | From day one, using the pinned GPUI's AccessKit bridge. Not a later polish item. |
| Testing | Layered: unit, GPUI in-process, headless render, real-window E2E, live-engine smoke. See `docs/TESTING.md`. |
| Work mode | Parallel sub-agents per crate; the parity backlog is the free-time queue. |

---

## 1. What must survive

The full behavior and visual contract is `docs/PRODUCT-SPEC.md`. The short
version:

### 1.1 Views

Overview · Containers · Images · Volumes · Networks · Events · Stacks · Disk ·
Runtimes · Settings. Five searchable/sortable tables. Four inspector kinds
(container, image, volume, network). One modal sheet at a time.

### 1.2 Refresh contract

- One pass every 5 s, re-armed from the **end** of the previous pass.
- Pass runs on the background executor and returns a complete `Snapshot`.
  Commit is one UI-thread step; the previous snapshot stays until then.
- Core failure aborts the pass and keeps the old snapshot. `/system/df` fails
  independently and keeps its last good document with age/error. A failed
  stats sample marks the pass partial. A failed `/events` keeps the old ring.
- `/system/df` has a ~30 s cadence (six passes).
- Stats fan out across all running containers, capped at 64 concurrent
  requests. Never use `one-shot=true`.
- Selection is re-resolved by id after every commit.
- A click during a pass requests the next pass immediately.

### 1.3 Actions

One mutation at a time: container action, bulk batch, pull/run, prune, engine
machine command. A second request is refused with a toast, not queued. Every
completion refreshes and then resumes any queued switch/rescan/pass.

Container start/stop/restart/pause/resume/delete · bulk start/stop/delete ·
Compose Start all/Stop all · image pull/run/delete · volume/network
create/remove · prune stopped containers, unused images/volumes/networks/build
cache · Colima/Lima/Podman machine start/stop/restart · terminal · open
published port · copy run command with best-effort redaction.

### 1.4 Data honesty

- Unknown is `—`.
- Memory is `usage − inactive_file` (`total_inactive_file`, then `cache`).
- Parts sum with saturation; no wrap.
- List answers capped at 5,000 per store; JSON depth capped at 64. A refusal is
  an availability message, not an empty store.
- Ports deduplicated on public/private pair.
- `<none>:<none>` → "untagged"; `null` driver → `—`.

### 1.5 Visual identity

Warm paper/charcoal, burnt amber selection, sage and rust state. Rail 200 pt
(88 pt below 1120), inspector 430/360, main padding 20/16/12, 44 pt table rows,
10 pt card radius, 4 pt cell-graph dots. Exact tokens and geometry are in
`docs/PRODUCT-SPEC.md`; reference screenshots are in `docs/reference-ui/`.

---

## 2. Target workspace

The split follows pilcrow: pure logic and IO never depend on GPUI, so they stay
testable without a display.

```
berth/
├─ Cargo.toml                     # workspace, pinned GPUI
├─ rust-toolchain.toml            # 1.98.0
├─ docs/
│   ├─ GPUI-REBUILD.md            # this file
│   ├─ PRODUCT-SPEC.md            # behavior + visual contract
│   ├─ GPUI-VERSION.md            # pin + upgrade policy
│   ├─ TESTING.md                 # L0–L6 layers, driver, E2E
│   ├─ PARITY-BACKLOG.md          # definition of parity + free-time queue
│   ├─ RELEASE.md                 # unsigned build policy
│   └─ reference-ui/              # Zig app screenshots (visual oracle)
├─ crates/
│   ├─ berth-core/                # pure: format, stats math, history, projects,
│   │                             # network counts, image refs, validation
│   ├─ berth-engine/              # pure IO: UDS HTTP, DTOs, discovery, prune,
│   │                             # df, events — no GPUI
│   ├─ berth-ui/                  # tokens, geometry, typography, table specs
│   ├─ berth-commands/            # typed actions, keymaps, menus
│   └─ berth-app/                 # GPUI views, state, jobs, tray, main
├─ assets/icons/                  # bundled SVGs (Lucide/Tabler)
├─ tests/e2e/                     # xa11y + test-driver (later)
├─ tools/                         # line rule, coverage helpers
├─ scripts/                       # coverage, screenshots, live smoke
├─ macos/                         # unsigned .app bundle
└─ packaging/linux/               # .desktop + icon
```

### 2.1 Crate rules

- `berth-core` has **no** GPUI, no filesystem, no network. It is pure data and
  math.
- `berth-engine` owns sockets, HTTP, JSON and process commands. It depends on
  `berth-core`, never on GPUI.
- `berth-ui` owns only numbers and colors; it paints no views. This keeps the
  tokens testable and stops a widget from inventing a spacing value.
- `berth-commands` owns action types, platform keymaps and the menu model.
- `berth-app` is the only crate that knows GPUI views, entities, tasks and the
  window.

### 2.2 State and jobs

```
UI thread (GPUI)                              background executor
----------------                              -------------------
BerthApp (Entity)
 ├─ Snapshot (Arc<Snapshot>)  <── commit ───  refresh::load(socket, gen, df_due)
 ├─ EngineState                                ├─ probe / version / info
 │   ├─ runtimes, active, socket               ├─ /containers/json
 │   └─ generation                             ├─ stats fan-out (≤ 64)
 ├─ ViewState                                  ├─ /images /volumes /networks
 │   ├─ tab, filter, searches, sorts           ├─ /events interval
 │   ├─ selection + detail generation          └─ /system/df (cadence)
 │   └─ bulk, sheets, toasts
 ├─ History (120 samples)
 └─ HostSample
        ▲
        └── commit → re-resolve selection → merge events → push history
                    → update tray → arm the 5 s timer
```

Rules:

- Background work never touches `BerthApp`. It returns owned data; the UI
  thread applies it.
- Every completion carries a generation token. Stale discovery, refresh, detail,
  mutation and image-task completions are dropped.
- `Task` handles live in the owning struct; dropping one cancels it.
- `Arc<Snapshot>` replaces the Zig arena rotation. No manual retirement, no
  use-after-free window.
- Selection is `(DetailKind, id, detail_generation)`; detail/logs/inspect
  completions must match all three plus their own clock/timestamps flag.
- The mutation slot is an enum (`Idle | Running(kind)`), not parallel booleans.

### 2.3 GPUI specifics

- `gpui_platform::application()` is the entry point, not crates.io
  `Application::new()`.
- Use `uniform_list` for tables (44 pt containers/images/volumes/networks,
  34 pt events). Give every stateful row a unique `.id(...)`.
- Draw cell graphs and proportional bars with `canvas` and paint bounds; do not
  create one element per dot.
- Use `Window::appearance()` + `observe_window_appearance`; no window
  recreation on theme change.
- Use `overflow_y_scroll` for panel pages and the log well; move the scroll
  handle to the end for follow.
- Text fields are ours: implement an `InputState`/`TextInput` entity on
  `EntityInputHandler`. GPUI has no ready-made input widget at this pin.
- Icons: bundled SVG through an `AssetSource`; GPUI cannot use SF Symbols.
- Tray: GPUI has no tray API. Use `tray-icon` or `objc2` NSStatusItem in M6.
- Accessibility: publish roles/labels with GPUI's AccessKit methods from the
  start. The pinned commit includes them.

---

## 3. Milestones

Each milestone ends with all gates green:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p berth-app          # the touched view, looked at
```

Plus, where relevant: live-engine smoke and a screenshot read as an image.
Do not start a milestone on a red gate.

### M0 — Spike and scaffold (3–5 days)

- [x] Workspace, rust-toolchain, pinned GPUI, workspace crates.
- [x] `berth-ui` tokens + geometry + contrast tests.
- [x] Static shell: rail, page header, containers table, two themes.
- [x] Headless render: `Window::render_to_image()` → PNG at 960/1280/1440,
      light and dark (`cargo run -p berth-app --example screenshots`).
- [ ] Text input spike: focus, caret, selection, clipboard, IME, Cmd/Ctrl+F.
- [ ] UDS transport spike: `/version` + `/containers/json`, chunked bodies,
      total deadline.
- [ ] Real-window E2E spike: xa11y attaches, table row is discoverable.
- [ ] Decide icon family and whether to bundle a font; record it.
- **Gate:** a static but real berth window, a text field that behaves, a live
  `/version` read, and a headless screenshot. *(Static shell + headless
  screenshot are green; input and transport are next.)*

### M1 — Engine and domain core (1–1.5 weeks)

- [ ] `berth-engine::transport`: blocking Unix socket, HTTP/1.1, chunked,
      header case, content-length, total deadline (5 s general, 3 s df/probe),
      8 MiB body cap.
- [ ] `client`: API prefix/version negotiation, engine error unwrapping, 304
      handling, every route in `docs/PRODUCT-SPEC.md` §9.2.
- [ ] Serde DTOs: Version, Info, Container, Image, Volume, Network, Stats, Df,
      ContainerDetail, Event. All fields defaulted; unknown fields ignored.
- [ ] Port pure logic + tests: stats math, guards, format, history, projects,
      network counts, image refs, validation, lifetime, prune, df, settings.
- [ ] Mock UDS engine test server (success, engine error, trickle, truncated,
      malformed, chunked).
- [ ] Discovery: candidates, canonicalisation, parallel probe, kind/dialect,
      Podman identity, dedupe, retry.
- **Gate:** engine-free tests green; a live engine produces a typed snapshot.

### M2 — Shell, theme, input, tables (1.5 weeks)

- [ ] Production `TextInput` entity (focus, caret, selection, clipboard, IME).
- [ ] Shell: 200/88 rail, page frame, inspector slot, sheet layer, status rail,
      toasts.
- [ ] Widgets: page header, sortable header, cells, pills, chips, buttons,
      cards, empty/loading/error, legend, stacked bar.
- [ ] Tables: exact density tiers/columns, `uniform_list`, hover/selected/
      alternating, bulk checkbox slot.
- [ ] Containers (fully interactive read-only), Images, Volumes, Networks.
- [ ] Keyboard: Cmd/Ctrl+1…9, Cmd/Ctrl+F, Esc.
- **Gate:** four table views on real data at 960/1280/1440, both themes;
  screenshots compared with `docs/reference-ui/`.

### M3 — Panels (1 week)

- [ ] Cell graph + proportional bar via `canvas`, fixed honest scales.
- [ ] Overview: four graph cards, HOST three-way split, engine facts, disk
      breakdown + legend.
- [ ] Disk: four stores, bar + legend, explanation, Clean up hook.
- [ ] Runtimes: custom socket field, kind cards, expandable sockets, engine ops
      UI.
- [ ] Settings: About, Appearance, Engine chooser, Maintenance.
- [ ] Host sampling: macOS Mach/Linux `/proc` (+ engine RSS) verified against
      `vm_stat`/`free`/`ps`.
- **Gate:** overview structure matches `overview.png`, including partial-sample
  language; host figures verified.

### M4 — Inspector, logs, actions (1–1.5 weeks)

- [ ] Detail jobs: lifetime/logs/inspect with generation/id/timestamp guards.
- [ ] Inspector Facts for four kinds, port pills, copy run command + redaction,
      Inspect JSON tab.
- [ ] Logs: 500-line tail, Follow, Times, search, Copy, follow-end scroll,
      stale-clock rejection.
- [ ] Mutation slot: start/stop/restart/pause/resume/delete + confirmation
      sheets + completion refresh.
- [ ] Terminal launch and open-published-port, quoting/validation.
- [ ] Toasts, error/success text variants.
- **Gate:** live engine start/stop/delete verified before/after; logs and
  inspect read live; inspector screenshot reviewed.

### M5 — Secondary features (1–1.5 weeks)

- [ ] Events feed: interval poll, merge/dedupe ring, `since` cursor, exit/OOM/
      restart toasts, search.
- [ ] Stacks: Compose grouping, members, Start all/Stop all.
- [ ] Bulk: select shown/all/none, batched start/stop/delete, cancel, partial
      results, confirm.
- [ ] Images creation card: pull, run (image/name/ports/volumes/env), inline
      validation.
- [ ] Volume/network create sheets.
- [ ] Prune sheet: reclaimable sizes, Docker/Podman route differences, build
      cache hidden on Podman.
- [ ] Engine machine ops: Colima/Lima/Podman start/stop/restart.
- **Gate:** each action verified on a disposable live engine; prune verified
  with disposable resources.

### M6 — Orchestration, persistence, tray, polish (1 week)

- [ ] Discovery lifecycle: startup scan off-thread, adoption, identity compare,
      rescan queue, retry, bounded teardown.
- [ ] Refresh lifecycle: 5 s re-arm, df cadence, events interval, partial
      failure rules, queued refresh.
- [ ] Settings persistence at the existing paths; load before window creation.
- [ ] Shortcuts/focus rules across sheets; backdrop blocks page input.
- [ ] Tray: macOS status item with "N up · CPU · mem", Open/Quit, reopen at last
      size.
- [ ] Performance pass with a 100-container fixture.
- **Gate:** kill/restart the engine mid-refresh; the window stays responsive and
  recovers; live theme switch keeps selection.

### M7 — Packaging and parity (1 week)

- [ ] `macos/build.sh`: release build, `.app`, Info.plist, icon, unsigned by
      default.
- [ ] Linux `.desktop`, icon, dependency notes, X11/Wayland smoke.
- [ ] CI: fmt, clippy, test, build, headless screenshots; macOS + Linux.
- [ ] Screenshot every view both themes at 960/1280/1440 into `docs/ui/`;
      review them.
- [ ] Parity checklist against `docs/PRODUCT-SPEC.md`; file every deviation.
- [ ] README, release notes, final gate.
- **Gate:** feature parity, green CI, no known crash, documented gaps.

---

## 4. Workstreams for parallel agents

Pilcrow runs sub-agents per crate. berth should do the same once M0 passes.

| Workstream | Owns | Must not touch |
|---|---|---|
| **Core** | `berth-core`: format, stats math, history, projects, validation, image refs | GPUI, sockets |
| **Engine** | `berth-engine`: transport, DTOs, client, discovery, prune, events, df | views |
| **UI** | `berth-ui`, shell, widgets, input, tables, graphs | engine internals |
| **Views** | `ui/views/*`, inspector, sheets | transport |
| **Quality** | tests, mock engine, xa11y, screenshot harness, CI | product decisions |

Interface contracts are written down in `docs/TESTING.md` and the crate `lib.rs`
docs before work starts. A sub-agent adds the lowest-layer test that can catch
its characteristic bug.

---

## 5. What we deliberately do not port

- Zig arenas and the retired-arena rotation (Rust ownership + `Arc` removes it).
- The 600-line-per-file workaround as a build gate. Keep modules small, but let
  `rustfmt` and review decide; `tools/lines.py` is a warning.
- nanoui's deferred `end()` layout traps.
- C shims and manual Objective-C for things GPUI/platform crates cover.
- The old app's settings, drafts or any legacy data. This is a new app.

## 6. Deliberate non-goals (same as the old app)

Compose `up`/`down` through the Compose CLI · true streaming `docker logs
--follow` · embedded terminal output · macOS Notification Centre · DMG,
Developer ID signing, notarization · Windows support.

## 7. First week

1. **Day 1** — workspace, pinned GPUI, hello window, tokens.
2. **Day 2** — rail + page header + static containers table; headless PNG.
3. **Day 3** — text input spike; decide hand-built vs library.
4. **Day 4** — UDS `/version` + `/containers/json`; render real rows.
5. **Day 5** — xa11y attach + one real-window click; commit the decisions.

Do not start with graphs or the inspector. The table + input + transport path
forces every foundational decision through one visible screen.

## 8. Reference points in pilcrow

| Need | Pilcrow file |
|---|---|
| Workspace + pinned GPUI | `Cargo.toml`, `docs/GPUI-VERSION.md` |
| App entry, window creation, appearance observer | `crates/pilcrow-app/src/lib.rs` |
| Tokens + contrast tests | `crates/pilcrow-ui/src/theme.rs` |
| Geometry + tests | `crates/pilcrow-ui/src/page.rs` |
| Action/keymap/menu split | `crates/pilcrow-commands/src/` |
| GPUI in-process tests | `crates/pilcrow-app/src/tests.rs` |
| Test-driver feature | `crates/pilcrow-app/src/test_driver.rs` |
| Test strategy | `docs/TESTING.md` |
| Unsigned release policy | `docs/RELEASE.md`, `macos/build.sh` |
| CI matrix | `.github/workflows/gpui.yml` |


---

## Status 2026-11-07

M0–M7 are implemented in this repository. The verified parity matrix and the
honest remaining gaps are in `docs/PARITY-CHECKLIST.md`:

- live Docker 29.9.2 probe and disposable action smoke pass;
- 100-container frame is 13.9 ms;
- full screenshot matrix is in `docs/ui/`;
- macOS `.app` builds unsigned (`target/Berth.app`);
- the tray title/menu exists, but reopening a closed window is still a gap;
- automated xa11y/test-driver E2E and a Linux runtime smoke remain.
