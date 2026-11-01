# A UI/UX review, done properly

Every view, both themes, judged against the design contract rather than against
the code. Nine defects; all fixed. Screenshots at the end of each row are the
evidence, in `docs/ui/`.

## What the surface is

This is a **Monitor** surface: the user is watching state change. Density and
glanceability beat decoration, numbers outrank prose, and no composition should
need explaining. Rows read *what is it · is it healthy · how much · what can I
do about it* in that order, left to right.

## Slop audit (lower is better)

| Tell | Fired? |
|---|---|
| Tech gradient, glassmorphism, blob art | no |
| Generic indigo accent by default | partly — the accent is blue, chosen to match platform chrome, not the model's favourite violet |
| Equal-weight feature-tile grid | partly — the Overview is four equal cards, but they carry live graphs, not copy |
| Accent rail / icon topper / center stack | no |
| Monument stat | no — large numbers are the point on a monitor |
| Wrong surface | no |

**Score: 2/10**, and both fired tells are the cards' sameness rather than
tell-tale decoration. Left as is deliberately: four equal graphs is the right
composition when the four series genuinely rank equally.

## Defects found, and what was done

**1. The Disk view's explanation card was clipped off the window.**
`defer row.end()` closed the metrics row *after* the note, so the note became a
sixth item in a horizontal row and was squeezed to 96pt at the right edge. The
third time this exact trap has bitten; the row now ends explicitly, with a
comment saying why. The reclaimed space holds a four-part breakdown strip, so
the view answers "how is it split", not only "how much".

**2. Images: the STATUS column drifted 30pt from its header.**
A `pill` sizes itself to its word; the header slot was fixed at 120. Every
intrinsic-width widget shifts everything after it, so the fix is the fixed-slot
`pillCell` — the same fix the containers table already needed.

**3. The API's vocabulary was leaking into the interface.**
`<none>:<none>` for an untagged image and `null` for a network with no driver.
Now "untagged" and an em dash. An interface that speaks only in the API's
dialect asks the user to learn the dialect.

**4. Published ports ran into the action buttons.**
`8080→80, 808…` collided with Stop. The ports column now takes the width the
PROJECT column no longer needs.

**5. A column of em dashes.**
PROJECT only exists for Compose groupings, so it was absent-value noise on every
plain container. It is now hidden unless something is actually grouped, which
buys the ports column 90pt.

**6. The same binding listed twice.**
Docker reports a port once per address family, so a container published on IPv4
and IPv6 read `8080→80, 8080→80`. Deduplicated on the public/private pair, and
the "+N more" count now counts real ports instead of raw entries.

**7. Dark mode had never actually been looked at.**
The earlier "dark" screenshots were light: the snapshot path takes the app's
window theme, so `--nanoui-dark` was overridden by the app's own `themeFor`.
`BERTH_THEME=dark|light` now exists as an explicit override — it wins for one
run, which is what a screenshot needs and also what someone whose desktop does
not report a theme needs.

**8. The dark theme's unlit graph grid was louder than the data.**
A 45% track that reads as a quiet baseline on white reads as noise on
`#1c1c1e`. The unlit alpha is now chosen from the background's brightness.

**9. Empty states were inconsistent in tone.**
Now every one names the state and the next action:
*No volumes — volumes appear here once a container declares one*,
*None match this filter — All shows every container*.

## What is still wrong, knowingly

- **The container list is a table, not a monitor.** With forty containers it
  will read as a spreadsheet. A compact/comfortable density switch is the fix,
  and it is not built.
- **No text search.** Filters only. Typing a name is the fastest way to find a
  container and it does not exist yet.
- **Actions are always visible on every row.** Right for three containers,
  noisy for thirty. A row-level menu is the usual answer; it costs a click and
  buys calm.
- **The Overview's four cards are equal weight**, which is honest but flat:
  nothing says which figure matters most right now.
- **Column headers and cells land within a pixel or two**, not exactly, because
  the pill inset is per-widget. It reads as deliberate inset rather than drift,
  which is why it was left.

## Method

Snapshots were taken of all nine views through the headless path
(`BERTH_TAB=… --nanoui-snapshot=…`), in both themes, plus one run with no engine
reachable to exercise the error state. Each was read as an image, not inferred
from code. The defects above are the ones that survived that reading; the fixes
were re-snapshotted and re-read.
