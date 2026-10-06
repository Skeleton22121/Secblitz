//! Design tokens: colours, type scale, spacing, radii and fonts.
#![allow(dead_code)]

use iced::font::{Family, Weight};
use iced::{Color, Font, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
    Neutral,
    Brand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub mode: Mode,
    pub bg: Color,
    pub sidebar: Color,
    pub surface: Color,
    pub surface_alt: Color,
    /// Floating layers (menus, dropdown lists). The only surface that may
    /// carry a faint outline, because nothing else separates it from the page.
    pub popup: Color,
    pub border: Color,
    pub border_strong: Color,
    pub hover: Color,
    pub hover_strong: Color,
    pub pressed: Color,
    pub selected: Color,
    pub focus_ring: Color,
    pub disabled_bg: Color,
    pub disabled_fg: Color,
    pub text: Color,
    pub text_muted: Color,
    pub brand: Color,
    pub on_brand: Color,
    pub brand_hover: Color,
    pub brand_pressed: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub neutral: Color,
    pub accent: Color,
    pub good_text: Color,
    pub warn_text: Color,
    pub bad_text: Color,
    pub danger: Color,
    pub danger_hover: Color,
    pub danger_pressed: Color,
    pub scrim: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const LIGHT: Palette = Palette {
    mode: Mode::Light,
    bg: rgb(0xF7F7F8),
    sidebar: rgb(0xF1F1F3),
    surface: rgb(0xFFFFFF),
    surface_alt: rgb(0xF1F1F3),
    popup: rgb(0xFFFFFF),
    border: rgb(0xE8E8EB),
    border_strong: rgb(0xDEDEE2),
    hover: rgb(0xEDEDF0),
    hover_strong: rgb(0xE8E8EB),
    pressed: rgb(0xE1E1E5),
    selected: rgb(0xEAEAED),
    focus_ring: rgb(0x3F3F46),
    disabled_bg: rgb(0xF1F1F3),
    disabled_fg: rgb(0xA1A1AA),
    text: rgb(0x18181B),
    text_muted: rgb(0x71717A),
    brand: rgb(0x18181B),
    on_brand: rgb(0xFFFFFF),
    brand_hover: rgb(0x303034),
    brand_pressed: rgb(0x52525B),
    good: rgb(0x16A34A),
    warn: rgb(0xD97706),
    bad: rgb(0xDC2626),
    neutral: rgb(0x71717A),
    accent: rgb(0x2563EB),
    good_text: rgb(0x15803D),
    warn_text: rgb(0xB45309),
    bad_text: rgb(0xB91C1C),
    danger: rgb(0xDC2626),
    danger_hover: rgb(0xC21F1F),
    danger_pressed: rgb(0x9F1818),
    scrim: Color::from_rgba(0.09, 0.09, 0.11, 0.40),
};

pub const DARK: Palette = Palette {
    mode: Mode::Dark,
    bg: rgb(0x0E0E10),
    sidebar: rgb(0x0A0A0C),
    surface: rgb(0x161618),
    surface_alt: rgb(0x1E1E21),
    popup: rgb(0x212124),
    border: rgb(0x2A2A2E),
    border_strong: rgb(0x36363B),
    hover: rgb(0x212125),
    hover_strong: rgb(0x2A2A2F),
    pressed: rgb(0x36363C),
    selected: rgb(0x303036),
    focus_ring: rgb(0xD4D4D8),
    disabled_bg: rgb(0x1C1C1F),
    disabled_fg: rgb(0x636368),
    text: rgb(0xFAFAFA),
    text_muted: rgb(0xA1A1AA),
    brand: rgb(0xFAFAFA),
    on_brand: rgb(0x18181B),
    brand_hover: rgb(0xE4E4E7),
    brand_pressed: rgb(0xC4C4CA),
    good: rgb(0x22C55E),
    warn: rgb(0xF59E0B),
    bad: rgb(0xEF4444),
    neutral: rgb(0xA1A1AA),
    accent: rgb(0x60A5FA),
    good_text: rgb(0x22C55E),
    warn_text: rgb(0xF59E0B),
    bad_text: rgb(0xEF4444),
    danger: rgb(0xDC2626),
    danger_hover: rgb(0xC21F1F),
    danger_pressed: rgb(0x9F1818),
    scrim: Color::from_rgba(0.0, 0.0, 0.0, 0.60),
};

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

impl Palette {
    pub fn of(mode: Mode) -> Self {
        match mode {
            Mode::Dark => DARK,
            Mode::Light => LIGHT,
        }
    }
    pub fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Good => self.good,
            Tone::Warn => self.warn,
            Tone::Bad => self.bad,
            Tone::Neutral => self.neutral,
            Tone::Brand => self.brand,
        }
    }
    pub fn tone_text(&self, tone: Tone) -> Color {
        match tone {
            Tone::Good => self.good_text,
            Tone::Warn => self.warn_text,
            Tone::Bad => self.bad_text,
            Tone::Neutral => self.neutral,
            Tone::Brand => self.brand,
        }
    }
    pub fn tint(&self, tone: Tone) -> Color {
        Color {
            a: if self.mode == Mode::Dark { 0.16 } else { 0.10 },
            ..self.tone(tone)
        }
    }
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "Secblitz",
            iced::theme::Palette {
                background: self.bg,
                text: self.text,
                primary: self.brand,
                success: self.good,
                warning: self.warn,
                danger: self.bad,
            },
        )
    }
}

pub const DISPLAY: f32 = 28.0;
pub const H1: f32 = 22.0;
pub const H2: f32 = 17.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.5;

pub const LINE_BODY: f32 = 18.0;
pub const LINE_SMALL: f32 = 16.0;

pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 20.0;
pub const S6: f32 = 24.0;
pub const S8: f32 = 32.0;
pub const S10: f32 = 40.0;

pub const CONTROL: f32 = 36.0;
pub const CONTROL_SMALL: f32 = 28.0;
pub const ROW: f32 = 48.0;
pub const ROW_ITEM: f32 = 56.0;
pub const ICON_ROW: f32 = 20.0;
pub const MENU_ROW: f32 = 32.0;
pub const CONTENT_MAX: f32 = 560.0;
pub const WINDOW_MIN_WIDTH: f32 = 880.0;
pub const WINDOW_MIN_HEIGHT: f32 = 600.0;
pub const CHECK: f32 = 18.0;
pub const SETTING_ROW: f32 = ROW + S2;
pub const CARD_BODY_MIN: f32 = ROW * 2.0;
pub const DOT: f32 = 8.0;
pub const HAIRLINE: f32 = 1.0;
pub const MAX_READABLE: f32 = 420.0;
pub const DETAILS_MAX: f32 = 140.0;

pub const R_SMALL: f32 = 6.0;
pub const R: f32 = 8.0;
pub const R_LARGE: f32 = 12.0;
pub const R_PILL: f32 = 999.0;

pub const FAMILY: &str = "IBM Plex Sans";

pub const REGULAR: Font = Font {
    family: Family::Name(FAMILY),
    weight: Weight::Normal,
    ..Font::DEFAULT
};
pub const MEDIUM: Font = Font {
    family: Family::Name(FAMILY),
    weight: Weight::Medium,
    ..Font::DEFAULT
};
pub const SEMIBOLD: Font = Font {
    family: Family::Name(FAMILY),
    weight: Weight::Semibold,
    ..Font::DEFAULT
};
pub const BOLD: Font = Font {
    family: Family::Name(FAMILY),
    weight: Weight::Bold,
    ..Font::DEFAULT
};

pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Bold.ttf"),
];
