//! Suffix matching over sorted 64-bit name hashes: 8 bytes per domain, a lookup
//! is a handful of binary searches.

use serde::{Deserialize, Serialize};

pub fn hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= b.to_ascii_lowercase() as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Clone, Debug, Default)]
pub struct HashSet64(Vec<u64>);

impl HashSet64 {
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut v: Vec<u64> = names.into_iter().map(hash).collect();
        v.sort_unstable();
        v.dedup();
        v.shrink_to_fit();
        HashSet64(v)
    }

    pub fn from_hashes(mut hashes: Vec<u64>) -> Self {
        hashes.sort_unstable();
        hashes.dedup();
        hashes.shrink_to_fit();
        HashSet64(hashes)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, h: u64) -> bool {
        self.0.binary_search(&h).is_ok()
    }

    /// True when the name or any parent of it with at least two labels is in
    /// the set (`a.b.c.com`, `b.c.com`, `c.com`; never `com`).
    pub fn any_suffix(&self, name: &str) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let mut rest = name;
        while rest.contains('.') {
            if self.contains(hash(rest)) {
                return true;
            }
            match rest.find('.') {
                Some(i) => rest = &rest[i + 1..],
                None => break,
            }
        }
        false
    }
}

#[derive(Clone, Debug, Default)]
pub struct Category {
    pub block: HashSet64,
    pub allow: HashSet64,
}

impl Category {
    pub fn blocks(&self, name: &str) -> bool {
        self.block.any_suffix(name) && !self.allow.any_suffix(name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Ads,
    Tracking,
    Dangerous,
    Adult,
    Gambling,
    Scam,
    Popups,
}

pub const KINDS: usize = 7;

impl Kind {
    pub const ALL: [Kind; KINDS] = [
        Kind::Ads,
        Kind::Tracking,
        Kind::Dangerous,
        Kind::Adult,
        Kind::Gambling,
        Kind::Scam,
        Kind::Popups,
    ];

    pub fn index(self) -> usize {
        match self {
            Kind::Ads => 0,
            Kind::Tracking => 1,
            Kind::Dangerous => 2,
            Kind::Adult => 3,
            Kind::Gambling => 4,
            Kind::Scam => 5,
            Kind::Popups => 6,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Switches {
    pub ads: bool,
    pub tracking: bool,
    pub dangerous: bool,
    pub adult: bool,
    pub gambling: bool,
    pub scam: bool,
    pub popups: bool,
    pub safe_search: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub ads: Category,
    pub tracking: Category,
    pub dangerous: Category,
    pub adult: Category,
    pub gambling: Category,
    pub scam: Category,
    pub popups: Category,
    pub never: HashSet64,
}

impl Filter {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn decide(&self, name: &str, on: Switches) -> Option<Kind> {
        self.decide_allowing(name, on, &HashSet64::default())
    }

    /// Allowed sites and the never-block set win; then Dangerous, Scam, Adult, Gambling, Pop-ups, Ads, Tracking. Ads go before tracking because ad networks are on the tracking lists too.
    pub fn decide_allowing(&self, name: &str, on: Switches, allowed: &HashSet64) -> Option<Kind> {
        if allowed.any_suffix(name) || self.never.any_suffix(name) {
            return None;
        }
        if on.dangerous && self.dangerous.blocks(name) {
            return Some(Kind::Dangerous);
        }
        if on.scam && self.scam.blocks(name) {
            return Some(Kind::Scam);
        }
        if on.adult && self.adult.blocks(name) {
            return Some(Kind::Adult);
        }
        if on.gambling && self.gambling.blocks(name) {
            return Some(Kind::Gambling);
        }
        if on.popups && self.popups.blocks(name) {
            return Some(Kind::Popups);
        }
        if on.ads && self.ads.blocks(name) {
            return Some(Kind::Ads);
        }
        if on.tracking && self.tracking.blocks(name) {
            return Some(Kind::Tracking);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> HashSet64 {
        HashSet64::from_names(names.iter().copied())
    }

    fn cat(block: &[&str], allow: &[&str]) -> Category {
        Category {
            block: set(block),
            allow: set(allow),
        }
    }

    const ALL: Switches = Switches {
        ads: true,
        tracking: true,
        dangerous: true,
        adult: true,
        gambling: true,
        scam: true,
        popups: true,
        safe_search: true,
    };

    #[test]
    fn suffix_match_blocks_subdomains() {
        let s = set(&["example.com"]);
        assert!(s.any_suffix("example.com"));
        assert!(s.any_suffix("a.b.example.com"));
        assert!(!s.any_suffix("notexample.com"));
        assert!(!s.any_suffix("example.org"));
        assert!(s.any_suffix("A.Example.COM"));
        assert_eq!(set(&["a.com", "a.com", "b.com"]).len(), 2);
    }

    #[test]
    fn two_label_minimum() {
        let s = set(&["com"]);
        assert!(!s.any_suffix("example.com"));
        assert!(!s.any_suffix("com"));
        let s = set(&["example.com"]);
        assert!(!s.any_suffix("com"));
    }

    #[test]
    fn exception_beats_block() {
        let c = cat(&["example.com"], &["ok.example.com"]);
        assert!(c.blocks("ads.example.com"));
        assert!(!c.blocks("ok.example.com"));
        assert!(!c.blocks("x.ok.example.com"));
    }

    #[test]
    fn never_block_wins() {
        let f = Filter {
            ads: cat(&["windowsupdate.com", "ads.com"], &[]),
            never: set(&["windowsupdate.com"]),
            ..Filter::empty()
        };
        assert_eq!(f.decide("dl.windowsupdate.com", ALL), None);
        assert_eq!(f.decide("ads.com", ALL), Some(Kind::Ads));
    }

    #[test]
    fn decide_order_dangerous_first() {
        let f = Filter {
            ads: cat(&["x.com"], &[]),
            tracking: cat(&["x.com"], &[]),
            dangerous: cat(&["x.com"], &[]),
            ..Filter::empty()
        };
        assert_eq!(f.decide("x.com", ALL), Some(Kind::Dangerous));
        let no_danger = Switches {
            dangerous: false,
            ..ALL
        };
        assert_eq!(f.decide("x.com", no_danger), Some(Kind::Ads));
        let tracking_only = Switches {
            tracking: true,
            ..Switches::default()
        };
        assert_eq!(f.decide("x.com", tracking_only), Some(Kind::Tracking));
    }

    #[test]
    fn family_lists_sit_between_dangerous_and_ads() {
        let f = Filter {
            ads: cat(&["x.com"], &[]),
            tracking: cat(&["x.com"], &[]),
            dangerous: cat(&["bad.com"], &[]),
            adult: cat(&["x.com", "bad.com", "adult.com"], &[]),
            gambling: cat(&["x.com", "bet.com", "adult.com"], &[]),
            scam: Category::default(),
            popups: Category::default(),
            never: HashSet64::default(),
        };
        assert_eq!(f.decide("bad.com", ALL), Some(Kind::Dangerous));
        assert_eq!(f.decide("adult.com", ALL), Some(Kind::Adult));
        assert_eq!(f.decide("bet.com", ALL), Some(Kind::Gambling));
        assert_eq!(f.decide("x.com", ALL), Some(Kind::Adult));
        let no_adult = Switches {
            adult: false,
            ..ALL
        };
        assert_eq!(f.decide("x.com", no_adult), Some(Kind::Gambling));
        assert_eq!(f.decide("adult.com", no_adult), Some(Kind::Gambling));
        let no_family = Switches {
            adult: false,
            gambling: false,
            ..ALL
        };
        assert_eq!(f.decide("x.com", no_family), Some(Kind::Ads));
        assert_eq!(f.decide("bet.com", no_family), None);
    }

    #[test]
    fn scam_and_popup_lists_keep_their_place_in_the_order() {
        let f = Filter {
            ads: cat(&["x.com", "p.com"], &[]),
            dangerous: cat(&["bad.com"], &[]),
            scam: cat(&["bad.com", "shop.com", "x.com"], &[]),
            adult: cat(&["x.com", "shop.com"], &[]),
            gambling: cat(&["x.com"], &[]),
            popups: cat(&["x.com", "p.com"], &[]),
            ..Filter::empty()
        };
        assert_eq!(f.decide("bad.com", ALL), Some(Kind::Dangerous));
        assert_eq!(f.decide("shop.com", ALL), Some(Kind::Scam));
        assert_eq!(f.decide("x.com", ALL), Some(Kind::Scam));
        let no_scam = Switches { scam: false, ..ALL };
        assert_eq!(f.decide("x.com", no_scam), Some(Kind::Adult));
        let no_family = Switches {
            scam: false,
            adult: false,
            gambling: false,
            ..ALL
        };
        assert_eq!(f.decide("x.com", no_family), Some(Kind::Popups));
        assert_eq!(f.decide("p.com", no_family), Some(Kind::Popups));
        let no_popups = Switches {
            popups: false,
            ..no_family
        };
        assert_eq!(f.decide("p.com", no_popups), Some(Kind::Ads));
        assert_eq!(f.decide("shop.com", no_popups), None);
    }

    #[test]
    fn never_block_beats_scam_and_popups() {
        let f = Filter {
            scam: cat(&["windowsupdate.com"], &[]),
            popups: cat(&["windowsupdate.com"], &[]),
            never: set(&["windowsupdate.com"]),
            ..Filter::empty()
        };
        assert_eq!(f.decide("dl.windowsupdate.com", ALL), None);
    }

    #[test]
    fn user_allow_beats_every_list() {
        let f = Filter {
            ads: cat(&["x.com"], &[]),
            dangerous: cat(&["bad.com"], &[]),
            adult: cat(&["adult.com"], &[]),
            ..Filter::empty()
        };
        let allowed = set(&["bad.com", "adult.com", "x.com"]);
        for name in ["bad.com", "www.bad.com", "adult.com", "a.b.x.com"] {
            assert_eq!(f.decide_allowing(name, ALL, &allowed), None, "{name}");
            assert!(f.decide(name, ALL).is_some(), "{name}");
        }
        let one = set(&["ok.x.com"]);
        assert_eq!(f.decide_allowing("ok.x.com", ALL, &one), None);
        assert_eq!(f.decide_allowing("x.com", ALL, &one), Some(Kind::Ads));
        assert_eq!(f.decide_allowing("notok.x.com", ALL, &one), Some(Kind::Ads));
    }

    #[test]
    fn kind_positions_match_the_counters() {
        for (i, k) in Kind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
        }
        assert_eq!(serde_json::to_string(&Kind::Adult).unwrap(), "\"adult\"");
    }

    #[test]
    fn switch_off_means_none() {
        let f = Filter {
            ads: cat(&["x.com"], &[]),
            ..Filter::empty()
        };
        assert_eq!(f.decide("x.com", Switches::default()), None);
        assert_eq!(Filter::empty().decide("x.com", ALL), None);
    }
}
