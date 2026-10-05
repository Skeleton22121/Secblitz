//! Portable tray logic: which icon/tooltip a status maps to, when to alert, and
//! the software renderer for the shield icons (so it is testable on any host).
use crate::i18n::Lang;
use secblitz::status::{State, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Protected,
    Attention,
    Problem,
    Unknown,
}

pub const ALL: [Icon; 4] = [Icon::Protected, Icon::Attention, Icon::Problem, Icon::Unknown];

/// More than half of the checks failing is shown as a problem (red).
pub fn icon_for(status: Option<&Status>) -> Icon {
    match status {
        None => Icon::Unknown,
        Some(s) => match s.state {
            State::Ok => Icon::Protected,
            State::Unknown => Icon::Unknown,
            State::Attention if (s.attention.len() as u32) * 2 > s.total => Icon::Problem,
            State::Attention => Icon::Attention,
        },
    }
}

/// Number of things shown to the user as needing attention.
pub fn attention_count(s: &Status) -> usize {
    s.attention.len().max(1)
}

pub fn tooltip(lang: Lang, status: Option<&Status>) -> String {
    let body = match status {
        Some(s) if s.state == State::Ok => lang.t("You're protected"),
        Some(s) if s.state == State::Attention => match attention_count(s) {
            1 => lang.t("1 thing needs your attention"),
            n => lang
                .t("{n} things need your attention")
                .replace("{n}", &n.to_string()),
        },
        _ => lang.t("Not checked yet"),
    };
    let text = format!("Secblitz \u{2014} {body}");
    // NOTIFYICONDATA tooltips hold 127 UTF-16 units.
    let mut out = String::new();
    let mut units = 0;
    for c in text.chars() {
        units += c.len_utf16();
        if units > 127 {
            break;
        }
        out.push(c);
    }
    out
}

/// True when protection got worse since the last status we saw: fewer protected
/// checks, or an id needing attention that was not before.
pub fn worsened(previous: &Status, now: &Status) -> bool {
    now.protected < previous.protected
        || now
            .attention
            .iter()
            .any(|id| !previous.attention.contains(id))
}

fn colour(icon: Icon) -> [f32; 3] {
    match icon {
        Icon::Protected => [22.0, 163.0, 74.0],
        Icon::Attention => [245.0, 158.0, 11.0],
        Icon::Problem => [220.0, 38.0, 38.0],
        Icon::Unknown => [113.0, 113.0, 122.0],
    }
}

fn in_shield(x: f32, y: f32) -> bool {
    let (top, bottom, mid, half) = (0.06, 0.96, 0.5, 0.40);
    if !(top..=bottom).contains(&y) {
        return false;
    }
    let w = if y <= mid {
        // Slightly rounded shoulders.
        let r = ((mid - y) / (mid - top)).powi(6);
        half * (1.0 - 0.18 * r)
    } else {
        let t = (y - mid) / (bottom - mid);
        half * (1.0 - t.powf(1.8))
    };
    (x - 0.5).abs() <= w
}

fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (px, py) = (a.0 + t * dx, a.1 + t * dy);
    ((x - px).powi(2) + (y - py).powi(2)).sqrt()
}

fn in_glyph(icon: Icon, x: f32, y: f32) -> bool {
    let r = 0.055;
    match icon {
        Icon::Protected => {
            segment(x, y, (0.32, 0.50), (0.45, 0.63)) <= r
                || segment(x, y, (0.45, 0.63), (0.69, 0.35)) <= r
        }
        Icon::Attention => {
            segment(x, y, (0.5, 0.27), (0.5, 0.54)) <= r + 0.01
                || (x - 0.5).powi(2) + (y - 0.69).powi(2) <= 0.065f32.powi(2)
        }
        Icon::Problem => {
            segment(x, y, (0.36, 0.34), (0.64, 0.62)) <= r
                || segment(x, y, (0.64, 0.34), (0.36, 0.62)) <= r
        }
        Icon::Unknown => segment(x, y, (0.35, 0.5), (0.65, 0.5)) <= r,
    }
}

/// Render a shield icon as `size`x`size` top-down BGRA with straight alpha.
pub fn render(icon: Icon, size: usize) -> Vec<u8> {
    const SS: usize = 4;
    let base = colour(icon);
    let mut out = vec![0u8; size * size * 4];
    for py in 0..size {
        for px in 0..size {
            let (mut covered, mut sum) = (0u32, [0f32; 3]);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if !in_shield(x, y) {
                        continue;
                    }
                    covered += 1;
                    let c = if in_glyph(icon, x, y) { [255.0; 3] } else { base };
                    for k in 0..3 {
                        sum[k] += c[k];
                    }
                }
            }
            if covered == 0 {
                continue;
            }
            let o = (py * size + px) * 4;
            out[o] = (sum[2] / covered as f32).round() as u8;
            out[o + 1] = (sum[1] / covered as f32).round() as u8;
            out[o + 2] = (sum[0] / covered as f32).round() as u8;
            out[o + 3] = (covered as f32 * 255.0 / (SS * SS) as f32).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(state: State, protected: u32, total: u32, ids: &[&str]) -> Status {
        Status {
            schema: secblitz::status::SCHEMA,
            t: 1,
            protected,
            total,
            attention: ids.iter().map(|s| s.to_string()).collect(),
            state,
        }
    }

    #[test]
    fn icon_and_tooltip_follow_status() {
        assert_eq!(icon_for(None), Icon::Unknown);
        assert_eq!(icon_for(Some(&st(State::Ok, 5, 5, &[]))), Icon::Protected);
        assert_eq!(icon_for(Some(&st(State::Unknown, 0, 0, &[]))), Icon::Unknown);
        assert_eq!(
            icon_for(Some(&st(State::Attention, 8, 10, &["a", "b"]))),
            Icon::Attention
        );
        assert_eq!(
            icon_for(Some(&st(State::Attention, 2, 10, &["a", "b", "c", "d", "e", "f"]))),
            Icon::Problem
        );
        let lang = Lang::parse("en").unwrap();
        assert_eq!(
            tooltip(lang, Some(&st(State::Ok, 5, 5, &[]))),
            "Secblitz \u{2014} You're protected"
        );
        assert_eq!(
            tooltip(lang, Some(&st(State::Attention, 2, 5, &["a", "b", "c"]))),
            "Secblitz \u{2014} 3 things need your attention"
        );
        assert_eq!(
            tooltip(lang, Some(&st(State::Attention, 4, 5, &["a"]))),
            "Secblitz \u{2014} 1 thing needs your attention"
        );
        assert_eq!(tooltip(lang, None), "Secblitz \u{2014} Not checked yet");
        assert!(tooltip(lang, None).encode_utf16().count() < 128);
    }

    #[test]
    fn alerts_only_when_protection_gets_worse() {
        let before = st(State::Attention, 4, 6, &["a", "b"]);
        assert!(!worsened(&before, &before));
        assert!(!worsened(&before, &st(State::Attention, 5, 6, &["a"])));
        assert!(worsened(&before, &st(State::Attention, 3, 6, &["a", "b"])));
        assert!(worsened(&before, &st(State::Attention, 4, 6, &["a", "c"])));
        assert!(!worsened(&st(State::Ok, 6, 6, &[]), &st(State::Ok, 6, 6, &[])));
        assert!(worsened(&st(State::Ok, 6, 6, &[]), &st(State::Attention, 5, 6, &["z"])));
    }

    #[test]
    fn rendered_icons_are_shield_shaped_with_alpha() {
        for icon in ALL {
            for size in [16usize, 32] {
                let px = render(icon, size);
                assert_eq!(px.len(), size * size * 4);
                let alpha = |x: usize, y: usize| px[(y * size + x) * 4 + 3];
                assert_eq!(alpha(0, 0), 0);
                assert_eq!(alpha(size - 1, size - 1), 0);
                assert_eq!(alpha(size / 2, size / 3), 255);
                assert!(px.chunks(4).filter(|p| p[3] > 0 && p[3] < 255).count() > 0);
            }
        }
        let rgb = |icon| {
            let p = render(icon, 32);
            let o = (8 * 32 + 8) * 4;
            (p[o + 2], p[o + 1], p[o])
        };
        assert_ne!(rgb(Icon::Protected), rgb(Icon::Attention));
        assert_ne!(rgb(Icon::Problem), rgb(Icon::Unknown));
    }
}
