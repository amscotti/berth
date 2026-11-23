//! What the inspector shows for whatever is selected, and the job that fills
//! it in: the logs, the inspect document, and the facts the list leaves out.
//!
//! Nothing here reads a socket: the shell's detail job does, and hands the
//! result to `Inspector`, which checks it is for what is still selected.

use gpui::prelude::*;
use gpui::{Entity, ScrollHandle, Subscription, Task};

use crate::app::InspectorTab;
use crate::ui::input::{InputEvent, InputState};

pub(crate) struct Inspector {
    pub(crate) tab: InspectorTab,
    pub(crate) logs_text: String,
    pub(crate) logs_loading: bool,
    pub(crate) logs_follow: bool,
    pub(crate) logs_timestamps: bool,
    pub(crate) log_search: Entity<InputState>,
    _log_search_sub: Subscription,
    pub(crate) inspect_text: Option<String>,
    pub(crate) inspect_loading: bool,
    pub(crate) inspect_facts: Option<berth_engine::detail::InspectFacts>,
    pub(crate) detail_top: Option<berth_engine::dto::ContainerTop>,
    pub(crate) detail_changes: Vec<berth_engine::dto::FsChange>,
    pub(crate) image_layers: Vec<berth_engine::dto::ImageLayer>,
    pub(crate) image_layers_loading: bool,
    /// What a detail job must still be for when it answers.
    pub(crate) detail_generation: u64,
    pub(crate) detail_task: Option<Task<()>>,
    /// The inspector's scroll handles, so Follow can keep the logs at their end.
    pub(crate) log_scroll: ScrollHandle,
    pub(crate) inspect_scroll: ScrollHandle,
}

impl Inspector {
    pub(crate) fn new(cx: &mut gpui::Context<crate::app::BerthApp>) -> Self {
        let log_search = cx.new(|cx| InputState::new("Search logs", 130.0, cx));
        let _log_search_sub = cx.subscribe(&log_search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed | InputEvent::Edited) {
                let _ = input.read(cx).text();
                this.redraw_inspector(cx);
            } else if matches!(event, InputEvent::Cancelled) {
                input.update(cx, |state, cx| state.clear(cx));
            }
        });

        Self {
            tab: InspectorTab::Facts,
            logs_text: String::new(),
            logs_loading: false,
            logs_follow: true,
            logs_timestamps: false,
            log_search,
            _log_search_sub,
            inspect_text: None,
            inspect_loading: false,
            inspect_facts: None,
            detail_top: None,
            detail_changes: Vec::new(),
            image_layers: Vec::new(),
            image_layers_loading: false,
            detail_generation: 0,
            detail_task: None,
            log_scroll: ScrollHandle::new(),
            inspect_scroll: ScrollHandle::new(),
        }
    }
}
