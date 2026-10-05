//! Web protection switches.
use super::Explainer;

pub(super) fn get(id: &str) -> Option<Explainer> {
    Some(match id {
        "web.ads" => Explainer {
            what: "Most ads come from a known list of advertising websites. This stops your PC from loading anything from that list.",
            risk: "Ads slow pages down, follow you from site to site, and sometimes lead to scam or virus websites.",
            change: "Many ads disappear from websites and apps. Some pages look a little different, and a few sites may ask you to turn this off.",
        },
        "web.tracking" => Explainer {
            what: "Many websites, apps and Windows itself quietly send data about what you do to companies that collect it.",
            risk: "Your habits can be collected, sold and used to show you ads built around you.",
            change: "Those requests are stopped. Everything should work as before, but a few pages that rely on them may load oddly.",
        },
        "web.dangerous" => Explainer {
            what: "Secblitz keeps a list of known scam and virus websites and updates it often.",
            risk: "A link in an email or message could take you to a fake bank page or a site that infects your PC.",
            change: "Your PC can't open websites on that list. If a safe site is blocked by mistake, pause web protection for an hour.",
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_switch_is_explained() {
        for id in ["web.ads", "web.tracking", "web.dangerous"] {
            assert!(get(id).is_some(), "{id}");
            assert!(crate::explain::for_check(id).is_some(), "{id}");
        }
        assert!(get("web.other").is_none());
    }
}
