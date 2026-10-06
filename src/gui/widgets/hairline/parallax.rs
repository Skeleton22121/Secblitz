//! A gentle tilt that follows the pointer: three layers shift by a few units
//! so the drawing feels like it has depth.
use super::motion::Spring;
use super::pointer::{Layer, Pointer};
use crate::gui::widgets::anim;
use iced::{Size, Vector};

pub const DEPTHS: [f32; 3] = [1.5, 3.0, 5.0];
const Y_SHARE: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parallax {
    pub x: Spring,
    pub y: Spring,
    pub depths: [f32; 3],
    pub enabled: bool,
}

impl Default for Parallax {
    fn default() -> Self {
        Parallax::new()
    }
}

impl Parallax {
    pub fn new() -> Parallax {
        Parallax::with_depths(DEPTHS)
    }

    pub fn with_depths(depths: [f32; 3]) -> Parallax {
        Parallax {
            x: Spring::with(0.0, 90.0, 15.0),
            y: Spring::with(0.0, 90.0, 15.0),
            depths,
            enabled: true,
        }
    }

    pub fn off() -> Parallax {
        Parallax {
            enabled: false,
            ..Parallax::new()
        }
    }

    pub fn aim(&mut self, pointer: &Pointer, units: Size) {
        let follow = self.enabled && pointer.inside && !anim::reduced();
        let (tx, ty) = if follow && units.width > 0.0 && units.height > 0.0 {
            (
                ((pointer.at.x / units.width - 0.5) * 2.0).clamp(-1.0, 1.0),
                ((pointer.at.y / units.height - 0.5) * 2.0).clamp(-1.0, 1.0),
            )
        } else {
            (0.0, 0.0)
        };
        self.x.target = tx;
        self.y.target = ty;
    }

    pub fn step(&mut self, dt: f32) -> bool {
        if !self.enabled || anim::reduced() {
            self.x.target = 0.0;
            self.y.target = 0.0;
            self.x.settle();
            self.y.settle();
            return false;
        }
        let a = self.x.step(dt);
        let b = self.y.step(dt);
        a || b
    }

    pub fn moving(&self) -> bool {
        self.x.moving() || self.y.moving()
    }

    pub fn offset(&self, layer: Layer) -> Vector {
        if !self.enabled || anim::reduced() {
            return Vector::ZERO;
        }
        let (tx, ty) = (self.x.value, self.y.value);
        let [d0, d1, d2] = self.depths;
        match layer {
            Layer::Back => Vector::new(-tx * d0, -ty * d0 * Y_SHARE),
            Layer::Mid => Vector::new(tx * d1, ty * d1 * Y_SHARE),
            Layer::Front => Vector::new(tx * d2, ty * d2 * Y_SHARE),
            Layer::Fixed => Vector::ZERO,
        }
    }

    #[cfg(test)]
    pub fn layers(&self) -> [Vector; 3] {
        [
            self.offset(Layer::Back),
            self.offset(Layer::Mid),
            self.offset(Layer::Front),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    #[test]
    fn follows_the_pointer_and_returns_home() {
        let _m = anim::forced::set(false);
        let box_ = Size::new(320.0, 256.0);
        let mut t = Parallax::new();
        let ptr = Pointer {
            at: Point::new(320.0, 0.0),
            inside: true,
            ..Pointer::default()
        };
        t.aim(&ptr, box_);
        assert_eq!((t.x.target, t.y.target), (1.0, -1.0));
        for _ in 0..240 {
            t.step(1.0 / 60.0);
        }
        assert!(!t.moving());
        let near = |a: Vector, x: f32, y: f32| (a.x - x).abs() < 1e-5 && (a.y - y).abs() < 1e-5;
        let [back, mid, front] = t.layers();
        assert!(near(front, 5.0, -3.5), "{front:?}");
        assert!(near(mid, 3.0, -2.1), "{mid:?}");
        assert!(near(back, -1.5, 1.05), "{back:?}");
        assert_eq!(t.offset(Layer::Fixed), Vector::ZERO);
        t.aim(&Pointer::default(), box_);
        assert!(t.step(1.0 / 60.0));
        for _ in 0..240 {
            t.step(1.0 / 60.0);
        }
        assert_eq!(t.layers(), [Vector::ZERO; 3]);
    }

    #[test]
    fn still_under_reduced_motion_or_when_off() {
        let ptr = Pointer {
            at: Point::new(0.0, 0.0),
            inside: true,
            ..Pointer::default()
        };
        let mut off = Parallax::off();
        off.aim(&ptr, Size::new(100.0, 100.0));
        assert!(!off.step(0.016));
        assert_eq!(off.layers(), [Vector::ZERO; 3]);
        let _m = anim::forced::set(true);
        let mut t = Parallax::new();
        t.x.value = 0.8;
        assert_eq!(t.offset(Layer::Front), Vector::ZERO);
        t.aim(&ptr, Size::new(100.0, 100.0));
        assert!(!t.step(0.016));
        assert_eq!(t.x.value, 0.0);
    }
}
