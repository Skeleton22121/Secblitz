//! Count chart: one calm bar per day, always measured from zero.
use super::anim::{self, DECELERATE};
use crate::gui::theme::{self, Palette, Tone};
use crate::gui::Message;
use iced::widget::canvas::{self, Cache, Frame, Geometry, Path, Text};
use iced::{mouse, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const ENTRANCE: Duration = Duration::from_millis(500);
const GUTTER: f32 = 44.0;
const PAD_R: f32 = 4.0;
const PAD_TOP: f32 = 12.0;
const LABEL_H: f32 = 24.0;
const MIN_BAR: f32 = 2.0;
const CHAR_W: f32 = 6.3;
const LABEL_GAP: f32 = 24.0;

/// Smallest round number that holds `max`, never below 4.
pub fn round_max(max: u64) -> u64 {
    const SMALL: [u64; 5] = [4, 5, 6, 8, 10];
    const STEPS: [u64; 11] = [10, 12, 15, 20, 25, 30, 40, 50, 60, 80, 100];
    if max <= 10 {
        return SMALL.into_iter().find(|n| *n >= max).unwrap_or(10);
    }
    let mut magnitude: u64 = 10;
    while let Some(next) = magnitude.checked_mul(10).filter(|n| *n <= max) {
        magnitude = next;
    }
    for step in STEPS {
        let candidate = step.saturating_mul(magnitude) / 10;
        if candidate >= max {
            return candidate;
        }
    }
    10 * magnitude
}

pub fn bar_span(i: usize, n: usize, left: f32, right: f32) -> (f32, f32) {
    let slot = (right - left).max(1.0) / n.max(1) as f32;
    let width = (slot * 0.66).max(1.0);
    (left + slot * i as f32 + (slot - width) / 2.0, width)
}

pub fn slot_at(x: f32, left: f32, right: f32, n: usize) -> Option<usize> {
    if n == 0 || right <= left || x < left || x > right {
        return None;
    }
    let slot = (right - left) / n as f32;
    Some((((x - left) / slot) as usize).min(n - 1))
}

pub fn label_slots(n: usize, span: f32, label_width: f32) -> Vec<usize> {
    match n {
        0 => vec![],
        1 => vec![0],
        _ => {
            let mut out = vec![0];
            if n >= 5 && span >= 4.0 * label_width + 2.0 * LABEL_GAP {
                out.push(n / 2);
            }
            out.push(n - 1);
            out
        }
    }
}

pub fn bar_height(value: u64, top: u64, plot_height: f32) -> f32 {
    if value == 0 || top == 0 || plot_height <= 0.0 {
        return 0.0;
    }
    (plot_height * (value as f32 / top as f32).min(1.0)).max(MIN_BAR)
}

struct Bars<'a> {
    p: Palette,
    tone: Tone,
    days: &'a [(u64, u64)],
    cache: &'a Cache,
    date: &'a dyn Fn(u64) -> String,
    count: &'a dyn Fn(u64) -> String,
}

#[derive(Default)]
pub struct BarsState {
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

impl Bars<'_> {
    fn key(&self, size: Size) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (day, n) in self.days {
            day.hash(&mut h);
            n.hash(&mut h);
        }
        (self.p.mode as u8).hash(&mut h);
        (self.tone as u8).hash(&mut h);
        size.width.to_bits().hash(&mut h);
        h.finish() | 1
    }

    fn top(&self) -> u64 {
        round_max(self.days.iter().map(|(_, n)| *n).max().unwrap_or(0))
    }

    fn paint_static(&self, f: &mut Frame, reveal: f32) {
        let p = &self.p;
        let size = f.size();
        let plot = Plot::of(size);
        let tone = p.tone(self.tone);
        let top = self.top();
        let guide = p.text.scale_alpha(if p.mode == theme::Mode::Dark {
            0.09
        } else {
            0.07
        });
        let mid = (plot.top + plot.bottom) / 2.0;
        for y in [plot.top, mid, plot.bottom] {
            f.stroke(
                &Path::line(Point::new(plot.left, y), Point::new(plot.right, y)),
                canvas::Stroke::default().with_width(1.0).with_color(guide),
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
        f.fill_text(small(
            (self.count)(top),
            plot.left - 8.0,
            plot.top,
            Right,
            0.8,
        ));
        f.fill_text(small(
            (self.count)(0),
            plot.left - 8.0,
            plot.bottom,
            Right,
            0.8,
        ));
        let n = self.days.len();
        if n == 0 {
            return;
        }
        let widest = self
            .days
            .iter()
            .map(|(d, _)| (self.date)(*d).chars().count())
            .max()
            .unwrap_or(0) as f32
            * CHAR_W;
        let ly = size.height - LABEL_H / 2.0 + 2.0;
        let slots = label_slots(n, plot.right - plot.left, widest);
        for (k, i) in slots.iter().enumerate() {
            let (x, w) = bar_span(*i, n, plot.left, plot.right);
            let (x, align) = if k == 0 {
                (plot.left, Left)
            } else if k == slots.len() - 1 {
                (plot.right, Right)
            } else {
                (x + w / 2.0, Center)
            };
            f.fill_text(small((self.date)(self.days[*i].0), x, ly, align, 1.0));
        }
        let height = plot.bottom - plot.top;
        for (i, (_, value)) in self.days.iter().enumerate() {
            let (x, w) = bar_span(i, n, plot.left, plot.right);
            if *value == 0 {
                f.fill_rectangle(
                    Point::new(x, plot.bottom - 1.0),
                    Size::new(w, 1.0),
                    tone.scale_alpha(0.35),
                );
                continue;
            }
            let h = bar_height(*value, top, height) * reveal;
            if h <= 0.0 {
                continue;
            }
            let radius = if h >= 4.0 { 2.0 } else { 1.0 };
            f.fill(
                &Path::rounded_rectangle(
                    Point::new(x, plot.bottom - h),
                    Size::new(w, h),
                    radius.into(),
                ),
                tone.scale_alpha(0.8),
            );
        }
    }

    fn paint_hover(&self, f: &mut Frame, i: usize) {
        let n = self.days.len();
        if i >= n {
            return;
        }
        let p = &self.p;
        let size = f.size();
        let plot = Plot::of(size);
        let tone = p.tone(self.tone);
        let (day, value) = self.days[i];
        let (x, w) = bar_span(i, n, plot.left, plot.right);
        let h = bar_height(value, self.top(), plot.bottom - plot.top);
        let center = x + w / 2.0;
        if h > 0.0 {
            f.fill(
                &Path::rounded_rectangle(
                    Point::new(x, plot.bottom - h),
                    Size::new(w, h),
                    2.0.into(),
                ),
                tone,
            );
        }
        let label = format!("{}  ·  {}", (self.date)(day), (self.count)(value));
        let tw = label.chars().count() as f32 * 6.8 + 20.0;
        let th = 26.0;
        let tx = (center - tw / 2.0).clamp(2.0, (size.width - tw - 2.0).max(2.0));
        let above = plot.bottom - h - th - 8.0;
        let ty = if above < 0.0 { 0.0 } else { above };
        f.fill(
            &Path::rounded_rectangle(Point::new(tx, ty), Size::new(tw, th), 6.0.into()),
            p.text,
        );
        f.fill_text(Text {
            content: label,
            position: Point::new(tx + tw / 2.0, ty + th / 2.0),
            color: p.bg,
            size: theme::SMALL.into(),
            font: theme::MEDIUM,
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
    }
}

impl canvas::Program<Message> for Bars<'_> {
    type State = BarsState;

    fn update(
        &self,
        st: &mut BarsState,
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
                    .and_then(|pt| slot_at(pt.x, plot.left, plot.right, self.days.len()));
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
        st: &BarsState,
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
        _: &BarsState,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

pub fn daily<'a>(
    p: Palette,
    tone: Tone,
    days: &'a [(u64, u64)],
    cache: &'a Cache,
    date: &'a dyn Fn(u64) -> String,
    count: &'a dyn Fn(u64) -> String,
    height: f32,
) -> Element<'a, Message> {
    canvas::Canvas::new(Bars {
        p,
        tone,
        days,
        cache,
        date,
        count,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_ends_on_a_round_number_that_holds_the_biggest_day() {
        assert_eq!(round_max(0), 4);
        assert_eq!(round_max(3), 4);
        assert_eq!(round_max(5), 5);
        assert_eq!(round_max(7), 8);
        assert_eq!(round_max(10), 10);
        assert_eq!(round_max(11), 12);
        assert_eq!(round_max(73), 80);
        assert_eq!(round_max(100), 100);
        assert_eq!(round_max(101), 120);
        assert_eq!(round_max(1_874), 2_000);
        assert_eq!(round_max(1_204), 1_500);
        assert_eq!(round_max(9_999), 10_000);
        for max in (0..5_000).chain([1_000_000, 123_456_789, u64::MAX / 100]) {
            let top = round_max(max);
            assert!(top >= max, "{max} -> {top}");
            if max >= 10 {
                assert!(top as f64 <= max as f64 * 1.34 + 1.0, "{max} -> {top}");
            }
        }
    }

    #[test]
    fn bars_sit_inside_their_slots_without_touching() {
        let (left, right, n) = (40.0, 340.0, 30);
        let mut end = left;
        for i in 0..n {
            let (x, w) = bar_span(i, n, left, right);
            assert!(x > end - 0.001, "bar {i} overlaps the one before");
            assert!(w >= 1.0);
            end = x + w;
        }
        assert!(end <= right);
        let (_, narrow) = bar_span(0, 30, 0.0, 10.0);
        assert!(narrow >= 1.0);
    }

    #[test]
    fn the_pointer_finds_its_day() {
        assert_eq!(slot_at(10.0, 0.0, 100.0, 0), None);
        assert_eq!(slot_at(-1.0, 0.0, 100.0, 10), None);
        assert_eq!(slot_at(101.0, 0.0, 100.0, 10), None);
        assert_eq!(slot_at(0.0, 0.0, 100.0, 10), Some(0));
        assert_eq!(slot_at(15.0, 0.0, 100.0, 10), Some(1));
        assert_eq!(slot_at(100.0, 0.0, 100.0, 10), Some(9));
        assert_eq!(slot_at(5.0, 100.0, 100.0, 10), None);
    }

    #[test]
    fn date_labels_never_crowd_each_other() {
        assert!(label_slots(0, 300.0, 40.0).is_empty());
        assert_eq!(label_slots(1, 300.0, 40.0), vec![0]);
        assert_eq!(label_slots(2, 300.0, 40.0), vec![0, 1]);
        assert_eq!(label_slots(4, 600.0, 40.0), vec![0, 3]);
        assert_eq!(label_slots(30, 300.0, 40.0), vec![0, 15, 29]);
        assert_eq!(label_slots(30, 150.0, 40.0), vec![0, 29]);
        assert_eq!(label_slots(30, 300.0, 70.0), vec![0, 29]);
    }

    #[test]
    fn a_day_with_blocks_is_always_visible_and_never_taller_than_the_plot() {
        assert_eq!(bar_height(0, 100, 120.0), 0.0);
        assert_eq!(bar_height(5, 0, 120.0), 0.0);
        assert_eq!(bar_height(1, 100_000, 120.0), MIN_BAR);
        assert_eq!(bar_height(50, 100, 120.0), 60.0);
        assert_eq!(bar_height(100, 100, 120.0), 120.0);
        assert_eq!(bar_height(500, 100, 120.0), 120.0);
    }
}
