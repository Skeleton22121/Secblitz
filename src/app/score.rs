//! Protection score: protected checks over all checks (findings excluded).
//! OWNER: app-core agent.
use crate::advice::{self, Group};
use secblitz::engine::Report;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    pub protected: usize,
    pub total: usize,
    /// Results that need the user's attention (fixable or not).
    pub attention: usize,
    /// Results that could not be checked.
    pub unknown: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Everything protected.
    Protected,
    /// Some things need attention.
    Attention,
    /// The check failed or is incomplete.
    Unknown,
}

impl Score {
    pub fn of(report: &Report) -> Self {
        let mut s = Score::default();
        for r in &report.results {
            s.total += 1;
            match advice::for_outcome(r).group {
                Group::Protected => s.protected += 1,
                Group::Recommended => s.attention += 1,
                _ => {
                    if matches!(r.status.as_str(), "unknown" | "error" | "pending") {
                        s.unknown += 1
                    } else {
                        s.attention += 1
                    }
                }
            }
        }
        s
    }
    pub fn verdict(&self) -> Verdict {
        if self.total == 0 {
            Verdict::Unknown
        } else if self.protected == self.total {
            Verdict::Protected
        } else if self.attention > 0 {
            Verdict::Attention
        } else {
            Verdict::Unknown
        }
    }
    /// 0.0..=1.0 for the ring.
    pub fn ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.protected as f32 / self.total as f32
        }
    }
}
