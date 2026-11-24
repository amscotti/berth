//! The application shell: view vocabulary, `BerthApp` state, and search
//! predicates. Everything here backs the GPUI views in `ui/`.

use std::sync::Arc;

use crate::ui::input::InputState;
use berth_engine::dto::Container;
use berth_ui::theme::{TintKind, Tokens};
use gpui::{AppContext, Context, Entity, Subscription, Task};

use crate::inspector::Inspector;
use crate::settings::{self, Appearance};
use crate::slot::MutationSlot;
use crate::state::{EngineState, HostSample, SharedSnapshot, Snapshot};
use crate::subviews::Subview;
use crate::view::ViewState;

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

impl InspectorTab {
    pub const ALL: [InspectorTab; 4] = [
        InspectorTab::Facts,
        InspectorTab::Config,
        InspectorTab::Runtime,
        InspectorTab::Inspect,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InspectorTab::Facts => "Facts",
            InspectorTab::Config => "Config",
            InspectorTab::Runtime => "Runtime",
            InspectorTab::Inspect => "Inspect",
        }
    }
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

    pub(crate) fn placeholder(self) -> &'static str {
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

/// The shell: the engine's answer, the jobs that keep it current, and the
/// window and tray it shows in. Everything about what the person is looking
/// at, or doing, is [`ViewState`]'s or [`crate::inspector::Inspector`]'s.
pub struct BerthApp {
    pub(crate) tokens: Tokens,
    pub(crate) snapshot: SharedSnapshot,
    /// Monotonic counter bumped whenever `snapshot` is replaced, so table row
    /// caches invalidate without retaining old `Arc<Snapshot>`s.
    pub(crate) snapshot_id: u64,
    /// The machine's own figures, from the last pass or the last host sample.
    pub(crate) host: HostSample,
    pub(crate) engine: EngineState,
    pub(crate) appearance: Appearance,
    pub(crate) appearance_sub: Option<Subscription>,
    pub(crate) refresh_task: Option<Task<()>>,
    pub(crate) timer_task: Option<Task<()>>,
    pub(crate) host_task: Option<Task<()>>,
    pub(crate) discovery_task: Option<Task<()>>,
    pub(crate) terminal_task: Option<Task<()>>,
    /// The one engine mutation in flight, whatever asked for it.
    pub(crate) mutation: MutationSlot,
    pub(crate) refresh_wanted: bool,
    /// A rescan asked for while one was already running. It goes next.
    pub(crate) rescan_wanted: bool,
    pub(crate) view: ViewState,
    pub(crate) inspector: Inspector,
    /// The page and the inspector, each drawn by a view of its own so that a
    /// change in one doesn't redraw the other. See `subviews`.
    pub(crate) page: Entity<Subview>,
    pub(crate) pane: Entity<Subview>,
    /// How many times the shell itself has been drawn. Tests read it.
    #[cfg(test)]
    pub(crate) draws: u32,
    pub(crate) window_handle: Option<gpui::AnyWindowHandle>,
    /// The preferences file, when preferences are persisted at all.
    pub(crate) settings_file: Option<std::path::PathBuf>,
    /// Where the window was last seen, to open there next time.
    pub(crate) window: Option<settings::Window>,
    /// The wait for the window to be still before that is written.
    pub(crate) settings_task: Option<Task<()>>,
    _bounds_sub: Option<Subscription>,
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

impl BerthApp {
    /// The running app: empty, with the preferences as they were left and a
    /// file to write them to.
    pub fn new(tokens: Tokens, saved: settings::Data, cx: &mut Context<Self>) -> Self {
        Self::with_settings(tokens, Snapshot::empty(), saved, settings::path(), cx)
    }

    /// An app on `snapshot`, with `saved` preferences, written to
    /// `settings_file` when there is one. Tests and the screenshots start
    /// here with nothing saved and no file, so they neither read nor write
    /// the preferences of whoever runs them.
    pub(crate) fn with_settings(
        tokens: Tokens,
        snapshot: Snapshot,
        saved: settings::Data,
        settings_file: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = saved.appearance;
        let mut engine = EngineState::default();
        if !saved.socket.is_empty() {
            engine.preferred_socket = Some(saved.socket.clone());
        }
        let tokens = match appearance {
            Appearance::Light => berth_ui::theme::light(),
            Appearance::Dark => berth_ui::theme::dark(),
            Appearance::System => tokens,
        };
        berth_ui::theme::ActiveTokens::set(tokens, cx);
        let shell = cx.weak_entity();
        let app = Self {
            tokens,
            snapshot: Arc::new(snapshot),
            snapshot_id: 0,
            host: HostSample::default(),
            engine,
            appearance,
            appearance_sub: None,
            refresh_task: None,
            timer_task: None,
            host_task: None,
            discovery_task: None,
            terminal_task: None,
            mutation: MutationSlot::Idle,
            refresh_wanted: false,
            rescan_wanted: false,
            view: ViewState::new(&saved.socket, cx),
            inspector: Inspector::new(cx),
            page: cx.new(|_| Subview::page(shell.clone())),
            pane: cx.new(|_| Subview::inspector(shell.clone())),
            #[cfg(test)]
            draws: 0,
            window_handle: None,
            settings_file,
            window: saved.window,
            settings_task: None,
            _bounds_sub: None,
            tray: None,
            tray_task: None,
            quiet_notices_until: 0,
        };
        // The two views watch the shell; they can only start once it exists.
        let shell = cx.entity();
        app.page.update(cx, |page, cx| page.watch(&shell, cx));
        app.pane.update(cx, |pane, cx| pane.watch(&shell, cx));
        app
    }

    pub fn set_appearance_subscription(&mut self, subscription: Subscription) {
        self.appearance_sub = Some(subscription);
    }

    /// Keep the subscription that reports the window moving: dropped, it
    /// would silently stop.
    pub fn remember_bounds(&mut self, subscription: Subscription) {
        self._bounds_sub = Some(subscription);
    }

    /// Swap the snapshot in one step and bump its monotonic id so cached
    /// table rows re-evaluate without retaining old `Arc<Snapshot>`s.
    pub(crate) fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = Arc::new(snapshot);
        self.snapshot_id = self.snapshot_id.wrapping_add(1);
    }

    pub fn set_tokens(&mut self, tokens: Tokens, cx: &mut Context<Self>) {
        if self.tokens != tokens {
            self.tokens = tokens;
            // Entities that render on their own read the global, not `self`.
            berth_ui::theme::ActiveTokens::set(tokens, cx);
            cx.notify();
        }
    }

    /// Follow an OS appearance change only when the user chose `System`.
    pub fn apply_window_appearance(
        &mut self,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        if self.appearance == Appearance::System {
            self.set_tokens(berth_ui::theme::for_appearance(appearance), cx);
        }
    }

    /// Empty a table's search field.
    pub(crate) fn clear_search(&mut self, kind: SearchKind, cx: &mut Context<Self>) {
        self.view.lists[kind as usize].query.clear();
        self.view.lists[kind as usize]
            .search
            .update(cx, |input, cx| input.clear(cx));
    }

    /// A table's search field.
    pub(crate) fn search(&self, kind: SearchKind) -> &Entity<InputState> {
        &self.view.list(kind).search
    }

    /// What was typed into a table's search field.
    pub(crate) fn query(&self, kind: SearchKind) -> &str {
        &self.view.list(kind).query
    }

    pub(crate) fn switch_to(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.view.tab != tab {
            self.view.tab = tab;
            self.view.event_cursor = None;
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
