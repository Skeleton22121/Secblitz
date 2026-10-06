//! Forgiving search over short texts. Case, accents and punctuation do not
//! matter, and a word may be partial, have skipped letters or one typo.

/// How well one typed word matches a word of the text, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Typo,
    Subsequence,
    Substring,
    Prefix,
    Exact,
}

impl Tier {
    fn points(self) -> u32 {
        self as u32 + 1
    }
}

fn base(c: char) -> char {
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// The words of `text` in lower case without accents. Apostrophes join the
/// letters around them, every other symbol ends a word.
fn words_of(text: &str) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut word: Vec<char> = Vec::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            '\u{300}'..='\u{36f}' | '\'' | '\u{2019}' => {}
            'ß' => word.extend(['s', 's']),
            'æ' => word.extend(['a', 'e']),
            'œ' => word.extend(['o', 'e']),
            c if base(c).is_alphanumeric() => word.push(base(c)),
            _ => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Text prepared once so that every keystroke only compares words.
#[derive(Debug, Clone, Default)]
pub struct Haystack {
    words: Vec<Vec<char>>,
}

impl Haystack {
    pub fn new<I, S>(parts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut words: Vec<Vec<char>> = Vec::new();
        for part in parts {
            for word in words_of(part.as_ref()) {
                if !words.contains(&word) {
                    words.push(word);
                }
            }
        }
        Haystack { words }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    words: Vec<Vec<char>>,
}

impl Query {
    pub fn new(typed: &str) -> Self {
        Query {
            words: words_of(typed),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// An empty query matches everything.
    pub fn matches(&self, haystack: &Haystack) -> bool {
        self.score(haystack).is_some()
    }

    /// `None` when some typed word matches nothing. Otherwise a number that
    /// is higher the closer the text is to what was typed.
    pub fn score(&self, haystack: &Haystack) -> Option<u32> {
        self.words
            .iter()
            .map(|word| {
                haystack
                    .words
                    .iter()
                    .filter_map(|h| tier(word, h))
                    .max()
                    .map(Tier::points)
            })
            .sum()
    }
}

fn tier(typed: &[char], word: &[char]) -> Option<Tier> {
    if typed == word {
        Some(Tier::Exact)
    } else if word.starts_with(typed) {
        Some(Tier::Prefix)
    } else if word.windows(typed.len()).any(|w| w == typed) {
        Some(Tier::Substring)
    } else if typed.len() >= 3 && is_subsequence(typed, word) {
        Some(Tier::Subsequence)
    } else if typed.len() >= 4 && one_typo(typed, word) {
        Some(Tier::Typo)
    } else {
        None
    }
}

fn is_subsequence(typed: &[char], word: &[char]) -> bool {
    let mut rest = word.iter();
    typed.iter().all(|c| rest.any(|w| w == c))
}

/// One letter wrong, missing, extra or swapped with its neighbour.
fn one_typo(a: &[char], b: &[char]) -> bool {
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let Some(i) = (0..short.len()).find(|&i| short[i] != long[i]) else {
        return short.len() != long.len();
    };
    if short.len() < long.len() {
        return short[i..] == long[i + 1..];
    }
    short[i + 1..] == long[i + 1..]
        || (i + 1 < short.len()
            && short[i] == long[i + 1]
            && short[i + 1] == long[i]
            && short[i + 2..] == long[i + 2..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(typed: &str, text: &str) -> bool {
        Query::new(typed).matches(&Haystack::new([text]))
    }

    fn score(typed: &str, text: &str) -> Option<u32> {
        Query::new(typed).score(&Haystack::new([text]))
    }

    #[test]
    fn an_empty_query_matches_everything() {
        for typed in ["", "   ", "...", "' ’"] {
            let query = Query::new(typed);
            assert!(query.is_empty());
            assert!(query.matches(&Haystack::new(["Windows Firewall"])));
            assert!(query.matches(&Haystack::default()));
        }
        assert!(!Query::new("a").is_empty());
        assert!(!found("a", ""));
    }

    #[test]
    fn case_and_accents_do_not_matter() {
        assert!(found("FIREWALL", "Windows firewall"));
        assert!(found("cafe", "Café"));
        assert!(found("café", "CAFE"));
        assert!(found("uber", "Übersicht"));
        assert!(found("strasse", "Straße"));
        assert!(found("senal", "Señal"));
        assert!(found("coeur", "cœur"));
        assert!(found("protegido", "PROTEGIDO"));
        assert!(found("proteção", "protecao"));
    }

    #[test]
    fn punctuation_splits_words_but_apostrophes_do_not() {
        assert!(found("shares", "smb.shares_exposed"));
        assert!(found("smb exposed", "smb.shares_exposed"));
        assert!(found("doesnt", "It doesn't help"));
        assert!(found("don't", "dont"));
    }

    #[test]
    fn every_typed_word_has_to_match() {
        let text = "Turn on Windows Firewall for public networks";
        assert!(found("firewall public", text));
        assert!(found("public firewall", text));
        assert!(!found("firewall printers", text));
        assert!(!found("zzz", text));
    }

    #[test]
    fn a_word_may_be_part_of_a_word() {
        assert!(found("fire", "Firewall"));
        assert!(found("wall", "Firewall"));
        assert!(found("ewa", "Firewall"));
        assert!(found("x", "Xbox"));
    }

    #[test]
    fn skipped_letters_need_three_typed_letters() {
        assert!(found("fwl", "Firewall"));
        assert!(found("scrn", "Screen"));
        assert!(!found("fl", "Firewall x"));
        assert!(!found("lwf", "Firewall"));
    }

    #[test]
    fn one_typo_is_forgiven_from_four_letters() {
        assert!(found("firwall", "Firewall"));
        assert!(found("firewal", "Firewall"));
        assert!(found("fierwall", "Firewall"));
        assert!(found("fyrewall", "Firewall"));
        assert!(found("firewalll", "Firewall"));
        assert!(found("bluetoth", "Bluetooth"));
        assert!(!found("firxwxll", "Firewall"));
        assert!(
            !found("firxwxll", "Firewall x"),
            "two letters wrong is too many"
        );
        assert!(
            !found("camrae", "Camera"),
            "a letter moved two places is too many"
        );
        assert!(!found("qzx", "Firewall"));
    }

    #[test]
    fn closer_matches_score_higher() {
        let exact = score("camera", "Camera").unwrap();
        let prefix = score("cam", "Camera").unwrap();
        let inside = score("mer", "Camera").unwrap();
        let skipped = score("cmr", "Camera").unwrap();
        let typo = score("camrea", "Camera").unwrap();
        assert!(exact > prefix, "{exact} {prefix}");
        assert!(prefix > inside, "{prefix} {inside}");
        assert!(inside > skipped, "{inside} {skipped}");
        assert!(skipped > typo, "{skipped} {typo}");
        assert!(typo > 0);
        assert_eq!(score("zzz", "Camera"), None);
    }

    #[test]
    fn the_best_word_of_the_text_counts() {
        let haystack = Haystack::new(["Microphone access", "camera"]);
        let exact = Query::new("camera").score(&haystack).unwrap();
        let fuzzy = Query::new("cmr").score(&haystack).unwrap();
        assert!(exact > fuzzy);
        assert!(
            Query::new("camera microphone").score(&haystack).unwrap()
                > Query::new("camera mic").score(&haystack).unwrap()
        );
    }

    #[test]
    fn several_texts_can_be_searched_as_one() {
        let haystack = Haystack::new(["Nutzung der Kamera", "Camera access", "privacy.camera"]);
        assert!(Query::new("camera").matches(&haystack));
        assert!(!Query::new("kamera zugriff").matches(&haystack));
        assert!(Query::new("kamera access").matches(&haystack));
    }
}
