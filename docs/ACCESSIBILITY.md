# Accessibility

berth uses GPUI's AccessKit bridge. The shell publishes roles and labels rather
than relying on pixels:

| Region | Role | Label / state |
|---|---|---|
| Root | `Application` | "berth" |
| Rail | `Navigation` | "berth navigation" |
| Rail item | `Button` | page name + live figure, selected state |
| Page header | text | page title and count |
| Search fields | `TextInput` | placeholder ("Search containers", …) |
| Filter chips | `Button` | All / Running / Stopped, selected state |
| Table header/rows | `Row` | row's container/image/volume/network name |
| Bulk checkbox | `CheckBox` | container name, selected state |
| Status pill | text | state word (colour is reinforcement only) |
| Inspector | `Complementary` | "Container inspector" / object inspector |
| Inspector tabs/actions | `Button` | Facts, Inspect, Copy, Refresh, Stop, Delete, Terminal |
| Ports | `Button` | "Open port 8080" |
| Sheets | `Dialog` + `Button` | title, Cancel, Confirm/Create/Clean up |

Keyboard paths:

- Cmd/Ctrl+1…9 switch the first nine pages.
- Cmd/Ctrl+F focuses the current table's search field.
- Esc closes a sheet or clears the inspector.
- Enter submits a focused form / confirms a sheet.
- Arrow/Home/End/Delete/Backspace/Cmd-A/C/V/X work in text fields.

## What is not proven here

The test harness does not run xa11y or VoiceOver in this environment. Roles and
labels are in the code, but the accessibility tree has not been asserted by an
automated real-window test. Treat a manual VoiceOver pass as a release gate.
