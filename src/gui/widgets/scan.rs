//! The PC-check illustration, the full-window checking screen and the
//! status ticker.
//!
//! [`check_hero`] is a calm "scanning nearby" radar: soft rings pulse out from
//! a centre point every two seconds and small dots blink in around it, as if
//! things nearby were being found and looked at.
//!
//! # Where it comes from
//!
//! It is a native redraw of the free LottieFiles animation "Scanning nearby"
//! by 章_koala (Lottie Simple License: free for commercial use, changes
//! allowed, credit optional; see `assets/ANIMATION-LICENSE.txt`). The geometry and
//! timing are the original's, read from its JSON: a 400-unit square at 30
//! frames per second, a ring every 60 frames that lives 160 frames, and nine
//! dots on a 180-frame cycle. Drawing it on a canvas instead of playing the
//! file keeps it sharp at any size, lets it follow the theme's ink colour in
//! light and dark mode, and draws the same on the GPU and CPU renderers. The
//! only change is a softer fill and outline (see [`GLOW`]).
//!
//! # Driving it
//!
//! The page runs a frame subscription while the check runs and passes the
//! time since it began:
//!
//! ```ignore
//! scan::check_hero(p, clock.elapsed_at(now), scan::HERO)
//! ```
//!
//! [`checking_screen`] is the first check's whole page: the radar as large as
//! the window allows, the title, a progress bar and the ticker, centred.
//!
//! [`status_ticker`] shows the last few status lines; each new line eases in
//! from below while the older ones step back and fade.
use super::anim::{self, arc_path, partial_line, stroke, Curve, EMPHASIZED};
use super::progress;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets;
use crate::gui::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::widget::{column, container, responsive};
use iced::{mouse, Alignment, Element, Length, Point, Rectangle, Renderer, Theme, Vector};
use std::time::{Duration, Instant};

/// Edge of the illustration inside a compact region, in logical pixels.
pub const HERO: f32 = 160.0;
/// Largest edge on the full-window checking screen.
pub const HERO_MAX: f32 = 320.0;

/// Side of the original composition, in its own units.
const COMP: f32 = 400.0;
const FPS: f32 = 30.0;
/// A new ring starts every this many frames…
const RING_EVERY: f32 = 60.0;
/// …and grows and fades for this many.
const RING_LIFE: f32 = 160.0;
/// Radius a ring reaches as it vanishes: 240 % of a 100-unit circle, inside
/// a group scaled 168.615 %.
const RING_MAX: f32 = 0.5 * 100.0 * 2.4 * 1.68615;
/// Ring outline width (1 unit in the same group).
const RING_WIDTH: f32 = 1.68615;
/// Strength at birth of the fill inside each ring and of its outline. The
/// original starts both higher (half and full); in a single ink colour that
/// reads as a heavy grey disc, so they are softened to layered tints.
const GLOW: f32 = 0.16;
const LINE: f32 = 0.55;
/// Centre point: a 2-unit circle with a 1-unit outline, group scale applied.
const CENTRE: f32 = 2.5292;
/// Found dots: 5-unit marks.
const DOT: f32 = 2.5;
/// The dot cycle repeats after this many frames.
const DOT_CYCLE: f32 = 180.0;
/// Each dot: centre in composition units, and the cycle frame it starts to
/// fade in. It is fully shown 12 frames later, holds 12, and fades out in 11.
const DOTS: [(f32, f32, f32); 9] = [
    (244.75, 253.5, 2.0),
    (106.25, 269.5, 23.0),
    (288.25, 256.0, 27.0),
    (120.75, 246.0, 68.0),
    (132.75, 288.5, 78.0),
    (237.25, 81.0, 91.0),
    (214.75, 223.5, 91.0),
    (253.75, 155.0, 118.0),
    (246.75, 291.0, 142.0),
];
/// The original's ease on rings: out (0.333, 0), in (0.667, 1).
const RING_EASE: Curve = anim::cubic_bezier(0.333, 0.0, 0.667, 1.0);
/// The original's near-linear ease on dots.
const DOT_EASE: Curve = anim::cubic_bezier(0.167, 0.167, 0.833, 0.833);
/// Frame shown when motion is reduced: two rings and two dots in view.
const STILL_FRAME: f32 = 40.0;

// ---------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------

/// Frame of the animation `elapsed` after it started.
fn frame_at(elapsed: Duration) -> f32 {
    if anim::reduced() {
        STILL_FRAME
    } else {
        elapsed.as_secs_f32() * FPS
    }
}

/// The rings alive at `frame`, youngest first, as (radius, strength) with the
/// radius in composition units and strength 1 at birth falling to 0.
fn rings(frame: f32) -> impl Iterator<Item = (f32, f32)> {
    let young = frame.max(0.0) % RING_EVERY;
    (0..)
        .map(move |k| young + k as f32 * RING_EVERY)
        .take_while(|age| *age < RING_LIFE)
        .map(|age| {
            let e = RING_EASE.at(age / RING_LIFE);
            (RING_MAX * e, 1.0 - e)
        })
}

/// How visible a dot that starts at `start` is at `frame`, 0..=1.
fn dot_alpha(frame: f32, start: f32) -> f32 {
    let t = frame.max(0.0) % DOT_CYCLE - start;
    if t <= 0.0 || t >= 35.0 {
        0.0
    } else if t < 12.0 {
        DOT_EASE.at(t / 12.0)
    } else if t <= 24.0 {
        1.0
    } else {
        1.0 - DOT_EASE.at((t - 24.0) / 11.0)
    }
}

/// Edge of the radar on the checking screen for the height it has. The text,
/// bar and ticker below it take `rest`; the radar gets what is left, within
/// [`HERO`]..=[`HERO_MAX`].
fn hero_edge(height: f32, rest: f32) -> f32 {
    (height - rest).clamp(HERO, HERO_MAX)
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

/// The scanning radar, `edge` logical pixels square. `elapsed` is the time
/// since the check began.
pub fn check_hero<'a>(p: Palette, elapsed: Duration, edge: f32) -> Element<'a, Message> {
    canvas::Canvas::new(Radar {
        ink: p.text,
        frame: frame_at(elapsed),
    })
    .width(Length::Fixed(edge))
    .height(Length::Fixed(edge))
    .into()
}

struct Radar {
    ink: iced::Color,
    frame: f32,
}

impl canvas::Program<Message> for Radar {
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
        // Scale by hand rather than with a frame transform: outline widths
        // must stay in screen pixels on both renderers.
        let k = bounds.width.min(bounds.height) / COMP;
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let line = (RING_WIDTH * k).max(1.0);
        for (radius, strength) in rings(self.frame) {
            let r = radius * k;
            if r < 0.5 || strength <= 0.004 {
                continue;
            }
            let circle = Path::circle(c, r);
            f.fill(&circle, self.ink.scale_alpha(GLOW * strength));
            f.stroke(&circle, stroke(self.ink.scale_alpha(LINE * strength), line));
        }
        for (x, y, start) in DOTS {
            let a = dot_alpha(self.frame, start);
            if a > 0.004 {
                let at = c + Vector::new((x - COMP / 2.0) * k, (y - COMP / 2.0) * k);
                f.fill(
                    &Path::circle(at, (DOT * k).max(1.25)),
                    self.ink.scale_alpha(a),
                );
            }
        }
        f.fill(&Path::circle(c, (CENTRE * k).max(1.75)), self.ink);
        vec![f.into_geometry()]
    }
}

// ---------------------------------------------------------------------------
// Checking screen
// ---------------------------------------------------------------------------

/// Height taken below the radar on the checking screen: title and subtitle,
/// bar, ticker, the gaps between them and some air.
const SCREEN_REST: f32 = 64.0 + progress::HEIGHT + TICKER_HEIGHT + 3.0 * theme::S6 + theme::S8;

/// The first check's whole page: the radar as large as the space allows,
/// then `title`, `subtitle`, the progress bar at `ratio` and the ticker, all
/// centred in the space the page gives it (fill it, don't scroll it).
pub fn checking_screen<'a>(
    p: Palette,
    title: String,
    subtitle: String,
    ratio: f32,
    lines: &'a [(String, Instant)],
    now: Instant,
    elapsed: Duration,
) -> Element<'a, Message> {
    responsive(move |size| {
        let body = column![
            check_hero(p, elapsed, hero_edge(size.height, SCREEN_REST)),
            column![
                widgets::h1(p, title.clone()),
                widgets::muted(p, subtitle.clone()),
            ]
            .spacing(theme::S1)
            .align_x(Alignment::Center),
            container(progress::bar_eased(p, ratio, Tone::Neutral)).max_width(theme::MAX_READABLE),
            container(status_ticker(p, lines, now)).max_width(theme::MAX_READABLE),
        ]
        .spacing(theme::S6)
        .align_x(Alignment::Center)
        .width(Length::Fill);
        container(body).center(Length::Fill).into()
    })
    .into()
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
    fn rings_follow_the_original_timing() {
        // Frame 0: rings aged 0, 60 and 120 frames; the 180-frame one is gone.
        let r: Vec<_> = rings(0.0).collect();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], (0.0, 1.0));
        assert!(r[1].0 < r[2].0 && r[1].1 > r[2].1);
        // A ring is born every 60 frames, so the picture repeats.
        let a: Vec<_> = rings(17.0).collect();
        let b: Vec<_> = rings(17.0 + RING_EVERY).collect();
        assert_eq!(a, b);
        // Near the end of its life a ring is almost full size and invisible.
        let last = rings(RING_LIFE - 2.0 * RING_EVERY - 0.01).last().unwrap();
        assert!(last.0 > 0.99 * RING_MAX && last.1 < 0.01);
        // The largest ring stays within the composition.
        const { assert!(RING_MAX < COMP / 2.0 + 3.0) };
    }

    #[test]
    fn dots_blink_in_their_slots() {
        let (_, _, start) = DOTS[2];
        assert_eq!(dot_alpha(start, start), 0.0);
        assert_eq!(dot_alpha(start + 18.0, start), 1.0);
        assert!(dot_alpha(start + 6.0, start) > 0.0 && dot_alpha(start + 6.0, start) < 1.0);
        assert_eq!(dot_alpha(start + 35.0, start), 0.0);
        // Same moment one cycle later.
        assert_eq!(dot_alpha(start + 18.0 + DOT_CYCLE, start), 1.0);
        // Every dot finishes inside its cycle and sits inside the frame.
        for (x, y, s) in DOTS {
            assert!(s + 35.0 <= DOT_CYCLE);
            assert!((0.0..COMP).contains(&x) && (0.0..COMP).contains(&y));
        }
    }

    #[test]
    fn hero_edge_uses_the_space_it_has() {
        assert_eq!(hero_edge(2000.0, SCREEN_REST), HERO_MAX);
        assert_eq!(hero_edge(100.0, SCREEN_REST), HERO);
        let mid = hero_edge(SCREEN_REST + 250.0, SCREEN_REST);
        assert_eq!(mid, 250.0);
    }

    #[test]
    fn still_frame_shows_rings_and_dots() {
        let _g = anim::MOTION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        anim::set_reduced_override(Some(true));
        let f = frame_at(Duration::from_secs(42));
        anim::set_reduced_override(None);
        assert_eq!(f, STILL_FRAME);
        assert!(rings(f).filter(|(r, s)| *r > 10.0 && *s > 0.1).count() >= 2);
        assert!(DOTS.iter().filter(|d| dot_alpha(f, d.2) > 0.5).count() >= 2);
    }
}
