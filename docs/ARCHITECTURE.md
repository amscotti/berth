# Architecture

berth is a single-window native app with background workers for refresh and
engine discovery. The window paints from an immutable snapshot; a refresh
worker reads the engine and builds the next one, and a discovery worker probes
sockets so startup never waits on a stale one. There are no locks in the UI path
because there is no shared mutable data between the threads.

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

                                   discovery worker (startup and rescan)
                                    ├─ candidates (preferred first)
                                    ├─ probe (bounded threads)
                                    └─ /version, /info
adopt scan ◄── dispatch main ──────┘
  ├─ first reachable runtime
  ├─ clear only on identity change
  └─ install client, start refresh
```

## Modules

| | |
|---|---|
| `main.zig` | process entry, window creation, `BERTH_*` harness flags |
| `boot.zig` | state construction and teardown |
| `app.zig` | `State`, selection, host sampling, action entry points |
| `session.zig` | discovery job, runtime choice, socket preference, refresh start |
| `refresh.zig` | `Job`, the worker load, the single commit callback, timer |
| `refresh_selection.zig` | re-resolve the inspector selection by id after a snapshot swap |
| `actions.zig` | one background job for container actions, create and prune |
| `cleanup.zig` | the cleanup sheet's run, delegating to the action job |
| `image_tasks.zig` | pull/create/start as background jobs, the create body |
| `image_refs.zig` | reference, name and port parsing for the creation card |
| `create.zig` | the volume/network create dialog and name validation |
| `terminal.zig` | the socket-pinned exec command and terminal launch |
| `tray.zig` | the menu-bar mini-monitor |
| `snapshot.zig` | a complete engine snapshot and its arena |
| `stats_pool.zig` | per-container stats, fetched together |
| `engine/http_uds.zig` | HTTP/1.1 over a Unix socket, timeouts, framing |
| `engine/api.zig` | Docker API types and parsing |
| `engine/stats.zig` | stats math, log demux |
| `engine/discovery.zig` | socket candidates, probing, runtime kinds |
| `engine/prune.zig` | prune routes and `SpaceReclaimed` parsing |
| `engine/create.zig` | volume and network creation |
| `host/` | host CPU/memory and engine-process RSS: Linux `/proc`, macOS Mach + `ps` |
| `ui/shell.zig` | rail, page frame, inspector, sheets |
| `ui/views/` | list tables, overview, disk, runtimes, settings, stacks |
| `ui/` | theme, widgets, format, history, cell graphs |

The 600-line rule is part of the build (`zig build lines`). That is why the
refresh worker, snapshot, boot and stats pool are separate files rather than
sections of `app.zig`.

## Discovery lifecycle

1. **Start** (`session.discover`): guard on a running refresh or scan, copy the
   preferred socket and the last engine identity into a job, then spawn a
   worker. The old runtimes and client stay usable, so a rescan keeps painting
   the engine already on screen. Forgetting or typing a socket during a scan
   changes the preference and queues a rescan; it does not wait on or replace
   the running job.
2. **Scan**: the worker runs `discovery.scan`; it reads only its own copies and
   never a mutable `State` field.
3. **Done** (dispatched to the main queue): join, adopt the first reachable
   runtime, compare the dialect, daemon id and socket with the identity
   captured at start, clear the snapshot only when it changed, remember the
   engine, install the client, and start a refresh. A dialect change is always
   an engine change, even on the same path. Podman's compat `/info` returns a
   fresh id per request, so Podman falls back to socket identity. Failure
   reports the same errors as before, samples the host, schedules the retry,
   and drops the old client/runtimes. A rescan requested while the scan was
   running is started after it finishes. Views and the tray keep saying
   "reading engine" until the first snapshot commits, not merely until
   adoption.
4. **Teardown**: `boot.destroy` abandons the job instead of joining a scan that
   may be stuck in a connect, so quit stays bounded. The completion callback
   checks the abandoned flag before touching state and frees only the job. If
   the worker is still inside the io/allocator teardown as the process exits,
   that exit-only race is accepted, matching other detached jobs; if the
   callback never runs, the job leaks at exit.

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
- A discovery job owns its copied socket/id strings and scan result. Adoption
  moves the runtimes into the state; abandonment leaves the job to the
  completion callback, or a leak at exit if the loop stops before it runs.
- The workers never read a mutable `State` field. A refresh job gets the socket
  path, IO handle, allocator and generation at start; a discovery job gets the
  preferred socket and old identity. Everything else is its own.

### Failure model

- A failed core call aborts the snapshot and leaves the last complete one on
  screen. A failed image/volume/network list is a core failure: committing an
  empty list would read as “they were deleted”.
- A failed `/system/df` keeps the previous disk figures; disk numbers are
  advisory and should not blank the page.
- A failed stats sample skips that container; the next pass retries. The pass is
  marked incomplete, so container totals render as partial rather than as a
  lower bound presented as the whole.
- A ping failure drops the client so the next timer tick rescans engines.
- Every socket has one total receive/send deadline: 5s general, 3s for
  `/system/df`, 3s for probes. A peer that trickles bytes cannot restart it.
  The initial Unix-domain `connect` is outside that deadline; a full listener
  backlog remains the one unbounded socket step.

## Testing

`zig build test` is engine-free and display-free. It covers:

- HTTP/1.1 parsing, chunked decoding, header case, unsupported codings,
  content-length truncation, malformed chunks, and the total socket deadline
  against both a listener that never answers and one that trickles bytes.
- A mock engine socket: stats fan-out and load balance, cancellation between
  containers, a canceled refresh stopping before its next request, and the
  teardown completion context.
- Docker JSON shapes: compose labels, ports, `/system/df` new and old shapes,
  prune replies, attached-network counts, image dangling state.
- Stats math: CPU delta and per-core fallback, missing previous samples,
  working-set memory and the older cache key.
- Host parsers for Linux `/proc` and macOS Mach tick math.
- Discovery's pure choices (first reachable runtime, Docker id vs socket
  identity, Podman's per-request id, Docker→Podman dialect changes, rescan
  re-arm), the asynchronous start with a fake scan, one-runtime adoption and
  refresh start, scan failure with one retry, queued rescans, and timers
  standing down while a scan runs.
- Formatting, history windows, tab/filter parsing, settings JSON, sorting
  comparators for all four tables, stack grouping, theme choices, prune routes.

Action jobs are tested against the mock engine: a one-shot success and engine
error, a refused second mutation, create completion on success and failure, and
prune accumulation when a middle step fails. Not unit-tested: the full worker
lifecycle (spawn/commit/timer), real socket scans, real engine mutations, and
pixel output. Those are exercised by the headless
`BERTH_TAB`/`BERTH_SELECT`/`BERTH_ACT`/`BERTH_PULL`/`BERTH_RUN_*` harness and by
running against a live engine. `docs/ui/` is the visual evidence.

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
| 1 | Stats spawn one thread per running container, capped at 64, every pass | Bounded per pass, but still thread churn on every refresh | Persistent sampler if thread creation shows up in profiles |
| 2 | Unix-domain `connect` has no deadline | A full listener backlog can hold a worker before the request budget starts | Add a connect deadline where the platform exposes one |
| 3 | No pixel-regression test in CI | Visual regressions still need a human to look | Snapshot every view as an artifact; add a diff gate when stable |
| 4 | macOS guest RAM belongs to no attributable process | The engine segment is helper-only; the VM's RAM (which holds the containers) sits in "elsewhere" | Documented limitation; Virtualization.framework exposes no per-VM accounting |

Fixed during this review: discovery used to scan sockets on the UI thread, so
startup or a rescan could block for `3s × stale candidates`; it now runs on a
worker and the window opens with skeletons. A failed images/volumes/networks
fetch used to commit an empty list; it now aborts the snapshot and keeps the
previous one. Container actions, create and prune used to POST from the click
handler; they now share one action job, and actions, bulk work, image runs and
engine commands refuse to overlap. An engine switch or rescan waits for any of
those mutating jobs. Every mutation completion refreshes and then resumes the
queued switch, rescan or pass through one state machine; the create dialog
stays open until the engine answers.
Stats used to top out at eight threads, so nine running containers stretched a
pass past the refresh period; the pool now runs up to 64 and cancels between
requests when the job is discarded. Socket reads now share one deadline per
request, so a trickled response cannot extend a quit.
