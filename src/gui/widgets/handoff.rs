//! The beat between "still working" and "here is the result": the progress
//! fills, the picture settles, the screen holds for a moment, then it gives
//! way to the result.
use super::anim;
use std::time::Duration;

/// The bar runs to the end and the last items tick off.
pub const FILL: Duration = Duration::from_millis(250);
/// The finished picture stays put so the person sees it is done.
pub const HOLD: Duration = Duration::from_millis(300);
/// The checking screen fades out, then the result eases in (see `appear::ENTER`).
pub const LEAVE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    /// Filling and holding: the working screen stays as it is, finished.
    Finishing,
    /// Fading out. `0..1`.
    Leaving(f32),
    /// Show the result.
    Done,
}

/// When the working screen starts to fade out.
pub fn leave_at() -> Duration {
    if anim::reduced() {
        Duration::ZERO
    } else {
        FILL + HOLD
    }
}

pub fn stage(elapsed: Duration) -> Stage {
    if anim::reduced() {
        return Stage::Done;
    }
    let at = FILL + HOLD;
    if elapsed < at {
        Stage::Finishing
    } else if elapsed < at + LEAVE {
        Stage::Leaving((elapsed - at).as_secs_f32() / LEAVE.as_secs_f32())
    } else {
        Stage::Done
    }
}

/// How long a progress sheet holds its finished look before the result takes
/// its place.
pub fn sheet_hold() -> Duration {
    if anim::reduced() {
        Duration::ZERO
    } else {
        FILL + HOLD / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::widgets::anim::forced;
    use crate::gui::widgets::appear;

    #[test]
    fn stages_follow_each_other() {
        let _m = forced::set(false);
        assert_eq!(stage(Duration::ZERO), Stage::Finishing);
        assert_eq!(stage(FILL + HOLD - Duration::from_millis(1)), Stage::Finishing);
        assert_eq!(stage(FILL + HOLD), Stage::Leaving(0.0));
        match stage(FILL + HOLD + LEAVE / 2) {
            Stage::Leaving(t) => assert!((t - 0.5).abs() < 1e-3),
            other => panic!("{other:?}"),
        }
        assert_eq!(stage(FILL + HOLD + LEAVE), Stage::Done);
        assert_eq!(stage(Duration::from_secs(60)), Stage::Done);
    }

    #[test]
    fn leaving_never_goes_backwards() {
        let _m = forced::set(false);
        let mut last = 0.0;
        for ms in 0..2000u64 {
            if let Stage::Leaving(t) = stage(Duration::from_millis(ms)) {
                assert!(t >= last && (0.0..1.0).contains(&t));
                last = t;
            }
        }
        assert!(last > 0.9);
    }

    #[test]
    fn the_whole_hand_off_is_short() {
        let _m = forced::set(false);
        assert!(FILL + HOLD + LEAVE + appear::ENTER < Duration::from_millis(900));
        assert!(HOLD >= Duration::from_millis(250) && HOLD <= Duration::from_millis(350));
        assert_eq!(leave_at(), FILL + HOLD);
    }

    #[test]
    fn reduced_motion_skips_it_all() {
        let _m = forced::set(true);
        assert_eq!(stage(Duration::ZERO), Stage::Done);
        assert_eq!(leave_at(), Duration::ZERO);
        assert_eq!(sheet_hold(), Duration::ZERO);
    }

    #[test]
    fn sheets_hold_less_than_the_screen() {
        let _m = forced::set(false);
        assert!(sheet_hold() < FILL + HOLD);
        assert!(sheet_hold() > FILL);
    }
}
