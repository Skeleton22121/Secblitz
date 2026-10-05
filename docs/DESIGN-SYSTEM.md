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
| `S3`  | 12 | section title to content, badge to text, rows inside a list |
| `S4`  | 16 | gap between cards, button side padding, notice padding |
| `S5`  | 20 | empty-state side padding |
| `S6`  | 24 | card padding, sheet padding, page header to content |
| `S8`  | 32 | page top/bottom padding, gap between page sections |
| `S10` | 40 | page side padding |

Rules:

- Card padding `S6`. Gap between cards `S4`.
- Section label/title to its content `S3`.
- Page header to first content `S6`.
- Gap between big page sections `S8`.
- Buttons in a row `S2`; footer action bar buttons `S2`, right aligned, `S4` above it.
- Old names `GAP` (=S4), `PAD` (=S5), `RADIUS` (=R_LARGE), `RADIUS_SMALL` (=R) still
  compile for pages that have not migrated. Do not use them in new code.

### Sizes

| Token | px | |
|-------|----|-|
| `CONTROL` | 36 | buttons, dropdowns, text fields, switches' row, segmented control (outer) |
| `CONTROL_SMALL` | 28 | segments inside the segmented control |
| `ROW` | 48 | minimum list row height |
| `CHECK` | 18 | checkbox box |

### Radii

`R_SMALL` 6 (inner pieces such as segments, close buttons), `R` 8 (controls,
notices, list rows), `R_LARGE` 12 (cards, sheets), `R_PILL` (pills, badges).

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

Colour only carries meaning. Surfaces are zinc greys; the only chromatic
colours are the status tones.

- Surfaces: `bg` (window), `sidebar`, `surface` (cards, sheets, inputs),
  `surface_alt` (wells, tracks, expander bodies).
- Text: `text`, `text_muted`. Lines: `border`, `border_strong` (inputs, secondary buttons).
- States: `hover` (rows/ghost on `bg`/`surface`), `hover_strong` (on `sidebar`
  and `surface_alt`), `pressed`, `selected` (active nav item), `focus_ring`
  (keyboard focus, open dropdown, focused field), `disabled_bg`, `disabled_fg`.
- Actions: `brand` (near-black / white) with `brand_hover`, `brand_pressed`,
  `on_brand`; `danger`, `danger_hover`, `danger_pressed` (solid, white text).
- Status: `good`, `warn`, `bad`, `neutral` via `Tone`; `p.tint(tone)` for soft backgrounds.

Hard rules: no box shadows (tiny-skia redraw accumulates them: never create a
`Shadow` with alpha > 0), no blur, no gradients, no large translucent overlays
that change often. The modal scrim is the only translucent full-window layer
and it is static.

## 2. Components (`crate::gui::widgets`)

| Widget | When to use |
|--------|-------------|
| `card(p, content)` | every content block. Never nest a card in a card. |
| `h1/h2/body/muted/small(p, text)` | text. Do not build raw `text()` styles in pages. |
| `page_header(p, title, subtitle)` | first element of every page. |
| `section_label(p, text)` | small caption above a group of rows. |
| `action(p, kind, label, icon, on_press)` | all buttons. `Primary` once per screen; `Secondary` for the rest; `Ghost` for quiet extras; `Danger` only to confirm removals/undo. `on_press: None` = disabled. |
| `icon_button(p, kind, icon, on_press)` | 36x36 icon-only button; only with a nearby label. |
| `link(p, label, msg)` | in-app "See all" style navigation (ghost button, arrow cursor). |
| `hyperlink(p, label, msg)` | opens a web page. The only widget with the hand cursor. |
| `list_button(p, row, msg)` | whole row clickable; hover and pressed fills. |
| `icon_badge(p, icon, tone)` | 36 px round leading icon of a list row. |
| `pill(p, text, tone)` | short status word ("Protected"). |
| `inline_notice(p, tone, text)` | calm tip/warning/error inside a page. |
| `empty_state(p, icon, title, body, action)` | nothing to show yet. |
| `progress_row(p, text, state)` / `bar(p, ratio, tone)` | checklists and a thin progress bar. |
| `expander(p, title, open, msg, content)` | "More details". Keep closed by default. |
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
one padding value. The menu has 1 px border, R radius, hover row = `surface_alt`.
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

**Card with title**: `card(column![h2, S3, content])`.

**List rows**: each row is `list_button(row![icon_badge, column![body, small], space, pill/chevron].spacing(S3))`,
minimum 48 px; separate rows with `S1`, no divider lines inside a card.

**Card grid**: two columns = `row![card, card].spacing(S4)`, each `Length::FillPortion(1)`.

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

- Page switches are instant. No fades of large areas on the CPU renderer.
- Animate only small regions (an icon, a switch knob, a progress bar), 120-200 ms,
  decelerate curve for things entering, accelerate for leaving.
- Drive animation only while it runs. Preferred: a widget that calls
  `shell.request_redraw()` from `RedrawRequested` until finished (as `switch`
  and the toast slide do). Otherwise `window::frames()` subscribed only while
  `is_animating`. Never a permanent timer.
- Hover/press feedback is an instant colour change from the theme (it is under
  one frame at 60 Hz and never flickers).
- Built in: `switch` knob slide (160 ms decelerate), toast slide-up (180 ms,
  once). `appear::decelerate(t)` is the shared easing for these.

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
