//! Portable logic of the browser warning: which programs count as a browser, whether a tab shows
//! the blocked site, where the panel goes and which start-up words the panel accepts.
use secblitz::filter::config::normalized_site;
use secblitz::filter::matcher::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chromium,
    Firefox,
}

/// A block older than this is not worth a warning any more.
pub const NOTICE_FRESH_SECS: u64 = 10;

/// File names of the programs that get a warning. The match ignores case and the folder.
pub fn browser_of_image(path: &str) -> Option<Browser> {
    let name = path.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
    match name.as_str() {
        "msedge.exe" | "chrome.exe" | "brave.exe" => Some(Browser::Chromium),
        "firefox.exe" => Some(Browser::Firefox),
        _ => None,
    }
}

pub fn fresh(notice_at: u64, now: u64) -> bool {
    now.saturating_sub(notice_at) <= NOTICE_FRESH_SECS
}

/// The site name inside an address as a browser shows it, with or without scheme and path.
pub fn host_from_address(value: &str) -> Option<String> {
    let value = value.trim();
    let rest = match value.split_once("://") {
        Some((scheme, rest))
            if !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.')) =>
        {
            rest
        }
        _ => value,
    };
    let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let authority = authority.rsplit('@').next()?;
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|c| c.is_ascii_digit()) => host,
        Some(_) => return None,
        None => authority,
    };
    normalized_site(host)
}

/// A Chromium window that could not look a site up is titled "site - Browser" (or with the
/// profile name before the browser name).
pub fn host_from_title(title: &str) -> Option<String> {
    let first = title.split(" - ").next()?;
    host_from_address(first)
}

fn bare(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// `host` is what the tab shows; the blocked `site` may be its parent domain.
pub fn same_site(host: &str, site: &str) -> bool {
    host == site
        || bare(host) == bare(site)
        || host
            .strip_suffix(bare(site))
            .is_some_and(|front| front.ends_with('.'))
}

pub fn title_shows(title: &str, site: &str) -> bool {
    host_from_title(title).is_some_and(|host| same_site(&host, site))
}

pub fn address_shows(value: &str, site: &str) -> bool {
    host_from_address(value).is_some_and(|host| same_site(&host, site))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}

pub const PANEL_WIDTH: f32 = 420.0;
pub const PANEL_HEIGHT: f32 = 190.0;
/// Below the tabs and the toolbar of a browser.
pub const PANEL_DROP: f32 = 110.0;

pub fn scaled(logical: f32, dpi: u32) -> i32 {
    (logical * dpi.clamp(96, 960) as f32 / 96.0).round() as i32
}

/// The panel in pixels: centred over the browser window, a little below its top, and inside the
/// usable part of the screen even when the browser is half off it.
pub fn place(browser: Rect, work: Rect, dpi: u32) -> Rect {
    let width = scaled(PANEL_WIDTH, dpi).min(work.width()).max(1);
    let height = scaled(PANEL_HEIGHT, dpi).min(work.height()).max(1);
    let wanted_left = browser.left + (browser.width() - width) / 2;
    let wanted_top = browser.top + scaled(PANEL_DROP, dpi);
    let left = wanted_left.min(work.right - width).max(work.left);
    let top = wanted_top.min(work.bottom - height).max(work.top);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarnArgs {
    pub kind: Kind,
    pub site: String,
    pub window: usize,
}

pub fn kind_word(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Dangerous => Some("dangerous"),
        Kind::Scam => Some("scam"),
        _ => None,
    }
}

/// Everything the panel is told on its command line, checked again on the other side.
pub fn parse_args(kind: &str, site: &str, window: &str) -> Option<WarnArgs> {
    let kind = match kind {
        "dangerous" => Kind::Dangerous,
        "scam" => Kind::Scam,
        _ => return None,
    };
    let site = normalized_site(site).filter(|clean| clean == site)?;
    let window = window
        .bytes()
        .all(|c| c.is_ascii_digit())
        .then(|| window.parse::<usize>().ok())
        .flatten()
        .filter(|w| *w != 0)?;
    Some(WarnArgs { kind, site, window })
}

pub fn start_arguments(kind: Kind, site: &str, window: usize) -> Option<Vec<String>> {
    Some(vec![
        "warn".into(),
        "--kind".into(),
        kind_word(kind)?.into(),
        "--site".into(),
        site.into(),
        "--window".into(),
        window.to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_four_browsers_count() {
        for (path, want) in [
            (
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                Some(Browser::Chromium),
            ),
            (r"C:\x\CHROME.EXE", Some(Browser::Chromium)),
            (r"brave.exe", Some(Browser::Chromium)),
            (
                r"C:\Program Files\Mozilla Firefox\firefox.exe",
                Some(Browser::Firefox),
            ),
            (r"C:\x\notepad.exe", None),
            (r"C:\msedge.exe\notepad.exe", None),
            (r"C:\x\chrome.exe.bak", None),
            ("", None),
        ] {
            assert_eq!(browser_of_image(path), want, "{path}");
        }
    }

    #[test]
    fn titles_from_chromium_browsers_give_the_site() {
        for title in [
            "accountgiveaway.com - Google Chrome",
            "accountgiveaway.com - Profile 1 - Microsoft Edge",
            "accountgiveaway.com - Brave",
            "ACCOUNTGIVEAWAY.COM - Google Chrome",
            "accountgiveaway.com - Microsoft\u{200b}Edge",
            "http://accountgiveaway.com/ - Google Chrome",
            "accountgiveaway.com. - Google Chrome",
            "accountgiveaway.com - Google Chrome - Persona\u{e7}",
        ] {
            assert_eq!(
                host_from_title(title).as_deref(),
                Some("accountgiveaway.com"),
                "{title}"
            );
        }
        assert_eq!(
            host_from_title("accountgiveaway.com").as_deref(),
            Some("accountgiveaway.com")
        );
    }

    #[test]
    fn titles_of_ordinary_pages_give_no_site() {
        for title in [
            "",
            " - Google Chrome",
            "New Tab - Google Chrome",
            "Sign in to your account - Microsoft Edge",
            "Inbox (3) - Google Chrome",
            "Problem loading page \u{2014} Mozilla Firefox",
            "Neuer Tab - Brave",
        ] {
            assert_eq!(host_from_title(title), None, "{title}");
        }
    }

    #[test]
    fn addresses_give_the_site_with_or_without_scheme_path_and_port() {
        for value in [
            "accountgiveaway.com",
            "https://accountgiveaway.com",
            "https://accountgiveaway.com/",
            "HTTPS://AccountGiveaway.com/login?x=1#top",
            "accountgiveaway.com:8080/path",
            "http://user:pass@accountgiveaway.com:80/",
            "  accountgiveaway.com.  ",
            "accountgiveaway.com?x=1",
            r"https://accountgiveaway.com\path",
        ] {
            assert_eq!(
                host_from_address(value).as_deref(),
                Some("accountgiveaway.com"),
                "{value}"
            );
        }
    }

    #[test]
    fn addresses_that_name_another_host_are_not_mistaken() {
        assert_eq!(
            host_from_address("https://accountgiveaway.com@evil.example/").as_deref(),
            Some("evil.example")
        );
        assert_eq!(
            host_from_address("https://evil.example/accountgiveaway.com").as_deref(),
            Some("evil.example")
        );
        assert_eq!(
            host_from_address("https://evil.example?accountgiveaway.com").as_deref(),
            Some("evil.example")
        );
        for value in [
            "",
            "https://",
            "localhost",
            "[::1]:80",
            "about:blank",
            "search words here",
            "host:notaport",
            "xn--caf-dma.example\u{e9}",
        ] {
            assert_eq!(host_from_address(value), None, "{value}");
        }
    }

    #[test]
    fn punycode_names_stay_as_they_are() {
        assert_eq!(
            host_from_address("https://xn--caf-dma.example/").as_deref(),
            Some("xn--caf-dma.example")
        );
        assert!(address_shows("xn--caf-dma.example", "xn--caf-dma.example"));
        assert!(!address_shows("caf\u{e9}.example", "xn--caf-dma.example"));
    }

    #[test]
    fn a_tab_shows_a_site_when_names_match_apart_from_one_www() {
        assert!(title_shows("evil.example - Google Chrome", "evil.example"));
        assert!(title_shows(
            "www.evil.example - Google Chrome",
            "evil.example"
        ));
        assert!(title_shows(
            "evil.example - Google Chrome",
            "www.evil.example"
        ));
        assert!(address_shows("https://www.evil.example/a", "evil.example"));
        assert!(!title_shows("evil.example - Google Chrome", "good.example"));
        assert!(title_shows(
            "login.evil.example - Google Chrome",
            "evil.example"
        ));
        assert!(address_shows("https://a.b.evil.example/x", "evil.example"));
        assert!(!title_shows(
            "evil.example - Google Chrome",
            "login.evil.example"
        ));
        assert!(!address_shows("https://notevil.example", "evil.example"));
        assert!(!address_shows("", "evil.example"));
    }

    #[test]
    fn only_a_fresh_block_gets_a_warning() {
        assert!(fresh(100, 100));
        assert!(fresh(100, 110));
        assert!(!fresh(100, 111));
        assert!(fresh(110, 100), "a clock a little behind still counts");
    }

    fn screen() -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }

    #[test]
    fn the_panel_is_centred_below_the_toolbar() {
        let browser = Rect {
            left: 100,
            top: 50,
            right: 1100,
            bottom: 850,
        };
        let panel = place(browser, screen(), 96);
        assert_eq!((panel.width(), panel.height()), (420, 190));
        assert_eq!(panel.left, 100 + (1000 - 420) / 2);
        assert_eq!(panel.top, 50 + 110);
    }

    #[test]
    fn the_panel_grows_with_the_screen_scale() {
        let browser = Rect {
            left: 0,
            top: 0,
            right: 1800,
            bottom: 1000,
        };
        let panel = place(browser, screen(), 144);
        assert_eq!((panel.width(), panel.height()), (630, 285));
        assert_eq!(panel.top, 165);
        assert_eq!(scaled(10.0, 0), 10, "a missing scale counts as 100%");
    }

    #[test]
    fn the_panel_stays_inside_the_screen() {
        let low = Rect {
            left: 1700,
            top: 900,
            right: 2700,
            bottom: 1700,
        };
        let panel = place(low, screen(), 96);
        assert_eq!(panel.right, 1920);
        assert_eq!(panel.bottom, 1040);
        let left = Rect {
            left: -900,
            top: -200,
            right: 100,
            bottom: 600,
        };
        let panel = place(left, screen(), 96);
        assert_eq!(panel.left, 0);
        assert_eq!(panel.top, 0);
        let tiny = Rect {
            left: 0,
            top: 0,
            right: 300,
            bottom: 150,
        };
        let panel = place(tiny, tiny, 96);
        assert_eq!(panel, tiny);
    }

    #[test]
    fn the_start_words_are_checked() {
        let ok = parse_args("scam", "evil.example", "1234").unwrap();
        assert_eq!(
            ok,
            WarnArgs {
                kind: Kind::Scam,
                site: "evil.example".into(),
                window: 1234
            }
        );
        assert_eq!(
            parse_args("dangerous", "a-b.example", "7").unwrap().kind,
            Kind::Dangerous
        );
        for (kind, site, window) in [
            ("ads", "evil.example", "1"),
            ("", "evil.example", "1"),
            ("scam", "Evil.example", "1"),
            ("scam", "evil.example.", "1"),
            ("scam", "evil.example/path", "1"),
            ("scam", "https://evil.example", "1"),
            ("scam", "evil", "1"),
            ("scam", "evil.example", "0"),
            ("scam", "evil.example", "-4"),
            ("scam", "evil.example", "0x10"),
            ("scam", "evil.example", ""),
            ("scam", "evil.example", "99999999999999999999999"),
        ] {
            assert_eq!(
                parse_args(kind, site, window),
                None,
                "{kind} {site} {window}"
            );
        }
    }

    #[test]
    fn the_tray_and_the_panel_agree_on_the_start_words() {
        for kind in [Kind::Dangerous, Kind::Scam] {
            let words = start_arguments(kind, "evil.example", 42).unwrap();
            assert_eq!(words[0], "warn");
            assert_eq!(words[1], "--kind");
            assert_eq!(words[3], "--site");
            assert_eq!(words[5], "--window");
            assert_eq!(
                parse_args(&words[2], &words[4], &words[6]),
                Some(WarnArgs {
                    kind,
                    site: "evil.example".into(),
                    window: 42
                })
            );
        }
        assert_eq!(start_arguments(Kind::Ads, "evil.example", 1), None);
    }
}
