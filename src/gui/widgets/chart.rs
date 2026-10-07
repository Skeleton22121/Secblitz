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
const LABEL_CHAR_W: f32 = 6.4;
const LABEL_GAP: f32 = 12.0;

/// Slopes that keep the line from swinging past its neighbours; points may be unevenly spaced.
pub fn monotone_slopes(xs: &[f32], ys: &[f32]) -> Vec<f32> {
    let n = xs.len().min(ys.len());
    if n < 2 {
        return vec![0.0; n];
    }
    let h: Vec<f32> = xs
        .windows(2)
        .take(n - 1)
        .map(|w| (w[1] - w[0]).max(1e-3))
        .collect();
    let d: Vec<f32> = (0..n - 1).map(|k| (ys[k + 1] - ys[k]) / h[k]).collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        let (a, b) = (d[k - 1], d[k]);
        m[k] = if a * b <= 0.0 {
            0.0
        } else {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            (w1 + w2) / (w1 / a + w2 / b)
        };
    }
    m
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

/// Equal time values share one pixel, so then the points spread evenly.
pub fn plot_xs(keys: &[u64], left: f32, right: f32) -> Vec<f32> {
    let n = keys.len();
    let (Some(&first), Some(&last)) = (keys.first(), keys.last()) else {
        return Vec::new();
    };
    if n == 1 {
        return vec![(left + right) / 2.0];
    }
    let span = last.saturating_sub(first);
    keys.iter()
        .enumerate()
        .map(|(i, k)| {
            let share = if span == 0 {
                i as f32 / (n - 1) as f32
            } else {
                k.saturating_sub(first) as f32 / span as f32
            };
            left + (right - left) * share
        })
        .collect()
}

pub fn nearest(x: f32, px: &[f32]) -> Option<usize> {
    px.iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|(i, _)| i)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateLabel {
    pub index: usize,
    pub x: f32,
    pub anchor: Anchor,
}

/// Labels never overlap or leave the plot; with no room for both ends only the last stays.
pub fn date_labels(
    px: &[f32],
    width: impl Fn(usize) -> f32,
    left: f32,
    right: f32,
) -> Vec<DateLabel> {
    let n = px.len();
    if n < 2 {
        return Vec::new();
    }
    let first_end = left + width(0);
    let last_start = right - width(n - 1);
    let last = DateLabel {
        index: n - 1,
        x: right,
        anchor: Anchor::End,
    };
    if first_end + LABEL_GAP > last_start {
        return vec![last];
    }
    let mut out = vec![DateLabel {
        index: 0,
        x: left,
        anchor: Anchor::Start,
    }];
    if n >= 5 {
        let centre = (left + right) / 2.0;
        let middle = (1..n - 1)
            .min_by(|a, b| (px[*a] - centre).abs().total_cmp(&(px[*b] - centre).abs()))
            .unwrap_or(1);
        let w = width(middle);
        let x = px[middle].clamp(left + w / 2.0, right - w / 2.0);
        if x - w / 2.0 >= first_end + LABEL_GAP && x + w / 2.0 + LABEL_GAP <= last_start {
            out.push(DateLabel {
                index: middle,
                x,
                anchor: Anchor::Middle,
            });
        }
    }
    out.push(last);
    out
}

fn label_width(label: &str) -> f32 {
    label.chars().count() as f32 * LABEL_CHAR_W
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

    fn xs(&self, plot: &Plot) -> Vec<f32> {
        let keys: Vec<u64> = self.points.iter().map(|(k, _)| *k).collect();
        plot_xs(&keys, plot.left, plot.right)
    }

    fn pixels(&self, plot: &Plot, xs: &[f32], domain: (f32, f32)) -> Vec<Point> {
        self.points
            .iter()
            .zip(xs)
            .map(|((_, v), x)| Point::new(*x, self.y_of(plot, domain, *v)))
            .collect()
    }

    fn paint_static(&self, f: &mut Frame, reveal: f32) {
        let n = self.points.len();
        if n < 2 {
            return;
        }
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
        use iced::alignment::Horizontal::{Center, Left, Right};
        for (y, v) in [(plot.top, domain.1), (plot.bottom, domain.0)] {
            f.fill_text(small(percent(v), plot.left - 8.0, y, Right, 0.8));
        }

        let xs = self.xs(&plot);
        let labels: Vec<String> = self.points.iter().map(|(k, _)| (self.date)(*k)).collect();
        let ly = size.height - LABEL_H / 2.0 + 2.0;
        for l in date_labels(&xs, |i| label_width(&labels[i]), plot.left, plot.right) {
            let align = match l.anchor {
                Anchor::Start => Left,
                Anchor::Middle => Center,
                Anchor::End => Right,
            };
            f.fill_text(small(labels[l.index].clone(), l.x, ly, align, 1.0));
        }

        let px = self.pixels(&plot, &xs, domain);
        let last = px[n - 1];
        let slopes = monotone_slopes(&xs, &px.iter().map(|q| q.y).collect::<Vec<_>>());
        let curve = |b: &mut canvas::path::Builder| {
            b.move_to(px[0]);
            for k in 0..n - 1 {
                let h = px[k + 1].x - px[k].x;
                b.bezier_curve_to(
                    Point::new(px[k].x + h / 3.0, px[k].y + slopes[k] * h / 3.0),
                    Point::new(px[k + 1].x - h / 3.0, px[k + 1].y - slopes[k + 1] * h / 3.0),
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
        if reveal >= 0.98 {
            f.fill(&Path::circle(last, 8.0), tone.scale_alpha(0.16));
            f.fill(&Path::circle(last, 3.8), tone);
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
        let pt = Point::new(self.xs(&plot)[i], self.y_of(&plot, domain, v));
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
                    return None;
                }
                let start = *st.start.get_or_insert(*now);
                let t = now.saturating_duration_since(start).as_secs_f32() / ENTRANCE.as_secs_f32();
                if t >= 1.0 {
                    st.reveal = 1.0;
                    st.entered = true;
                    return None;
                }
                st.reveal = DECELERATE.at(t);
                Some(canvas::Action::request_redraw())
            }
            iced::Event::Mouse(_) => {
                let plot = Plot::of(bounds.size());
                let hover = cursor
                    .position_in(bounds)
                    .and_then(|pt| nearest(pt.x, &self.xs(&plot)));
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

/// Needs at least two points; with fewer it is empty.
pub fn trend<'a>(
    p: Palette,
    tone: Tone,
    points: &'a [(u64, f32)],
    cache: &'a Cache,
    date: &'a dyn Fn(u64) -> String,
    height: f32,
) -> Element<'a, Message> {
    if points.len() < 2 {
        return iced::widget::space::vertical().height(0.0).into();
    }
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

    fn hermite(y0: f32, y1: f32, m0: f32, m1: f32, h: f32, u: f32) -> f32 {
        let (u2, u3) = (u * u, u * u * u);
        (2.0 * u3 - 3.0 * u2 + 1.0) * y0
            + (u3 - 2.0 * u2 + u) * m0 * h
            + (-2.0 * u3 + 3.0 * u2) * y1
            + (u3 - u2) * m1 * h
    }

    fn even(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32).collect()
    }

    fn sample(xs: &[f32], ys: &[f32], per: usize) -> Vec<f32> {
        let m = monotone_slopes(xs, ys);
        let mut out = vec![];
        for k in 0..ys.len() - 1 {
            let h = xs[k + 1] - xs[k];
            for s in 0..=per {
                out.push(hermite(
                    ys[k],
                    ys[k + 1],
                    m[k],
                    m[k + 1],
                    h,
                    s as f32 / per as f32,
                ));
            }
        }
        out
    }

    #[test]
    fn passes_through_points() {
        let xs = [0.0, 1.0, 4.0, 5.0];
        let ys = [0.2, 0.5, 0.4, 0.9];
        let m = monotone_slopes(&xs, &ys);
        for k in 0..ys.len() - 1 {
            let h = xs[k + 1] - xs[k];
            assert!((hermite(ys[k], ys[k + 1], m[k], m[k + 1], h, 0.0) - ys[k]).abs() < 1e-6);
            assert!((hermite(ys[k], ys[k + 1], m[k], m[k + 1], h, 1.0) - ys[k + 1]).abs() < 1e-6);
        }
    }

    #[test]
    fn never_overshoots_between_neighbours_however_unevenly_spaced() {
        let ys = [0.1, 0.9, 0.92, 0.2, 0.2, 0.8, 0.3];
        for xs in [even(7), vec![0.0, 1.0, 2.0, 30.0, 31.0, 90.0, 400.0]] {
            let m = monotone_slopes(&xs, &ys);
            for k in 0..ys.len() - 1 {
                let h = xs[k + 1] - xs[k];
                let (lo, hi) = (ys[k].min(ys[k + 1]), ys[k].max(ys[k + 1]));
                for s in 0..=100 {
                    let v = hermite(ys[k], ys[k + 1], m[k], m[k + 1], h, s as f32 / 100.0);
                    assert!(v >= lo - 1e-5 && v <= hi + 1e-5, "k={k} s={s} v={v}");
                }
            }
        }
    }

    #[test]
    fn monotone_data_gives_monotone_curve() {
        let ys = [0.1, 0.2, 0.7, 0.75, 0.99];
        for xs in [even(5), vec![0.0, 1.0, 9.0, 10.0, 60.0]] {
            let v = sample(&xs, &ys, 50);
            assert!(v.windows(2).all(|w| w[1] + 1e-5 >= w[0]));
        }
    }

    #[test]
    fn a_falling_score_gives_a_falling_curve() {
        let v = sample(&[0.0, 3.0, 4.0, 20.0], &[1.0, 0.8, 0.5, 0.2], 50);
        assert!(v.windows(2).all(|w| w[1] <= w[0] + 1e-5));
    }

    #[test]
    fn extrema_and_flat_runs_get_flat_slopes() {
        let m = monotone_slopes(&even(5), &[0.2, 0.8, 0.3, 0.3, 0.6]);
        assert_eq!(m[1], 0.0);
        assert_eq!(m[2], 0.0);
        assert_eq!(m[3], 0.0);
        assert_eq!(monotone_slopes(&[1.0], &[0.5]), vec![0.0]);
        assert!(monotone_slopes(&[], &[]).is_empty());
    }

    #[test]
    fn a_score_that_stays_at_one_hundred_gives_a_flat_curve() {
        let m = monotone_slopes(&[0.0, 5.0, 6.0], &[1.0, 1.0, 1.0]);
        assert!(m.iter().all(|s| *s == 0.0));
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
    fn domain_holds_every_value_when_the_score_drops() {
        let (lo, hi) = y_domain([1.0, 0.8, 0.4, 0.1].into_iter());
        assert!(lo <= 0.1 && hi >= 1.0);
        assert!((0.0..=1.0).contains(&lo) && (0.0..=1.0).contains(&hi));
    }

    #[test]
    fn place_follows_time() {
        let xs = plot_xs(&[10, 11, 20], 34.0, 134.0);
        assert_eq!(xs, vec![34.0, 44.0, 134.0]);
        assert!(plot_xs(&[], 0.0, 10.0).is_empty());
    }

    #[test]
    fn two_days_far_apart_sit_at_the_two_ends() {
        let xs = plot_xs(&[100, 300], 34.0, 434.0);
        assert_eq!(xs, vec![34.0, 434.0]);
    }

    #[test]
    fn a_month_of_days_stays_inside_the_plot_and_in_order() {
        let keys: Vec<u64> = (0..45).map(|d| 20_000 + d * 2 + d / 7).collect();
        let xs = plot_xs(&keys, 34.0, 700.0);
        assert_eq!((xs[0], xs[44]), (34.0, 700.0));
        assert!(xs.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn equal_times_do_not_divide_by_zero() {
        let xs = plot_xs(&[5, 5, 5], 0.0, 10.0);
        assert_eq!(xs, vec![0.0, 5.0, 10.0]);
    }

    #[test]
    fn nearest_point_by_x() {
        assert_eq!(nearest(10.0, &[]), None);
        assert_eq!(nearest(10.0, &[50.0]), Some(0));
        let px = [0.0, 10.0, 12.0, 100.0];
        assert_eq!(nearest(-50.0, &px), Some(0));
        assert_eq!(nearest(500.0, &px), Some(3));
        assert_eq!(nearest(11.5, &px), Some(2));
        assert_eq!(nearest(5.1, &px), Some(1));
    }

    fn rects(labels: &[DateLabel], width: f32) -> Vec<(f32, f32)> {
        labels
            .iter()
            .map(|l| match l.anchor {
                Anchor::Start => (l.x, l.x + width),
                Anchor::Middle => (l.x - width / 2.0, l.x + width / 2.0),
                Anchor::End => (l.x - width, l.x),
            })
            .collect()
    }

    fn assert_clear(labels: &[DateLabel], width: f32, left: f32, right: f32) {
        let r = rects(labels, width);
        for (a, b) in &r {
            assert!(*a >= left - 0.01 && *b <= right + 0.01, "clipped {a} {b}");
        }
        for w in r.windows(2) {
            assert!(w[1].0 - w[0].1 >= LABEL_GAP - 0.01, "overlap {w:?}");
        }
    }

    #[test]
    fn two_days_get_a_label_under_each() {
        let px = [34.0, 400.0];
        let l = date_labels(&px, |_| 50.0, 34.0, 400.0);
        assert_eq!(
            l.iter().map(|l| (l.index, l.anchor)).collect::<Vec<_>>(),
            vec![(0, Anchor::Start), (1, Anchor::End)]
        );
        assert_clear(&l, 50.0, 34.0, 400.0);
    }

    #[test]
    fn four_days_get_no_middle_label() {
        let px = [34.0, 150.0, 300.0, 400.0];
        let l = date_labels(&px, |_| 50.0, 34.0, 400.0);
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn five_or_more_days_get_a_middle_label_that_is_clear_of_both_ends() {
        let px = [34.0, 100.0, 220.0, 330.0, 400.0];
        let l = date_labels(&px, |_| 50.0, 34.0, 400.0);
        assert_eq!(l.len(), 3);
        assert_eq!((l[1].index, l[1].anchor), (2, Anchor::Middle));
        assert_clear(&l, 50.0, 34.0, 400.0);
    }

    #[test]
    fn a_middle_label_that_would_crowd_an_end_is_left_out() {
        let px = [34.0, 36.0, 38.0, 40.0, 400.0];
        let l = date_labels(&px, |_| 50.0, 34.0, 400.0);
        assert_eq!(l.len(), 2);
        assert_clear(&l, 50.0, 34.0, 400.0);
    }

    #[test]
    fn a_narrow_chart_keeps_only_the_last_label() {
        let px = [34.0, 150.0];
        let l = date_labels(&px, |_| 70.0, 34.0, 150.0);
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].index, l[0].anchor), (1, Anchor::End));
    }

    #[test]
    fn a_month_of_days_never_overlaps_for_any_width() {
        let keys: Vec<u64> = (0..30).collect();
        for right in [120.0, 200.0, 320.0, 560.0, 900.0] {
            let px = plot_xs(&keys, 34.0, right);
            let l = date_labels(&px, |_| 48.0, 34.0, right);
            assert!(!l.is_empty());
            assert_clear(&l, 48.0, 34.0, right);
        }
    }

    #[test]
    fn fewer_than_two_points_get_no_labels() {
        assert!(date_labels(&[], |_| 10.0, 0.0, 100.0).is_empty());
        assert!(date_labels(&[50.0], |_| 10.0, 0.0, 100.0).is_empty());
    }
}
