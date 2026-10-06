//! Fluent-like line icons in a 24-unit box, stroke only.
use super::svg::PathData;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    Shield,
    Tick,
    Cross,
    Excl,
    Wall,
    Update,
    Lock,
    Wifi,
    Person,
    Eye,
    Game,
    Music,
    Play,
    Cart,
    News,
    Cards,
    Mail,
    Camera,
    Warn,
    Gear,
    Bin,
    Power,
    Search,
    Doc,
    Remote,
    Folder,
}

impl Glyph {
    pub const ALL: [Glyph; 26] = [
        Glyph::Shield,
        Glyph::Tick,
        Glyph::Cross,
        Glyph::Excl,
        Glyph::Wall,
        Glyph::Update,
        Glyph::Lock,
        Glyph::Wifi,
        Glyph::Person,
        Glyph::Eye,
        Glyph::Game,
        Glyph::Music,
        Glyph::Play,
        Glyph::Cart,
        Glyph::News,
        Glyph::Cards,
        Glyph::Mail,
        Glyph::Camera,
        Glyph::Warn,
        Glyph::Gear,
        Glyph::Bin,
        Glyph::Power,
        Glyph::Search,
        Glyph::Doc,
        Glyph::Remote,
        Glyph::Folder,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Glyph::Shield => "shield",
            Glyph::Tick => "tick",
            Glyph::Cross => "cross",
            Glyph::Excl => "excl",
            Glyph::Wall => "wall",
            Glyph::Update => "update",
            Glyph::Lock => "lock",
            Glyph::Wifi => "wifi",
            Glyph::Person => "person",
            Glyph::Eye => "eye",
            Glyph::Game => "game",
            Glyph::Music => "music",
            Glyph::Play => "play",
            Glyph::Cart => "cart",
            Glyph::News => "news",
            Glyph::Cards => "cards",
            Glyph::Mail => "mail",
            Glyph::Camera => "camera",
            Glyph::Warn => "warn",
            Glyph::Gear => "gear",
            Glyph::Bin => "bin",
            Glyph::Power => "power",
            Glyph::Search => "search",
            Glyph::Doc => "doc",
            Glyph::Remote => "remote",
            Glyph::Folder => "folder",
        }
    }

    pub const fn d(self) -> &'static str {
        match self {
            Glyph::Shield => "M12 2.6c2.5 1.8 5.2 2.8 8.3 3V11c0 4.7-2.9 8-8.3 10.2C6.6 19 3.7 15.7 3.7 11V5.6c3.1-.2 5.8-1.2 8.3-3z",
            Glyph::Tick => "M7.6 12.4l3 3 5.8-6.2",
            Glyph::Cross => "M8.6 8.6l6.8 6.8M15.4 8.6l-6.8 6.8",
            Glyph::Excl => "M12 7.6v5.6M12 16.5v.1",
            Glyph::Wall => "M3.5 6h17v12h-17zM3.5 10h17M3.5 14h17M9 6v4M15 6v4M6.2 10v4M12 10v4M17.8 10v4M9 14v4M15 14v4",
            Glyph::Update => "M19 8.2a7.6 7.6 0 0 0-13.6 1.4M5 15.8a7.6 7.6 0 0 0 13.6-1.4M19.2 3.8v4.6h-4.6M4.8 20.2v-4.6h4.6",
            Glyph::Lock => "M6.5 11h11v9h-11zM8.6 11V8.2a3.4 3.4 0 0 1 6.8 0V11M12 14.6v2",
            Glyph::Wifi => "M3.5 9.6a12 12 0 0 1 17 0M6.6 12.9a7.6 7.6 0 0 1 10.8 0M9.6 16.1a3.3 3.3 0 0 1 4.8 0M12 19.3v.1",
            Glyph::Person => "M12 11.8a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM5.2 20a6.8 6.8 0 0 1 13.6 0",
            Glyph::Eye => "M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6zM12 14.6a2.6 2.6 0 1 0 0-5.2 2.6 2.6 0 0 0 0 5.2z",
            Glyph::Game => "M7 8.5h10a4 4 0 0 1 4 4v1.2a3 3 0 0 1-5.5 1.6l-1-1.8h-5l-1 1.8A3 3 0 0 1 3 13.7v-1.2a4 4 0 0 1 4-4zM7.6 10.6v3.2M6 12.2h3.2M15.6 11.6h.1M17.6 13.2h.1",
            Glyph::Music => "M9 17.5V6.2l10-2v11.3M9 17.5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0zM19 15.5a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z",
            Glyph::Play => "M4 6h16v12H4zM10.2 9.4v5.2l4.6-2.6z",
            Glyph::Cart => "M3 4.5h2.4l2.2 10h10l2.2-7H6.3M9 19.3h.1M16.6 19.3h.1",
            Glyph::News => "M5 5h14v14H5zM8 9h8M8 12.4h8M8 15.8h5",
            Glyph::Cards => "M7 3.8h10v16.4H7zM12 8.4c-2 2.2-3 3.2-3 4.3a1.6 1.6 0 0 0 3 .7 1.6 1.6 0 0 0 3-.7c0-1.1-1-2.1-3-4.3zM12 13.6v2.2",
            Glyph::Mail => "M3.5 6h17v12h-17zM3.5 6.5l8.5 6.5 8.5-6.5",
            Glyph::Camera => "M4 8h3.5l1.5-2.2h6L16.5 8H20v10.5H4zM12 16.2a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
            Glyph::Warn => "M12 3.6l9 16H3z",
            Glyph::Gear => "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM12 3v2.4M12 18.6V21M3 12h2.4M18.6 12H21M5.6 5.6l1.7 1.7M16.7 16.7l1.7 1.7M5.6 18.4l1.7-1.7M16.7 7.3l1.7-1.7",
            Glyph::Bin => "M5 7h14M9.2 7V4.8h5.6V7M6.8 7l1 13h8.4l1-13M10.2 10.6v6.2M13.8 10.6v6.2",
            Glyph::Power => "M12 4v7M7.2 6.8a7 7 0 1 0 9.6 0",
            Glyph::Search => "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.2 15.2L20 20",
            Glyph::Doc => "M6.5 3.5h7.5l3.5 3.5v13.5h-11zM9 11h6M9 14h6M9 17h4",
            Glyph::Remote => "M3.5 5h17v11h-17zM9 20h6M12 16v4",
            Glyph::Folder => "M3.5 6.5h6l2 2h9v10h-17z",
        }
    }

    pub fn data(self) -> &'static PathData {
        static ALL: OnceLock<Vec<PathData>> = OnceLock::new();
        let all = ALL.get_or_init(|| Glyph::ALL.iter().map(|g| PathData::of(g.d())).collect());
        &all[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Glyph> {
        Glyph::ALL.into_iter().find(|g| g.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_parses_and_fits_its_box() {
        for (i, g) in Glyph::ALL.into_iter().enumerate() {
            assert_eq!(g as usize, i, "ALL is in declaration order");
            let d = PathData::parse(g.d()).unwrap_or_else(|e| panic!("{}: {e}", g.name()));
            assert!(!d.segs.is_empty(), "{}", g.name());
            assert_eq!(g.data(), &d);
            assert!(d.length() > 0.0, "{}", g.name());
            let b = d.bounds().unwrap();
            assert!(
                b.x >= 0.0 && b.y >= 0.0 && b.x + b.width <= 24.0 && b.y + b.height <= 24.0,
                "{} leaves its box: {b:?}",
                g.name()
            );
            assert_eq!(Glyph::from_name(g.name()), Some(g));
        }
    }

    #[test]
    fn arcs_in_the_set_land_on_their_end_points() {
        let lock = Glyph::Lock.data();
        assert!(lock
            .segs
            .iter()
            .any(|s| matches!(s, super::super::svg::Seg::Cubic(_, _, p) if (p.x - 15.4).abs() < 1e-4 && (p.y - 8.2).abs() < 1e-4)));
        let end = Glyph::Power.data().end().unwrap();
        assert!((end.x - 16.8).abs() < 1e-4 && (end.y - 6.8).abs() < 1e-4);
    }
}
