//! Springs, eased phases and the click pulse.
use super::stage::{stroke, Stage, W_ACCENT};
use crate::gui::widgets::anim::{self, Curve, DECELERATE};
use iced::widget::canvas::Frame;
use iced::{Color, Point};

pub const MAX_DT: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Spring::new(0.0)
    }
}

impl Spring {
    pub const fn new(value: f32) -> Spring {
        Spring::with(value, 140.0, 18.0)
    }

    pub const fn with(value: f32, stiffness: f32, damping: f32) -> Spring {
        Spring {
            stiffness,
            damping,
            value,
            velocity: 0.0,
            target: value,
        }
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, MAX_DT);
        if dt > 0.0 {
            let n = (dt / 0.008).ceil().max(1.0) as usize;
            let h = dt / n as f32;
            for _ in 0..n {
                let a = -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
                self.velocity += a * h;
                self.value += self.velocity * h;
            }
        }
        if (self.value - self.target).abs() < 1e-3 && self.velocity.abs() < 1e-3 {
            self.settle();
        }
        self.moving()
    }

    pub fn settle(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    pub fn aim(&mut self, target: f32) {
        self.target = target;
        if anim::reduced() {
            self.settle();
        }
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        if anim::reduced() {
            self.settle();
            false
        } else {
            self.step(dt)
        }
    }

    pub fn moving(&self) -> bool {
        self.value != self.target || self.velocity != 0.0
    }

    pub fn kick(&mut self, velocity: f32) {
        if !anim::reduced() {
            self.velocity += velocity;
        }
    }
}

pub fn phase(age: f32, a: f32, b: f32, curve: Curve) -> f32 {
    if b <= a {
        return if age >= b { 1.0 } else { 0.0 };
    }
    curve.at((age - a) / (b - a))
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub const PULSE_LIFE: f32 = 0.8;
pub const PULSE_SMALL: f32 = 46.0;
pub const PULSE_BIG: f32 = 140.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pulse {
    at: Point,
    age: f32,
    big: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pulses {
    list: Vec<Pulse>,
}

impl Pulses {
    pub fn push(&mut self, at: Point, big: bool) {
        if !anim::reduced() && self.list.len() < 8 {
            self.list.push(Pulse { at, age: 0.0, big });
        }
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, MAX_DT);
        for p in &mut self.list {
            p.age += dt;
        }
        self.list.retain(|p| p.age < PULSE_LIFE);
        !self.list.is_empty()
    }

    pub fn alive(&self) -> bool {
        !self.list.is_empty()
    }

    pub fn draw(&self, frame: &mut Frame, stage: &Stage, color: Color) {
        for p in &self.list {
            let e = DECELERATE.at(p.age / PULSE_LIFE);
            let r = if p.big { PULSE_BIG } else { PULSE_SMALL } * e;
            if r > 0.1 && e < 1.0 {
                frame.stroke(
                    &stage.circle(p.at, r),
                    stroke(color.scale_alpha(1.0 - e), W_ACCENT),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::widgets::anim::{LINEAR, STANDARD};

    #[test]
    fn spring_arrives_and_rests() {
        let mut s = Spring::new(0.0);
        s.target = 1.0;
        let mut frames = 0;
        while s.step(1.0 / 60.0) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        assert_eq!(s.value, 1.0);
        assert_eq!(s.velocity, 0.0);
        assert!(!s.moving());
        let mut t = Spring::with(0.0, 200.0, 13.0);
        t.target = 10.0;
        t.step(5.0);
        assert!(t.value.is_finite() && t.value > 0.0 && t.value < 20.0);
    }

    #[test]
    fn soft_spring_overshoots_stiff_one_less() {
        let peak = |k: f32, c: f32| {
            let mut s = Spring::with(0.0, k, c);
            s.target = 1.0;
            let mut max: f32 = 0.0;
            for _ in 0..240 {
                s.step(1.0 / 60.0);
                max = max.max(s.value);
            }
            max
        };
        assert!(peak(200.0, 13.0) > 1.01);
        assert!(peak(90.0, 20.0) <= 1.0 + 1e-3);
    }

    #[test]
    fn spring_jumps_under_reduced_motion() {
        let _m = anim::forced::set(true);
        let mut s = Spring::new(0.0);
        s.aim(3.0);
        assert_eq!(s.value, 3.0);
        s.target = 5.0;
        assert!(!s.tick(0.016));
        assert_eq!(s.value, 5.0);
        let mut p = Pulses::default();
        p.push(Point::ORIGIN, true);
        assert!(!p.alive());
    }

    #[test]
    fn phase_runs_between_its_times() {
        assert_eq!(phase(0.1, 0.25, 0.85, STANDARD), 0.0);
        assert_eq!(phase(0.9, 0.25, 0.85, STANDARD), 1.0);
        assert!((phase(0.55, 0.25, 0.85, LINEAR) - 0.5).abs() < 1e-3);
        assert_eq!(phase(1.0, 1.0, 1.0, LINEAR), 1.0);
        assert_eq!(lerp(2.0, 4.0, 0.5), 3.0);
    }

    #[test]
    fn pulses_fade_out_on_time() {
        let _m = anim::forced::set(false);
        let mut p = Pulses::default();
        p.push(Point::new(10.0, 10.0), false);
        assert!(p.alive());
        let mut t = 0.0;
        while p.step(1.0 / 60.0) {
            t += 1.0 / 60.0;
        }
        assert!((t - PULSE_LIFE).abs() < 0.05, "{t}");
    }
}
