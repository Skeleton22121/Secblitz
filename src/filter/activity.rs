//! What the filter blocked lately. The service writes the files; only administrators can read them.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::config::{
    read_capped, BlockHistory, DayCount, RecentItem, RecentList, TopSite, MAX_RECENT_ITEMS,
    RECENT_SECONDS, STATS_DAYS, TOP_SITES,
};
use super::matcher::Kind;

const SECONDS_PER_DAY: u64 = 86_400;
const SITES_TODAY: usize = 2000;
const SITES_PER_DAY: usize = 100;
const RECENT_EVERY: u64 = 2;
const STATS_EVERY: u64 = 10;
const DETAIL_EVERY: u64 = 60;

const SECOND_LEVEL: [&str; 14] = [
    "co", "com", "net", "org", "gov", "edu", "ac", "or", "ne", "go", "gob", "mil", "nom", "ltd",
];

/// `shop.example.co.uk` gives `example.co.uk`.
pub fn registrable_domain(name: &str) -> &str {
    let labels: Vec<&str> = name.split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return name;
    }
    let (second, last) = (labels[n - 2], labels[n - 1]);
    let keep =
        if last.len() == 2 && (2..=3).contains(&second.len()) && SECOND_LEVEL.contains(&second) {
            3
        } else {
            2
        };
    let skip: usize = labels[..n - keep].iter().map(|l| l.len() + 1).sum();
    &name[skip..]
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Detail {
    #[serde(default)]
    pub days: Vec<DetailDay>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct DetailDay {
    pub day: u64,
    pub blocked: [u64; 5],
    pub sites: Vec<(String, u64)>,
}

#[derive(Default)]
struct Day {
    blocked: [u64; 5],
    sites: HashMap<String, u64>,
}

impl Day {
    fn add(&mut self, kind: Kind, site: &str) {
        self.blocked[kind.index()] += 1;
        if let Some(n) = self.sites.get_mut(site) {
            *n += 1;
            return;
        }
        if self.sites.len() >= SITES_TODAY {
            let least = self
                .sites
                .iter()
                .min_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(k, _)| k.clone());
            if let Some(least) = least {
                self.sites.remove(&least);
            }
        }
        self.sites.insert(site.to_string(), 1);
    }

    fn top(&self, limit: usize) -> Vec<(String, u64)> {
        let mut all: Vec<(String, u64)> = self.sites.iter().map(|(k, v)| (k.clone(), *v)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        all.truncate(limit);
        all
    }

    fn compact(&mut self) {
        if self.sites.len() > SITES_PER_DAY {
            self.sites = self.top(SITES_PER_DAY).into_iter().collect();
        }
    }
}

#[derive(Default)]
struct Recent {
    items: Vec<RecentItem>,
    dirty: bool,
    written: u64,
}

#[derive(Default)]
struct History {
    days: BTreeMap<u64, Day>,
    dirty: bool,
    stats_written: u64,
    detail_dirty: bool,
    detail_written: u64,
}

impl History {
    fn roll(&mut self, today: u64) {
        let oldest = today.saturating_sub(STATS_DAYS - 1);
        let before = self.days.len();
        self.days.retain(|d, _| *d >= oldest);
        for (d, day) in self.days.iter_mut() {
            if *d != today {
                day.compact();
            }
        }
        if self.days.len() != before {
            self.dirty = true;
            self.detail_dirty = true;
        }
    }
}

#[derive(Default)]
pub struct Activity {
    recent: Mutex<Recent>,
    history: Mutex<History>,
}

#[derive(Default, Debug, PartialEq)]
pub struct Writes {
    pub recent: Option<RecentList>,
    pub stats: Option<BlockHistory>,
    pub detail: Option<Detail>,
}

const MAX_DETAIL: u64 = 2 * 1024 * 1024;

pub fn load_detail(path: &Path) -> Detail {
    read_capped(path, MAX_DETAIL)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Activity {
    pub fn resume(&self, detail: Detail, now: u64) {
        let mut h = locked(&self.history);
        for d in detail.days {
            let mut day = Day {
                blocked: d.blocked,
                sites: HashMap::new(),
            };
            for (site, n) in d.sites.into_iter().take(SITES_PER_DAY) {
                day.sites.insert(site, n);
            }
            h.days.insert(d.day, day);
        }
        h.roll(now / SECONDS_PER_DAY);
        h.dirty = !h.days.is_empty();
    }

    pub fn record(&self, name: &str, kind: Kind, now: u64) {
        {
            let mut r = locked(&self.recent);
            r.items
                .retain(|i| i.name != name && now.saturating_sub(i.at) <= RECENT_SECONDS);
            r.items.insert(
                0,
                RecentItem {
                    name: name.to_string(),
                    kind,
                    at: now,
                },
            );
            r.items.truncate(MAX_RECENT_ITEMS);
            r.dirty = true;
        }
        let today = now / SECONDS_PER_DAY;
        let mut h = locked(&self.history);
        let new_day = !h.days.contains_key(&today);
        h.days
            .entry(today)
            .or_default()
            .add(kind, registrable_domain(name));
        if new_day {
            h.roll(today);
        }
        h.dirty = true;
        h.detail_dirty = true;
    }

    /// `force` writes everything changed at once (service stop).
    pub fn take_writes(&self, now: u64, force: bool) -> Writes {
        let mut out = Writes::default();
        {
            let mut r = locked(&self.recent);
            let before = r.items.len();
            r.items
                .retain(|i| now.saturating_sub(i.at) <= RECENT_SECONDS);
            if r.items.len() != before {
                r.dirty = true;
            }
            if r.dirty && (force || now.saturating_sub(r.written) >= RECENT_EVERY) {
                r.dirty = false;
                r.written = now;
                out.recent = Some(RecentList {
                    items: r.items.clone(),
                });
            }
        }
        let mut h = locked(&self.history);
        let today = now / SECONDS_PER_DAY;
        h.roll(today);
        if h.dirty && (force || now.saturating_sub(h.stats_written) >= STATS_EVERY) {
            h.dirty = false;
            h.stats_written = now;
            out.stats = Some(history_of(&h));
        }
        if h.detail_dirty && (force || now.saturating_sub(h.detail_written) >= DETAIL_EVERY) {
            h.detail_dirty = false;
            h.detail_written = now;
            out.detail = Some(detail_of(&h));
        }
        out
    }

    pub fn recent_now(&self, now: u64) -> RecentList {
        let r = locked(&self.recent);
        RecentList {
            items: r
                .items
                .iter()
                .filter(|i| now.saturating_sub(i.at) <= RECENT_SECONDS)
                .cloned()
                .collect(),
        }
    }

    pub fn history_now(&self, now: u64) -> BlockHistory {
        let mut h = locked(&self.history);
        h.roll(now / SECONDS_PER_DAY);
        history_of(&h)
    }
}

fn history_of(h: &History) -> BlockHistory {
    let days = h
        .days
        .iter()
        .filter(|(_, d)| d.blocked.iter().any(|n| *n > 0))
        .map(|(day, d)| DayCount {
            day: *day,
            blocked: d.blocked,
        })
        .collect();
    let mut total: HashMap<&str, u64> = HashMap::new();
    for day in h.days.values() {
        for (site, n) in &day.sites {
            *total.entry(site).or_default() += n;
        }
    }
    let mut top: Vec<TopSite> = total
        .into_iter()
        .map(|(site, count)| TopSite {
            site: site.to_string(),
            count,
        })
        .collect();
    top.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.site.cmp(&b.site)));
    top.truncate(TOP_SITES);
    BlockHistory { days, top }
}

fn detail_of(h: &History) -> Detail {
    Detail {
        days: h
            .days
            .iter()
            .map(|(day, d)| DetailDay {
                day: *day,
                blocked: d.blocked,
                sites: d.top(SITES_PER_DAY),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = SECONDS_PER_DAY;

    #[test]
    fn registrable_domains() {
        for (name, want) in [
            ("ads.example.com", "example.com"),
            ("example.com", "example.com"),
            ("a.b.c.example.org", "example.org"),
            ("shop.example.co.uk", "example.co.uk"),
            ("example.co.uk", "example.co.uk"),
            ("tracker.example.com.au", "example.com.au"),
            ("x.y.example.co.jp", "example.co.jp"),
            ("www.ab.de", "ab.de"),
            ("cdn.example.io", "example.io"),
            ("localhost", "localhost"),
            ("", ""),
        ] {
            assert_eq!(registrable_domain(name), want, "{name}");
        }
    }

    fn names(list: &RecentList) -> Vec<&str> {
        list.items.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn recent_is_newest_first_without_repeats() {
        let a = Activity::default();
        a.record("a.example", Kind::Ads, 1000);
        a.record("b.example", Kind::Tracking, 1001);
        a.record("a.example", Kind::Ads, 1002);
        let list = a.recent_now(1003);
        assert_eq!(names(&list), ["a.example", "b.example"]);
        assert_eq!(list.items[0].at, 1002);
        assert_eq!(list.items[1].kind, Kind::Tracking);
    }

    #[test]
    fn recent_keeps_fifty_and_fifteen_minutes() {
        let a = Activity::default();
        for i in 0..60 {
            a.record(&format!("n{i}.example"), Kind::Ads, 1000 + i);
        }
        let list = a.recent_now(1100);
        assert_eq!(list.items.len(), MAX_RECENT_ITEMS);
        assert_eq!(list.items[0].name, "n59.example");
        assert_eq!(list.items[49].name, "n10.example");
        assert_eq!(a.recent_now(1059 + RECENT_SECONDS).items.len(), 1);
        assert!(a.recent_now(1059 + RECENT_SECONDS + 1).items.is_empty());
    }

    #[test]
    fn recent_write_waits_two_seconds_and_drops_old_items() {
        let a = Activity::default();
        a.record("a.example", Kind::Ads, 1000);
        let first = a.take_writes(1000, false);
        assert_eq!(names(&first.recent.unwrap()), ["a.example"]);
        a.record("b.example", Kind::Ads, 1001);
        assert!(a.take_writes(1001, false).recent.is_none());
        let second = a.take_writes(1002, false);
        assert_eq!(names(&second.recent.unwrap()), ["b.example", "a.example"]);
        assert!(a.take_writes(1003, false).recent.is_none());
        let later = a.take_writes(1002 + RECENT_SECONDS + 1, false);
        assert_eq!(later.recent, Some(RecentList::default()));
    }

    #[test]
    fn force_writes_what_changed_at_once() {
        let a = Activity::default();
        a.record("a.example", Kind::Ads, 5 * DAY);
        a.take_writes(5 * DAY, false);
        a.record("b.example", Kind::Adult, 5 * DAY + 1);
        let w = a.take_writes(5 * DAY + 1, true);
        assert!(w.recent.is_some() && w.stats.is_some() && w.detail.is_some());
        assert_eq!(a.take_writes(5 * DAY + 2, true), Writes::default());
    }

    #[test]
    fn stats_count_per_day_and_per_kind() {
        let a = Activity::default();
        let d0 = 100 * DAY;
        a.record("ads.example.com", Kind::Ads, d0 + 5);
        a.record("t.example.com", Kind::Tracking, d0 + 6);
        a.record("x.other.org", Kind::Dangerous, d0 + DAY + 1);
        a.record("y.other.org", Kind::Gambling, d0 + DAY + 2);
        a.record("z.other.org", Kind::Adult, d0 + DAY + 3);
        let h = a.history_now(d0 + DAY + 10);
        assert_eq!(
            h.days,
            [
                DayCount {
                    day: 100,
                    blocked: [1, 1, 0, 0, 0]
                },
                DayCount {
                    day: 101,
                    blocked: [0, 0, 1, 1, 1]
                }
            ]
        );
        assert_eq!(
            h.top[0],
            TopSite {
                site: "other.org".into(),
                count: 3
            }
        );
        assert_eq!(
            h.top[1],
            TopSite {
                site: "example.com".into(),
                count: 2
            }
        );
    }

    #[test]
    fn stats_roll_over_after_thirty_days() {
        let a = Activity::default();
        a.record("a.example.com", Kind::Ads, 10 * DAY);
        a.record("b.example.net", Kind::Ads, 39 * DAY);
        let h = a.history_now(39 * DAY + 1);
        assert_eq!(h.days.iter().map(|d| d.day).collect::<Vec<_>>(), [10, 39]);
        // Day 10 is the 30th day back and still counts; day 9 would not.
        let h = a.history_now(40 * DAY);
        assert_eq!(h.days.iter().map(|d| d.day).collect::<Vec<_>>(), [39]);
        assert_eq!(h.top.len(), 1);
        assert_eq!(h.top[0].site, "example.net");
    }

    #[test]
    fn top_is_ten_sites_most_blocked_first() {
        let a = Activity::default();
        for i in 0..15 {
            for _ in 0..=i {
                a.record(&format!("x.site{i:02}.com"), Kind::Ads, 50 * DAY);
            }
        }
        let h = a.history_now(50 * DAY);
        assert_eq!(h.top.len(), TOP_SITES);
        assert_eq!(
            h.top[0],
            TopSite {
                site: "site14.com".into(),
                count: 15
            }
        );
        assert_eq!(h.top[9].site, "site05.com");
    }

    #[test]
    fn per_day_site_map_is_capped_and_evicts_the_smallest() {
        let a = Activity::default();
        let now = 60 * DAY;
        for _ in 0..50 {
            a.record("busy.example.com", Kind::Ads, now);
        }
        for i in 0..(SITES_TODAY + 500) {
            a.record(&format!("s{i}.n{i}.com"), Kind::Ads, now);
        }
        {
            let h = locked(&a.history);
            assert!(h.days[&60].sites.len() <= SITES_TODAY);
        }
        let h = a.history_now(now);
        assert_eq!(
            h.top[0],
            TopSite {
                site: "example.com".into(),
                count: 50
            }
        );
        assert_eq!(h.days[0].blocked[0], 50 + (SITES_TODAY as u64 + 500));
    }

    #[test]
    fn earlier_days_are_trimmed_and_saved_files_stay_small() {
        let a = Activity::default();
        for i in 0..300 {
            a.record(&format!("a.n{i}.com"), Kind::Ads, 70 * DAY);
        }
        a.record("b.example.com", Kind::Ads, 71 * DAY);
        let w = a.take_writes(71 * DAY + 100, true);
        let detail = w.detail.unwrap();
        assert_eq!(detail.days.len(), 2);
        assert!(detail.days[0].sites.len() <= SITES_PER_DAY);
        assert_eq!(detail.days[0].blocked[0], 300);
        assert!(serde_json::to_vec(&w.stats.unwrap()).unwrap().len() < 4096);
    }

    #[test]
    fn counts_survive_a_restart() {
        let a = Activity::default();
        let now = 80 * DAY + 50;
        a.record("a.example.com", Kind::Ads, now);
        a.record("a.example.com", Kind::Ads, now + 1);
        a.record("g.bet.com", Kind::Gambling, now + 2);
        let saved = a.take_writes(now + 100, true).detail.unwrap();
        let json = serde_json::to_string(&saved).unwrap();
        let b = Activity::default();
        b.resume(serde_json::from_str(&json).unwrap(), now + 200);
        b.record("a.example.com", Kind::Ads, now + 201);
        let h = b.history_now(now + 202);
        assert_eq!(h.days[0].blocked, [3, 0, 0, 0, 1]);
        assert_eq!(
            h.top[0],
            TopSite {
                site: "example.com".into(),
                count: 3
            }
        );
        let c = Activity::default();
        c.resume(serde_json::from_str(&json).unwrap(), now + 40 * DAY);
        assert!(c.history_now(now + 40 * DAY).days.is_empty());
    }
}
