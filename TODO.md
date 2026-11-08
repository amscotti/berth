# berth TODO

Living checklist for the Rust + GPUI rewrite. Product/behavior is in
`docs/PRODUCT-SPEC.md`; the plan is `docs/GPUI-REBUILD.md`; the feature queue is
`docs/PARITY-BACKLOG.md`.

Last updated: 2026-11-07

## Completed

- [x] Cargo workspace, rust-toolchain 1.98, pinned GPUI
- [x] Tokens, contrast tests, layout geometry/density tests
- [x] Shell: rail, pages, inspector, sheets, status, toasts, icons
- [x] Text input with focus, caret, selection, clipboard, IME hooks
- [x] UDS HTTP/1.1 transport with deadlines, chunked, 8 MiB cap
- [x] DTOs, guards, discovery, client and mock-engine tests
- [x] Format, stats, history, image refs, validation, projects, networks
- [x] Ten views, search/filter/sort, tables, graphs and host sampling
- [x] Inspector facts, logs, inspect JSON, terminal, open port
- [x] Container actions, bulk, Compose start/stop, create, pull/run, prune
- [x] Engine machine commands, settings persistence, tray title/menu
- [x] Headless screenshots (10 views × 3 sizes light, 10 dark)
- [x] 100-container perf: 13.9 ms/frame
- [x] Live probe and live action smoke against Docker 29.9.2
- [x] Unsigned macOS `.app` bundle (`target/Berth.app`)

## Remaining gaps

- [ ] Reopen a closed window from the tray (GPUI window hide/show adapter).
- [ ] Feature-gated test driver + xa11y real-window E2E.
- [ ] Linux X11/Wayland runtime smoke.
- [ ] Virtualized tables (`uniform_list`) if the fleet grows past the tested 100.
- [ ] Version: `0.0.0` until parity; tag `v0.1.0` after the remaining gaps.
- [ ] Windows remains compile-only.
- [ ] DMG / Developer ID / notarization are deliberate non-goals.
