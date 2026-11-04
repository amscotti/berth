# Review-fix program (working checklist)

Source: nine-agent review of main @ 60200f2 (architecture, UX, Zig, Docker/Podman,
memory/concurrency, security, performance, metrics, testing/docs/platform).
Each item is coded by an agent, then reviewed by a regular and an adversarial
reviewer; findings are fixed before the next item starts.

## 1. Memory and stream integrity — done
- [x] Events ring must not point into a freed arena when `/events` fails on
      consecutive refreshes (`events_feed.commit`, `refresh.commit`)
- [x] `dechunk` chunk-size overflow (`i + size`)
- [x] `stats.demux` double-deinit when the raw fallback copy fails
- [x] `engine/command.zig` double-free of argv on error

## 2. Bulk selection and action semantics — done
- [x] "Select" chip never enables selection (`lists.zig` + `bulk.clear`)
- [x] 304 from Docker/Podman treated as success for actions
- [x] Bulk start/stop stops counting already-in-state containers as failures

## 3. Engine correctness — done
- [x] RestartCount read from the top level of inspect
- [x] Image prune sends the `filters` JSON, not `?dangling=false`
- [x] Podman: volume-prune filter and build-cache prune
- [x] Podman 5 machine sockets and Podman 4 machine names
- [x] Podman CPU unknown when `precpu_stats` is absent
- [x] Legacy `/system/df` aggregates (containers/volumes/cache/reclaimable)

## 4. UI-thread I/O, refresh cadence and teardown — 4a, 4b done
- [ ] Actions/prune/create off the UI thread
- [ ] Selection fetches inspect/logs off the UI thread
- [x] Discovery off the UI thread before the first frame
- [x] Refresh worker observes cancellation; quit is bounded after connect (connect residual documented)
- [x] Stats pool concurrency / two-cycle cost

## 5. UX truth and polish
- [ ] Overview disk bar proportional; shared colour tokens
- [ ] Log follow scrolls to the end
- [ ] First-paint false zeros and missing error states
- [ ] Rail labels/tooltips/accessibility at 960
- [ ] Modal confirmations, Escape, engine stop/restart confirmation
- [ ] Inline validation in create/pull/run
- [ ] Light-theme contrast for status text
- [ ] Prune opt-ins reset; Stacks health and clickable rows; AGE sort
- [ ] Select all/none; inspector unknown facts

## 6. Scale and performance
- [ ] 512-row container cap and pre-virtualization sorting
- [ ] Per-frame project scans, volume merge, image order caching
- [ ] Rotating arenas + `/system/df` duplication; list body caps

## 7. Security and robustness
- [ ] Copy-run port quoting; port-pill IP validation
- [ ] Inspect JSON depth/output bound
- [ ] Numeric casts from engine data; container-count cap
- [ ] Copy-run secret redaction

## 8. Build, release, platform and docs
- [ ] `dist` packs the emitted artifact; target-based naming; ReleaseFast nanoui
- [ ] Release workflow checkout/gates; Linux CI executes a snapshot
- [ ] Linux theme signal and rootless sockets; packaging extras
- [ ] Docs drift, version metadata, `zig fmt`
