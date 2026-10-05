//! The PC-check illustration and its status ticker.
//!
//! [`check_hero`] is a 160 px layered illustration: a shield that fills from
//! the bottom as the check advances, a radar sweep and an orbit with three
//! dots turning around it, and an inner magnifier that gives way to a check
//! (or an exclamation mark) at the end.
//!
//! # How it is built
//!
//! The still layers are SVG assets in `assets/illustrations/` (shield outline,
//! glyph, check, alert, orbit), tinted at runtime through the colour filter
//! and faded with `Svg::opacity`. The moving layers (sweep, orbit dots, rising
//! fill, check draw-in) are drawn on two small canvases, because iced's
//! CPU renderer (`tiny-skia`) ignores `Svg::rotation`; canvas geometry rotates
//! identically on every backend. Only this 160 px region redraws per frame.
//!
//! # Driving it
//!
//! The page keeps the instant the current phase began and runs a frame
//! subscription while [`animating`] says so:
//!
//! ```ignore
//! scan::check_hero(p, HeroPhase::Checking, clock.elapsed_at(now), progress)
//! ```
//!
//! `elapsed` is time since the current phase began (restart it when the phase
//! changes). `progress` is the checks done so far, 0..=1; jumps are smoothed
//! inside, so it can be fed raw counts.
//!
//! [`status_ticker`] shows the last few status lines; each new line eases in
//! from below while the older ones step back and fade.
use super::anim::{self, arc_path, partial_line, phase, stroke, DECELERATE, EMPHASIZED, TAU, TOP};
use crate::gui::theme::{self, Palette};
use crate::gui::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::widget::{container, stack, svg};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::cell::Cell;
use std::time::{Duration, Instant};

/// Edge of the illustration in logical pixels.
pub const HERO: f32 = 160.0;

const SHIELD: &[u8] = include_bytes!("../../../assets/illustrations/shield.svg");
const GLYPH: &[u8] = include_bytes!("../../../assets/illustrations/glyph.svg");
const CHECK: &[u8] = include_bytes!("../../../assets/illustrations/check.svg");
const ALERT: &[u8] = include_bytes!("../../../assets/illustrations/alert.svg");
const ORBIT: &[u8] = include_bytes!("../../../assets/illustrations/orbit.svg");

/// What the illustration shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroPhase {
    /// Waiting: slow breathing.
    Idle,
    /// Working: orbit turns, sweep turns, shield fills with progress.
    Checking,
    /// Finished and everything is fine: green check settles in.
    Good,
    /// Finished with something to look at: amber exclamation mark.
    Attention,
}

/// Whether the hero (and the page's frame subscription) needs frames now.
/// Idle and Checking always do; the done phases only until they settle.
pub fn animating(phase: HeroPhase, elapsed: Duration) -> bool {
    if anim::reduced() {
        return false;
    }
    match phase {
        HeroPhase::Idle | HeroPhase::Checking => true,
        HeroPhase::Good | HeroPhase::Attention => elapsed < Duration::from_millis(1400),
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------

/// Move `shown` towards `target` with an exponential ease; frame-rate
/// independent. `dt` in seconds.
pub fn approach(shown: f32, target: f32, dt: f32, rate: f32) -> f32 {
    let k = 1.0 - (-rate * dt.max(0.0)).exp();
    shown + (target - shown) * k
}

/// Y of the rising fill's surface in hero coordinates for a fill of 0..=1.
fn level_y(fill: f32) -> f32 {
    140.0 - 120.0 * fill.clamp(0.0, 1.0)
}

/// How far each ticker line has entered, 0..=1, from its age.
fn entered(age: Duration) -> f32 {
    if anim::reduced() {
        return 1.0;
    }
    EMPHASIZED.at(age.as_secs_f32() / TICKER_ENTER.as_secs_f32())
}

/// Depth of each line given how far each has entered (oldest first):
/// `d_j = sum(e_m for m after j) + e_j - 1`. The newest line fully entered
/// has depth 0, the one before it 1, and so on; mid-entrance values are
/// fractional so everything glides.
pub fn ticker_depths(entered: &[f32]) -> Vec<f32> {
    let mut after = 0.0;
    let mut out = vec![0.0; entered.len()];
    for j in (0..entered.len()).rev() {
        out[j] = after + entered[j] - 1.0;
        after += entered[j];
    }
    out
}

/// (alpha, muted 0..1) of a line at `depth`.
pub fn ticker_style(depth: f32) -> (f32, f32) {
    const STOPS: [(f32, f32); 5] = [
        (-1.0, 0.0),
        (0.0, 1.0),
        (1.0, 0.55),
        (2.0, 0.28),
        (3.0, 0.0),
    ];
    let mut alpha = 0.0;
    for w in STOPS.windows(2) {
        let ((d0, a0), (d1, a1)) = (w[0], w[1]);
        if depth >= d0 && depth <= d1 {
            alpha = a0 + (a1 - a0) * (depth - d0) / (d1 - d0);
        }
    }
    (alpha, depth.clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// Hero
// ---------------------------------------------------------------------------

fn tone_of(p: &Palette, phase: HeroPhase) -> Color {
    match phase {
        HeroPhase::Good => p.good,
        HeroPhase::Attention => p.warn,
        _ => p.text,
    }
}

fn layer<'a>(bytes: &'static [u8], color: Color, opacity: f32) -> Element<'a, Message> {
    if opacity <= 0.004 {
        return iced::widget::space()
            .width(Length::Fixed(HERO))
            .height(Length::Fixed(HERO))
            .into();
    }
    svg(svg::Handle::from_memory(bytes))
        .width(Length::Fixed(HERO))
        .height(Length::Fixed(HERO))
        .opacity(opacity.min(1.0))
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

/// The PC-check illustration. See the module docs for `elapsed`/`progress`.
pub fn check_hero<'a>(
    p: Palette,
    phase: HeroPhase,
    elapsed: Duration,
    progress: f32,
) -> Element<'a, Message> {
    let reduced = anim::reduced();
    let secs = elapsed.as_secs_f32();
    let done = matches!(phase, HeroPhase::Good | HeroPhase::Attention);
    // 0 -> 1 crossfade from the neutral look into the result tone.
    let e = if !done {
        0.0
    } else if reduced {
        1.0
    } else {
        DECELERATE.at(secs / 0.3)
    };
    let breath = 0.5 - 0.5 * (secs * TAU / 4.0).cos();
    let tone = tone_of(&p, phase);

    let back = canvas::Canvas::new(Back {
        p,
        phase,
        secs,
        progress,
    })
    .width(Length::Fixed(HERO))
    .height(Length::Fixed(HERO));

    let mut layers: Vec<Element<'a, Message>> = vec![back.into()];
    // Static orbit while idle (breathing) and under reduced motion.
    match phase {
        HeroPhase::Idle => layers.push(layer(ORBIT, p.text, 0.28 + 0.22 * breath)),
        HeroPhase::Checking if reduced => layers.push(layer(ORBIT, p.text, 0.5)),
        _ => {}
    }
    layers.push(layer(SHIELD, p.text, 1.0 - e));
    if done {
        layers.push(layer(SHIELD, tone, e));
    }
    match phase {
        HeroPhase::Idle => layers.push(layer(GLYPH, p.text, 0.55 + 0.35 * breath)),
        HeroPhase::Checking => layers.push(layer(GLYPH, p.text, 0.9)),
        _ => layers.push(layer(GLYPH, p.text, 1.0 - e)),
    }
    if done {
        if reduced {
            let mark = if phase == HeroPhase::Good {
                CHECK
            } else {
                ALERT
            };
            layers.push(layer(mark, tone, 1.0));
        } else {
            layers.push(
                canvas::Canvas::new(Front { p, phase, secs })
                    .width(Length::Fixed(HERO))
                    .height(Length::Fixed(HERO))
                    .into(),
            );
        }
    }
    container(stack(layers))
        .width(Length::Fixed(HERO))
        .height(Length::Fixed(HERO))
        .into()
}

fn shield_path() -> Path {
    Path::new(|b| {
        b.move_to(Point::new(80.0, 20.0));
        b.bezier_curve_to(
            Point::new(92.0, 30.0),
            Point::new(106.0, 35.0),
            Point::new(124.0, 36.0),
        );
        b.line_to(Point::new(124.0, 74.0));
        b.bezier_curve_to(
            Point::new(124.0, 104.0),
            Point::new(108.0, 126.0),
            Point::new(80.0, 140.0),
        );
        b.bezier_curve_to(
            Point::new(52.0, 126.0),
            Point::new(36.0, 104.0),
            Point::new(36.0, 74.0),
        );
        b.line_to(Point::new(36.0, 36.0));
        b.bezier_curve_to(
            Point::new(54.0, 35.0),
            Point::new(68.0, 30.0),
            Point::new(80.0, 20.0),
        );
        b.close();
    })
}

/// Fill, orbit dots and radar sweep; sits below the SVG outline.
struct Back {
    p: Palette,
    phase: HeroPhase,
    secs: f32,
    progress: f32,
}

#[derive(Default)]
struct BackState {
    shown: Cell<f32>,
    last: Cell<f32>,
}

impl canvas::Program<Message> for Back {
    type State = BackState;
    fn draw(
        &self,
        st: &BackState,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let p = &self.p;
        let reduced = anim::reduced();
        let target = match self.phase {
            HeroPhase::Idle => 0.0,
            HeroPhase::Checking => self.progress.clamp(0.0, 1.0),
            _ => 1.0,
        };
        let dt = self.secs - st.last.get();
        st.last.set(self.secs);
        let dt = if (0.0..0.25).contains(&dt) { dt } else { 0.0 };
        let shown = if reduced {
            target
        } else {
            approach(st.shown.get(), target, dt, 5.0)
        };
        st.shown.set(shown);

        let mut f = Frame::new(renderer, bounds.size());
        let done = matches!(self.phase, HeroPhase::Good | HeroPhase::Attention);
        let tone = tone_of(p, self.phase);
        let c = Point::new(80.0, 80.0);

        // Rising fill, clipped to the shield. Tone eases in when the check ends.
        let fill_col = if done {
            let e = if reduced {
                1.0
            } else {
                DECELERATE.at(self.secs / 0.3)
            };
            theme::mix(p.text, tone, e)
        } else {
            p.text
        };
        let ly = level_y(shown);
        let shield = shield_path();
        if shown > 0.002 {
            let h = (160.0 - ly).max(0.0);
            if h > 0.5 {
                f.with_clip(
                    Rectangle::new(Point::new(0.0, ly), Size::new(160.0, h)),
                    |g| {
                        g.fill(
                            &shield,
                            fill_col.scale_alpha(if done { 0.16 } else { 0.12 }),
                        );
                    },
                );
            }
            if self.phase == HeroPhase::Checking && shown < 0.995 {
                f.with_clip(
                    Rectangle::new(Point::new(0.0, ly), Size::new(160.0, 1.6)),
                    |g| {
                        g.fill(&shield, p.text.scale_alpha(0.5));
                    },
                );
            }
        }

        if reduced {
            return vec![f.into_geometry()];
        }

        // Orbit and sweep: fade in with the check, fade out after it.
        let (alpha, converge) = match self.phase {
            HeroPhase::Checking => (DECELERATE.at(self.secs / 0.4), 0.0),
            HeroPhase::Good | HeroPhase::Attention => {
                let e = DECELERATE.at(self.secs / 0.35);
                (1.0 - e, e)
            }
            HeroPhase::Idle => (0.0, 0.0),
        };
        if alpha > 0.01 {
            let radius = 72.0 - 30.0 * converge;
            f.stroke(
                &Path::circle(c, 72.0 * (1.0 - 0.0 * converge)),
                stroke(p.text.scale_alpha(0.10 * alpha), 1.5),
            );
            let base = if done {
                TOP
            } else {
                TOP + self.secs * TAU / 3.2
            };
            for (i, a) in [1.0f32, 0.6, 0.35].into_iter().enumerate() {
                let ang = base + i as f32 * TAU / 3.0;
                let pt = Point::new(c.x + radius * ang.cos(), c.y + radius * ang.sin());
                f.fill(&Path::circle(pt, 4.5), p.text.scale_alpha(a * alpha));
            }
            if self.phase == HeroPhase::Checking {
                let head = TOP + self.secs * TAU / 2.6;
                const SLICES: usize = 14;
                let slice = 5f32.to_radians();
                for s in 0..SLICES {
                    let a1 = head - s as f32 * slice;
                    let a0 = a1 - slice * 1.15;
                    let fade = (1.0 - s as f32 / SLICES as f32).powf(1.6);
                    let wedge = Path::new(|b| {
                        b.move_to(c);
                        b.arc(canvas::path::Arc {
                            center: c,
                            radius: 62.0,
                            start_angle: iced::Radians(a0),
                            end_angle: iced::Radians(a1),
                        });
                        b.close();
                    });
                    let max = if p.mode == theme::Mode::Dark {
                        0.2
                    } else {
                        0.15
                    };
                    f.fill(&wedge, p.text.scale_alpha(max * fade * alpha));
                }
                let tip = Point::new(c.x + 62.0 * head.cos(), c.y + 62.0 * head.sin());
                f.stroke(
                    &Path::line(c, tip),
                    stroke(p.text.scale_alpha(0.28 * alpha), 1.5),
                );
            }
        }
        vec![f.into_geometry()]
    }
}

/// Result mark drawing in over the glyph position, with a soft scale settle.
struct Front {
    p: Palette,
    phase: HeroPhase,
    secs: f32,
}

impl canvas::Program<Message> for Front {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let tone = tone_of(&self.p, self.phase);
        let t = ((self.secs - 0.12) / 0.5).clamp(0.0, 1.0);
        let bump = (std::f32::consts::PI * phase(t, 0.45, 1.0)).sin();
        // Settle: slightly small, overshoot by 6 %, rest at 1.
        let k = (0.9 + 0.1 * DECELERATE.at(t)) + 0.06 * bump;
        f.translate(iced::Vector::new(80.0, 80.0));
        f.scale(k);
        f.translate(iced::Vector::new(-80.0, -80.0));
        let w = 6.0;
        if self.phase == HeroPhase::Good {
            let pts = [
                Point::new(62.0, 82.0),
                Point::new(74.0, 94.0),
                Point::new(100.0, 66.0),
            ];
            if let Some(path) = partial_line(&pts, DECELERATE.at(t)) {
                f.stroke(&path, stroke(tone, w));
            }
        } else {
            let pop = EMPHASIZED.at(t);
            let bar = [Point::new(80.0, 60.0), Point::new(80.0, 86.0)];
            if let Some(path) = partial_line(&bar, pop) {
                f.stroke(&path, stroke(tone, w));
            }
            f.fill(
                &Path::circle(Point::new(80.0, 102.0), 3.8 * pop),
                tone.scale_alpha(pop),
            );
        }
        vec![f.into_geometry()]
    }
}

// ---------------------------------------------------------------------------
// Status ticker
// ---------------------------------------------------------------------------

const TICKER_ENTER: Duration = Duration::from_millis(450);
const ROW: f32 = 26.0;
const VISIBLE: usize = 3;

/// Height of the ticker region.
pub const TICKER_HEIGHT: f32 = ROW * VISIBLE as f32;

struct Ticker<'a> {
    p: Palette,
    lines: &'a [(String, Instant)],
    now: Instant,
}

impl canvas::Program<Message> for Ticker<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let p = &self.p;
        // Only the lines that can still be visible: VISIBLE plus the one leaving.
        let from = self.lines.len().saturating_sub(VISIBLE + 1);
        let lines = &self.lines[from..];
        let es: Vec<f32> = lines
            .iter()
            .map(|(_, at)| entered(self.now.saturating_duration_since(*at)))
            .collect();
        let depths = ticker_depths(&es);
        for (j, (text, at)) in lines.iter().enumerate() {
            let (alpha, muted) = ticker_style(depths[j]);
            if alpha <= 0.01 {
                continue;
            }
            let slot = (VISIBLE - 1) as f32 - depths[j];
            let y = (slot + 0.5) * ROW;
            let col = theme::mix(p.text, p.text_muted, muted).scale_alpha(alpha);
            // Leading marker: a tiny spinner on the newest line, a faint tick on finished ones.
            let mx = 12.0;
            if j == lines.len() - 1 && muted < 0.5 {
                let age = self.now.saturating_duration_since(*at).as_secs_f32();
                let (a0, len) = anim::spinner_arc(age);
                f.stroke(
                    &Path::circle(Point::new(mx, y), 5.0),
                    stroke(col.scale_alpha(0.14), 1.75),
                );
                f.stroke(
                    &arc_path(Point::new(mx, y), 5.0, a0, len),
                    stroke(col, 1.75),
                );
            } else {
                let pts = [
                    Point::new(mx - 4.0, y),
                    Point::new(mx - 1.2, y + 3.0),
                    Point::new(mx + 4.0, y - 3.0),
                ];
                if let Some(tick) = partial_line(&pts, 1.0) {
                    f.stroke(&tick, stroke(p.text_muted.scale_alpha(alpha * 0.8), 1.75));
                }
            }
            f.fill_text(Text {
                content: text.clone(),
                position: Point::new(30.0, y),
                color: col,
                size: theme::BODY.into(),
                font: if muted < 0.5 {
                    theme::MEDIUM
                } else {
                    theme::REGULAR
                },
                align_x: iced::alignment::Horizontal::Left.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        }
        vec![f.into_geometry()]
    }
}

/// The last few status lines of the check. `lines` are `(text, started_at)`
/// oldest first; when a line is appended it eases in from below (450 ms,
/// emphasized decelerate) while the older ones glide up, soften and fade.
/// Needs frames for 450 ms after each new line, or for as long as the newest
/// line shows its small spinner.
pub fn status_ticker<'a>(
    p: Palette,
    lines: &'a [(String, Instant)],
    now: Instant,
) -> Element<'a, Message> {
    canvas::Canvas::new(Ticker { p, lines, now })
        .width(Length::Fill)
        .height(Length::Fixed(TICKER_HEIGHT))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_converges_and_is_frame_rate_independent() {
        let once = approach(0.0, 1.0, 0.2, 5.0);
        let mut twice = 0.0;
        twice = approach(twice, 1.0, 0.1, 5.0);
        twice = approach(twice, 1.0, 0.1, 5.0);
        assert!((once - twice).abs() < 1e-5);
        assert!(approach(0.0, 1.0, 10.0, 5.0) > 0.999);
        assert_eq!(approach(0.3, 1.0, 0.0, 5.0), 0.3);
        assert_eq!(approach(0.3, 1.0, -1.0, 5.0), 0.3);
    }

    #[test]
    fn level_rises_from_bottom_of_shield() {
        assert_eq!(level_y(0.0), 140.0);
        assert_eq!(level_y(1.0), 20.0);
        assert_eq!(level_y(2.0), 20.0);
    }

    #[test]
    fn ticker_depths_settle_and_glide() {
        // Everything entered: newest 0, then 1, 2.
        assert_eq!(ticker_depths(&[1.0, 1.0, 1.0]), vec![2.0, 1.0, 0.0]);
        // Newest just arrived: it sits at -1, the previous one still at 0.
        assert_eq!(ticker_depths(&[1.0, 1.0, 0.0]), vec![1.0, 0.0, -1.0]);
        // Half way: everything moved half a row.
        let d = ticker_depths(&[1.0, 1.0, 0.5]);
        assert_eq!(d, vec![1.5, 0.5, -0.5]);
        assert!(ticker_depths(&[]).is_empty());
    }

    #[test]
    fn ticker_style_steps_back() {
        assert_eq!(ticker_style(-1.0).0, 0.0);
        assert_eq!(ticker_style(0.0), (1.0, 0.0));
        let (a1, m1) = ticker_style(1.0);
        assert!((a1 - 0.55).abs() < 1e-6 && m1 == 1.0);
        let (a2, _) = ticker_style(2.0);
        assert!(a2 < a1);
        assert_eq!(ticker_style(3.5).0, 0.0);
        // Monotone fade-out with depth.
        let mut prev = 1.0;
        for i in 0..=30 {
            let (a, _) = ticker_style(i as f32 / 10.0);
            assert!(a <= prev + 1e-6);
            prev = a;
        }
    }

    #[test]
    fn assets_are_valid_svg_with_current_color() {
        for (name, bytes) in [
            ("shield", SHIELD),
            ("glyph", GLYPH),
            ("check", CHECK),
            ("alert", ALERT),
            ("orbit", ORBIT),
        ] {
            let s = std::str::from_utf8(bytes).unwrap();
            assert!(s.contains("viewBox=\"0 0 160 160\""), "{name}");
            assert!(s.contains("currentColor"), "{name}");
            assert!(s.trim_end().ends_with("</svg>"), "{name}");
        }
    }

    #[test]
    fn hero_stops_asking_for_frames_when_settled() {
        let _g = anim::MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(false));
        assert!(animating(HeroPhase::Checking, Duration::from_secs(99)));
        assert!(animating(HeroPhase::Good, Duration::from_millis(500)));
        assert!(!animating(HeroPhase::Good, Duration::from_secs(5)));
        anim::set_reduced_override(Some(true));
        assert!(!animating(HeroPhase::Checking, Duration::ZERO));
        anim::set_reduced_override(None);
    }
}
