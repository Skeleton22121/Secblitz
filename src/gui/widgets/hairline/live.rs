//! Everything a drawing's canvas state needs to answer the pointer and keep
//! its own time, in one value.
use super::motion::{Pulses, MAX_DT};
use super::parallax::Parallax;
use super::pointer::{interaction, tooltip, Gesture, Hotspots, Layer, Pointer};
use super::stage::Stage;
use crate::gui::theme::Palette;
use crate::gui::widgets::anim;
use iced::widget::canvas::{Action, Event, Frame};
use iced::{mouse, window, Rectangle};
use std::time::Instant;

/// Age reported once a transition is over (and always under reduced motion,
/// so every transition shows its end at once).
pub const SETTLED_AGE: f32 = 99.0;

/// A gap between frames longer than this means the drawing was idle; the
/// next frame then counts as one normal frame instead of a jump.
const IDLE_GAP: f32 = 0.25;

/// What one event did, from [`Live::update`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Step {
    /// `Some(seconds)` when the event was a frame: step the drawing's own
    /// springs by it.
    pub dt: Option<f32>,
    /// The pointer gesture, if the event was one.
    pub gesture: Option<Gesture>,
    /// Something visible changed (the hovered part, a press): redraw.
    pub dirty: bool,
}

impl Step {
    /// The event was a click released at this unit point.
    pub fn click(&self) -> Option<iced::Point> {
        match self.gesture {
            Some(Gesture::Release { at, click: true }) => Some(at),
            _ => None,
        }
    }
}

/// Pointer, tilt, hovered part, click pulses and the frame clock for one
/// drawing. Keep it in the canvas `State` and feed it every event.
#[derive(Debug, Clone, PartialEq)]
pub struct Live<Id> {
    pub pointer: Pointer,
    pub tilt: Parallax,
    /// The part under the pointer.
    pub hover: Option<Id>,
    pub pulses: Pulses,
    /// Time of the last frame this drawing saw.
    pub now: Option<Instant>,
    /// Time of its first frame: the start of its ambient clock.
    pub born: Option<Instant>,
    /// The last state change [`Live::fresh`] saw.
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
    /// Without pointer tilt (set once, for example from the first
    /// `update`, or use `Live { tilt: Parallax::off(), ..Live::default() }`
    /// in the state's `Default`).
    pub fn without_tilt() -> Self {
        Live {
            tilt: Parallax::off(),
            ..Live::default()
        }
    }

    /// Feed one canvas event. On a frame it advances the clock, the tilt
    /// and the pulses and returns `dt`; on pointer events it tracks the
    /// pointer. Either way it updates [`Live::hover`] from `spots`.
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

    /// True once for each new state: the first time it is called with a
    /// `changed` instant it has not seen (the prototype's `f.fresh`). Call
    /// it at the top of `update` to reset per-state things (a burst that
    /// fires once, items ticked during the last run).
    pub fn fresh(&mut self, changed: Instant) -> bool {
        if self.seen_change == Some(changed) {
            return false;
        }
        self.seen_change = Some(changed);
        true
    }

    /// The action to return from `Program::update`: ask for the next frame
    /// while something moves (`busy` is the drawing's own answer: a loop
    /// running, a transition not finished, one of its springs moving) or
    /// when the event changed what is shown. `None` once everything has
    /// settled, so a still drawing costs nothing.
    pub fn redraw<M>(&self, step: &Step, busy: bool) -> Option<Action<M>> {
        (step.dirty || busy || self.moving()).then(Action::request_redraw)
    }

    /// The tilt or a pulse is still moving.
    pub fn moving(&self) -> bool {
        self.tilt.moving() || self.pulses.alive()
    }

    /// The latest time known: this drawing's last frame or the page's `now`.
    pub fn clock(&self, page_now: Instant) -> Instant {
        match self.now {
            Some(n) if n > page_now => n,
            _ => page_now,
        }
    }

    /// Seconds since the state changed at `changed`. [`SETTLED_AGE`] under
    /// reduced motion, so transitions jump to their end.
    pub fn age(&self, changed: Instant, page_now: Instant) -> f32 {
        if anim::reduced() {
            return SETTLED_AGE;
        }
        self.clock(page_now)
            .saturating_duration_since(changed)
            .as_secs_f32()
    }

    /// Ambient seconds for loops (the prototype's `f.t`): time since this
    /// drawing first drew. Under reduced motion the drawing's chosen
    /// `still` frame instead.
    pub fn ambient(&self, page_now: Instant, still: f32) -> f32 {
        if anim::reduced() {
            return still;
        }
        self.born
            .map(|b| self.clock(page_now).saturating_duration_since(b).as_secs_f32())
            .unwrap_or(0.0)
    }

    /// `stage` moved by `layer`'s parallax offset: draw that layer's parts
    /// through it.
    pub fn layer(&self, stage: &Stage, layer: Layer) -> Stage {
        stage.shifted(self.tilt.offset(layer))
    }

    /// The hovered part's name above it. `label` maps a part to its
    /// (translated) name; draw this last.
    pub fn draw_tooltip(
        &self,
        frame: &mut Frame,
        p: &Palette,
        stage: &Stage,
        spots: &Hotspots<Id>,
        label: impl Fn(Id) -> String,
    ) {
        let Some(id) = self.hover else { return };
        if let Some(anchor) = spots.anchor(id, &self.tilt) {
            tooltip(frame, p, stage, anchor, &label(id));
        }
    }

    /// The mouse cursor to return from `Program::mouse_interaction`.
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
        // The clock is the later of the page's time and the last frame.
        assert_eq!(live.clock(t0), t0);
        live.now = Some(t1);
        assert_eq!(live.clock(t0), t1);
        let t2 = t1 + Duration::from_secs(1);
        assert_eq!(live.clock(t2), t2);
    }
}
