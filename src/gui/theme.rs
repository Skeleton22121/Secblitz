//! Design tokens: colours, type scale, spacing, radii and fonts.
//!
//! OWNER: shell agent. Public names are a contract used by every page; the
//! values may be refined freely.
use iced::font::{Family, Weight};
use iced::{Color, Font, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

/// Semantic tone used for status colours, pills, icons and the score ring.
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
    /// Window background.
    pub bg: Color,
    /// Sidebar background.
    pub sidebar: Color,
    /// Cards and sheets.
    pub surface: Color,
    /// Hovered rows, inputs, secondary buttons.
    pub surface_alt: Color,
    pub border: Color,
    pub text: Color,
    pub text_muted: Color,
    /// Brand / primary action.
    pub brand: Color,
    pub on_brand: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub neutral: Color,
    /// Modal backdrop.
    pub scrim: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// Neutral base; colour only carries meaning (status). Light is the default.
pub const LIGHT: Palette = Palette {
    mode: Mode::Light,
    bg: rgb(0xFAFAFA),
    sidebar: rgb(0xF4F4F5),
    surface: rgb(0xFFFFFF),
    surface_alt: rgb(0xF4F4F5),
    border: rgb(0xE4E4E7),
    text: rgb(0x18181B),
    text_muted: rgb(0x71717A),
    brand: rgb(0x18181B),
    on_brand: rgb(0xFFFFFF),
    good: rgb(0x16A34A),
    warn: rgb(0xD97706),
    bad: rgb(0xDC2626),
    neutral: rgb(0x71717A),
    scrim: Color::from_rgba(0.09, 0.09, 0.11, 0.40),
};

pub const DARK: Palette = Palette {
    mode: Mode::Dark,
    bg: rgb(0x09090B),
    sidebar: rgb(0x111113),
    surface: rgb(0x18181B),
    surface_alt: rgb(0x27272A),
    border: rgb(0x2E2E33),
    text: rgb(0xFAFAFA),
    text_muted: rgb(0xA1A1AA),
    brand: rgb(0xFAFAFA),
    on_brand: rgb(0x18181B),
    good: rgb(0x22C55E),
    warn: rgb(0xF59E0B),
    bad: rgb(0xEF4444),
    neutral: rgb(0xA1A1AA),
    scrim: Color::from_rgba(0.0, 0.0, 0.0, 0.60),
};

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
    /// The same tone at low opacity, for pill/icon backgrounds.
    pub fn tint(&self, tone: Tone) -> Color {
        Color {
            a: if self.mode == Mode::Dark { 0.18 } else { 0.12 },
            ..self.tone(tone)
        }
    }
    /// iced theme so built-in widgets (checkbox, toggler, scrollbar…) match.
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

// Type scale (px).
pub const DISPLAY: f32 = 30.0;
pub const H1: f32 = 24.0;
pub const H2: f32 = 18.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.5;

// Spacing / radii (px).
pub const GAP: f32 = 16.0;
pub const PAD: f32 = 20.0;
pub const RADIUS: f32 = 12.0;
pub const RADIUS_SMALL: f32 = 8.0;

pub const FAMILY: &str = "Inter";

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

/// Font files to register with the application.
pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
];
