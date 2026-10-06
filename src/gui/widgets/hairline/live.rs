//! Per-drawing canvas state: pointer answers and its own clock.
use super::motion::{Pulses, MAX_DT};
use super::parallax::Parallax;
use super::pointer::{interaction, tooltip_around, Gesture, Hotspots, Layer, Pointer};
use super::stage::Stage;
use crate::gui::theme::Palette;
use crate::gui::widgets::anim;
use iced::widget::canvas::{Action, Event, Frame};
use iced::{mouse, window, Color, Rectangle};
use std::time::Instant;

pub const SETTLED_AGE: f32 = 99.0;

const IDLE_GAP: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Step {
    pub dt: Option<f32>,
    pub gesture: Option<Gesture>,
    pub dirty: bool,
}

impl Step {
    pub fn click(&self) -> Option<iced::Point> {
        match self.gesture {
            Some(Gesture::Release { at, click: true }) => Some(at),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Live<Id> {
    pub pointer: Pointer,
    pub tilt: Parallax,
    pub hover: Option<Id>,
    pub pulses: Pulses,
    pub now: Option<Instant>,
    pub born: Option<Instant>,
    pub seen_change: Option<Instant>,
}

impl<Id> Default for Live<Id> {
    fn default() -> Self {
        Live {
            pointer: Pointer::default(),
            tilt: Parallax::new(),
            hover: None,
            pulses: Pulses::default(),
            now: None,
            born: None,
            seen_change: None,
        }
    }
}

impl<Id: Copy + PartialEq> Live<Id> {
    pub fn without_tilt() -> Self {
        Live {
            tilt: Parallax::off(),
            ..Live::default()
        }
    }

    pub fn update(
        &mut self,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        stage: &Stage,
        spots: &Hotspots<Id>,
    ) -> Step {
        let mut step = Step::default();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let gap = self
                .now
                .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
                .unwrap_or(f32::INFINITY);
            let dt = if gap > IDLE_GAP {
                1.0 / 60.0
            } else {
                gap.min(MAX_DT)
            };
            self.now = Some(*now);
            self.born.get_or_insert(*now);
            self.tilt.aim(&self.pointer, stage.units);
            self.tilt.step(dt);
            self.pulses.step(dt);
            step.dt = Some(dt);
        } else if let Some(g) = self.pointer.handle(event, bounds, cursor, stage) {
            self.tilt.aim(&self.pointer, stage.units);
            step.gesture = Some(g);
            step.dirty = !matches!(g, Gesture::Hover);
        }
        let hover = if self.pointer.inside {
            spots.hit(self.pointer.at, &self.tilt)
        } else {
            None
        };
        if hover != self.hover {
            self.hover = hover;
            step.dirty = true;
        }
        step
    }

    pub fn fresh(&mut self, changed: Instant) -> bool {
        if self.seen_change == Some(changed) {
            return false;
        }
        self.seen_change = Some(changed);
        true
    }

    pub fn redraw<M>(&self, step: &Step, busy: bool) -> Option<Action<M>> {
        (step.dirty || busy || self.moving()).then(Action::request_redraw)
    }

    pub fn moving(&self) -> bool {
        self.tilt.moving() || self.pulses.alive()
    }

    pub fn clock(&self, page_now: Instant) -> Instant {
        match self.now {
            Some(n) if n > page_now => n,
            _ => page_now,
        }
    }

    pub fn age(&self, changed: Instant, page_now: Instant) -> f32 {
        if anim::reduced() {
            return SETTLED_AGE;
        }
        self.clock(page_now)
            .saturating_duration_since(changed)
            .as_secs_f32()
    }

    pub fn ambient(&self, page_now: Instant, still: f32) -> f32 {
        if anim::reduced() {
            return still;
        }
        self.born
            .map(|b| self.clock(page_now).saturating_duration_since(b).as_secs_f32())
            .unwrap_or(0.0)
    }

    pub fn layer(&self, stage: &Stage, layer: Layer) -> Stage {
        stage.shifted(self.tilt.offset(layer))
    }

    pub fn draw_tooltip(
        &self,
        frame: &mut Frame,
        p: &Palette,
        stage: &Stage,
        spots: &Hotspots<Id>,
        label: impl Fn(Id) -> String,
    ) {
        let Some(id) = self.hover else { return };
        let above = spots.anchor(id, &self.tilt);
        let below = spots.below(id, &self.tilt);
        if let (Some(above), Some(below)) = (above, below) {
            tooltip_around(frame, p, stage, above, below, &label(id));
        }
    }

    /// The shared top layer of every drawing: click ripples, then the hover tooltip.
    pub fn draw_overlay(
        &self,
        frame: &mut Frame,
        p: &Palette,
        stage: &Stage,
        pulse: Color,
        spots: &Hotspots<Id>,
        label: impl Fn(Id) -> String,
    ) {
        self.pulses.draw(frame, stage, pulse);
        self.draw_tooltip(frame, p, stage, spots, label);
    }

    pub fn interaction(
        &self,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        draggable: bool,
    ) -> mouse::Interaction {
        interaction(
            &self.pointer,
            self.hover.is_some(),
            draggable,
            bounds,
            cursor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fresh_once_per_state_and_clock_is_latest() {
        let mut live: Live<u8> = Live::default();
        let t0 = Instant::now();
        assert!(live.fresh(t0));
        assert!(!live.fresh(t0));
        let t1 = t0 + Duration::from_secs(1);
        assert!(live.fresh(t1));
        assert!(!live.fresh(t1));
        assert_eq!(live.clock(t0), t0);
        live.now = Some(t1);
        assert_eq!(live.clock(t0), t1);
        let t2 = t1 + Duration::from_secs(1);
        assert_eq!(live.clock(t2), t2);
    }
}
