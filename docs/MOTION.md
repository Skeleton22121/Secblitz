# Secblitz motion

Implemented in `src/gui/widgets/anim.rs`. Icons are Microsoft Fluent UI System
Icons (MIT), see `assets/ICONS-LICENSE.txt`.

## Principles

1. **Fast first.** The user's own action (hover, press, select, tab) responds
   on the very next frame. Fluent says it best: motion is "a digital medium,
   which comes with an expectation of speed and performance". Hover and press
   are colour changes only and never wait on an animation.
2. **Motion explains, it does not decorate.** We animate only to show that
   something is working (spinner, scan), that something finished (check
   draw-in, ring fill, count-up), or that something needs attention (warning).
3. **Enter fast, settle softly; leave faster.** Arriving things use a
   decelerate curve (fast start, long gentle stop). Leaving things use accelerate
   and a shorter duration.
4. **Small areas only.** Animations live in 16 to 64 px icons or one progress
   bar. The software (CPU) renderer redraws only damaged regions, so a tiny
   moving region stays cheap; whole-window motion is not used.
5. **Stop when idle.** `iced::window::frames()` is subscribed only while
   something is animating. A finished one-shot clears its clock and the
   subscription turns off. At rest the app draws nothing.
6. **Respect the system.** If Windows "Show animations in Windows" is off
   (`SPI_GETCLIENTAREAANIMATION`), every helper returns its final state at
   once and `anim::animating()` is false.
7. **No shadows, blur or large translucent overlays.** They are banned
   because they accumulate artefacts under damage-region redraw.

## Curves

All are real cubic Béziers solved per sample (Newton-Raphson with a bisection
fallback), not approximations. `Curve::at(t)` clamps `t` to 0..1 and returns
exactly 0 and 1 at the ends.

| Token | cubic-bezier | Use | Source |
| --- | --- | --- | --- |
| `DECELERATE` | 0, 0, 0, 1 | Entering, settling, value tweens, draw-ins | Fluent "Fast out, slow in" |
| `ACCELERATE` | 1, 0, 1, 1 | Leaving the scene | Fluent "Slow out, fast in" |
| `POINT_TO_POINT` | 0.55, 0.55, 0, 1 | Moving between two resting places; spinner head/tail | Fluent point-to-point |
| `EMPHASIZED` | 0.05, 0.7, 0.1, 1 | Pop-in of the warning mark | Material 3 emphasized decelerate |
| `STANDARD` | 0.2, 0, 0, 1 | Calm general-purpose in-out | Material 3 standard |
| `EASE_IN_OUT` | 0.42, 0, 0.58, 1 | Symmetric ping-pong (scan sweep) | CSS ease-in-out |
| `LINEAR` | 0, 0, 1, 1 | Constant rotation only | n/a |

## Durations

| Token | Value | Use |
| --- | --- | --- |
| `FASTER` | 83 ms | Micro feedback: button press-down, switch knob growth (WinUI ControlFasterAnimationDuration) |
| `FAST` | 150 ms | Hover tween, toggle knob, toast exit (between WinUI 167 and Material short3 150) |
| `NORMAL` | 250 ms | Short-distance moves such as the toast slide-up (WinUI ControlNormalAnimationDuration) |
| `SLOW` | 400 ms | Completion moments: check draw-in, ring fill, count-up |

Rule of thumb: nothing the user triggers directly takes longer than `NORMAL`.
`SLOW` is only for results the user is waiting to see.

## Where each animation is used

| Helper | Where | Motion |
| --- | --- | --- |
| `spinner` | Any wait with unknown length (checking, applying, updating) | Ring turns every 2.2 s while the arc grows and shrinks every 1.5 s, head racing ahead and tail catching up on `POINT_TO_POINT` (the WinUI ProgressRing feel) |
| `check_draw` | A fix or check finished successfully | Circle strokes in (first half), check draws (from 40%), 7% overshoot settle, 400 ms |
| `cross_draw` | Something could not be done | Same timing, cross drawn in two strokes |
| `warn_draw` | Needs attention | Triangle outlines, then the mark pops with `EMPHASIZED` |
| `ring_fill`, `Tween` | Score ring when the score changes | Old value to new value on `DECELERATE`, 400 ms; `retarget` keeps it continuous if the score changes mid-way |
| `count_up`, `count_up_int` | Numbers beside the ring | Same tween, rounded |
| `appear::slide_in` | Toast | Slides up 12 px on `DECELERATE` in `NORMAL`; on dismissal or time-out slides back down on `ACCELERATE` in `FAST`, then is removed |
| `pulse_dot` | Status dot for live protection | One soft halo every 2.4 s, peak alpha 0.28 |

Security and antivirus dashboards commonly use the same four moments (scan
sweep, indeterminate ring, check draw-in on completion, number count-up). We
keep each one small and short.

## Where NOT to animate

- Page switches: no cross-fade and no slide-out; the old page is dropped at once (see "Page entrance" below for the incoming page only).
- Hover, press, focus, selection never *wait* on an animation: the target is set on the very next frame and the tween (<= 150 ms) starts moving at once.
- Dropdown menus and the expander chevron / body: they open instantly.
- Lists and tables appearing, or scrolling.
- Anything behind a modal; no animated backdrops.
- Text, window resize, or the whole window's background.
- Repeating animations while the window is minimised or idle (no subscription).
- Under reduced motion: all of the above helpers jump to the final state. The
  spinner becomes a fixed three-quarter arc, so pair it with a status word such
  as "Checking..." so it never looks frozen without explanation.

## How a page drives it

See the module docs at the top of `anim.rs`. In short: keep a `Clock` in page
state, return `iced::window::frames()` from `subscription()` only while the
clock is live, store the frame `Instant`, and compute progress from that
timestamp in `view()`. Static end states are cached by `canvas::Cache` inside
each icon, keyed on kind, size and colour, so a finished check costs nothing
per redraw.

## Sources

- Microsoft, [Timing and easing](https://learn.microsoft.com/en-us/windows/apps/design/motion/timing-and-easing): durations 250 / 167 / 83 ms, decelerate (0,0,0,1), accelerate (1,0,1,1).
- Microsoft, [Motion in Windows apps](https://learn.microsoft.com/en-us/windows/apps/design/motion/): entrance, exit, connected motion.
- Microsoft, [Guidelines for progress controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls) and the Lottie-based indeterminate [ProgressRing](https://github.com/microsoft/microsoft-ui-xaml/pull/1858) (arc grows and shrinks while rotating).
- Material Design 3, [Easing and duration](https://m3.material.io/styles/motion/easing-and-duration/tokens-specs): emphasized decelerate (0.05, 0.7, 0.1, 1), standard (0.2, 0, 0, 1), duration tokens in 50 ms steps.
- Apple, [Human Interface Guidelines: Motion](https://developer.apple.com/design/human-interface-guidelines/motion): purposeful, brief motion; honour Reduce Motion.
- Microsoft, [SPI_GETCLIENTAREAANIMATION](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow): the "Show animations in Windows" setting.
- Microsoft, [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) (MIT).

## Round 3: navigation and micro-interactions

### Research notes (what we copied and why)

- **WinUI / Fluent 2 page entrance.** `EntranceThemeTransition` moves the
  incoming content up from a small vertical offset while fading it in with a
  decelerate curve (the XAML default is a 40 px offset; Fluent's guidance for
  page navigation is "slide up + fade, about 300 ms, decelerate"). *Drill-in*
  (`DrillInNavigationTransitionInfo`) scales as well, and is meant for
  hierarchical navigation. Our sidebar is flat (peer pages), so we use the
  entrance variant, not drill-in.
- **Material 3.** Peer destinations use *fade through* (outgoing fades out
  first, incoming fades in with a slight scale) and sibling steps use *shared
  axis*. Both keep the outgoing page for one more beat. We deliberately do not:
  the software renderer redraws the whole content area every frame of a
  transition, so keeping two pages alive would double the cost.
- **Buttons.** Fluent controls use a state layer (hover and pressed tints of
  the same fill) and Windows 11 buttons shrink slightly on press. Material 3
  state layers are 8% hover, 10% press, tweened over short2/short3 (100 to
  150 ms). Common press scales are 0.97 to 0.98. We take the Fluent tints
  from the palette (`hover`, `pressed`), 0.97 scale and 83 ms down, 220 ms up.

### Page entrance (`gui/mod.rs`, `appear::{enter_progress, fade_palette, lift}`)

On `Navigate(page)` the new page is swapped in at once (no outgoing animation)
and enters over 220 ms (`appear::ENTER`) on `DECELERATE`:

- **Rise**: drawn 12 px low, `Renderer::with_translation`, easing to 0.
- **Fade**: iced 0.14 has no subtree opacity, so the page is *built* from a
  palette lerped towards the window background (`appear::fade_palette`),
  starting at 30% strength. No off-screen layer is needed.

While it runs the shell subscribes to `window::frames()` (`Message::PageFrame`)
and rewrites `ctx.palette`; the sidebar, footer and sheets use the settled
palette (`Palette::of(mode)`). When it ends `ctx.palette` is restored and the
subscription drops, so an idle window draws nothing. `anim::reduced()` skips the
entrance entirely. Navigating also snaps the page scroll back to the top.

### Preloading

The shell tracks `warmed` / `flight` per warmable page (History, Clean up apps,
Settings). `preload_all()` runs when the engine opens and after every check;
each page's `preload()` does its read on a worker thread (`blocking`) and the
`flight` flag blocks a second load while one is running (cleared when the page's
`Loaded` / `Scanned` message arrives). Opening a warmed page triggers a *silent*
refresh: the data on screen stays and is replaced when the new read lands, so
the page never flashes back to a spinner. Home, Protection and Tools have no
background reads.

### Buttons (`widgets/press.rs`)

`press::button` is a drop-in for `iced::widget::button` (same builder methods,
same `Fn(&Theme, Status) -> Style` closures, so all existing styles work).

| Part | Behaviour |
| --- | --- |
| Hover | fill, text and border tween `Active` to `Hovered` over `FAST` (150 ms) on `STANDARD`; a missing fill fades from transparent |
| Press | fill continues to `Pressed`; the button scales to 0.97 about its centre in `FASTER` (83 ms) |
| Release | spring-back in 220 ms with a small overshoot (ease-out-back, about 0.5% over) |
| Wide elements | no scale above 260 px wide (`SCALE_MAX_WIDTH`); rows only tint |
| Focus | Tab / Shift+Tab (shell sends `focus_next` / `focus_previous`), 2 px ring, Enter or Space activates; a mouse press clears it |
| Disabled | the style's `Disabled` look, no hover, no press, not focusable |

State lives in the widget tree; `shell.request_redraw()` is called only while a
tween runs.

### Other controls (`widgets/controls.rs`)

- **Segmented**: one selection pill slides between options in 200 ms on
  `STANDARD` (retargets from where it is); labels tint as the pill passes;
  hover tints the other options.
- **Switch**: knob slides on `DECELERATE` (150 ms), track colour tweens, knob
  grows 12 px, 14 px on hover, 15 px pressed (83 ms).
- **Checkbox**: the box fills in the first 40% of 250 ms, then the tick draws
  itself in (stroked polyline, round caps); unchecking reverses in 150 ms on
  `ACCELERATE`. Mixed draws a dash.
- **Sidebar marker** (`slide_marker`): the selected fill and a 3 x 16 px accent
  bar glide to the chosen item in 260 ms on `POINT_TO_POINT`.
- Dropdown and text field are filled tonal fields (no outline); the focus ring
  is the only stroke. The open dropdown list keeps a faint edge because
  shadows are not allowed.

### Idle check

Every animated widget requests a redraw only from inside a running tween, and
the only new subscription (`PageFrame`) exists for the 220 ms entrance. After
the last tween ends no event requests another frame, so CPU use returns to 0.

### Sources (round 3)

- Microsoft, [Page transitions](https://learn.microsoft.com/en-us/windows/apps/design/motion/page-transitions) (entrance, drill-in, suppress).
- Microsoft, [Timing and easing](https://learn.microsoft.com/en-us/windows/apps/design/motion/timing-and-easing).
- Material Design 3, [Transitions: fade through, shared axis](https://m3.material.io/styles/motion/transitions/transition-patterns) and [State layers](https://m3.material.io/foundations/interaction/states/state-layers).
## Visuals

Round 3 visuals: spinner, PC-check illustration, progress bars, charts, score
ring. All of them are borderless, draw plain tinted shapes (no backing discs),
use no shadows, honour `anim::reduced()`, and cost nothing when nothing moves.

**Renderer fact that shaped the design.** iced's CPU renderer (`tiny-skia`,
what the test VM uses) ignores `Svg::rotation`; `Svg::opacity` and the colour
tint work on both backends. So SVG assets are used for still layers (faded
with opacity), and everything that rotates or fills is drawn as canvas
geometry, which transforms identically everywhere.

### Spinner (`anim::spinner`, `anim::dots`)

- Ring with a 12 % track and a round-capped arc. Stroke is `size * 0.085`,
  clamped to 1.75..4 px (about 2 px at 24). Sizes used: 16, 20, 32, 48.
- The whole arc turns at constant speed (1.6 s per turn) while its length
  breathes between 30 and 270 degrees on `POINT_TO_POINT`, one breath per
  1.4 s. It grows around its own middle, so it pulses rather than chases.
  `anim::spinner_arc(secs)` is the pure function behind it.
- `anim::dots(size, color, elapsed)`: three dots pulsing in sequence (1.2 s,
  0.16 offset, `STANDARD` ease) for inline "Working..." text. Width is 2.2x
  the height; pass the text size.
- Reduced motion: fixed three-quarter arc / static dots.

### PC-check drawing (`hairline::magnifier`)

A hairline drawing: a monitor shows five settings (Firewall, Microsoft
Defender, Windows Update, Remote Desktop, Network sharing) and a magnifying
glass looks them over. It is 256 by 188 units, one unit per logical pixel at
`FULL` (the first check's screen, sized by `responsive` to the height left
after the title, bar and ticker) and `COMPACT` = 0.625 (160 px wide) in the
Protection page's region while a new check runs. The Ready state replaces
the shield icon on Home's "Let's check your PC". Lines are the theme's greys;
blue (`accent`) only for what the lens is reading and switches turning on,
green for ticks.

| State | What moves |
| --- | --- |
| Ready | The glass rests beside the monitor and bobs (3 and 4 units on 1.3 and 1.7 rad/s). The bob runs 6 s after the drawing appears or the pointer was last over it, then fades out over 1.5 s on `EASE_IN_OUT`, so a resting page asks for no frames. Over the screen the lens drifts 55 % of the way to the pointer. |
| Checking | The lens reads the row the check has reached: each fifth of the page's real progress finishes one row, whose tick draws in (0.35 s, `DECELERATE`, rows finished together 150 ms apart) while its switch knob springs on (k 160, c 20). The lens sweeps along that row (x on 0.9 rad/s, a 2.5 unit bob on 1.7 rad/s). Without a known total it falls back to the prototype's autopilot over all five rows. |

The lens position is a spring (k 70, c 13), so it glides between the
pointer, the check's row and a clicked row. Inside the lens the screen is
drawn again at 1.55x about its centre and cut to the lens by hand (iced only
clips to rectangles); magnified lines are 1.55x bolder, at most 2.5 px.
Interaction: the lens follows the pointer over the screen while checking,
hovering a row names it (above the lens when the lens is on it), clicking a
row holds the lens there for 1.6 s and draws its tick again, clicking
elsewhere sends a pulse. Reduced motion: still frame (ambient 0.9 s), ticks
and switches jump to their end, the lens jumps to the pointer; names and
clicks still work. The drawing asks for its own frames only while something
moves.

`scan::checking_screen` lays the first check out across the whole page: the
drawing, title, bar and ticker, all centred.

### Status ticker (`scan::status_ticker`)

Three rows, newest at the bottom. A new line enters from below over 450 ms on
`EMPHASIZED` while older lines glide up one row, soften from text to muted
colour and fade (alpha 1, 0.55, 0.28, 0). Positions are one pure function of
how far each line has entered (`ticker_depths`, `ticker_style`), so lines that
arrive in quick succession never jump. The newest line carries a tiny spinner,
finished lines a faint tick.

### Progress (`widgets::progress`)

6 px capsule, track is the text colour at 8 %. `bar` draws a page-owned
`Tween` (400 ms decelerate); `bar_eased` tweens by itself; `indeterminate`
glides a highlight with a faint trail along the track (1.7 s, `STANDARD`) and
requests its own redraws; `steps` is a row of 4 px segments for flows.

### Charts and ring

- `chart::trend`: monotone cubic line (Fritsch-Butland tangents, never
  overshoots), 2 px, area as four stacked 4.5 % bands fading downward, three
  guide lines, percent labels top and bottom, first and last date, latest
  point marker, hover tooltip with value and date. The line draws in once,
  left to right, 500 ms `DECELERATE`; afterwards the geometry is cached and
  only the hover layer redraws. One point shows a level line and the value.
- `ring::ring` / `ring::ring_counting`: stroke `size * 0.045` (4..9 px),
  7 % track, round cap, arc and number ease together over `SLOW`.
