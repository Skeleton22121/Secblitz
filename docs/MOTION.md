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
