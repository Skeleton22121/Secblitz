//! Inline SVG icon set (Lucide, ISC licence — see assets/ICONS-LICENSE.txt).
//! OWNER: shell agent. Variants are a contract; artwork may be refined.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Shield,
    ShieldCheck,
    ShieldAlert,
    ShieldX,
    Home,
    Wrench,
    Sparkles,
    Toolbox,
    History,
    Settings,
    Check,
    CheckCircle,
    X,
    AlertTriangle,
    Info,
    Refresh,
    Undo,
    ChevronRight,
    ChevronDown,
    Lock,
    Key,
    Download,
    Scan,
    HardDrive,
    Bug,
    Trash,
    Package,
    ExternalLink,
    Restart,
    Gamepad,
    Bot,
    Globe,
    Moon,
    Sun,
    Bell,
}

impl Icon {
    /// SVG source using `stroke="currentColor"` so the svg style can tint it.
    pub fn svg(self) -> &'static [u8] {
        // TODO(shell): real Lucide artwork per variant.
        br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/></svg>"#
    }
}
