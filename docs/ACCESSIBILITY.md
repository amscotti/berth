# Accessibility

berth uses GPUI's AccessKit bridge. The shell publishes roles and labels rather
than relying on pixels:

| Region | Role | Label / state |
|---|---|---|
| Root | `Application` | "berth" |
| Rail | `Navigation` | "berth navigation" |
| Rail pages | `TabList` | "Pages", vertical |
| Rail item | `Tab` | page name + live figure, selected state |
| Page header | text | page title and count |
| Search fields | `TextInput` | placeholder ("Search containers", …); with text, a `Button` named "Clear search containers" |
| Buttons | `Button` | the label; an icon-only button is named for what it does ("Close inspector"); a port is "Open port 8080" |
| Filter chips | `RadioButton` | All / Running / Stopped, toggled |
| Appearance, engine choice | `RadioButton` in a `RadioGroup` | toggled |
| Inspector tabs | `Tab` in a `TabList` | Facts / Config / Runtime / Inspect, selected |
| Follow, Times | `Button` | toggled |
| Sortable headers | `ColumnHeader` | "NAME", or "NAME, sorted ascending" |
| Table | `Table` | "Containers" and so on; one tab stop, whose selected row is what it reads out |
| Table header / rows | `Row` | row's container/image/volume/network name, selected state |
| Bulk checkbox | `CheckBox` | "Select web", toggled |
| Cleanup options | `CheckBox` | "Unused images, 1 · 13.0 MiB", toggled |
| A row that leads somewhere | `Link` | "Inspect container web", "Inspect volume shop-data" |
| "Get and run images" | `DisclosureTriangle` | expanded state |
| Status pill | text | state word (colour is reinforcement only) |
| Inspector | `Complementary` | "Container inspector" / object inspector |
| Toast | `Status`, or `Alert` for bad news | the notice |
| Sheets | `Dialog` + its controls | title, Cancel, Confirm / Create / Clean up |

A control that can't be used is drawn at 45% and its description says why; Tab
passes it by.

Keyboard paths:

- Cmd/Ctrl+1…9 switch the first nine pages.
- Cmd/Ctrl+F focuses the current table's search field.
- Tab and Shift+Tab cross the rail's pages, the page action, the search, the
  chips, the sortable headers, then the table, as one stop for all its rows,
  then the inspector. (GPUI binds nothing to Tab; the app does.)
- Enter and Space press the control that has focus; Enter or Space on a
  header sorts by it.
- In a field: a click puts the caret where it is clicked, a drag selects, a
  second click selects the word, and the caret stays in view in long text.
  Cmd/Ctrl+Z steps back a word at a time, Cmd+Shift+Z (Ctrl+Y) steps forward
  again. Tab from a field with text reaches its clear button; pressing it
  empties the field and gives it focus back.
- Up and Down move the table's selection, and the list scrolls to keep the
  selected row on screen.
- Esc closes a sheet, empties a focused field, or clears the inspector.
- Enter submits a focused form.
- While a sheet is open focus is inside it, Tab and Shift+Tab cross only its
  controls, none of the page's shortcuts fire, and focus returns to where it
  was when the sheet closes. A destructive confirmation opens on Cancel.

The focus ring is one pixel in the text colour, drawn in each control's own
hairline, and shows for the keyboard only. Every control has one: the kit
builds it in, and a view can give it to the rare control that isn't the kit's.

## What is not proven here

The test harness does not run xa11y or VoiceOver in this environment. Roles and
labels are in the code, and `focus_tests.rs` asserts where keyboard focus goes,
but the accessibility tree has not been read by an automated real-window test.
Treat a manual VoiceOver pass as a release gate.
