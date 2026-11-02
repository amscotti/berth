# Feature research: what Docker Desktop and Podman Desktop have that berth could adopt

Researched 2 November 2026 against the shipping feature sets of both apps, then
ranked for **berth's** identity: a small, native, local-first monitor with light
management, on macOS and Linux, over any Docker-compatible socket. "Usefulness"
is for that audience, not for a general-purpose container platform.

Sources

- Docker Desktop — Dashboard overview: <https://docs.docker.com/desktop/use-desktop/>
- Docker Desktop — Containers view: <https://docs.docker.com/desktop/use-desktop/container/>
- Docker Desktop — Images view: <https://docs.docker.com/desktop/use-desktop/images/>
- Docker Desktop — Volumes view: <https://docs.docker.com/desktop/use-desktop/volumes/>
- Docker Desktop 4.12 release notes (integrated terminal): <https://www.docker.com/blog/integrated-terminal-for-running-containers-extended-integration-with-containerd-and-more-in-docker-desktop-4-12/>
- Podman Desktop — features: <https://podman-desktop.io/features>
- Podman Desktop — discover: <https://podman-desktop.io/docs/discover-podman-desktop>
- Podman Desktop — docs index (Compose, Kubernetes, Lima, Kind, registries): <https://podman-desktop.io/docs/intro>

## What berth already covers

Multi-engine discovery and switching, container lifecycle actions, per-container
CPU/memory with history graphs, host memory split, port display, Compose project
grouping, image/volume/network lists with per-item removal, inspect-lite panes,
prune-based cleanup with reclaimable totals, dark/light appearance, headless
screenshot harness. Both desktop apps' "clean/purge data" and basic
start/stop/delete parity is already done.

## Ranking legend

- **Use** — usefulness 1–5 for a lightweight local monitor.
- **Diff** — S: under a day; M: a few days; L: a week or two; XL: a multi-week
  subsystem. Includes both engine-protocol work and UI work on nanoui.
- **Priority** — 1 do next, 2 soon, 3 later, 4 out of scope for berth.

## Tier 1 — do next (high value, small)

| # | Feature | Seen in | Use | Diff | Why / how |
|---|---|---|---|---|---|
| 1 | **Text search** in Containers, Images, Volumes, Networks | DD all views; PD "enhanced search" | 5 | S | The most-missed item already named in berth's README. Case-insensitive substring over the fields each table already shows; a search field beside the filter chips, `⌘F`/`Ctrl+F` to focus. No protocol work. |
| 2 | **Open published port in browser** | DD Containers ("Open the port…") | 4 | S | Ports are already parsed. A click on `8080→80` runs `open http://localhost:8080` (macOS) / `xdg-open` (Linux). First HTTP-looking port wins; tooltip shows the mapping. |
| 3 | **Pause / resume** containers | DD row actions | 3 | S | `POST /containers/{id}/pause` and `/unpause`, supported by Docker and Podman. Adds a `paused` pill colour and reuses the existing confirm/action path. |
| 4 | **Health status** (healthy/unhealthy/starting) | DD status chips | 4 | S | The list endpoint already carries it inside `Status` ("Up 2 minutes (healthy)"); fall back to inspect's `State.Health` for detail. A pill next to state — no new requests. |
| 5 | **Inspect viewer** (container/image/volume/network JSON) | DD Inspect tab; PD inspect | 4 | S–M | `GET /containers/{id}/json`, `/images/{name}/json`, `/volumes/{name}`, `/networks/{id}` already exist as client methods or are one-liners. Render pretty-printed JSON in a scroll in the inspector pane. Pure UI over data berth already fetches. |
| 6 | **More stat columns**: network I/O, disk I/O, PIDs, restart count | DD column picker | 4 | S–M | `/stats` already returns `networks`, `blkio_stats` and `pids_stats`; berth currently reads only CPU and memory. Add columns and sort keys. Column *customization* (DD/PD both have it) is the M-sized follow-on once the columns exist. |
| 7 | **Copy `docker run` / `podman run` command** | DD row action | 3 | S | Reconstructed from inspect (image, name, ports, env, volumes). Needs clipboard access — verify nanoui's clipboard support before promising the button. |
| 8 | **Last-started time and uptime columns** | DD "last-started" column | 3 | S | `Created`/`StartedAt` are in the list and inspect payloads; berth already formats durations. |

## Tier 2 — soon (high value, medium)

| # | Feature | Seen in | Use | Diff | Why / how |
|---|---|---|---|---|---|
| 9 | **Engine/VM lifecycle controls** — start/stop/restart Colima, Lima, Podman machine; create with CPU/memory/disk presets | PD tray + Resources page ("create new machine… start or stop Podman machines", allocated CPU/memory/storage) | 5 | M | The feature berth's own development history argues for: the user's Lima VM sat wedged and the app could only watch. berth already knows which engine kind owns each socket; spawn `colima start/stop`, `limactl start/stop`, `podman machine start/stop` as background jobs and keep the Runtimes view honest while the engine is down. Resource *editing* (CPU/RAM/disk) is the L-sized second half. |
| 10 | **Events feed** (created/started/died/oom/destroy) | neither app has a real one; berth is a monitor | 4 | M | `GET /events?since=<ts>&until=<now>` works without streaming: the refresh worker can collect the interval each pass into a ring, and the view is a table like any other. Groundwork for notifications and "why did it restart". |
| 11 | **Bulk selection + bulk start/stop/delete** | DD checkbox toolbar; PD bulk delete | 4 | M | A checkbox column, a selection set in state, loop the existing action path with one confirm naming the count. DD's "estimated space reclaimed" for selected images falls out of sizes berth already has. |
| 12 | **Log pane upgrades** — follow/auto-refresh, timestamps toggle, wrap, search-in-logs, copy | DD Logs (`⌘F`, copy, clear, timestamps); PD logs | 5 | M (auto-refresh) → L (true streaming) | Honest staging: today the pane is a 200-line tail with a manual refresh. Auto-refresh on the worker + timestamp toggle + in-pane search is M. True `follow` streaming needs hijacked connections and `fdwatch`, which nanoui has but berth's HTTP client does not yet — that's the L follow-up and also unlocks #17. |
| 13 | **Notifications on exit/OOM/restart** | DD's Gordon flags issues; PD tasks/notifications | 4 | M | Built on #10's events. In-app toasts are trivial (berth has them); macOS notification centre needs a small native call or `osascript` — verify before sizing. |
| 14 | **Run image as a container** with optional name/ports/volumes/env | DD Images "Run" + optional settings | 4 | M | `POST /containers/create` then `/start`; works identically on Podman. The cost is the options dialog, not the protocol. Pairs with #15. |
| 15 | **Pull image** by reference | DD/PD | 4 | M | `POST /images/create?fromImage=…&tag=…` streams JSON progress. berth can run it as a background job with a spinner and the final "Downloaded" line — no streaming UI required for v1. |
| 16 | **Menu-bar / tray mini-monitor** (engine dot, CPU/mem, running count) | PD tray management | 4 | M | nanoui ships a tray API (`App.newTray`), and berth's sibling nanomon proves the pattern on this exact framework. A natural fit for a monitor; keeps the window closed. |

## Tier 3 — later (useful, larger)

| # | Feature | Seen in | Use | Diff | Why / how |
|---|---|---|---|---|---|
| 17 | **Integrated exec terminal** | DD Exec/Debug tab; PD terminal | 5 devs, 3 berth | XL | The single most-used Docker Desktop feature, but a real terminal needs a PTY, ANSI rendering and input plumbing nanoui does not have today. **Recommended substitute:** "Open in Terminal" (S) — spawn Terminal.app/iTerm/gnome-terminal with `docker exec -it …`, exactly the escape hatch Docker Desktop itself offers. Do that in Tier 1.5; revisit embedded only if a terminal widget lands in nanoui. |
| 18 | **Compose up/down** for a project | DD Compose row actions/dev environments; PD Compose | 4 | L | There is no engine API for compose; it means shelling out to `docker compose` per project directory — which berth would have to learn from labels (`com.docker.compose.project.working_dir`). Honest subset first (Tier 1): "start/stop all containers in project" over the existing action path. |
| 19 | **Image build** from a Containerfile/Dockerfile | DD Builds view; PD build | 3 | L | `POST /build` with a tar context and streamed output; needs a file picker, context tarring, and a progress/log surface. Only worth it after #12's streaming groundwork. |
| 20 | **Volume file browser** (browse/edit/export) and **container Files tab** | DD Volumes "Stored data"; DD container Files | 3 | L | `GET/PUT /containers/{id}/archive` and volume equivalents; the protocol is the easy part, the tree + editor + drag-drop UI is the cost. |
| 21 | **Push/tag image, registries management, sign-in** | DD My Hub/push; PD registries | 2 | M–L | Registry auth and account UX pull berth away from local-first; Podman's registries live in `registries.conf`, Docker's in `config.json`. Defer until someone asks for it. |
| 22 | **Volume create / network create** | DD Volumes "Create"; PD networks page | 2 | S–M | `POST /volumes/create`, `POST /networks/create` — trivial protocol, low demand in a monitor. Cheap to add whenever. |
| 23 | **Pods view** (Podman-only) | PD Pods page | 3 for Podman users | L | Libpod API (`/v4.0.0/libpod/pods/json`) is engine-specific; adds a second protocol dialect. Only if Podman users become a primary audience. |
| 24 | **Engine VM resource attribution** (Lima/QEMU/Podman machine RSS as the engine segment) | DD resource graphs; PD Resources | 3 | M | Fills a known berth gap: on macOS the host bar's "engine" segment is always 0 because no sampler attributes the VM processes. Scan `ps` for the engine's PIDs by kind. |

## Tier 4 — out of scope for berth

| Feature | Seen in | Why not |
|---|---|---|
| Kubernetes cluster + explorer (deploy YAML, port-forward, kind/minikube/OpenShift) | DD Kubernetes; PD Kubernetes explorer | A second orchestration platform; berth monitors container engines. |
| Extensions marketplace / import Docker Desktop extensions | DD Extensions; PD extensions | berth is deliberately one native binary; an extension runtime is a platform, not a feature. |
| AI assistants, model runner, MCP toolkit, AI Lab | DD Gordon/Model Runner; PD AI Lab | Service-backed, network-dependent, and far from monitoring. |
| Docker Hub sign-in, org repos, pull-rate upsells | DD sign-in/My Hub | Account UX and secrets management contradict local-first. |
| Cloud/offload builds, cloud volume export, scheduled exports | DD Offload, volume exports | Cloud services with accounts and quotas. |
| SBOM/CVE scanning (Scout) | DD image CVEs | Requires Docker's service; a link-out is the honest ceiling here. |
| Managed/enterprise configuration | PD managed settings | No enterprise deployment story to serve yet. |

## Recommended sequence

1. **Wave 1 — the table gets smarter** (Tier 1): search, open-port, pause,
   health, inspect viewer, extra stat columns, uptime, copy-run.
2. **Wave 2 — the monitor gets a memory** (Tier 2): events feed, log
   auto-refresh + search, bulk actions, notifications, engine/VM controls.
3. **Wave 3 — creation, not just watching** (Tier 2/3): pull, run dialog,
   external exec terminal, tray mini-monitor.
4. **Wave 4 — only if asked**: streaming logs/exec, compose up/down, build,
   volume browsing, registries, pods.

## Architecture prerequisites these imply

- **A background command runner** (spawn `colima`/`limactl`/`podman`/`docker
  compose` with timeout and captured output) — needed by #9, #17-substitute,
  #18; same shape as the refresh job.
- **Streaming HTTP** (hijacked connections + `fdwatch`) — needed by true log
  follow, exec, build and pull progress; nanoui has `fdwatch`, berth's client
  does not. Build it once, behind the job pattern.
- **Settings persistence for more than appearance** — column choices, log
  options, notification prefs; `settings.zig` already owns a JSON store.
- **Clipboard and notification APIs** — verify nanoui's coverage before
  scheduling #7 and #13.
- **A second protocol dialect** only if pods (#23) is ever accepted; everything
  else in Tiers 1–2 is plain Docker-compatible API that Podman already speaks.
