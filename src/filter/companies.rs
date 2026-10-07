//! Which company is behind a blocked website, for the "Most blocked" list.

use super::activity::registrable_domain;

const COMPANIES: &[(&str, &[&str])] = &[
    (
        "Google",
        &[
            "2mdn.net",
            "admob.com",
            "app-measurement.com",
            "crashlytics.com",
            "doubleclick.net",
            "google-analytics.com",
            "google.com",
            "googleadservices.com",
            "googlesyndication.com",
            "googletagmanager.com",
            "googletagservices.com",
            "youtube.com",
        ],
    ),
    (
        "Meta",
        &[
            "facebook.com",
            "facebook.net",
            "fbcdn.net",
            "instagram.com",
            "whatsapp.net",
        ],
    ),
    ("Amazon", &["amazon-adsystem.com", "amazon.com"]),
    (
        "Microsoft",
        &[
            "bing.com",
            "clarity.ms",
            "microsoft.com",
            "msads.net",
            "msn.com",
            "windows.com",
        ],
    ),
    ("Apple", &["apple.com", "icloud.com"]),
    (
        "Yahoo",
        &["advertising.com", "oath.com", "yahoo.com", "yimg.com"],
    ),
    ("X", &["ads-twitter.com", "t.co", "twitter.com", "x.com"]),
    (
        "TikTok",
        &[
            "byteoversea.com",
            "bytedance.com",
            "ibytedtos.com",
            "tiktok.com",
            "tiktokv.com",
        ],
    ),
    ("Snap", &["sc-static.net", "snapchat.com"]),
    ("Pinterest", &["pinimg.com", "pinterest.com"]),
    ("LinkedIn", &["licdn.com", "linkedin.com"]),
    (
        "Adobe",
        &[
            "2o7.net",
            "adobedtm.com",
            "demdex.net",
            "everesttech.net",
            "omtrdc.net",
        ],
    ),
    (
        "Oracle",
        &["addthis.com", "bluekai.com", "eloqua.com", "moatads.com"],
    ),
    ("Criteo", &["criteo.com", "criteo.net"]),
    ("Taboola", &["taboola.com"]),
    ("Outbrain", &["outbrain.com"]),
    ("The Trade Desk", &["adsrvr.org"]),
    ("Magnite", &["magnite.com", "rubiconproject.com"]),
    ("PubMatic", &["pubmatic.com"]),
    ("OpenX", &["openx.net"]),
    ("Index Exchange", &["casalemedia.com"]),
    ("Media.net", &["media.net"]),
    ("Samsung", &["samsung.com", "samsungads.com"]),
    ("Xiaomi", &["miui.com", "xiaomi.com"]),
    ("Yandex", &["yandex.com", "yandex.net", "yandex.ru"]),
    ("Baidu", &["baidu.com"]),
    ("Tencent", &["qq.com", "tencent.com"]),
    ("Unity", &["unity3d.com"]),
    ("AppLovin", &["applovin.com", "applvn.com"]),
    ("ironSource", &["ironsrc.com", "supersonicads.com"]),
    ("Liftoff", &["vungle.com"]),
    ("Chartboost", &["chartboost.com"]),
    ("InMobi", &["inmobi.com"]),
    ("Mixpanel", &["mixpanel.com"]),
    ("Segment", &["segment.com", "segment.io"]),
    ("Hotjar", &["hotjar.com"]),
    ("Amplitude", &["amplitude.com"]),
    ("Quantcast", &["quantcount.com", "quantserve.com"]),
    ("comScore", &["scorecardresearch.com"]),
    ("Nielsen", &["imrworldwide.com"]),
    ("Adjust", &["adjust.com", "adjust.io"]),
    ("AppsFlyer", &["appsflyer.com"]),
    ("Branch", &["branch.io"]),
    ("Lotame", &["crwdcntrl.net"]),
    ("LiveRamp", &["liveramp.com", "rlcdn.com"]),
    ("Tapad", &["tapad.com"]),
];

pub fn company_of(site: &str) -> Option<&'static str> {
    let site = site.trim().trim_end_matches('.').to_ascii_lowercase();
    let domain = registrable_domain(&site);
    COMPANIES
        .iter()
        .find(|(_, domains)| domains.contains(&domain))
        .map(|(name, _)| *name)
}

pub fn display_name(site: &str) -> String {
    company_of(site)
        .map(str::to_string)
        .unwrap_or_else(|| site.trim().trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_sites_name_their_company() {
        assert_eq!(company_of("doubleclick.net"), Some("Google"));
        assert_eq!(company_of("googlesyndication.com"), Some("Google"));
        assert_eq!(company_of("google-analytics.com"), Some("Google"));
        assert_eq!(company_of("facebook.net"), Some("Meta"));
        assert_eq!(company_of("amazon-adsystem.com"), Some("Amazon"));
        assert_eq!(company_of("clarity.ms"), Some("Microsoft"));
        assert_eq!(company_of("bing.com"), Some("Microsoft"));
    }

    #[test]
    fn names_under_a_known_site_and_capitals_still_match() {
        assert_eq!(company_of("stats.g.doubleclick.net"), Some("Google"));
        assert_eq!(company_of("Pixel.Facebook.NET."), Some("Meta"));
    }

    #[test]
    fn unknown_sites_show_themselves() {
        assert_eq!(company_of("example.org"), None);
        assert_eq!(display_name("example.org"), "example.org");
        assert_eq!(display_name("Tracker.Example.org."), "tracker.example.org");
        assert_eq!(display_name("doubleclick.net"), "Google");
    }

    #[test]
    fn a_site_belongs_to_one_company_and_is_written_the_way_it_is_looked_up() {
        let mut seen = std::collections::HashSet::new();
        for (name, domains) in COMPANIES {
            assert!(!name.is_empty() && !name.contains('\u{2014}'));
            for d in *domains {
                assert_eq!(*d, d.to_ascii_lowercase(), "{d}");
                assert_eq!(registrable_domain(d), *d, "{d} is not a registrable name");
                assert!(seen.insert(*d), "{d} listed twice");
            }
        }
    }
}
