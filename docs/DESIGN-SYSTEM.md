# Secblitz design system (0.7.0)

Guide for page authors. Code: `src/gui/theme.rs` (tokens) and
`src/gui/widgets/` (components). Motion: `docs/MOTION.md` and
`widgets::anim` (written in parallel; follow them for anything that moves).

The audience is people who barely know how to use a PC. Everything should feel
like a calm, native Windows 11 product: neutral colours, one clear action per
screen, plain words.

## 1. Tokens

Never write a raw pixel number or colour in a page. Use these.

### Spacing (`theme::S*`)

| Token | px | Typical use |
|-------|----|-------------|
| `S1`  | 4  | title to subtitle, icon-only padding, gap between nav items |
| `S2`  | 8  | icon to label, gap in a button row |
| `S3`  | 12 | group heading to rows, icon to text in tight spots |
| `S4`  | 16 | row side padding, icon to title in a row, button side padding |
| `S5`  | 20 | empty-state side padding |
| `S6`  | 24 | region padding, sheet padding, page header to content |
| `S8`  | 32 | page top/bottom padding, gap between groups and regions |
| `S10` | 40 | page side padding |

Rules:

- Region padding `S6`. Gap between a region and the next block `S8`.
- Group heading to its rows `S3`; rows inside a group `S1` apart.
- Page header to first content `S6`.
- Buttons in a row `S2`; footer action bar buttons `S2`, right aligned, `S4` above it.
- Old names `GAP` (=S4), `PAD` (=S5), `RADIUS` (=R_LARGE), `RADIUS_SMALL` (=R) still
  compile for pages that have not migrated. Do not use them in new code.

### Sizes

| Token | px | |
|-------|----|-|
| `CONTROL` | 36 | buttons, dropdowns, text fields, switches' row, segmented control (outer) |
| `CONTROL_SMALL` | 28 | segments inside the segmented control |
| `ROW` | 48 | minimum compact list row height |
| `ROW_ITEM` | 56 | minimum `row_item` height |
| `ICON_ROW` | 20 | plain row icon edge |
| `MENU_ROW` | 32 | popup-menu row |
| `CHECK` | 18 | checkbox box |

### Radii

`R_SMALL` 6 (inner pieces such as segments, menu rows), `R` 8 (controls,
notices, list rows, menus), `R_LARGE` 12 (regions, sheets), `R_PILL` (pills).

### Type (IBM Plex Sans, SIL OFL 1.1, bundled in the exe)

| Token | px | Font | Use |
|-------|----|------|-----|
| `DISPLAY` | 28 | SemiBold/Bold | the score number only |
| `H1` | 22 | SemiBold | page title (`page_header`) |
| `H2` | 17 | SemiBold | card title (`h2`) |
| `BODY` | 14 | Regular / Medium | all reading text, buttons |
| `SMALL` | 12.5 | Regular / Medium | captions, pills, helper text |

Plex's natural line height is 1.3 em, which iced applies by default. Fixed
height controls set an absolute line height (`LINE_BODY` 18, `LINE_SMALL` 16)
so 36 px stays exactly 36 px. Use at most three weights on a screen.

### Colour (`Palette`, light default + neutral dark)

Colour only carries meaning. Surfaces are near-identical neutral greys a few
percent apart; the only chromatic colours are the status tones.

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `bg` | #F7F7F8 | #0E0E10 | the page. Most content sits directly on it. |
| `sidebar` | #F1F1F3 | #0A0A0C | navigation rail |
| `surface` | #FFFFFF | #161618 | a `region` (hero / primary block), sheets |
| `surface_alt` | #F1F1F3 | #1E1E21 | fields, wells, expander bodies |
| `hover` / `hover_strong` | #F0F0F2 / #E8E8EB | #1C1C1F / #252528 | row hover, secondary button rest / hover |
| `pressed`, `selected` | #E1E1E5, #EAEAED | #2D2D31, #26262A | pressed, active nav item |
| `popup` | #FFFFFF | #212124 | menus and dropdown lists (floating layers) |
| `border` | #E8E8EB | #2A2A2E | **hairline**: popup outline and the rare divider only |
| `border_strong` | #DEDEE2 | #36363B | legacy input outline; prefer a filled field |

- Text: `text`, `text_muted`. Focus: `focus_ring`. Disabled: `disabled_bg`, `disabled_fg`.
- Actions: `brand` (near-black / white) with `brand_hover`, `brand_pressed`,
  `on_brand`; `danger*` (solid, white text).
- Status: `good`, `warn`, `bad`, `neutral` via `Tone`; `*_text` for small text;
  `p.tint(tone)` only behind a status `pill`, never behind an icon.

Hard rules: no box shadows (tiny-skia redraw accumulates them: never create a
`Shadow` with alpha > 0), no blur, no gradients, no large translucent overlays
that change often. The modal scrim is the only translucent full-window layer
and it is static.

### Borderless, tonal (design round 3)

Findings from Windows 11 Settings and Fluent 2 (layered tonal surfaces, rows
with a soft hover fill, a heading above a list instead of a box around it),
Linear and Arc (hairline-free lists, whitespace as the main separator, hover
as the only affordance) and the 2024-25 consumer AV dashboards (Malwarebytes 5,
Norton 360, Bitdefender: one big status area, everything else a flat list with
a single accent button). Rules we took from them:

1. **No borders, no boxes.** Nothing but a popup carries an outline. Tone
   (`bg` < `surface` < `surface_alt`, a few percent apart) and whitespace
   separate things.
2. **Few regions.** At most one or two `region`s per page (the hero status, the
   main task). All other content is a `group` straight on the page.
3. **Rows, not cards.** A list is `row_item`s `S1` apart with a soft tonal hover.
4. **Icons are plain.** A 20 px tinted glyph; no circle or square behind it.
5. **One primary per region**; secondary actions go in the `overflow_menu`.
6. **Long data collapses** (`collapsible`) with a one-line summary.
7. **Short labels**: two or three words on buttons and menu entries.

(Reference gathering was done from knowledge of these products; no Mobbin
screens were pulled in this pass.)

## 2. Components (`crate::gui::widgets`)

| Widget | When to use |
|--------|-------------|
| `region(p, content)` | a borderless tonal block (`surface`, `R_LARGE`, `S6`). One or two per page: hero status, main task. Never nest. |
| `card(p, content)` | legacy adapter for `region`. Do not use in new code. |
| `group(p, title, subtitle, trailing, rows)` | a titled set of rows with no box. The default way to lay out a page body. |
| `row_item(p, icon, title, subtitle, trailing, on_press)` | one 56 px row: plain icon, title, subtitle, trailing control. Whole row is clickable with `on_press`; otherwise it only gets a soft hover. `row_item_tinted` adds a status tint to the icon. |
| `collapsible(p, title, summary, open, msg, body)` | disclosure header with a chevron that turns 0 to 90 degrees. Use for any list over about six rows: collapsed by default, `summary` like "24 items". The page stores `open`. |
| `limited(items, n, expanded)` + `show_more_button(p, label, msg)` | show the first `n` rows and a quiet "Show 12 more" / "Show less" under them. |
| `overflow_menu(p, [(icon, label, msg, danger)])` | icon-only "More" (three dots) button opening a small popup. All secondary actions live here. Closes on outside click, Esc, selection. |
| `hoverable(p, content)` | soft hover fill behind a non-clickable row that holds controls. |
| `h1/h2/body/muted/small(p, text)` | text. Do not build raw `text()` styles in pages. |
| `page_header(p, title, subtitle)` | first element of every page. |
| `section_label(p, text)` | small caption above a group of rows. |
| `action(p, kind, label, icon, on_press)` | all buttons. `Primary` once per screen; `Secondary` for the rest; `Ghost` for quiet extras; `Danger` only to confirm removals/undo. `on_press: None` = disabled. |
| `icon_button(p, kind, icon, on_press)` | 36x36 icon-only button; only with a nearby label. |
| `link(p, label, msg)` | in-app "See all" style navigation (ghost button, arrow cursor). |
| `hyperlink(p, label, msg)` | opens a web page. The only widget with the hand cursor. |
| `list_button(p, row, msg)` | whole row clickable; hover and pressed fills (prefer `row_item`). |
| `icon_badge(p, icon, tone)` | legacy name: now a plain 20 px tinted icon, no background. Prefer `row_item`. |
| `pill(p, text, tone)` | short status word ("Protected"). Very light tint, no border; the only thing with a tinted background. |
| `inline_notice(p, tone, text)` | calm tip/warning/error inside a page. |
| `empty_state(p, icon, title, body, action)` | nothing to show yet. |
| `progress_row(p, text, state)` / `bar(p, ratio, tone)` | checklists and a thin progress bar. |
| `expander(p, title, open, msg, content)` | small "More details" text toggle. Keep closed by default. For long data use `collapsible`. |
| `dropdown(p, &options, selected, placeholder, on_select)` | choose one of many (language, schedule). |
| `segmented(p, &[(value, label)], selected, on_select)` | choose one of 2-4 (Light / Dark). |
| `switch(p, on, Some(\|v\| msg))` | on/off setting that applies immediately. `None` = disabled. |
| `checkbox(p, CheckState, label, msg)` | pick several; `Mixed` for a group header. |
| `text_field(p, placeholder, value, on_input)` | single line input (I-beam cursor). |
| `ring::ring(Ring{..}, size)` | the score ring (cached canvas). |
| `sheet(p, base, content)` | modal panel above a page. Esc closes it (shell). |
| `toast` | built by the shell from `ctx.toast`; pages only set it. |
| `arrow(element)` | force the normal arrow cursor on a custom clickable element. |

### Cursor

Windows convention, enforced in code: arrow over buttons, rows, tabs, toggles,
checkboxes and dropdowns (they are wrapped with `arrow`); I-beam only in text
fields; hand only for `hyperlink`. If you build your own clickable piece from
`iced::widget::button`, wrap it in `widgets::arrow(...)`.

### Dropdown note

Menu rows are as tall as the control (36 px) because iced derives both from
one padding value. The list is a floating layer, so it may keep a hairline
outline; hover row = `surface_alt`.
The row for the current value is highlighted when the menu opens.

## 3. Layout recipes

The shell already provides: sidebar (232 px, items 36 px, gap `S1`), page
scroll area with `S8`/`S10` padding, content centred at max 960 px.

**Page**

```text
column![
    page_header(..),              // H1 + muted subtitle (S1 apart)
    space::vertical().height(S6),
    cards..., spacing S4
].spacing(S4)
```

**Region (hero)**: `region(column![h2, S3, content])`. One per page, two at most.

**Group of rows (default body)**

```text
group(p, ctx.t("Startup"), Some(summary), Some(overflow_menu(..)), vec![
    row_item(p, Some(Icon::Shield), title, Some(sub), pill(..), Some(msg)),
    ...
])
```

Groups are `S8` apart on the bare page. Rows are `S1` apart and have no
dividers or boxes; the soft hover is the only chrome. Settings-style rows put a
`switch` / `dropdown` in the trailing slot and pass `on_press: None`.

**Long data**: `collapsible(p, title, Some(summary), open, Message::Toggle, body)`.
Render at most `limited(&items, 6, expanded)` rows and put a `show_more_button`
under them when there are more. Never print 40 rows on first view.

**Actions**: one visible `Primary` per region. Everything else (Export, Undo,
Details, Remove) goes in an `overflow_menu` at the end of the group heading or
the row's trailing slot. Labels are two or three words ("Check again").

**Page grid**: two columns = `row![region, region].spacing(S4)`, each
`Length::FillPortion(1)`; avoid it unless both regions are equally important.

**Footer action bar**: `row![space::horizontal(), Secondary, Primary].spacing(S2)`
placed `S4` below the content, primary on the right.

**Sheet**: `h2`, `S3`, text, `S6`, footer action bar. One primary action.

## 4. Copy rules

- Plain, calm, short. One idea per sentence. Say what happens, not how.
- Always `ctx.t("...")`; English source text is the key.
- Banned on primary surfaces: control, id, journal, DISM, SFC, registry,
  provisioned, elevated, broker, PowerShell, raw status words. Technical
  detail may live behind an `expander` titled "More details".
- Buttons are verbs ("Fix now", "Remove", "Check again"). Titles are short noun
  phrases. No exclamation marks, no blame in errors ("We could not ...").
- Numbers: "14 of 18 protected", never "14/18 controls".

## 5. Animation rules

Read `docs/MOTION.md` and use the tokens in `widgets::anim`.

- Page switches: see `docs/MOTION.md` (fade by lerping the `Palette` toward `bg`, never whole-tree opacity).
- Overflow menus slide 6 px in over `FAST`; the chevron in `collapsible` turns over `FAST`; both redraw only while moving.
- Animate only small regions (an icon, a switch knob, a progress bar), 120-200 ms,
  decelerate curve for things entering, accelerate for leaving.
- Drive animation only while it runs. Preferred: a widget that calls
  `shell.request_redraw()` from `RedrawRequested` until finished (as `switch`
  and the toast slide do). Otherwise `window::frames()` subscribed only while
  `is_animating`. Never a permanent timer.
- Hover/press feedback is an instant colour change from the theme (it is under
  one frame at 60 Hz and never flickers).
- Built in: `switch` knob slide (160 ms decelerate), toast slide-up (`NORMAL`
  250 ms on `DECELERATE`) and slide-down exit (`FAST` 150 ms on `ACCELERATE`).
  The easing curves live in `anim.rs`.

## 6. Performance rules

- `view()` only builds widgets. No file IO, sorting or formatting of big lists;
  precompute in `update()` and keep results in page state.
- Every interaction must respond on the next frame; long work goes through
  `blocking(..)` / `blocking_stream(..)`.
- Static canvas content uses `canvas::Cache` (see `ring`): rebuild only when
  inputs change.
- Idle CPU must be about zero: no subscriptions or timers when nothing runs
  (the toast clock exists only while a toast is shown).
- Keep widget trees small; use `Length::Fill` containers, avoid per-row
  closures capturing big data (borrow with `'a`).
