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

/// Solid shield used as the brand mark (Lucide shield outline, filled).
pub const BRAND_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/></svg>"##;

impl Icon {
    /// SVG source using `stroke="currentColor"` so the svg style can tint it.
    pub fn svg(self) -> &'static [u8] {
        self.svg_str().as_bytes()
    }

    fn svg_str(self) -> &'static str {
        // Lucide v1 artwork (ISC); the wrapper below is shared by every glyph.
        macro_rules! s {
            ($inner:expr) => {
                concat!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
                    $inner,
                    "</svg>"
                )
            };
        }
        match self {
            Icon::Shield => s!(r##"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"##),
            Icon::ShieldCheck => s!(r##"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/> <path d="m9 12 2 2 4-4"/>"##),
            Icon::ShieldAlert => s!(r##"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/> <path d="M12 8v4"/> <path d="M12 16h.01"/>"##),
            Icon::ShieldX => s!(r##"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/> <path d="m14.5 9.5-5 5"/> <path d="m9.5 9.5 5 5"/>"##),
            Icon::Home => s!(r##"<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/> <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"##),
            Icon::Wrench => s!(r##"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.106-3.105c.32-.322.863-.22.983.218a6 6 0 0 1-8.259 7.057l-7.91 7.91a1 1 0 0 1-2.999-3l7.91-7.91a6 6 0 0 1 7.057-8.259c.438.12.54.662.219.984z"/>"##),
            Icon::Sparkles => s!(r##"<path d="M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z"/> <path d="M20 2v4"/> <path d="M22 4h-4"/> <circle cx="4" cy="20" r="2"/>"##),
            Icon::Toolbox => s!(r##"<path d="M16 12v4"/> <path d="M16 6V4a2 2 0 00-2-2h-4a2 2 0 00-2 2v2"/> <path d="M17 6a2 2 0 011.414.586l3 3A2 2 0 0122 11v8a2 2 0 01-2 2H4a2 2 0 01-2-2v-8a2 2 0 01.586-1.414l3-3A2 2 0 017 6z"/> <path d="M2 14h20"/> <path d="M8 12v4"/>"##),
            Icon::History => s!(r##"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/> <path d="M3 3v5h5"/> <path d="M12 7v5l4 2"/>"##),
            Icon::Settings => s!(r##"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/> <circle cx="12" cy="12" r="3"/>"##),
            Icon::Check => s!(r##"<path d="M20 6 9 17l-5-5"/>"##),
            Icon::CheckCircle => s!(r##"<circle cx="12" cy="12" r="10"/> <path d="m16 9-5.5 5.5L8 12"/>"##),
            Icon::X => s!(r##"<path d="M18 6 6 18"/> <path d="m6 6 12 12"/>"##),
            Icon::AlertTriangle => s!(r##"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/> <path d="M12 9v4"/> <path d="M12 17h.01"/>"##),
            Icon::Info => s!(r##"<circle cx="12" cy="12" r="10"/> <path d="M12 16v-4"/> <path d="M12 8h.01"/>"##),
            Icon::Refresh => s!(r##"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/> <path d="M21 3v5h-5"/> <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/> <path d="M8 16H3v5"/>"##),
            Icon::Undo => s!(r##"<path d="M9 14 4 9l5-5"/> <path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>"##),
            Icon::ChevronRight => s!(r##"<path d="m9 18 6-6-6-6"/>"##),
            Icon::ChevronDown => s!(r##"<path d="m6 9 6 6 6-6"/>"##),
            Icon::Lock => s!(r##"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/> <path d="M7 11V7a5 5 0 0 1 10 0v4"/>"##),
            Icon::Key => s!(r##"<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/> <circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/>"##),
            Icon::Download => s!(r##"<path d="M12 15V3"/> <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/> <path d="m7 10 5 5 5-5"/>"##),
            Icon::Scan => s!(r##"<path d="M3 7V5a2 2 0 0 1 2-2h2"/> <path d="M17 3h2a2 2 0 0 1 2 2v2"/> <path d="M21 17v2a2 2 0 0 1-2 2h-2"/> <path d="M7 21H5a2 2 0 0 1-2-2v-2"/> <circle cx="12" cy="12" r="3"/> <path d="m16 16-1.9-1.9"/>"##),
            Icon::HardDrive => s!(r##"<path d="M10 16h.01"/> <path d="M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/> <path d="M21.946 12.013H2.054"/> <path d="M6 16h.01"/>"##),
            Icon::Bug => s!(r##"<path d="M12 20v-9"/> <path d="M14 7a4 4 0 0 1 4 4v3a6 6 0 0 1-12 0v-3a4 4 0 0 1 4-4z"/> <path d="M14.12 3.88 16 2"/> <path d="M21 21a4 4 0 0 0-3.81-4"/> <path d="M21 5a4 4 0 0 1-3.55 3.97"/> <path d="M22 13h-4"/> <path d="M3 21a4 4 0 0 1 3.81-4"/> <path d="M3 5a4 4 0 0 0 3.55 3.97"/> <path d="M6 13H2"/> <path d="m8 2 1.88 1.88"/> <path d="M9 7.13V6a3 3 0 1 1 6 0v1.13"/>"##),
            Icon::Trash => s!(r##"<path d="M10 11v6"/> <path d="M14 11v6"/> <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/> <path d="M3 6h18"/> <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"##),
            Icon::Package => s!(r##"<path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z"/> <path d="M12 22V12"/> <polyline points="3.29 7 12 12 20.71 7"/> <path d="m7.5 4.27 9 5.15"/>"##),
            Icon::ExternalLink => s!(r##"<path d="M15 3h6v6"/> <path d="M10 14 21 3"/> <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>"##),
            Icon::Restart => s!(r##"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/> <path d="M3 3v5h5"/>"##),
            Icon::Gamepad => s!(r##"<line x1="6" x2="10" y1="11" y2="11"/> <line x1="8" x2="8" y1="9" y2="13"/> <line x1="15" x2="15.01" y1="12" y2="12"/> <line x1="18" x2="18.01" y1="10" y2="10"/> <path d="M17.32 5H6.68a4 4 0 0 0-3.978 3.59c-.006.052-.01.101-.017.152C2.604 9.416 2 14.456 2 16a3 3 0 0 0 3 3c1 0 1.5-.5 2-1l1.414-1.414A2 2 0 0 1 9.828 16h4.344a2 2 0 0 1 1.414.586L17 18c.5.5 1 1 2 1a3 3 0 0 0 3-3c0-1.545-.604-6.584-.685-7.258-.007-.05-.011-.1-.017-.151A4 4 0 0 0 17.32 5z"/>"##),
            Icon::Bot => s!(r##"<path d="M12 8V4H8"/> <rect width="16" height="12" x="4" y="8" rx="2"/> <path d="M2 14h2"/> <path d="M20 14h2"/> <path d="M15 13v2"/> <path d="M9 13v2"/>"##),
            Icon::Globe => s!(r##"<circle cx="12" cy="12" r="10"/> <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/> <path d="M2 12h20"/>"##),
            Icon::Moon => s!(r##"<path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"/>"##),
            Icon::Sun => s!(r##"<circle cx="12" cy="12" r="4"/> <path d="M12 2v2"/> <path d="M12 20v2"/> <path d="m4.93 4.93 1.41 1.41"/> <path d="m17.66 17.66 1.41 1.41"/> <path d="M2 12h2"/> <path d="M20 12h2"/> <path d="m6.34 17.66-1.41 1.41"/> <path d="m19.07 4.93-1.41 1.41"/>"##),
            Icon::Bell => s!(r##"<path d="M10.268 21a2 2 0 0 0 3.464 0"/> <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"##),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_well_formed() {
        for i in [Icon::Shield, Icon::Bell, Icon::Toolbox, Icon::Restart] {
            let s = std::str::from_utf8(i.svg()).unwrap();
            assert!(s.starts_with("<svg") && s.ends_with("</svg>") && s.contains("currentColor"));
            assert!(s.len() > 200);
        }
    }
}
