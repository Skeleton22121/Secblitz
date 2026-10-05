//! Suffix matching over sorted 64-bit name hashes: 8 bytes per domain, a lookup
//! is a handful of binary searches.

/// FNV-1a 64 over the lowercase bytes of `name`.
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

    /// From hashes that were computed elsewhere (any order, duplicates fine).
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Ads,
    Tracking,
    Dangerous,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Switches {
    pub ads: bool,
    pub tracking: bool,
    pub dangerous: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub ads: Category,
    pub tracking: Category,
    pub dangerous: Category,
    pub never: HashSet64,
}

impl Filter {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Which enabled switch blocks `name`, if any. The never-block set wins over
    /// everything; otherwise Dangerous, then Tracking, then Ads.
    pub fn decide(&self, name: &str, on: Switches) -> Option<Kind> {
        if self.never.any_suffix(name) {
            return None;
        }
        if on.dangerous && self.dangerous.blocks(name) {
            return Some(Kind::Dangerous);
        }
        if on.tracking && self.tracking.blocks(name) {
            return Some(Kind::Tracking);
        }
        if on.ads && self.ads.blocks(name) {
            return Some(Kind::Ads);
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
            never: HashSet64::default(),
        };
        assert_eq!(f.decide("x.com", ALL), Some(Kind::Dangerous));
        let no_danger = Switches {
            dangerous: false,
            ..ALL
        };
        assert_eq!(f.decide("x.com", no_danger), Some(Kind::Tracking));
        let ads_only = Switches {
            ads: true,
            ..Switches::default()
        };
        assert_eq!(f.decide("x.com", ads_only), Some(Kind::Ads));
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
