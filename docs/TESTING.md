# berth testing strategy

**Status:** proposal
**Date:** 2026-11-07
**Companions:** `docs/GPUI-REBUILD.md`, `docs/GPUI-VERSION.md`,
`docs/PARITY-BACKLOG.md`

---

## 1. Why this exists

berth's expensive bugs are not arithmetic bugs. They are interaction and data
honesty bugs:

- a table blanks for one frame during refresh;
- a stale stats sample is shown as a current zero;
- an action starts a refresh that retires the snapshot the action belongs to;
- a detail pane shows the previous container's logs for one frame;
- a prune sheet offers bytes it will not actually reclaim;
- the engine is down and the page says "0 containers" instead of "not
  answering".

Unit tests alone do not catch these. Neither do screenshots alone. The strategy
is layered, and every feature must have a test at the lowest layer that can
catch its characteristic failure.

---

## 2. Layers

| Layer | Tool | Speed | Where | Catches |
|---|---|---|---|---|
| **L0 static** | `cargo fmt`, `clippy -D warnings` | seconds | every change | broken code, obvious lint |
| **L1 unit** | `#[test]` in `berth-core` / `berth-engine` | ms | every change | format, stats, guards, discovery choices, prune routes, validation |
| **L2 engine integration** | mock Unix-socket server | ms | every change | HTTP framing, JSON shapes, failure rules, action routes |
| **L3 GPUI in-process** | `#[gpui::test]`, `TestAppContext` | ms–s | every change | state transitions, clicking, sorting, focus, sheets |
| **L4 headless render** | `HeadlessAppContext`, `Window::render_to_image()` | s | PR/nightly | layout, pixels, overflow, density tiers, themes |
| **L5 real-window E2E** | xa11y + test driver | s–min | PR smoke/nightly | launch, real input, a11y tree, dialogs, visual truth |
| **L6 live engine** | disposable Docker/Podman | min | nightly/manual | real API compatibility, real actions, prune, stats |

Rule: **a feature is done when its characteristic bug has a test**, not when it
looks right once.

---

## 3. L1 — unit

Pure, no display, no engine.

### `berth-core`

- `format`: bytes, percent, duration, ago, clock; pluralisation; em dash for
  unknown.
- `stats`: CPU delta and per-core fallback, missing previous sample,
  working-set memory fallbacks, net/disk totals, saturating sums, "all sampled".
- `history`: 120-sample ring, unknown holds last known, peak/ceiling math.
- `projects`: Compose labels, group order, running counts.
- `network_counts`: attached containers per network.
- `image_ref`: valid references, bare name → `:latest`, tag/digest split.
- `validation`: pull/run/create names, ports, volumes, env.
- `lifetime`: uptime parsing, started-at, restarts.
- `prune`: route selection, Docker vs Podman filters, reclaim accumulation.
- `df`: old/new shapes, shares, reclaimable by store.

### `berth-engine`

- `transport`: request/response framing, chunked, header case, content-length
  truncation, unsupported coding, malformed chunks, total deadline against a
  silent listener and a trickling one.
- `guards`: depth > 64 refused, list > 5,000 refused, refusal text is
  "availability limit", not an empty store.
- `discovery`: candidate order, preferred socket first, canonical paths,
  Docker id vs Podman socket identity, dialect change, rescan re-arm.
- `client`: API prefix, version adoption, engine message unwrapping, 304.

### Properties / fuzz

- JSON parsers never panic on truncated or hostile input.
- Format never produces `NaN`, `inf`, or a negative byte count.
- Stats sums never wrap.
- Depth/list guards are monotonic and bounded.

---

## 4. L2 — mock engine

A test-only Unix-socket server that speaks scripted HTTP/1.1. It is how the
engine layer is tested without Docker.

Capabilities:

- respond by method + path with a queue of bodies/statuses;
- chunked, fixed-length, slow trickle, close-mid-body, malformed chunk;
- record requests (method, path, headers, body) for assertions;
- hold a request open to test cancellation/timeouts.

Required scenarios:

- refresh success; each core failure; df failure keeps old df; stats partial;
  events failure keeps ring; ping failure drops client.
- snapshot replacement: old selection re-resolved by id; old snapshot is not
  mutated.
- stats fan-out: N running containers sampled concurrently, capped at 64.
- detail: lifetime/logs/inspect success; stale generation; stale id; stale
  timestamp flag.
- actions: one-shot success; engine error; refused second mutation; completion
  refresh; prune accumulation when a middle step fails.
- discovery: probe list, unreachable socket, identity change.

---

## 5. L3 — GPUI in-process

`#[gpui::test]` with `TestAppContext`, `VisualTestContext` when a window is
needed.

Cover:

- tab switching; selection; sort key flip; search filtering; filter chips.
- row click opens the inspector; close clears selection.
- confirmation sheet: Esc cancels, Enter confirms, backdrop blocks the page.
- bulk mode: select shown/all/none; count in the bar; confirm.
- create form: Enter submits; invalid name shows the sentence beside the field.
- text input: typing, selection, copy/paste, Cmd/Ctrl+F focus, IME composition.
- refresh state machine: no overlapping passes; queued click; generation drop.
- action slot: second mutation refused; completion refreshes; queued switch
  resumes.
- appearance: System/Light/Dark resolves the right tokens; observer updates.

Store every `Task` and `Subscription` in the view; dropping silently cancels.

---

## 6. L4 — headless render

The pinned GPUI exposes `Window::render_to_image()` and `HeadlessAppContext`.
Use the real text system and renderer factory from the pinned commit's
`examples/testing.rs`.

- Every view, both themes, at 960×600, 1280×760 and 1440×900.
- Fixtures: empty engine, one container, 40 containers, 100 containers, long
  names, no ports, Compose group, prune sheet, inspector open, bulk mode.
- Assert: no clipped text, no horizontal overflow, expected column density,
  correct selected/hover colours, graph wells non-empty.
- Goldens are a separate serial binary/job, behind a `golden` feature. Do not
  enable golden features in `cargo test --workspace`.
- Keep reference screenshots in `docs/reference-ui/`; generated ones in
  `docs/ui/`. Read the images; do not only diff them.

Because the pinned commit can render headlessly, this replaces the old plan's
OS-screenshot-only approach. Keep an OS screenshot path for real-window checks.

---

## 7. L5 — real-window E2E

Two tools, split by what they can see:

### xa11y

Drives the platform accessibility tree:

- launch by path, temp `HOME` and data dir;
- locate `button[name="Containers"]`, `text_field[name="Search containers"]`,
  `row[name="berth-demo-web"]` (roles/labels come from our AccessKit code);
- press, focus, type, set value, screenshot;
- assert the a11y tree and focus order.

### Test driver (feature-gated)

A newline-delimited JSON service on a Unix socket, compiled only with
`--features test-driver`:

- `hello` — version, pid, capabilities;
- `status` — tab, theme, engine state, selection, bulk mode, sheet, counts;
- `tab` / `select` / `sort` / `search` / `filter` — deterministic UI state;
- `act` — run an action through the same path a click uses;
- `refresh` — request a pass and wait for commit;
- `screenshot` — PNG via `render_to_image`;
- `quit`.

Security: random token in `BERTH_TEST_TOKEN`, socket `0600`, no shell, no
arbitrary file access, never enabled in release artifacts.

The driver and xa11y are complementary: xa11y proves the public surface and
accessibility; the driver proves exact state transitions and waits.

---

## 8. L6 — live engine

`scripts/smoke-live.sh` on a disposable engine:

1. list containers/images/volumes/networks; one stats sample;
2. start/stop/restart/pause/resume/delete a test container;
3. pull `alpine:3.22`, run it, build the terminal command;
4. create/remove a volume and network;
5. prune stopped containers only;
6. engine machine start/stop where available.

Assert before/after with the engine's own CLI. Never run destructive prune on a
developer's main engine. Use a unique name prefix and clean up in a trap.

---

## 9. Visual parity

- `docs/reference-ui/` is the oracle from the Zig app.
- `docs/ui/` is the Rust output, one file per view/theme/size.
- Compare by reading: structure, hierarchy, spacing, colour, truncation.
- Do not chase pixel identity; text shaping and SVG icons will differ.
- A visual regression is a defect when it changes hierarchy, hides data, breaks
  alignment, or lowers contrast.

---

## 10. CI topology

### PR gate

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`
- `cargo test -p berth-app --features test-driver --locked`
- headless render smoke (if the runner has a renderer)
- `cargo build --workspace --locked`
- macOS + Linux; Windows compile only

### Nightly

- full screenshot matrix (both themes, three sizes)
- real-window macOS smoke + Linux X11 smoke (xa11y)
- live-engine smoke
- 100-container performance/fixture run
- kill/restart engine during refresh

### Release

- all of the above
- unsigned bundle smoke (`BERTH_SIGN=0`)
- human five-minute pass: launch, read, act, clean up, quit

---

## 11. Artifacts on failure

Write to `target/e2e/<test>/`:

```
report.json
app.log
driver.jsonl
screenshots/
  before.png
  after.png
environment.txt
```

A failing test should answer: what was on screen, what did the engine return,
which generation lost, and what did the driver state say.

---

## 12. Flakiness rules

- No `sleep()`; wait on a condition with a timeout.
- One app process per E2E test module; fresh temp `HOME`/data dir.
- Pin theme, window size, scale, locale.
- Run input/a11y tests serially.
- A capability that cannot run (no Accessibility grant, no Vulkan, no engine)
  skips with a visible reason; it never passes falsely.

---

## 13. Test matrix before parity

| Feature | U | Engine | GPUI | Headless | E2E | Live |
|---|---|---|---|---|---|---|
| Format / stats / history | ✅ | — | — | — | — | — |
| Transport / JSON / guards | — | ✅ | — | — | — | — |
| Discovery | ✅ | ✅ | — | — | — | ✅ |
| Refresh commit / failure | — | ✅ | ✅ | — | ✅ | ✅ |
| Tables / sort / search | — | — | ✅ | ✅ | ✅ | — |
| Text input / shortcuts | — | — | ✅ | — | ✅ | — |
| Inspector / logs / inspect | — | ✅ | ✅ | ✅ | ✅ | ✅ |
| Actions / bulk / prune | — | ✅ | ✅ | — | ✅ | ✅ |
| Events / stacks | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Create / pull / run | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Engine ops | — | — | ✅ | — | ✅ | ✅ |
| Appearance / persistence | ✅ | — | ✅ | ✅ | ✅ | — |
| Tray | — | — | — | — | ✅ | ✅ |
| Accessibility | — | — | — | — | ✅ | — |

---

## 14. Definition of done for a feature

1. The lowest-layer test that catches its characteristic bug exists and passes.
2. The view was rendered headlessly and read as an image.
3. If a person can see it, one real-window smoke touched it.
4. If it talks to an engine, the mock tests cover success and failure.
5. The parity backlog row is updated with its level and status.
