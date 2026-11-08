//! The M0 shell: rail, page header, and a static containers table.
//!
//! Everything here is a GPUI view over `BerthApp`. The engine, refresh worker
//! and real data arrive in M1; the geometry and tokens are already the ones the
//! final app uses, so the table survives that transition.

use crate::ui::input::{InputEvent, InputState};
use std::sync::Arc;

use berth_engine::dto::Container;
use berth_ui::theme::{TintKind, Tokens};
use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Subscription, Task};

use crate::jobs::mutation::PruneOptions;
use crate::settings::{self, Appearance};
use crate::state::{EngineState, SharedSnapshot, Snapshot, fixture_snapshot};

/// The views a window can show, in rail order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Containers,
    Images,
    Volumes,
    Networks,
    Events,
    Stacks,
    Disk,
    Runtimes,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Overview,
        Tab::Containers,
        Tab::Images,
        Tab::Volumes,
        Tab::Networks,
        Tab::Events,
        Tab::Stacks,
        Tab::Disk,
        Tab::Runtimes,
        Tab::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Containers => "Containers",
            Tab::Images => "Images",
            Tab::Volumes => "Volumes",
            Tab::Networks => "Networks",
            Tab::Events => "Events",
            Tab::Stacks => "Stacks",
            Tab::Disk => "Disk",
            Tab::Runtimes => "Runtimes",
            Tab::Settings => "Settings",
        }
    }

    pub(crate) fn icon_name(self) -> &'static str {
        match self {
            Tab::Overview => "overview",
            Tab::Containers => "containers",
            Tab::Images => "images",
            Tab::Volumes => "volumes",
            Tab::Networks => "networks",
            Tab::Events => "events",
            Tab::Stacks => "stacks",
            Tab::Disk => "disk",
            Tab::Runtimes => "runtimes",
            Tab::Settings => "settings",
        }
    }

    pub(crate) fn tint_kind(self) -> TintKind {
        match self {
            Tab::Overview => TintKind::Overview,
            Tab::Containers => TintKind::Containers,
            Tab::Images => TintKind::Images,
            Tab::Volumes => TintKind::Volumes,
            Tab::Networks => TintKind::Networks,
            Tab::Events => TintKind::Events,
            Tab::Stacks => TintKind::Stacks,
            Tab::Disk => TintKind::Disk,
            Tab::Runtimes => TintKind::Runtimes,
            Tab::Settings => TintKind::Settings,
        }
    }

    /// The figure the rail shows beside the name. Empty means "no badge".
    pub(crate) fn figure(self, snapshot: &Snapshot, reachable: usize) -> String {
        match self {
            Tab::Overview | Tab::Settings => String::new(),
            Tab::Containers => format!("{}/{}", snapshot.running(), snapshot.containers.len()),
            Tab::Images => snapshot.images.len().to_string(),
            Tab::Volumes => snapshot.volumes.len().to_string(),
            Tab::Networks => snapshot.networks.len().to_string(),
            Tab::Events => snapshot.events.len().to_string(),
            Tab::Stacks => snapshot.projects.len().to_string(),
            Tab::Disk => snapshot
                .store_bytes()
                .map(|bytes| berth_core::format::bytes(bytes as u64))
                .unwrap_or_else(|| "—".to_owned()),
            Tab::Runtimes => reachable.to_string(),
        }
    }
}

/// What the right-hand inspector is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailKind {
    #[default]
    Container,
    Image,
    Volume,
    Network,
}

/// Which sub-tab of the container inspector is active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    #[default]
    Facts,
    Config,
    Runtime,
    Inspect,
}

/// Which table's search field is being addressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Containers = 0,
    Images = 1,
    Volumes = 2,
    Networks = 3,
    Events = 4,
}

impl SearchKind {
    pub const ALL: [SearchKind; 5] = [
        SearchKind::Containers,
        SearchKind::Images,
        SearchKind::Volumes,
        SearchKind::Networks,
        SearchKind::Events,
    ];

    fn placeholder(self) -> &'static str {
        match self {
            SearchKind::Containers => "Search containers",
            SearchKind::Images => "Search images",
            SearchKind::Volumes => "Search volumes",
            SearchKind::Networks => "Search networks",
            SearchKind::Events => "Search events",
        }
    }
}

/// What the container list shows. Filters stay where the user left them, but
/// they are visible as chips — a filtered list must never look like a short one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Running,
    Stopped,
}

impl Filter {
    pub const ALL: [Filter; 3] = [Filter::All, Filter::Running, Filter::Stopped];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Running => "Running",
            Filter::Stopped => "Stopped",
        }
    }

    pub(crate) fn keeps(self, container: &Container) -> bool {
        match self {
            Filter::All => true,
            Filter::Running => container.is_up(),
            Filter::Stopped => !container.is_up(),
        }
    }
}

/// The root view.
pub struct BerthApp {
    pub(crate) tab: Tab,
    pub(crate) filter: Filter,
    pub(crate) tokens: Tokens,
    pub(crate) snapshot: SharedSnapshot,
    pub(crate) engine: EngineState,
    pub(crate) selected: Option<usize>,
    pub(crate) selected_id: Option<String>,
    pub(crate) selected_name: String,
    pub(crate) detail_kind: DetailKind,
    pub(crate) inspector_tab: InspectorTab,
    pub(crate) searches: [Entity<InputState>; 5],
    pub(crate) queries: [String; 5],
    pub(crate) sorts: [(usize, bool); 4],
    pub(crate) focus_handle: FocusHandle,
    pub(crate) appearance_sub: Option<Subscription>,
    pub(crate) _search_subs: [Subscription; 5],
    pub(crate) _form_subs: Vec<Subscription>,
    pub(crate) refresh_task: Option<Task<()>>,
    pub(crate) timer_task: Option<Task<()>>,
    pub(crate) host_task: Option<Task<()>>,
    pub(crate) discovery_task: Option<Task<()>>,
    pub(crate) terminal_task: Option<Task<()>>,
    pub(crate) refresh_wanted: bool,
    pub(crate) fixture: bool,
    pub(crate) appearance: Appearance,
    pub(crate) socket_input: Entity<InputState>,
    pub(crate) toast: Option<Toast>,
    pub(crate) pending: Option<Pending>,
    pub(crate) mutation_task: Option<Task<()>>,
    pub(crate) toast_task: Option<Task<()>>,
    pub(crate) creation_open: bool,
    pub(crate) pull_input: Entity<InputState>,
    pub(crate) run_image: Entity<InputState>,
    pub(crate) run_name: Entity<InputState>,
    pub(crate) run_ports: Entity<InputState>,
    pub(crate) run_volumes: Entity<InputState>,
    pub(crate) run_env: Entity<InputState>,
    pub(crate) pull_attempted: bool,
    pub(crate) run_attempted: bool,
    pub(crate) create_kind: Option<CreateKind>,
    pub(crate) create_name: Entity<InputState>,
    pub(crate) create_attempted: bool,
    pub(crate) create_in_flight: bool,
    pub(crate) prune_open: bool,
    pub(crate) prune_options: PruneOptions,
    pub(crate) prune_in_flight: bool,
    pub(crate) logs_text: String,
    pub(crate) logs_loading: bool,
    pub(crate) logs_follow: bool,
    pub(crate) logs_timestamps: bool,
    pub(crate) log_search: Entity<InputState>,
    _log_search_sub: Subscription,
    pub(crate) inspect_text: Option<String>,
    pub(crate) detail_lifetime: Option<berth_engine::detail::Lifetime>,
    pub(crate) detail_top: Option<berth_engine::dto::ContainerTop>,
    pub(crate) detail_changes: Vec<berth_engine::dto::FsChange>,
    pub(crate) image_layers: Vec<berth_engine::dto::ImageLayer>,
    pub(crate) image_layers_loading: bool,
    pub(crate) inspect_loading: bool,
    pub(crate) detail_generation: u64,
    pub(crate) detail_task: Option<Task<()>>,
    pub(crate) bulk_enabled: bool,
    pub(crate) bulk_selected: std::collections::BTreeSet<String>,
    pub(crate) window_handle: Option<gpui::AnyWindowHandle>,
    pub(crate) tray: Option<crate::tray::Tray>,
    pub(crate) tray_task: Option<Task<()>>,
    pub(crate) quiet_notices_until: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateKind {
    Volume,
    Network,
}

impl CreateKind {
    pub(crate) fn noun(self) -> &'static str {
        match self {
            CreateKind::Volume => "volume",
            CreateKind::Network => "network",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub bad: bool,
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub method: String,
    pub path: String,
    pub verb: String,
    pub name: String,
}

impl BerthApp {
    pub fn new(tokens: Tokens, cx: &mut Context<Self>) -> Self {
        Self::with_snapshot(tokens, Snapshot::empty(), false, cx)
    }

    /// Headless screenshots and layout tests use the fixture; the running app
    /// always starts empty and waits for a real snapshot.
    pub fn new_fixture(tokens: Tokens, cx: &mut Context<Self>) -> Self {
        let mut app = Self::with_snapshot(tokens, fixture_snapshot(), true, cx);
        app.engine.runtimes = vec![crate::state::fixture_runtime()];
        app.engine.active = 0;
        app.engine.last_update = berth_core::format::now();
        app.engine.loading = false;
        app.engine.df = app.snapshot.df.clone();
        app.engine.df_at = app.snapshot.df_at;
        for step in 0..24 {
            app.engine.history.push(berth_core::history::Sample {
                cpu: 4.0 + (step as f32 * 0.7) % 9.0,
                cpu_known: true,
                mem: (4.2 + (step as f32 * 0.06) % 0.8) * 1024.0 * 1024.0,
                mem_known: true,
                running: if step > 12 { 2.0 } else { 1.0 },
                disk: (103.0 + step as f32 * 0.05) * 1024.0 * 1024.0,
                host_cpu: 42.0 + step as f32,
                host_mem: 43.0 * 1024.0 * 1024.0 * 1024.0,
            });
        }
        app
    }

    /// Used by the headless screenshot binary to reach every page.
    pub fn show_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn with_snapshot(
        tokens: Tokens,
        snapshot: Snapshot,
        fixture: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let searches =
            SearchKind::ALL.map(|kind| cx.new(|cx| InputState::new(kind.placeholder(), 190.0, cx)));
        let _search_subs = std::array::from_fn(|index| {
            let search = &searches[index];
            cx.subscribe(
                search,
                move |this, input, event: &InputEvent, cx| match event {
                    InputEvent::Changed => {
                        this.queries[index] = input.read(cx).text().to_owned();
                        cx.notify();
                    }
                    InputEvent::Cancelled => {
                        input.update(cx, |state, cx| state.clear(cx));
                    }
                    InputEvent::Submitted => {}
                },
            )
        });
        let socket_input = cx.new(|cx| InputState::new("/path/to/docker.sock", 320.0, cx));
        let pull_input = cx.new(|cx| InputState::new("nginx:alpine", 320.0, cx));
        let run_image = cx.new(|cx| InputState::new("nginx:alpine", 220.0, cx));
        let run_name = cx.new(|cx| InputState::new("web", 220.0, cx));
        let run_ports = cx.new(|cx| InputState::new("18080:80", 220.0, cx));
        let run_volumes = cx.new(|cx| InputState::new("data:/data", 220.0, cx));
        let run_env = cx.new(|cx| InputState::new("KEY=value", 460.0, cx));
        let create_name = cx.new(|cx| InputState::new("name", 360.0, cx));
        let log_search = cx.new(|cx| InputState::new("Search logs", 130.0, cx));
        let _log_search_sub = cx.subscribe(&log_search, |_, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed) {
                let _ = input.read(cx).text();
                cx.notify();
            } else if matches!(event, InputEvent::Cancelled) {
                input.update(cx, |state, cx| state.clear(cx));
            }
        });
        let mut _form_subs = Vec::with_capacity(8);
        _form_subs.push(cx.subscribe(
            &create_name,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Submitted => this.submit_create(cx),
                InputEvent::Cancelled => this.close_create(cx),
                InputEvent::Changed => {}
            },
        ));
        _form_subs.push(
            cx.subscribe(&pull_input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted) {
                    this.start_pull(cx);
                }
            }),
        );
        for field in [&run_image, &run_name, &run_ports, &run_volumes, &run_env] {
            _form_subs.push(cx.subscribe(field, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted) {
                    this.start_run(cx);
                }
            }));
        }
        _form_subs.push(
            cx.subscribe(&socket_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted) {
                    let path = input.read(cx).text().trim().to_owned();
                    if !path.is_empty() {
                        this.use_socket(&path, cx);
                    }
                }
            }),
        );
        let saved = settings::load();
        let appearance = saved.appearance;
        let mut engine = EngineState::default();
        if !saved.socket.is_empty() {
            engine.preferred_socket = Some(saved.socket.clone());
            socket_input.update(cx, |input, cx| input.set_value(saved.socket, cx));
        }
        let tokens = match appearance {
            Appearance::Light => berth_ui::theme::light(),
            Appearance::Dark => berth_ui::theme::dark(),
            Appearance::System => tokens,
        };
        Self {
            // The real app opens on Overview once the graphs exist. Until then
            // the table is the fully drawn surface.
            tab: Tab::Containers,
            filter: Filter::All,
            tokens,
            snapshot: Arc::new(snapshot),
            engine,
            selected: None,
            selected_id: None,
            selected_name: String::new(),
            detail_kind: DetailKind::Container,
            inspector_tab: InspectorTab::Facts,
            searches,
            queries: std::array::from_fn(|_| String::new()),
            sorts: [(0, true); 4],
            focus_handle: cx.focus_handle(),
            appearance_sub: None,
            _search_subs,
            _form_subs,
            refresh_task: None,
            timer_task: None,
            host_task: None,
            discovery_task: None,
            terminal_task: None,
            refresh_wanted: false,
            fixture,
            appearance,
            socket_input,
            toast: None,
            pending: None,
            mutation_task: None,
            toast_task: None,
            creation_open: false,
            pull_input,
            run_image,
            run_name,
            run_ports,
            run_volumes,
            run_env,
            pull_attempted: false,
            run_attempted: false,
            create_kind: None,
            create_name,
            create_attempted: false,
            create_in_flight: false,
            prune_open: false,
            prune_options: PruneOptions::default(),
            prune_in_flight: false,
            logs_text: String::new(),
            logs_loading: false,
            logs_follow: true,
            logs_timestamps: false,
            log_search,
            _log_search_sub,
            inspect_text: None,
            detail_lifetime: None,
            detail_top: None,
            detail_changes: Vec::new(),
            image_layers: Vec::new(),
            image_layers_loading: false,
            inspect_loading: false,
            detail_generation: 0,
            detail_task: None,
            bulk_enabled: false,
            bulk_selected: std::collections::BTreeSet::new(),
            window_handle: None,
            tray: None,
            tray_task: None,
            quiet_notices_until: 0,
        }
    }

    pub fn set_appearance_subscription(&mut self, subscription: Subscription) {
        self.appearance_sub = Some(subscription);
    }

    pub fn set_tokens(&mut self, tokens: Tokens, cx: &mut Context<Self>) {
        if self.tokens != tokens {
            self.tokens = tokens;
            cx.notify();
        }
    }

    pub(crate) fn search(&self, kind: SearchKind) -> &Entity<InputState> {
        &self.searches[kind as usize]
    }

    pub(crate) fn query(&self, kind: SearchKind) -> &str {
        &self.queries[kind as usize]
    }

    pub(crate) fn switch_to(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            self.clear_selection(cx);
        }
    }
}

pub(crate) fn container_matches(container: &Container, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let ports = container.ports_text(4);
    [
        container.name(),
        &container.image,
        &container.state,
        &container.status,
        container.id.as_str(),
        ports.as_str(),
        container.project().unwrap_or(""),
        container.service().unwrap_or(""),
    ]
    .iter()
    .any(|field| field.to_ascii_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_search_matches_the_fields_the_row_shows() {
        let snapshot = crate::state::fixture_snapshot();
        let container = &snapshot.containers[0];
        assert!(container_matches(container, ""));
        assert!(container_matches(container, "demo"));
        assert!(container_matches(container, "nginx"));
        assert!(container_matches(container, "8080"));
        assert!(container_matches(container, "shop"));
        assert!(!container_matches(container, "redis"));
    }

    #[test]
    fn the_running_filter_counts_paused_as_up() {
        let running = Container {
            state: "running".into(),
            ..Default::default()
        };
        let paused = Container {
            state: "paused".into(),
            ..Default::default()
        };
        let exited = Container {
            state: "exited".into(),
            ..Default::default()
        };
        assert!(Filter::Running.keeps(&running));
        assert!(Filter::Running.keeps(&paused));
        assert!(!Filter::Running.keeps(&exited));
        assert!(Filter::Stopped.keeps(&exited));
        assert!(Filter::All.keeps(&exited));
    }

    #[test]
    fn rail_figures_come_from_the_snapshot() {
        let snapshot = crate::state::fixture_snapshot();
        assert_eq!(Tab::Containers.figure(&snapshot, 1), "2/2");
        assert_eq!(Tab::Images.figure(&snapshot, 1), "2");
        assert_eq!(Tab::Volumes.figure(&snapshot, 1), "1");
        assert_eq!(Tab::Networks.figure(&snapshot, 1), "4");
        assert_eq!(Tab::Settings.figure(&snapshot, 1), "");
        assert!(Tab::Disk.figure(&snapshot, 1).contains("MiB"));
    }
}
