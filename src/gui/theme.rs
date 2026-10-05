//! Design tokens: colours, type scale, spacing, radii and fonts.
//!
//! OWNER: design-system agent. Public names are a contract used by every page; the
//! values may be refined freely.
// Tokens are a shared vocabulary; not every one is used by every build.
#![allow(dead_code)]

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
    /// Input and control outlines (stronger than `border`).
    pub border_strong: Color,
    /// Row / ghost-button hover on `bg` and `surface`.
    pub hover: Color,
    /// Hover on the sidebar and on `surface_alt` fills.
    pub hover_strong: Color,
    /// Pressed state for rows, ghost and secondary buttons.
    pub pressed: Color,
    /// Selected row / active sidebar item / selected segment track.
    pub selected: Color,
    /// Keyboard focus and open-dropdown outline.
    pub focus_ring: Color,
    /// Disabled control fill and text.
    pub disabled_bg: Color,
    pub disabled_fg: Color,
    pub text: Color,
    pub text_muted: Color,
    /// Brand / primary action.
    pub brand: Color,
    pub on_brand: Color,
    /// Primary button hover / pressed (brand, one step).
    pub brand_hover: Color,
    pub brand_pressed: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub neutral: Color,
    /// Solid destructive button fill (white text) and its hover / pressed steps.
    pub danger: Color,
    pub danger_hover: Color,
    pub danger_pressed: Color,
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
    border_strong: rgb(0xD4D4D8),
    hover: rgb(0xF4F4F5),
    hover_strong: rgb(0xEBEBED),
    pressed: rgb(0xE4E4E7),
    selected: rgb(0xE4E4E7),
    focus_ring: rgb(0x3F3F46),
    disabled_bg: rgb(0xF4F4F5),
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
    danger: rgb(0xDC2626),
    danger_hover: rgb(0xC21F1F),
    danger_pressed: rgb(0x9F1818),
    scrim: Color::from_rgba(0.09, 0.09, 0.11, 0.40),
};

pub const DARK: Palette = Palette {
    mode: Mode::Dark,
    bg: rgb(0x09090B),
    sidebar: rgb(0x111113),
    surface: rgb(0x18181B),
    surface_alt: rgb(0x27272A),
    border: rgb(0x2E2E33),
    border_strong: rgb(0x3F3F46),
    hover: rgb(0x232326),
    hover_strong: rgb(0x2C2C30),
    pressed: rgb(0x3F3F46),
    selected: rgb(0x2C2C30),
    focus_ring: rgb(0xD4D4D8),
    disabled_bg: rgb(0x1F1F22),
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
    danger: rgb(0xDC2626),
    danger_hover: rgb(0xEF4444),
    danger_pressed: rgb(0xB91C1C),
    scrim: Color::from_rgba(0.0, 0.0, 0.0, 0.60),
};

/// Linear blend of two colours (`t` 0 = `a`, 1 = `b`), for tiny animated regions.
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

// Type scale (px). IBM Plex Sans runs a little wider than Inter, so the
// sizes are one notch tighter. Plex's natural line height is 1.3 em.
pub const DISPLAY: f32 = 28.0;
pub const H1: f32 = 22.0;
pub const H2: f32 = 17.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.5;

/// Absolute line heights (px) used for fixed-height controls.
pub const LINE_BODY: f32 = 18.0;
pub const LINE_SMALL: f32 = 16.0;

// Spacing scale (px). Use only these.
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 20.0;
pub const S6: f32 = 24.0;
pub const S8: f32 = 32.0;
pub const S10: f32 = 40.0;

// Control heights (px).
pub const CONTROL: f32 = 36.0;
pub const CONTROL_SMALL: f32 = 28.0;
/// Minimum height of a list row.
pub const ROW: f32 = 48.0;
/// Checkbox box edge.
pub const CHECK: f32 = 18.0;

// Radii (px).
pub const R_SMALL: f32 = 6.0;
pub const R: f32 = 8.0;
pub const R_LARGE: f32 = 12.0;
/// Fully rounded (pills, badges).
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

/// Font files to register with the application.
pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Bold.ttf"),
];
