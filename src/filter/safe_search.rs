//! Safe search: search sites answer with their safe versions, like a CNAME filter.

/// Published at <https://www.google.com/supported_domains>: `google.` plus one of these.
const GOOGLE_SUFFIXES: &str = concat!(
    "com ad ae com.af com.ag al am co.ao com.ar as at com.au az ba com.bd be ",
    "bf bg com.bh bi bj com.bn com.bo com.br bs bt co.bw by com.bz ca cd cf ",
    "cg ch ci co.ck cl cm cn com.co co.cr com.cu cv com.cy cz de dj dk dm ",
    "com.do dz com.ec ee com.eg es com.et fi com.fj fm fr ga ge gg com.gh ",
    "com.gi gl gm gr com.gt gy com.hk hn hr ht hu co.id ie co.il im co.in iq ",
    "is it je com.jm jo co.jp co.ke com.kh ki kg co.kr com.kw kz la com.lb ",
    "li lk co.ls lt lu lv com.ly co.ma md me mg mk ml com.mm mn com.mt mu mv ",
    "mw com.mx com.my co.mz com.na com.ng com.ni ne nl no com.np nr nu co.nz ",
    "com.om com.pa com.pe com.pg com.ph com.pk pl pn com.pr ps pt com.py ",
    "com.qa ro ru rw com.sa com.sb sc se com.sg sh si sk com.sl sn so sm sr ",
    "st com.sv td tg co.th com.tj tl tm tn to com.tr tt com.tw co.tz com.ua ",
    "co.ug co.uk com.uy co.uz com.vc co.ve co.vi com.vn vu ws rs co.za co.zm ",
    "co.zw cat ",
);

const GOOGLE: &str = "forcesafesearch.google.com";
const BING: &str = "strict.bing.com";
const YOUTUBE: &str = "restrictmoderate.youtube.com";
const DUCKDUCKGO: &str = "safe.duckduckgo.com";

const BING_NAMES: [&str; 2] = ["www.bing.com", "bing.com"];
const YOUTUBE_NAMES: [&str; 5] = [
    "www.youtube.com",
    "m.youtube.com",
    "youtubei.googleapis.com",
    "youtube.googleapis.com",
    "www.youtube-nocookie.com",
];
const DUCKDUCKGO_NAMES: [&str; 2] = ["duckduckgo.com", "www.duckduckgo.com"];

fn google_search_domain(name: &str) -> bool {
    let name = name.strip_prefix("www.").unwrap_or(name);
    let Some(suffix) = name.strip_prefix("google.") else {
        return false;
    };
    GOOGLE_SUFFIXES.split_whitespace().any(|s| s == suffix)
}

/// `name` is lowercase without a trailing dot, as the DNS parser hands it over.
pub fn target_for(name: &str) -> Option<&'static str> {
    if BING_NAMES.contains(&name) {
        Some(BING)
    } else if YOUTUBE_NAMES.contains(&name) {
        Some(YOUTUBE)
    } else if DUCKDUCKGO_NAMES.contains(&name) {
        Some(DUCKDUCKGO)
    } else if google_search_domain(name) {
        Some(GOOGLE)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::lists::valid_hostname;

    #[test]
    fn google_search_domains_in_every_country() {
        for name in [
            "google.com",
            "www.google.com",
            "google.co.uk",
            "www.google.com.au",
            "www.google.de",
            "google.cat",
            "www.google.co.jp",
        ] {
            assert_eq!(target_for(name), Some(GOOGLE), "{name}");
        }
        assert!(GOOGLE_SUFFIXES.split_whitespace().count() > 150);
    }

    #[test]
    fn other_google_names_are_left_alone() {
        for name in [
            "mail.google.com",
            "accounts.google.com",
            "maps.google.com",
            "google.example.com",
            "www.google.zz",
            "www.www.google.com",
            "notgoogle.com",
            "google.com.evil.example",
            "www.googleapis.com",
            "",
        ] {
            assert_eq!(target_for(name), None, "{name}");
        }
    }

    #[test]
    fn bing_youtube_and_duckduckgo() {
        assert_eq!(target_for("www.bing.com"), Some(BING));
        assert_eq!(target_for("bing.com"), Some(BING));
        assert_eq!(target_for("www.youtube.com"), Some(YOUTUBE));
        assert_eq!(target_for("m.youtube.com"), Some(YOUTUBE));
        assert_eq!(target_for("youtubei.googleapis.com"), Some(YOUTUBE));
        assert_eq!(target_for("youtube.googleapis.com"), Some(YOUTUBE));
        assert_eq!(target_for("www.youtube-nocookie.com"), Some(YOUTUBE));
        assert_eq!(target_for("duckduckgo.com"), Some(DUCKDUCKGO));
        assert_eq!(target_for("www.duckduckgo.com"), Some(DUCKDUCKGO));
        assert_eq!(target_for("youtube.com"), None);
        assert_eq!(target_for("music.youtube.com"), None);
        assert_eq!(target_for("images.bing.com"), None);
        assert_eq!(target_for("safe.duckduckgo.com"), None);
    }

    #[test]
    fn targets_are_themselves_never_rewritten() {
        for target in [GOOGLE, BING, YOUTUBE, DUCKDUCKGO] {
            assert!(valid_hostname(target));
            assert_eq!(target_for(target), None, "{target}");
        }
    }
}
