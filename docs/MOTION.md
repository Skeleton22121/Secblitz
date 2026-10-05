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
| `FASTER` | 83 ms | Reserved for micro feedback (WinUI ControlFasterAnimationDuration). Not used today: hover and press are instant colour changes |
| `FAST` | 150 ms | Toggle knob, toast exit (between WinUI 167 and Material short3 150) |
| `NORMAL` | 250 ms | Short-distance moves such as the toast slide-up (WinUI ControlNormalAnimationDuration) |
| `SLOW` | 400 ms | Completion moments: check draw-in, ring fill, count-up |

Rule of thumb: nothing the user triggers directly takes longer than `NORMAL`.
`SLOW` is only for results the user is waiting to see.

## Where each animation is used

| Helper | Where | Motion |
| --- | --- | --- |
| `spinner` | Any wait with unknown length (checking, applying, updating) | Ring turns every 2.2 s while the arc grows and shrinks every 1.5 s, head racing ahead and tail catching up on `POINT_TO_POINT` (the WinUI ProgressRing feel) |
| `shield_scan` | Home "Checking your PC" | Thin scan line plus a faint 3-unit band sweeps top to bottom and back every 2.4 s on `EASE_IN_OUT`, clipped inside the shield |
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

- Page switches: no fades or slides. Content appears on the next frame.
- Hover, press, focus, selection: instant colour change.
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

### PC-check hero (`scan::check_hero`)

160 px, layered. Assets in `assets/illustrations/` (`shield`, `glyph`,
`check`, `alert`, `orbit`, all `currentColor`, viewBox 160). Layers bottom to
top: canvas (rising fill, orbit dots, radar sweep) -> orbit SVG (idle only) ->
shield outline -> glyph -> canvas (check / exclamation draw-in).

| Phase | What moves |
| --- | --- |
| Idle | Orbit and glyph breathe on a 4 s cosine; nothing else. |
| Checking | Orbit dots turn (3.2 s), radar wedge sweeps (2.6 s, 14 fading slices), shield fills bottom to top with `progress` (exponential smoothing, rate 5/s, so count jumps glide). Orbit and sweep fade in over 400 ms. |
| Good | Orbit dots converge and fade (350 ms), outline and fill cross-fade to green (300 ms), the check draws in (450 ms after a 120 ms delay) with a 6 % scale overshoot settle. |
| Attention | Same with amber; the exclamation bar draws and the dot pops. |

`scan::animating(phase, elapsed)` tells the page whether to keep the frame
subscription (Idle and Checking: yes; done phases: until 1.4 s).

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
