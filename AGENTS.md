# Working in berth

berth is a native monitor for Docker, Podman, Colima and Lima. It reads the
Docker Engine API over a Unix socket. One window, no webview, no daemon of its
own, no telemetry.

**Rewrite in progress (2026-11):** the product is moving to Rust + GPUI. Read,
in order:

1. `docs/GPUI-REBUILD.md` — plan, decisions, milestones
2. `docs/PRODUCT-SPEC.md` — behavior and visual contract (what must survive)
3. `docs/GPUI-VERSION.md` — pinned GPUI commit and upgrade policy
4. `docs/TESTING.md` — test layers, driver, screenshot policy
5. `docs/PARITY-BACKLOG.md` — feature matrix and free-time queue
6. `docs/RELEASE.md` — unsigned builds

The process is modeled on `../pilcrow`, which completed the same
product-preserving rewrite. Copy its structure, not its product code.

## Commands

```sh
cargo check --workspace           # tight loop
cargo run -p berth-app            # open the window
cargo test --workspace            # unit + GPUI in-process
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
./tools/lines.py                  # 600-line rule
cargo run -p berth-app --example screenshots   # headless PNGs in docs/ui/
./scripts/coverage.sh             # line coverage gate (85%)
```

Live engine:

```sh
cargo run -p berth-app            # discovers sockets itself
BERTH_SOCKET=/path/to.sock cargo run -p berth-app
```

## Rules that matter

- **The snapshot is immutable.** A worker builds a complete `Snapshot`; the UI
  thread swaps it in one commit. Never mutate a snapshot that is on screen.
- **No engine IO on the UI thread.** All sockets, `ps`, and process commands
  run on the background executor.
- **Unknown is `—`, never `0`.** A partial stats sum says "partial sample". A
  failed list keeps the previous list, never an empty one.
- **One mutation at a time.** Container actions, bulk, pull/run, prune and
  engine machine commands share one slot. A second request is refused.
- **Generations guard every completion.** Discovery, refresh, detail, mutation
  and image tasks compare their generation before touching state.
- **Selection is by id**, re-resolved after every commit. Never hold a pointer
  into a previous snapshot.
- **Tokens live in `berth-ui`.** A view must not invent a colour or a spacing
  value that the design contract already owns.
- **Accessibility from day one.** Every control gets a role and a name. Tables
  and rows are navigable. Do not land a view without it.
- **No `sleep()` in tests.** Wait on a condition. A skipped capability reports
  why; it never passes falsely.

## GPUI rules that are not obvious

- `cx.notify()` after every state mutation. No notify, no repaint.
- Any stateful element in a list needs a unique `.id(...)`; duplicate ids give
  wrong hitboxes and stale hover.
- Keybindings need all three: an action, a `.key_context("...")` on the focused
  element, and `cx.bind_keys([KeyBinding::new("...", Action, Some("..."))])`.
- `uniform_list` only for fixed-height rows. Use `list` or a custom element for
  variable heights.
- Store every `Task` and `Subscription` in the owning struct. Dropping one
  silently cancels the work or the observation.
- Text shaping is UI-thread work. Shape only visible rows.
- GPUI has no text-input widget, tray API, or SF Symbols. The plan budgets for
  all three.
- Pin GPUI exactly. `cargo update` without reading the diff is a regression
  waiting to happen.

## Data rules

- `GET /containers/{id}/stats?stream=false` waits two collection cycles (~2 s).
  Never use `one-shot=true`; it empties `precpu_stats` and turns CPU into a
  lifetime average.
- Memory is a working set: `usage − inactive_file`, then `total_inactive_file`,
  then `cache`.
- Lists are capped at 5,000 entries per store; JSON deeper than 64 levels is
  refused. The refusal is an availability message, not an empty store.
- `/system/df` is expensive: fetch on a ~30 s cadence and keep the last good
  document with its age.
- Port bindings are deduplicated on the public/private pair.
- Copy-run redaction is best-effort; say "some env values redacted", not "all
  secrets found".
- The host bar has three segments: containers, engine helpers, elsewhere.
  Containerd shims are excluded from "engine" or counted twice.

## Repository layout

| Path | What |
|---|---|
| `crates/berth-core` | Pure data/math; no GPUI, no IO |
| `crates/berth-engine` | UDS HTTP, DTOs, discovery, prune, df, events |
| `crates/berth-ui` | Tokens, geometry, typography, table specs |
| `crates/berth-commands` | Actions, platform keymaps, menus |
| `crates/berth-app` | GPUI shell, views, jobs, tray, main |
| `docs/reference-ui/` | Zig app screenshots — the visual oracle |
| `tests/e2e/` | xa11y + test-driver (feature-gated) |

## Signing

Build signing is off by default (`BERTH_SIGN=0`). No `codesign`, no
notarization, no keychain prompt. Git commit signing is disabled locally. Do
not re-enable either in an automated context.
