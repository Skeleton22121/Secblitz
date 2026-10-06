//! The full-window checking screen and the status ticker.
//!
//! [`checking_screen`] is the first check's whole page: the magnifying glass
//! ([`magnifier`]) as large as the window allows, the title, a progress bar
//! and the ticker, centred. The drawing drives its own frames; the page's
//! frame subscription is for the ticker and its `now`.
//!
//! [`status_ticker`] shows the last few status lines; each new line eases in
//! from below while the older ones step back and fade.
use super::anim::{self, arc_path, partial_line, stroke, EMPHASIZED};
use super::hairline::magnifier::{self, Magnifier};
use super::progress;
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::widgets;
use crate::gui::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::widget::{column, container, responsive};
use iced::{mouse, Alignment, Element, Length, Point, Rectangle, Renderer, Theme};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------

/// Scale of the drawing on the checking screen for the height it has. The
/// text, bar and ticker below it take `rest`; the drawing gets what is left,
/// from its compact size up to its full size.
fn hero_scale(height: f32, rest: f32) -> f32 {
    ((height - rest) / magnifier::VIEW.height).clamp(magnifier::COMPACT, magnifier::FULL)
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
// Checking screen
// ---------------------------------------------------------------------------

/// Height taken below the drawing on the checking screen: title and
/// subtitle, bar, ticker, the gaps between them and some air.
const SCREEN_REST: f32 = 64.0 + progress::HEIGHT + TICKER_HEIGHT + 3.0 * theme::S6 + theme::S8;

/// The first check's whole page: `art` (the magnifying glass, on the page
/// background) as large as the space allows, then `title`, `subtitle`, the
/// progress bar at `ratio` and the ticker, all centred in the space the page
/// gives it (fill it, don't scroll it).
pub fn checking_screen<'a>(
    p: Palette,
    title: String,
    subtitle: String,
    ratio: f32,
    art: Magnifier,
    lines: &'a [(String, Instant)],
    now: Instant,
) -> Element<'a, Message> {
    responsive(move |size| {
        let body = column![
            art.clone().view(hero_scale(size.height, SCREEN_REST)),
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
    fn hero_scale_uses_the_space_it_has() {
        assert_eq!(hero_scale(2000.0, SCREEN_REST), magnifier::FULL);
        assert_eq!(hero_scale(100.0, SCREEN_REST), magnifier::COMPACT);
        let mid = hero_scale(SCREEN_REST + 150.0, SCREEN_REST);
        assert!((mid - 150.0 / magnifier::VIEW.height).abs() < 1e-6);
        // At its largest it is the prototype's full-window size.
        assert_eq!(magnifier::VIEW.width * magnifier::FULL, 256.0);
        // In a compact region it is 160 px wide.
        assert_eq!(magnifier::VIEW.width * magnifier::COMPACT, 160.0);
    }
}
