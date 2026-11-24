//! What the person is looking at, and what they are doing about it: the page,
//! the filter, each table's search and sort, the selection, the forms and
//! sheets, the toasts, and where keyboard focus is.
//!
//! The engine's answer is the shell's (`BerthApp`); this is everything that
//! isn't. Nothing here reads a socket or starts a job: it asks the shell to.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Entity, FocusHandle, Subscription, Task, UniformListScrollHandle};

use crate::actions::Pending;
use crate::app::{BerthApp, CreateKind, DetailKind, Filter, SearchKind, Tab, Toast};
use crate::focus::SheetFocus;
use crate::jobs::mutation::PruneOptions;
use crate::state::SharedSnapshot;
use crate::ui::input::{InputEvent, InputState};

/// One table's own state: its search field, what was typed into it, and how
/// the table is sorted.
pub(crate) struct ListState {
    pub(crate) search: Entity<InputState>,
    pub(crate) query: String,
    /// The column it is sorted by, and whether ascending. Events are always
    /// newest first and never look at it.
    pub(crate) sort: (usize, bool),
    /// The rows on screen, kept from one frame to the next while what they
    /// were worked out from holds.
    pub(crate) kept: Kept,
    /// Where the table is scrolled to, kept from one frame to the next, and
    /// the way to bring a row into view.
    pub(crate) scroll: UniformListScrollHandle,
}

/// A table's rows, worked out from the snapshot and the table's own query,
/// sort and filter, and kept from one frame to the next while those hold.
///
/// A commit makes a new snapshot, and a row depends on nothing else that
/// changes behind the settings' back, so comparing those is comparing
/// everything the rows depend on.
#[derive(Default)]
pub(crate) struct Kept(RefCell<Option<Worked>>);

struct Worked {
    /// The snapshot, by identity.
    from: SharedSnapshot,
    /// The query, lowercased.
    query: String,
    sort: (usize, bool),
    filter: Filter,
    rows: Vec<usize>,
}

impl Kept {
    /// The rows, each an index into the table's list. `figure` works them
    /// out when anything they depend on has changed, and only then.
    pub(crate) fn rows(
        &self,
        from: &SharedSnapshot,
        query: &str,
        sort: (usize, bool),
        filter: Filter,
        figure: impl FnOnce() -> Vec<usize>,
    ) -> Vec<usize> {
        {
            let worked = self.0.borrow();
            if let Some(worked) = worked.as_ref()
                && Arc::ptr_eq(&worked.from, from)
                && query.eq_ignore_ascii_case(&worked.query)
                && worked.sort == sort
                && worked.filter == filter
            {
                return worked.rows.clone();
            }
        }
        let rows = figure();
        *self.0.borrow_mut() = Some(Worked {
            from: from.clone(),
            query: query.to_ascii_lowercase(),
            sort,
            filter,
            rows: rows.clone(),
        });
        rows
    }
}

impl Kept {
    /// The rows as they were last worked out, if they were.
    pub(crate) fn peek(&self) -> Option<Vec<usize>> {
        self.0.borrow().as_ref().map(|worked| worked.rows.clone())
    }
}

impl ListState {
    /// Sort by `col`: the other way round if it already was, ascending if not.
    pub(crate) fn toggle_sort(&mut self, col: usize) {
        let (current, ascending) = self.sort;
        self.sort = if current == col {
            (col, !ascending)
        } else {
            (col, true)
        };
    }
}

/// The sheet that is open. There is one slot for it (`ViewState::sheet`), so
/// two sheets are never open at once, and whatever only a sheet needs lives
/// in its variant: what a confirmation is asking, whether the create form
/// has been tried, what a cleanup has ticked. A sheet opened over another
/// replaces it whole, and nothing of the first is left to find later.
#[derive(Clone, Debug)]
pub(crate) enum Sheet {
    /// "Delete web?": a destructive action, held until it is confirmed.
    Confirm(Pending),
    /// A new volume or network. `attempted` is set once Create was pressed,
    /// so an invalid name gets its sentence, and not before.
    Create { kind: CreateKind, attempted: bool },
    /// Clean up unused data, with what is ticked for it.
    Prune(PruneOptions),
}

impl Sheet {
    /// What the dialog is called, which is also its name to assistive
    /// technology.
    pub(crate) fn label(&self) -> String {
        match self {
            Sheet::Confirm(pending) => pending.title(),
            Sheet::Create { kind, .. } => format!("New {}", kind.noun()),
            Sheet::Prune(_) => "Clean up unused data".to_owned(),
        }
    }
}

pub(crate) struct ViewState {
    /// The real app opens on Overview once the graphs exist. Until then the
    /// table is the fully drawn surface.
    pub(crate) tab: Tab,
    pub(crate) filter: Filter,
    /// In `SearchKind` order: containers, images, volumes, networks, events.
    pub(crate) lists: [ListState; SearchKind::ALL.len()],
    /// What is open in the inspector, by id, and which kind of thing it is.
    pub(crate) selected_id: Option<String>,
    pub(crate) selected_name: String,
    pub(crate) detail_kind: DetailKind,
    pub(crate) bulk_enabled: bool,
    pub(crate) bulk_selected: BTreeSet<String>,
    pub(crate) toast: Option<Toast>,
    pub(crate) toast_task: Option<Task<()>>,
    /// The sheet that is open, when one is. One slot, so two are never open
    /// at once.
    pub(crate) sheet: Option<Sheet>,
    /// How many table rows have been drawn. Tests read it, to hold a table to
    /// drawing no more than the window shows.
    #[cfg(test)]
    pub(crate) rows_drawn: usize,

    // The forms: Runtimes' custom socket, and Images' pull and run.
    pub(crate) socket_input: Entity<InputState>,
    pub(crate) creation_open: bool,
    pub(crate) pull_input: Entity<InputState>,
    pub(crate) run_image: Entity<InputState>,
    pub(crate) run_name: Entity<InputState>,
    pub(crate) run_ports: Entity<InputState>,
    pub(crate) run_volumes: Entity<InputState>,
    pub(crate) run_env: Entity<InputState>,
    pub(crate) pull_attempted: bool,
    pub(crate) run_attempted: bool,
    /// How many env values a clone left empty because they were redacted.
    pub(crate) run_env_missing: usize,

    pub(crate) create_name: Entity<InputState>,

    // Keyboard focus: the page's own handle (rail, table and inspector; focus
    // lives here unless a sheet is open), the container table's, and the
    // sheets'.
    pub(crate) focus_handle: FocusHandle,
    /// One stop for all the table's rows.
    pub(crate) table_focus: FocusHandle,
    pub(crate) sheet_focus: SheetFocus,
    pub(crate) _search_subs: [Subscription; SearchKind::ALL.len()],
    pub(crate) _form_subs: Vec<Subscription>,
}

impl ViewState {
    /// One table's own state.
    pub(crate) fn list(&self, table: SearchKind) -> &ListState {
        &self.lists[table as usize]
    }

    /// The view as it starts, with `socket` already in the custom-socket
    /// field when one was remembered.
    pub(crate) fn new(socket: &str, cx: &mut gpui::Context<BerthApp>) -> Self {
        let searches =
            SearchKind::ALL.map(|kind| cx.new(|cx| InputState::new(kind.placeholder(), 190.0, cx)));
        let _search_subs = std::array::from_fn(|index| {
            let search = &searches[index];
            cx.subscribe(
                search,
                move |this, input, event: &InputEvent, cx| match event {
                    InputEvent::Changed | InputEvent::Edited => {
                        this.view.lists[index].query = input.read(cx).text().to_owned();
                        this.redraw_page(cx);
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
        if !socket.is_empty() {
            socket_input.update(cx, |input, cx| input.set_value(socket, cx));
        }
        let mut _form_subs = Vec::with_capacity(8);
        _form_subs.push(cx.subscribe(
            &create_name,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Submitted => this.submit_create(cx),
                InputEvent::Cancelled => this.close_create(cx),
                InputEvent::Changed | InputEvent::Edited => {}
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
        _form_subs.push(cx.subscribe(&run_env, |this, _, event: &InputEvent, cx| {
            // Only the user's own edit retires the note: the clone that
            // filled the field in must not clear it a moment later.
            if matches!(event, InputEvent::Edited) && this.view.run_env_missing != 0 {
                this.view.run_env_missing = 0;
                cx.notify();
            }
        }));
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

        Self {
            tab: Tab::Containers,
            filter: Filter::All,
            lists: std::array::from_fn(|index| ListState {
                search: searches[index].clone(),
                query: String::new(),
                sort: (0, true),
                kept: Kept::default(),
                scroll: UniformListScrollHandle::new(),
            }),
            selected_id: None,
            selected_name: String::new(),
            detail_kind: DetailKind::Container,
            bulk_enabled: false,
            bulk_selected: BTreeSet::new(),
            toast: None,
            toast_task: None,
            sheet: None,
            #[cfg(test)]
            rows_drawn: 0,
            socket_input,
            creation_open: false,
            pull_input,
            run_image,
            run_name,
            run_ports,
            run_volumes,
            run_env,
            pull_attempted: false,
            run_attempted: false,
            run_env_missing: 0,
            create_name,
            focus_handle: cx.focus_handle(),
            table_focus: cx.focus_handle().tab_stop(true),
            sheet_focus: SheetFocus::new(cx),
            _search_subs,
            _form_subs,
        }
    }
}
