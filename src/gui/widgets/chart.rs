//! Trend chart: a calm, smooth line with a soft area under it.
use super::anim::{self, DECELERATE};
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, Cache, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const ENTRANCE: Duration = Duration::from_millis(500);
const GUTTER: f32 = 34.0;
const PAD_R: f32 = 8.0;
const PAD_TOP: f32 = 12.0;
const LABEL_H: f32 = 24.0;
const BANDS: usize = 4;


pub fn monotone_tangents(ys: &[f32]) -> Vec<f32> {
    let n = ys.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let d: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        let (a, b) = (d[k - 1], d[k]);
        m[k] = if a * b <= 0.0 {
            0.0
        } else {
            2.0 * a * b / (a + b)
        };
    }
    m
}

pub fn hermite(y0: f32, y1: f32, m0: f32, m1: f32, u: f32) -> f32 {
    let (u2, u3) = (u * u, u * u * u);
    (2.0 * u3 - 3.0 * u2 + 1.0) * y0
        + (u3 - 2.0 * u2 + u) * m0
        + (-2.0 * u3 + 3.0 * u2) * y1
        + (u3 - u2) * m1
}

pub fn y_domain(values: impl Iterator<Item = f32>) -> (f32, f32) {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for v in values {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        return (0.0, 1.0);
    }
    let mut lo = (((lo - 0.05).max(0.0)) * 10.0).floor() / 10.0;
    let mut hi = (((hi + 0.05).min(1.0)) * 10.0).ceil() / 10.0;
    if hi - lo < 0.3 {
        let mid = (hi + lo) / 2.0;
        lo = (mid - 0.15).max(0.0);
        hi = (lo + 0.3).min(1.0);
        lo = (hi - 0.3).max(0.0);
    }
    (lo, hi)
}

pub fn nearest(x: f32, left: f32, right: f32, n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    if n == 1 || right <= left {
        return Some(0);
    }
    let k = ((x - left) / (right - left) * (n - 1) as f32).round();
    Some(k.clamp(0.0, (n - 1) as f32) as usize)
}


struct Trend<'a> {
    p: Palette,
    tone: Tone,
    points: &'a [(u64, f32)],
    cache: &'a Cache,
    date: &'a dyn Fn(u64) -> String,
}

#[derive(Default)]
pub struct TrendState {
    start: Option<Instant>,
    reveal: f32,
    entered: bool,
    hover: Option<usize>,
    key: std::cell::Cell<u64>,
}

struct Plot {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Plot {
    fn of(size: Size) -> Self {
        Self {
            left: GUTTER,
            right: (size.width - PAD_R).max(GUTTER + 1.0),
            top: PAD_TOP,
            bottom: (size.height - LABEL_H).max(PAD_TOP + 1.0),
        }
    }
    fn x(&self, i: usize, n: usize) -> f32 {
        if n < 2 {
            (self.left + self.right) / 2.0
        } else {
            self.left + (self.right - self.left) * i as f32 / (n - 1) as f32
        }
    }
}

fn percent(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}

impl Trend<'_> {
    fn key(&self, size: Size) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (t, v) in self.points {
            t.hash(&mut h);
            v.to_bits().hash(&mut h);
        }
        (self.p.mode as u8).hash(&mut h);
        (self.tone as u8).hash(&mut h);
        size.width.to_bits().hash(&mut h);
        h.finish() | 1
    }

    fn y_of(&self, plot: &Plot, domain: (f32, f32), v: f32) -> f32 {
        let span = (domain.1 - domain.0).max(0.001);
        plot.bottom - (plot.bottom - plot.top) * ((v - domain.0) / span).clamp(0.0, 1.0)
    }

    fn pixels(&self, plot: &Plot, domain: (f32, f32)) -> Vec<Point> {
        let n = self.points.len();
        self.points
            .iter()
            .enumerate()
            .map(|(i, (_, v))| Point::new(plot.x(i, n), self.y_of(plot, domain, *v)))
            .collect()
    }

    fn paint_static(&self, f: &mut Frame, reveal: f32) {
        let p = &self.p;
        let size = f.size();
        let plot = Plot::of(size);
        let domain = y_domain(self.points.iter().map(|(_, v)| *v));
        let tone = p.tone(self.tone);
        let guide = p.text.scale_alpha(if p.mode == theme::Mode::Dark {
            0.09
        } else {
            0.07
        });
        let mid = (plot.top + plot.bottom) / 2.0;
        for y in [plot.top, mid, plot.bottom] {
            f.stroke(
                &Path::line(Point::new(plot.left, y), Point::new(plot.right, y)),
                Stroke::default().with_width(1.0).with_color(guide),
            );
        }
        let small =
            |content: String, x: f32, y: f32, ax: iced::alignment::Horizontal, a: f32| Text {
                content,
                position: Point::new(x, y),
                color: p.text_muted.scale_alpha(a),
                size: (theme::SMALL - 1.0).into(),
                font: theme::REGULAR,
                align_x: ax.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            };
        let n = self.points.len();
        if n == 0 {
            return;
        }
        use iced::alignment::Horizontal::{Left, Right};
        f.fill_text(small(
            percent(domain.1),
            plot.left - 8.0,
            plot.top,
            Right,
            0.8,
        ));
        f.fill_text(small(
            percent(domain.0),
            plot.left - 8.0,
            plot.bottom,
            Right,
            0.8,
        ));
        let ly = size.height - LABEL_H / 2.0 + 2.0;
        f.fill_text(small(
            (self.date)(self.points[0].0),
            plot.left,
            ly,
            Left,
            1.0,
        ));
        if n > 1 {
            f.fill_text(small(
                (self.date)(self.points[n - 1].0),
                plot.right,
                ly,
                Right,
                1.0,
            ));
        }

        let px = self.pixels(&plot, domain);
        let last = px[n - 1];
        if n == 1 {
            f.stroke(
                &Path::line(
                    Point::new(plot.left, last.y),
                    Point::new(plot.right, last.y),
                ),
                Stroke::default()
                    .with_width(1.5)
                    .with_color(tone.scale_alpha(0.35 * reveal)),
            );
        } else {
            let tangents = monotone_tangents(&px.iter().map(|q| q.y).collect::<Vec<_>>());
            let step = (plot.right - plot.left) / (n - 1) as f32;
            let curve = |b: &mut canvas::path::Builder| {
                b.move_to(px[0]);
                for k in 0..n - 1 {
                    b.bezier_curve_to(
                        Point::new(px[k].x + step / 3.0, px[k].y + tangents[k] / 3.0),
                        Point::new(
                            px[k + 1].x - step / 3.0,
                            px[k + 1].y - tangents[k + 1] / 3.0,
                        ),
                        px[k + 1],
                    );
                }
            };
            let line = Path::new(curve);
            let area = Path::new(|b| {
                curve(b);
                b.line_to(Point::new(px[n - 1].x, plot.bottom));
                b.line_to(Point::new(px[0].x, plot.bottom));
                b.close();
            });
            let reveal_x = plot.left + (plot.right - plot.left) * reveal + 2.0;
            let top_y = px.iter().map(|q| q.y).fold(f32::MAX, f32::min) - 2.0;
            let band_alpha = if p.mode == theme::Mode::Dark {
                0.05
            } else {
                0.045
            };
            for k in 0..BANDS {
                let frac = (BANDS - k) as f32 / (BANDS + 1) as f32;
                let h = (plot.bottom - top_y) * frac;
                let clip = Rectangle::new(
                    Point::new(0.0, top_y),
                    Size::new(reveal_x.min(size.width), h),
                );
                f.with_clip(clip, |c| c.fill(&area, tone.scale_alpha(band_alpha)));
            }
            let clip = Rectangle::new(
                Point::new(0.0, 0.0),
                Size::new(reveal_x.min(size.width), size.height),
            );
            f.with_clip(clip, |c| {
                c.stroke(
                    &line,
                    Stroke::default()
                        .with_width(2.0)
                        .with_color(tone)
                        .with_line_cap(canvas::LineCap::Round)
                        .with_line_join(canvas::LineJoin::Round),
                )
            });
        }
        if reveal >= 0.98 {
            f.fill(&Path::circle(last, 8.0), tone.scale_alpha(0.16));
            f.fill(&Path::circle(last, 3.8), tone);
            if n == 1 {
                f.fill_text(Text {
                    content: percent(self.points[0].1),
                    position: Point::new(last.x, last.y - 20.0),
                    color: p.text,
                    size: theme::BODY.into(),
                    font: theme::SEMIBOLD,
                    align_x: iced::alignment::Horizontal::Center.into(),
                    align_y: iced::alignment::Vertical::Center,
                    ..Text::default()
                });
            }
        }
    }

    fn paint_hover(&self, f: &mut Frame, i: usize) {
        let n = self.points.len();
        if i >= n || n < 2 {
            return;
        }
        let p = &self.p;
        let size = f.size();
        let plot = Plot::of(size);
        let domain = y_domain(self.points.iter().map(|(_, v)| *v));
        let tone = p.tone(self.tone);
        let (t, v) = self.points[i];
        let pt = Point::new(plot.x(i, n), self.y_of(&plot, domain, v));
        f.stroke(
            &Path::line(Point::new(pt.x, plot.top), Point::new(pt.x, plot.bottom)),
            Stroke::default()
                .with_width(1.0)
                .with_color(p.text.scale_alpha(0.14)),
        );
        f.fill(&Path::circle(pt, 9.0), tone.scale_alpha(0.16));
        f.fill(&Path::circle(pt, 4.5), tone);

        let label = format!("{}  ·  {}", percent(v), (self.date)(t));
        let w = label.chars().count() as f32 * 6.8 + 20.0;
        let h = 26.0;
        let x = (pt.x - w / 2.0).clamp(2.0, (size.width - w - 2.0).max(2.0));
        let y = if pt.y - h - 12.0 < 0.0 {
            pt.y + 12.0
        } else {
            pt.y - h - 12.0
        };
        f.fill(
            &Path::rounded_rectangle(Point::new(x, y), Size::new(w, h), 6.0.into()),
            p.text,
        );
        f.fill_text(Text {
            content: label,
            position: Point::new(x + w / 2.0, y + h / 2.0),
            color: p.bg,
            size: (theme::SMALL).into(),
            font: theme::MEDIUM,
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
    }
}

impl canvas::Program<Message> for Trend<'_> {
    type State = TrendState;

    fn update(
        &self,
        st: &mut TrendState,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            iced::Event::Window(iced::window::Event::RedrawRequested(now)) => {
                if st.entered {
                    return None;
                }
                if anim::reduced() {
                    st.reveal = 1.0;
                    st.entered = true;
                    return Some(canvas::Action::request_redraw());
                }
                let start = *st.start.get_or_insert(*now);
                let t = now.saturating_duration_since(start).as_secs_f32() / ENTRANCE.as_secs_f32();
                if t >= 1.0 {
                    st.reveal = 1.0;
                    st.entered = true;
                } else {
                    st.reveal = DECELERATE.at(t);
                }
                Some(canvas::Action::request_redraw())
            }
            iced::Event::Mouse(_) => {
                let plot = Plot::of(bounds.size());
                let hover = cursor
                    .position_in(bounds)
                    .and_then(|pt| nearest(pt.x, plot.left, plot.right, self.points.len()));
                if hover != st.hover {
                    st.hover = hover;
                    Some(canvas::Action::request_redraw())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        st: &TrendState,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let key = self.key(bounds.size());
        if st.key.get() != key {
            self.cache.clear();
            st.key.set(key);
        }
        let mut out = Vec::with_capacity(2);
        if st.entered {
            out.push(
                self.cache
                    .draw(renderer, bounds.size(), |f| self.paint_static(f, 1.0)),
            );
        } else {
            let mut f = Frame::new(renderer, bounds.size());
            self.paint_static(&mut f, st.reveal);
            out.push(f.into_geometry());
        }
        if let (Some(i), true) = (st.hover, st.entered) {
            let mut f = Frame::new(renderer, bounds.size());
            self.paint_hover(&mut f, i);
            out.push(f.into_geometry());
        }
        out
    }

    fn mouse_interaction(
        &self,
        _: &TrendState,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

pub fn trend<'a>(
    p: Palette,
    tone: Tone,
    points: &'a [(u64, f32)],
    cache: &'a Cache,
    date: &'a dyn Fn(u64) -> String,
    height: f32,
) -> Element<'a, Message> {
    canvas::Canvas::new(Trend {
        p,
        tone,
        points,
        cache,
        date,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ys: &[f32], per: usize) -> Vec<f32> {
        let m = monotone_tangents(ys);
        let mut out = vec![];
        for k in 0..ys.len() - 1 {
            for s in 0..=per {
                out.push(hermite(
                    ys[k],
                    ys[k + 1],
                    m[k],
                    m[k + 1],
                    s as f32 / per as f32,
                ));
            }
        }
        out
    }

    #[test]
    fn passes_through_points() {
        let ys = [0.2, 0.5, 0.4, 0.9];
        let m = monotone_tangents(&ys);
        for k in 0..ys.len() - 1 {
            assert!((hermite(ys[k], ys[k + 1], m[k], m[k + 1], 0.0) - ys[k]).abs() < 1e-6);
            assert!((hermite(ys[k], ys[k + 1], m[k], m[k + 1], 1.0) - ys[k + 1]).abs() < 1e-6);
        }
    }

    #[test]
    fn never_overshoots_between_neighbours() {
        let ys = [0.1, 0.9, 0.92, 0.2, 0.2, 0.8, 0.3];
        let m = monotone_tangents(&ys);
        for k in 0..ys.len() - 1 {
            let (lo, hi) = (ys[k].min(ys[k + 1]), ys[k].max(ys[k + 1]));
            for s in 0..=100 {
                let v = hermite(ys[k], ys[k + 1], m[k], m[k + 1], s as f32 / 100.0);
                assert!(v >= lo - 1e-5 && v <= hi + 1e-5, "k={k} s={s} v={v}");
            }
        }
    }

    #[test]
    fn monotone_data_gives_monotone_curve() {
        let v = sample(&[0.1, 0.2, 0.7, 0.75, 0.99], 50);
        assert!(v.windows(2).all(|w| w[1] + 1e-5 >= w[0]));
    }

    #[test]
    fn extrema_and_flat_runs_get_flat_tangents() {
        let m = monotone_tangents(&[0.2, 0.8, 0.3, 0.3, 0.6]);
        assert_eq!(m[1], 0.0);
        assert_eq!(m[2], 0.0);
        assert_eq!(m[3], 0.0);
        assert_eq!(monotone_tangents(&[0.5]), vec![0.0]);
        assert!(monotone_tangents(&[]).is_empty());
    }

    #[test]
    fn domain_is_padded_rounded_and_never_a_sliver() {
        let (lo, hi) = y_domain([0.62, 0.64].into_iter());
        assert!(hi - lo >= 0.29 && lo >= 0.0 && hi <= 1.0);
        assert!(lo <= 0.62 && hi >= 0.64);
        let (lo, hi) = y_domain([0.98, 1.0].into_iter());
        assert!(hi == 1.0 && hi - lo >= 0.29);
        assert_eq!(y_domain(std::iter::empty()), (0.0, 1.0));
    }

    #[test]
    fn nearest_point_by_x() {
        assert_eq!(nearest(10.0, 0.0, 100.0, 0), None);
        assert_eq!(nearest(10.0, 0.0, 100.0, 1), Some(0));
        assert_eq!(nearest(-50.0, 0.0, 100.0, 5), Some(0));
        assert_eq!(nearest(500.0, 0.0, 100.0, 5), Some(4));
        assert_eq!(nearest(51.0, 0.0, 100.0, 5), Some(2));
    }
}
