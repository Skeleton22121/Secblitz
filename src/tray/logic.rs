//! Portable tray logic (icon, tooltip, alerts) and the shield icon renderer.
use crate::i18n::Lang;
use secblitz::status::{Notify, State, Status};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Protected,
    Attention,
    Problem,
    Unknown,
}

pub const ALL: [Icon; 4] = [
    Icon::Protected,
    Icon::Attention,
    Icon::Problem,
    Icon::Unknown,
];

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
    let text = format!("Secblitz: {body}");
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

pub fn worsened(previous: &Status, now: &Status) -> bool {
    now.protected < previous.protected
        || now
            .attention
            .iter()
            .any(|id| !previous.attention.contains(id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Balloon {
    Worsened,
    Reverted(Vec<String>),
    Dangerous,
}

impl Balloon {
    pub fn page(&self) -> Option<&'static str> {
        match self {
            Balloon::Worsened => None,
            Balloon::Reverted(_) => Some("protection"),
            Balloon::Dangerous => Some("web"),
        }
    }

    pub fn text(&self, lang: Lang) -> String {
        match self {
            Balloon::Worsened => {
                lang.t("Your protection dropped. Open Secblitz to see what needs attention.")
            }
            Balloon::Reverted(ids) => reverted_text(lang, ids),
            Balloon::Dangerous => lang.t(
                "Secblitz blocked a dangerous website. It may try to steal passwords or install harmful software.",
            ),
        }
    }
}

fn reverted_text(lang: Lang, ids: &[String]) -> String {
    match ids {
        [id] if secblitz::advice::control_label(id) != "Protection check" => lang
            .t("Windows switched back a setting Secblitz fixed: {title}. Click to put it back.")
            .replace("{title}", lang.control(id).trim_end_matches('.')),
        [_] => lang.t("Windows switched back a setting Secblitz fixed. Click to put it back."),
        many => lang
            .t("Windows switched back {n} settings Secblitz fixed. Click to put them back.")
            .replace("{n}", &many.len().to_string()),
    }
}

pub fn newly_reverted(previous: &Status, now: &Status) -> Vec<String> {
    now.reverted
        .iter()
        .filter(|id| !previous.reverted.contains(id))
        .cloned()
        .collect()
}

pub fn status_balloon(previous: &Status, now: &Status, notify: &Notify) -> Option<Balloon> {
    let gained = newly_reverted(previous, now);
    if gained.is_empty() {
        return worsened(previous, now).then_some(Balloon::Worsened);
    }
    if notify.reverted {
        return Some(Balloon::Reverted(gained));
    }
    let something_else = now
        .attention
        .iter()
        .any(|id| !previous.attention.contains(id) && !gained.contains(id));
    something_else.then_some(Balloon::Worsened)
}

pub const DANGEROUS_GAP: u64 = 10 * 60;
/// Clock skew tolerated before a block time counts as forged.
const DANGEROUS_FUTURE: u64 = 5 * 60;
pub const DANGEROUS_LIMIT: u64 = 16 * 1024;

#[derive(Deserialize)]
struct DangerousAt {
    #[serde(default)]
    dangerous_at: Option<u64>,
}

/// Never a site name.
pub fn dangerous_at(bytes: &[u8]) -> Option<u64> {
    if bytes.len() as u64 > DANGEROUS_LIMIT {
        return None;
    }
    serde_json::from_slice::<DangerousAt>(bytes)
        .ok()?
        .dangerous_at
}

#[derive(Debug, Default)]
pub struct Dangerous {
    started: bool,
    seen: u64,
    shown: Option<u64>,
}

impl Dangerous {
    /// The first reading only sets the starting point, so old blocks do not notify at sign-in.
    pub fn observe(&mut self, at: Option<u64>, now: u64, allowed: bool) -> bool {
        let at = at.filter(|at| *at <= now.saturating_add(DANGEROUS_FUTURE));
        if !self.started {
            self.started = true;
            self.seen = at.unwrap_or(0);
            return false;
        }
        let Some(at) = at else {
            return false;
        };
        if at <= self.seen {
            return false;
        }
        self.seen = at;
        let spaced = self
            .shown
            .is_none_or(|last| now.saturating_sub(last) >= DANGEROUS_GAP);
        if !allowed || !spaced {
            return false;
        }
        self.shown = Some(now);
        true
    }
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
                    let c = if in_glyph(icon, x, y) {
                        [255.0; 3]
                    } else {
                        base
                    };
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
            reverted: Vec::new(),
            state,
        }
    }

    fn back(mut s: Status, ids: &[&str]) -> Status {
        s.reverted = ids.iter().map(|s| s.to_string()).collect();
        s
    }

    #[test]
    fn icon_and_tooltip_follow_status() {
        assert_eq!(icon_for(None), Icon::Unknown);
        assert_eq!(icon_for(Some(&st(State::Ok, 5, 5, &[]))), Icon::Protected);
        assert_eq!(
            icon_for(Some(&st(State::Unknown, 0, 0, &[]))),
            Icon::Unknown
        );
        assert_eq!(
            icon_for(Some(&st(State::Attention, 8, 10, &["a", "b"]))),
            Icon::Attention
        );
        assert_eq!(
            icon_for(Some(&st(
                State::Attention,
                2,
                10,
                &["a", "b", "c", "d", "e", "f"]
            ))),
            Icon::Problem
        );
        let lang = Lang::parse("en").unwrap();
        assert_eq!(
            tooltip(lang, Some(&st(State::Ok, 5, 5, &[]))),
            "Secblitz: You're protected"
        );
        assert_eq!(
            tooltip(lang, Some(&st(State::Attention, 2, 5, &["a", "b", "c"]))),
            "Secblitz: 3 things need your attention"
        );
        assert_eq!(
            tooltip(lang, Some(&st(State::Attention, 4, 5, &["a"]))),
            "Secblitz: 1 thing needs your attention"
        );
        assert_eq!(tooltip(lang, None), "Secblitz: Not checked yet");
        assert!(tooltip(lang, None).encode_utf16().count() < 128);
    }

    #[test]
    fn alerts_only_when_protection_gets_worse() {
        let before = st(State::Attention, 4, 6, &["a", "b"]);
        assert!(!worsened(&before, &before));
        assert!(!worsened(&before, &st(State::Attention, 5, 6, &["a"])));
        assert!(worsened(&before, &st(State::Attention, 3, 6, &["a", "b"])));
        assert!(worsened(&before, &st(State::Attention, 4, 6, &["a", "c"])));
        assert!(!worsened(
            &st(State::Ok, 6, 6, &[]),
            &st(State::Ok, 6, 6, &[])
        ));
        assert!(worsened(
            &st(State::Ok, 6, 6, &[]),
            &st(State::Attention, 5, 6, &["z"])
        ));
    }

    #[test]
    fn no_tray_text_has_an_em_dash() {
        let lang = Lang::parse("en").unwrap();
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for b in [
            Balloon::Worsened,
            Balloon::Dangerous,
            Balloon::Reverted(ids(&["defender.realtime"])),
            Balloon::Reverted(ids(&["not.a.control"])),
            Balloon::Reverted(ids(&["a", "b"])),
        ] {
            assert!(!b.text(lang).contains('\u{2014}'));
        }
        assert!(!tooltip(lang, None).contains('\u{2014}'));
    }

    #[test]
    fn switched_back_text_names_one_setting_or_counts_several() {
        let lang = Lang::parse("en").unwrap();
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            Balloon::Reverted(ids(&["defender.realtime"])).text(lang),
            "Windows switched back a setting Secblitz fixed: Live virus protection. Click to put it back."
        );
        assert_eq!(
            Balloon::Reverted(ids(&["defender.realtime", "defender.ioav", "uac.enabled"]))
                .text(lang),
            "Windows switched back 3 settings Secblitz fixed. Click to put them back."
        );
        let unknown = Balloon::Reverted(ids(&["not.a.control"])).text(lang);
        assert_eq!(
            unknown,
            "Windows switched back a setting Secblitz fixed. Click to put it back."
        );
        assert!(!unknown.contains("not.a.control"));
        assert_eq!(
            Balloon::Dangerous.text(lang),
            "Secblitz blocked a dangerous website. It may try to steal passwords or install harmful software."
        );
    }

    #[test]
    fn balloons_open_the_page_that_explains_them() {
        assert_eq!(Balloon::Worsened.page(), None);
        assert_eq!(
            Balloon::Reverted(vec!["a".into()]).page(),
            Some("protection")
        );
        assert_eq!(Balloon::Dangerous.page(), Some("web"));
    }

    #[test]
    fn a_gained_switched_back_item_gets_its_own_balloon() {
        let on = Notify::new(true, true);
        let before = st(State::Attention, 5, 6, &["a"]);
        let same = back(st(State::Attention, 5, 6, &["a"]), &["a"]);
        assert_eq!(newly_reverted(&before, &same), ["a"]);
        assert_eq!(
            status_balloon(&before, &same, &on),
            Some(Balloon::Reverted(vec!["a".into()]))
        );
        let more = back(
            st(State::Attention, 4, 6, &["a", "b", "c"]),
            &["a", "b", "c"],
        );
        assert_eq!(
            status_balloon(&same, &more, &on),
            Some(Balloon::Reverted(vec!["b".into(), "c".into()]))
        );
        assert_eq!(status_balloon(&more, &more, &on), None);
        let fewer = back(st(State::Attention, 5, 6, &["a"]), &["a"]);
        assert_eq!(status_balloon(&more, &fewer, &on), None);
    }

    #[test]
    fn without_new_switched_back_items_the_old_alert_rules_apply() {
        let on = Notify::new(true, true);
        let before = st(State::Attention, 4, 6, &["a", "b"]);
        assert_eq!(status_balloon(&before, &before, &on), None);
        assert_eq!(
            status_balloon(&before, &st(State::Attention, 3, 6, &["a", "b"]), &on),
            Some(Balloon::Worsened)
        );
        assert_eq!(
            status_balloon(&before, &st(State::Attention, 4, 6, &["a", "c"]), &on),
            Some(Balloon::Worsened)
        );
        let kept = back(st(State::Attention, 4, 6, &["a", "b"]), &["a"]);
        assert_eq!(
            status_balloon(&back(before.clone(), &["a"]), &kept, &on),
            None
        );
    }

    #[test]
    fn switching_the_notice_off_silences_switched_back_items_only() {
        let off = Notify::new(false, true);
        let before = st(State::Attention, 5, 6, &["a"]);
        let flipped = back(st(State::Attention, 4, 6, &["a", "b"]), &["b"]);
        assert_eq!(status_balloon(&before, &flipped, &off), None);
        let and_more = back(st(State::Attention, 3, 6, &["a", "b", "c"]), &["b"]);
        assert_eq!(
            status_balloon(&before, &and_more, &off),
            Some(Balloon::Worsened)
        );
    }

    #[test]
    fn the_dangerous_notice_waits_for_a_new_block_and_a_quiet_ten_minutes() {
        let mut d = Dangerous::default();
        assert!(
            !d.observe(Some(900), 1000, true),
            "first reading is the start"
        );
        assert!(!d.observe(Some(900), 1060, true));
        assert!(d.observe(Some(1100), 1120, true));
        assert!(!d.observe(Some(1100), 1180, true));
        assert!(!d.observe(Some(1200), 1240, true), "inside ten minutes");
        assert!(!d.observe(Some(1300), 1120 + DANGEROUS_GAP - 1, true));
        assert!(d.observe(Some(1400), 1120 + DANGEROUS_GAP, true));
        assert!(!d.observe(None, 5000, true));
        assert!(!d.observe(Some(1000), 5000, true), "older times never show");
    }

    #[test]
    fn the_first_block_after_a_quiet_start_is_announced() {
        let mut d = Dangerous::default();
        assert!(!d.observe(None, 1000, true));
        assert!(!d.observe(None, 1060, true));
        assert!(d.observe(Some(1100), 1120, true));
        let mut forged = Dangerous::default();
        assert!(!forged.observe(Some(u64::MAX), 1000, true));
        assert!(forged.observe(Some(1100), 1120, true));
    }

    #[test]
    fn the_dangerous_notice_respects_the_switch_and_ignores_forged_times() {
        let mut d = Dangerous::default();
        assert!(!d.observe(Some(100), 200, false));
        assert!(!d.observe(Some(150), 210, false), "off: seen, not shown");
        assert!(d.observe(Some(300), 400, true), "off did not use the gap");
        let mut e = Dangerous::default();
        assert!(!e.observe(Some(100), 200, true));
        assert!(!e.observe(Some(u64::MAX), 300, true));
        assert!(
            e.observe(Some(350), 400, true),
            "a forged time changed nothing"
        );
    }

    #[test]
    fn only_the_block_time_is_read_from_the_web_status() {
        let full = br#"{"listening":true,"state":"on","blocked":[1,2,3],"dangerous_at":1791334020,"future":"x"}"#;
        assert_eq!(dangerous_at(full), Some(1_791_334_020));
        assert_eq!(dangerous_at(br#"{"listening":true}"#), None);
        assert_eq!(dangerous_at(br#"{"dangerous_at":null}"#), None);
        assert_eq!(dangerous_at(br#"{"dangerous_at":"soon"}"#), None);
        assert_eq!(dangerous_at(b"not json"), None);
        assert_eq!(dangerous_at(&vec![b' '; 20_000]), None);
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
