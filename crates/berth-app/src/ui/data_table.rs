//! One table for every list page: a header built from its columns, and rows
//! drawn only as far as the window can show them, so that five thousand rows
//! cost a frame what a dozen do.
//!
//! A table is one tab stop for all its rows: a hundred rows must not mean a
//! hundred presses. The arrow keys move inside it, and the selected row is
//! what assistive technology reads out.

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, FocusHandle, FontWeight, Hsla, IntoElement, ListSizingBehavior, Role,
    SharedString, UniformListScrollHandle, div, px, uniform_list,
};

use super::kit;
use super::tabular;
use crate::app::BerthApp;

/// The rows a table keeps room for when the window is too short to show more:
/// below that, the page itself scrolls, and the headers go with it.
const LEAST_ROWS: f32 = 3.0;

/// How a column's cell is drawn: for the row at `ix` of the page's list, in
/// a column `width` wide.
pub(crate) type Cell = fn(&mut BerthApp, usize, f32, &mut Context<BerthApp>) -> AnyElement;

/// What opens the row at `ix` of the page's list, as a click on the row does.
pub(crate) type Open = fn(&mut BerthApp, usize, &mut Context<BerthApp>);

/// What a row is before its cells are drawn.
pub(crate) struct RowKey {
    /// Its element id, which tells two rows apart however the table is
    /// sorted.
    pub(crate) id: SharedString,
    /// What assistive technology calls it.
    pub(crate) name: String,
    /// Whether it is the one open in the inspector.
    pub(crate) selected: bool,
    /// Whether a click on it opens something: every row of a list does, and
    /// an event does when what it happened to is still there to open.
    pub(crate) opens: bool,
}

/// One column of a table.
pub(crate) struct Column {
    /// Names the header's element.
    key: &'static str,
    title: &'static str,
    width: f32,
    /// A figure sits against the end of its column.
    end: bool,
    /// The column of the table's sort its header sorts by, if it sorts.
    sort: Option<usize>,
    cell: Cell,
}

impl Column {
    /// A column whose header only names it.
    pub(crate) fn plain(
        key: &'static str,
        title: &'static str,
        width: f32,
        end: bool,
        cell: Cell,
    ) -> Self {
        Self {
            key,
            title,
            width,
            end,
            sort: None,
            cell,
        }
    }

    /// A column whose header sorts the table by its column `by`.
    pub(crate) fn sorted(
        key: &'static str,
        title: &'static str,
        width: f32,
        end: bool,
        by: usize,
        cell: Cell,
    ) -> Self {
        Self {
            sort: Some(by),
            ..Self::plain(key, title, width, end, cell)
        }
    }

    fn header(
        &self,
        table: usize,
        sort: (usize, bool),
        tokens: Tokens,
        cx: &mut Context<BerthApp>,
    ) -> AnyElement {
        match self.sort {
            Some(column) => {
                sortable_header(table, self, column, sort, tokens, cx).into_any_element()
            }
            None => header_cell(self, tokens).into_any_element(),
        }
    }
}

/// A table: its header and its rows, from the same columns.
pub(crate) struct DataTable {
    id: &'static str,
    label: &'static str,
    header_id: &'static str,
    header_label: Option<&'static str>,
    /// Which table's sort the headers turn.
    table: usize,
    columns: Vec<Column>,
    row: fn(&BerthApp, usize) -> RowKey,
    open: Open,
    /// The handle the shell keeps for the table, where it keeps one.
    focus: Option<FocusHandle>,
    /// Whether the table keeps a hairline for its focus ring to be drawn in.
    hairline: bool,
}

impl DataTable {
    pub(crate) fn new(
        id: &'static str,
        label: &'static str,
        header_id: &'static str,
        table: usize,
        row: fn(&BerthApp, usize) -> RowKey,
        open: Open,
    ) -> Self {
        Self {
            id,
            label,
            header_id,
            header_label: None,
            table,
            columns: Vec::new(),
            row,
            open,
            focus: None,
            hairline: false,
        }
    }

    pub(crate) fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    /// What assistive technology calls the header row, where that is more
    /// than its cells.
    pub(crate) fn header_label(mut self, label: &'static str) -> Self {
        self.header_label = Some(label);
        self
    }

    pub(crate) fn focus(mut self, handle: FocusHandle) -> Self {
        self.focus = Some(handle);
        self
    }

    pub(crate) fn hairline(mut self) -> Self {
        self.hairline = true;
        self
    }

    /// The header row, and the rule beneath it.
    pub(crate) fn header(
        &self,
        app: &BerthApp,
        tokens: Tokens,
        cx: &mut Context<BerthApp>,
    ) -> AnyElement {
        let sort = app.sort_for(self.table);
        let mut row = div()
            .id(self.header_id)
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .px(px(layout::ROW_PADDING))
            .gap(px(layout::ROW_GAP))
            .role(Role::Row);
        if let Some(label) = self.header_label {
            row = row.aria_label(label);
        }
        for column in &self.columns {
            row = row.child(column.header(self.table, sort, tokens, cx));
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(row)
            .child(div().w_full().h(px(1.0)).bg(tokens.border))
            .into_any_element()
    }

    /// The table: one tab stop, what sits above the rows, and the rows at
    /// `rows`, each an index into the page's list, `height` tall, in a list
    /// whose place `scroll` keeps.
    pub(crate) fn body(
        self,
        rows: Vec<usize>,
        height: f32,
        scroll: &UniformListScrollHandle,
        tokens: Tokens,
        cx: &mut Context<BerthApp>,
    ) -> AnyElement {
        let Self {
            id,
            label,
            table: _,
            columns,
            row,
            open,
            focus,
            hairline,
            ..
        } = self;
        let row_height = px(height);
        let count = rows.len();

        // A row is drawn for the window that shows it, and only then. The
        // shell is in hand while it is drawn, as it is for any other element.
        let draw = cx.processor(
            move |app: &mut BerthApp, shown: std::ops::Range<usize>, _window, cx| {
                shown
                    .filter_map(|position| {
                        let ix = *rows.get(position)?;
                        let key = row(app, ix);
                        let (selected, opens) = (key.selected, key.opens);
                        #[cfg(test)]
                        {
                            app.view.rows_drawn += 1;
                        }
                        Some(
                            div()
                                .id(key.id)
                                .flex()
                                .flex_row()
                                .items_center()
                                .w_full()
                                .h(row_height)
                                .px(px(layout::ROW_PADDING))
                                .gap(px(layout::ROW_GAP))
                                .rounded(px(layout::RADIUS_ROW))
                                .bg(shade(position, selected).background(tokens))
                                .when(opens && !selected, |this| {
                                    this.hover(move |style| style.bg(tokens.surface_hover))
                                })
                                .role(Role::Row)
                                .aria_label(key.name)
                                .aria_selected(selected)
                                // While the table has focus it speaks for its
                                // selected row.
                                .when(selected, |this| this.aria_active_descendant())
                                .when(opens, |this| {
                                    this.on_click(
                                        cx.listener(move |this, _, _, cx| open(this, ix, cx)),
                                    )
                                })
                                .children(
                                    columns
                                        .iter()
                                        .map(|column| (column.cell)(app, ix, column.width, cx)),
                                )
                                .into_any_element(),
                        )
                    })
                    .collect::<Vec<_>>()
            },
        );

        let list = uniform_list(
            SharedString::from(format!("{id}-rows")),
            count,
            move |shown, window, cx| draw(shown, window, cx),
        )
        .size_full()
        // Its size is the room the page leaves, not a sum of its rows, so the
        // rows are measured once, when it is painted, and not again for layout.
        .with_sizing_behavior(ListSizingBehavior::Auto)
        .track_scroll(scroll);

        // The table is one tab stop: a hundred rows must not mean a hundred
        // presses. The arrow keys move inside it.
        let frame = kit::focused(div().id(id), focus.as_ref(), tokens.focus_ring(), true);
        let frame = if hairline {
            frame
                .rounded(px(layout::RADIUS_ROW))
                // The hairline that the focus ring is drawn in.
                .border_1()
                .border_color(tokens.invisible())
        } else {
            frame
        };
        frame
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .role(Role::Table)
            .aria_label(label)
            .child(div().flex_1().min_h(px(height * LEAST_ROWS)).child(list))
            .into_any_element()
    }
}

/// How a row is shaded. Striping follows the row's position on screen, not
/// its place in the snapshot: a sorted or filtered table still alternates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shade {
    Selected,
    Alt,
    Plain,
}

pub(crate) fn shade(position: usize, selected: bool) -> Shade {
    if selected {
        Shade::Selected
    } else if position % 2 == 1 {
        Shade::Alt
    } else {
        Shade::Plain
    }
}

impl Shade {
    fn background(self, tokens: Tokens) -> gpui::Hsla {
        match self {
            Shade::Selected => tokens.selected_row(),
            Shade::Alt => tokens.alt_row(),
            Shade::Plain => tokens.surface,
        }
    }
}

fn header_cell(column: &Column, tokens: Tokens) -> Div {
    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .w(px(column.width))
        .h(px(layout::TABLE_HEADER))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.muted)
                .child(column.title),
        );
    if column.end {
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
    }
    cell
}

/// A header that sorts. It is a control: Tab reaches it, and Enter or Space
/// sorts by it.
fn sortable_header(
    table: usize,
    column: &Column,
    by: usize,
    sort: (usize, bool),
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement + use<> {
    let active = sort.0 == by;
    let arrow = if active {
        if sort.1 { " ▲" } else { " ▼" }
    } else {
        ""
    };
    let aria = if active {
        format!(
            "{}, sorted {}",
            column.title,
            if sort.1 { "ascending" } else { "descending" }
        )
    } else {
        column.title.to_owned()
    };
    let mut cell = div()
        .id((column.key, table as u64))
        .flex()
        .flex_row()
        .items_center()
        .w(px(column.width))
        .h(px(layout::CONTROL_SMALL))
        .rounded(px(layout::RADIUS_ROW))
        // The hairline that the focus ring is drawn in.
        .border_1()
        .border_color(tokens.invisible())
        .role(Role::ColumnHeader)
        .aria_label(aria)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_sort(table, by, cx);
        }))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if active { tokens.accent } else { tokens.muted })
                .child(format!("{}{arrow}", column.title)),
        );
    if column.end {
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
    }
    kit::focusable(cell, tokens.focus_ring())
}

pub(crate) fn text_cell(text: String, width: f32, align_end: bool, tokens: Tokens) -> Div {
    let mut cell = div().flex().flex_row().items_center().w(px(width)).child(
        div()
            .text_size(px(type_scale::BODY))
            .font_features(tabular())
            .text_color(tokens.text)
            .truncate()
            .child(text),
    );
    if align_end {
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
    }
    cell
}

pub(crate) fn mono_cell(text: String, width: f32, align_end: bool, tokens: Tokens) -> Div {
    let mut cell = div().flex().flex_row().items_center().w(px(width)).child(
        div()
            .text_size(px(type_scale::MONO))
            .font_family(type_scale::MONO_FAMILY)
            .font_features(tabular())
            .text_color(tokens.text)
            .truncate()
            .child(text),
    );
    if align_end {
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
    }
    cell
}

/// A status or action pill inside a fixed-width table column, so columns to
/// its right stay aligned with their headers regardless of pill text width.
pub(crate) fn pill_cell(state: &str, width: f32, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w(px(width))
        .overflow_hidden()
        .child(pill(state, tokens))
}

pub(crate) fn pill(state: &str, tokens: Tokens) -> Div {
    let color = status_color(tokens, state);
    let bg = if color == tokens.danger_text() {
        tokens.danger_wash()
    } else {
        tokens.pill_wash(color)
    };
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(layout::PILL_HEIGHT))
        .px(px(layout::PILL_PADDING))
        .rounded(px(layout::PILL_HEIGHT / 2.0))
        .bg(bg)
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(state.to_owned()),
        )
}

pub(crate) fn status_color(tokens: Tokens, state: &str) -> Hsla {
    match state {
        "running" | "healthy" | "ready" | "start" | "unpause" => tokens.success_text(),
        "exited" | "stopped" => tokens.muted_text_pill(),
        "paused" | "restarting" | "starting" | "dangling" | "stop" | "restart" => {
            tokens.warning_text()
        }
        "dead" | "unhealthy" | "die" | "oom" | "kill" | "destroy" => tokens.danger_text(),
        "created" | "create" | "pull" => tokens.accent_text_pill(),
        _ => tokens.muted_text_pill(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn striping_follows_the_position_on_screen_not_the_snapshot() {
        // Rows two and four of the snapshot, shown as the first and second
        // rows of a filtered table: they still alternate.
        for (position, expected) in [(0, Shade::Plain), (1, Shade::Alt), (2, Shade::Plain)] {
            assert_eq!(shade(position, false), expected);
        }
        // Whatever its position, the open row is the selected one.
        assert_eq!(shade(0, true), Shade::Selected);
        assert_eq!(shade(3, true), Shade::Selected);
    }

    #[test]
    fn status_pills_distinguish_image_and_event_semantics() {
        for tokens in [berth_ui::theme::light(), berth_ui::theme::dark()] {
            assert_ne!(
                status_color(tokens, "ready"),
                status_color(tokens, "dangling")
            );
            assert_ne!(status_color(tokens, "start"), status_color(tokens, "die"));
            assert_eq!(status_color(tokens, "oom"), tokens.danger_text());
            assert_eq!(status_color(tokens, "kill"), tokens.danger_text());
        }
    }
}
