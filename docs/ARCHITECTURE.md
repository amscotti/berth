# Architecture

berth is a single-window native app with one background refresh worker. The
window paints from an immutable snapshot; the worker reads the engine and builds
the next one. There are no locks in the UI path because there is no shared
mutable data between the two threads.

```
main thread                        refresh worker
-----------                        --------------
NSApplication.run
  └─ shell.view
       ├─ rail                     Job.load
       ├─ page (current snapshot)    ├─ discovery.probe
       └─ inspector                 ├─ /version, /info
                                    ├─ /containers
commit ◄──── dispatch main ───────  ├─ stats_pool (bounded threads)
  ├─ retire old arena               ├─ /images, /volumes, /networks
  ├─ adopt new snapshot             └─ /system/df
  ├─ re-resolve selection
  ├─ sample history
  └─ schedule next pass
```

## Modules

| | |
|---|---|
| `main.zig` | process entry, window creation, `BERTH_*` harness flags |
| `boot.zig` | state construction and teardown |
| `app.zig` | `State`, selection, actions, cleanup, host sampling |
| `refresh.zig` | `Job`, the worker load, the single commit callback, timer |
| `snapshot.zig` | a complete engine snapshot and its arena |
| `stats_pool.zig` | per-container stats, fetched together |
| `engine/http_uds.zig` | HTTP/1.1 over a Unix socket, timeouts, framing |
| `engine/api.zig` | Docker API types and parsing |
| `engine/stats.zig` | stats math, log demux |
| `engine/discovery.zig` | socket candidates, probing, runtime kinds |
| `engine/prune.zig` | prune routes and `SpaceReclaimed` parsing |
| `host/` | host CPU/memory: Linux `/proc`, macOS Mach host statistics |
| `ui/shell.zig` | rail, page frame, inspector, sheets |
| `ui/views/` | list tables, overview, disk, runtimes, settings, stacks |
| `ui/` | theme, widgets, format, history, cell graphs |

The 600-line rule is part of the build (`zig build lines`). That is why the
refresh worker, snapshot, boot and stats pool are separate files rather than
sections of `app.zig`.

## Refresh lifecycle

1. **Start** (`refresh.start`): guard on an in-flight job, copy the socket path
   into memory owned by the job, create the snapshot arena, spawn a thread.
2. **Load** (`refresh.load`): probe, negotiate `/version`, read `/info`, the
   container list, stats, image/volume/network lists, then `/system/df`.
3. **Done** (`refresh.done`, dispatched to the main queue): join the worker,
   copy any error into the state buffer, then either commit or keep the old
   snapshot.
4. **Commit**: retire the old arena, adopt the new one, update the selection by
   id, sample history, and schedule the next pass five seconds after this one
   finished. A click during a pass sets `refresh_wanted` and starts the next one
   immediately.

### Ownership rules

- `State.arena` owns what is on screen. The previous arena becomes
  `State.retired` and is freed one cycle later, because the frame already on
  screen may still point into it.
- A job owns its snapshot arena until commit. Commit moves the arena into the
  state and replaces the job's copy with an empty one.
- Logs are copied into a `gpa`-owned buffer. They must survive snapshot swaps
  and are refreshed on demand, not on every pass.
- `Client.alloc` always points at the state's current arena for interactive
  calls (actions, logs, prune).
- The worker never reads a mutable `State` field. It gets the socket path,
  IO handle, allocator and generation at start; everything else is its own.

### Failure model

- A failed core call aborts the snapshot and leaves the last complete one on
  screen. A failed image/volume/network list is a core failure: committing an
  empty list would read as “they were deleted”.
- A failed `/system/df` keeps the previous disk figures; disk numbers are
  advisory and should not blank the page.
- A failed stats sample skips that container; the next pass retries.
- A ping failure drops the client so the next timer tick rescans engines.
- Every socket has a receive/send timeout: 5s general, 3s for `/system/df`,
  2s for probes. The UI never waits on any of them.

## Testing

`zig build test` is engine-free and display-free. It covers:

- HTTP/1.1 parsing, chunked decoding, header case, unsupported codings,
  content-length truncation, malformed chunks, and the socket timeout against
  a listener that accepts but never answers.
- Docker JSON shapes: compose labels, ports, `/system/df` new and old shapes,
  prune replies, attached-network counts, image dangling state.
- Stats math: CPU delta and per-core fallback, missing previous samples,
  working-set memory and the older cache key.
- Host parsers for Linux `/proc` and macOS Mach tick math.
- Formatting, history windows, tab/filter parsing, settings JSON, sorting
  comparators for all four tables, stack grouping, theme choices, prune routes.

Not unit-tested: the worker orchestration, discovery scans, real engine
actions/prune, and pixel output. Those are exercised by the headless
`BERTH_TAB`/`BERTH_SELECT`/`BERTH_ACT` harness and by running against a live
engine. `docs/ui/` is the visual evidence.

## Review findings

**Strengths**

- One snapshot swap makes stale/partial data structurally hard to show.
- The engine is read on a worker; the UI thread only commits and samples.
- Arenas give a clear lifetime for parsed JSON and make the commit atomic.
- The API/transport split is testable without a window.
- Failure keeps the last good snapshot instead of blanking the page.

**Open risks**

| | Risk | Impact | Recommendation |
|---|---|---|---|
| 1 | `discover` scans sockets synchronously on the UI thread | Startup or Rediscover can block for `2s × stale candidates` | Move the scan into a job, like refresh |
| 2 | `readAll` has a per-read timeout, not a total deadline | A slow trickle can hold a worker past the intended budget | Add a monotonic deadline around the whole response |
| 3 | Actions and prune run synchronously on the UI thread | A slow daemon or build-cache prune can stall a frame | Put both behind the job/status pattern |
| 4 | Stats spawn up to eight threads per pass | Many containers stretch the refresh period; thread churn | Persistent sampler or adaptive concurrency |
| 5 | No pixel-regression test in CI | Visual regressions still need a human to look | Snapshot every view as an artifact; add a diff gate when stable |
| 6 | macOS engine RSS is always 0 | The host bar's engine segment is empty on macOS | Attribute the Lima/Docker VM process RSS |

Fixed during this review: a failed images/volumes/networks fetch used to
commit an empty list; it now aborts the snapshot and keeps the previous one.
