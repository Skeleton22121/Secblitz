//! Takes the personal parts out of text before it goes into a support file.

#[derive(Debug, Default, Clone)]
pub struct Redactor {
    user: Option<Vec<char>>,
    computer: Option<Vec<char>>,
}

fn chars_of(name: &str) -> Option<Vec<char>> {
    let name = name.trim();
    (name.chars().count() >= 2).then(|| name.chars().collect())
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn matches_at(text: &[char], at: usize, needle: &[char]) -> bool {
    text.len() >= at + needle.len() && needle.iter().zip(&text[at..]).all(|(a, b)| same(*a, *b))
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn ends_segment(c: char, stop_at_space: bool) -> bool {
    matches!(
        c,
        '\\' | '/' | '"' | '\'' | '<' | '>' | '|' | '*' | '?' | '\n' | '\r' | '\t'
    ) || (stop_at_space && c.is_whitespace())
}

const SHARED_FOLDERS: [&str; 4] = ["public", "default", "all users", "default user"];

impl Redactor {
    pub fn new(user: &str, computer: &str) -> Self {
        Redactor {
            user: chars_of(user),
            computer: chars_of(computer),
        }
    }

    pub fn clean(&self, text: &str) -> String {
        let text: Vec<char> = text.chars().collect();
        let text = self.profile_paths(&sids(&text));
        let text = replace_name(&text, self.computer.as_deref(), "[computer]");
        replace_name(&text, self.user.as_deref(), "[user]")
            .into_iter()
            .collect()
    }

    fn profile_paths(&self, text: &[char]) -> Vec<char> {
        let marker: Vec<char> = r"\users\".chars().collect();
        let slash_marker: Vec<char> = "/users/".chars().collect();
        let mut out = Vec::with_capacity(text.len());
        let mut i = 0;
        while i < text.len() {
            let drive = i >= 2 && text[i - 1] == ':' && text[i - 2].is_ascii_alphabetic();
            let hit = drive && (matches_at(text, i, &marker) || matches_at(text, i, &slash_marker));
            if !hit {
                out.push(text[i]);
                i += 1;
                continue;
            }
            let start = i + marker.len();
            let known = self
                .user
                .as_deref()
                .filter(|u| matches_at(text, start, u))
                .filter(|u| {
                    text.get(start + u.len())
                        .is_none_or(|c| ends_segment(*c, false))
                });
            let shared = SHARED_FOLDERS.iter().find_map(|name| {
                let name: Vec<char> = name.chars().collect();
                (matches_at(text, start, &name)
                    && text
                        .get(start + name.len())
                        .is_none_or(|c| ends_segment(*c, false)))
                .then_some(name.len())
            });
            let end = match (known, shared) {
                (Some(u), _) => start + u.len(),
                (None, Some(len)) => start + len,
                (None, None) => (start..text.len())
                    .find(|&j| ends_segment(text[j], true))
                    .unwrap_or(text.len()),
            };
            let folder: String = text[start..end].iter().collect::<String>().to_lowercase();
            out.extend(&text[i..start]);
            if end == start || SHARED_FOLDERS.contains(&folder.as_str()) {
                out.extend(&text[start..end]);
            } else {
                out.extend("[user]".chars());
            }
            i = end;
        }
        out
    }
}

fn sids(text: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let starts = matches!(text[i], 'S' | 's')
            && text.get(i + 1) == Some(&'-')
            && text.get(i + 2).is_some_and(char::is_ascii_digit)
            && (i == 0 || !word_char(text[i - 1]));
        if starts {
            let mut j = i + 1;
            let mut groups = 0;
            while text.get(j) == Some(&'-') && text.get(j + 1).is_some_and(char::is_ascii_digit) {
                j += 1;
                while text.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                groups += 1;
            }
            if groups >= 3 && text.get(j).is_none_or(|c| !word_char(*c)) {
                out.extend("[sid]".chars());
                i = j;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

fn replace_name(text: &[char], name: Option<&[char]>, with: &str) -> Vec<char> {
    let Some(name) = name else {
        return text.to_vec();
    };
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let whole = matches_at(text, i, name)
            && (i == 0 || !word_char(text[i - 1]))
            && text.get(i + name.len()).is_none_or(|c| !word_char(*c));
        if whole {
            out.extend(with.chars());
            i += name.len();
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    out
}

const FILE_ENDINGS: [&str; 14] = [
    "json", "jsonl", "txt", "exe", "dll", "zip", "log", "ps1", "tmp", "msi", "dat", "db", "cfg",
    "ini",
];

/// Replaces anything that looks like a website name, such as `shop.example.com`.
pub fn hide_sites(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if looks_like_site(word) {
            let core = word.trim_matches(['.', '-']);
            let start = word.len() - word.trim_start_matches(['.', '-']).len();
            out.push_str(&word[..start]);
            out.push_str("[site]");
            out.push_str(&word[start + core.len()..]);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || matches!(c, '.' | '-') {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn looks_like_site(word: &str) -> bool {
    let word = word.trim_matches(['.', '-']);
    let labels: Vec<&str> = word.split('.').collect();
    let Some(last) = labels.last() else {
        return false;
    };
    labels.len() >= 2
        && labels
            .iter()
            .all(|l| !l.is_empty() && !l.starts_with('-') && !l.ends_with('-'))
        && last.chars().count() >= 2
        && last.chars().all(char::is_alphabetic)
        && !FILE_ENDINGS.contains(&last.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Redactor {
        Redactor::new("Maria Lopez", "MARIAS-PC")
    }

    #[test]
    fn profile_folders_lose_the_account_name() {
        let r = r();
        assert_eq!(
            r.clean(r"C:\Users\Maria Lopez\AppData\Local\x.json"),
            r"C:\Users\[user]\AppData\Local\x.json"
        );
        assert_eq!(r.clean(r"c:\users\maria lopez"), r"c:\users\[user]");
        assert_eq!(
            r.clean(r"D:\USERS\jdoe\Desktop"),
            r"D:\USERS\[user]\Desktop"
        );
        assert_eq!(r.clean("C:/Users/jdoe/Desktop"), "C:/Users/[user]/Desktop");
        assert_eq!(
            r.clean(r"\\?\C:\Users\john.DESKTOP-1\x"),
            r"\\?\C:\Users\[user]\x"
        );
        assert_eq!(
            r.clean(r#"could not open "C:\Users\bob\file.txt" twice"#),
            r#"could not open "C:\Users\[user]\file.txt" twice"#
        );
    }

    #[test]
    fn an_unknown_account_name_stops_at_the_next_space() {
        assert_eq!(
            Redactor::default().clean(r"No access to C:\Users\bob today"),
            r"No access to C:\Users\[user] today"
        );
    }

    #[test]
    fn shared_profile_folders_are_not_personal() {
        let r = r();
        assert_eq!(r.clean(r"C:\Users\Public\x"), r"C:\Users\Public\x");
        assert_eq!(r.clean(r"C:\Users\Default\x"), r"C:\Users\Default\x");
        assert_eq!(r.clean(r"C:\Users\All Users\x"), r"C:\Users\All Users\x");
        assert_eq!(r.clean(r"C:\Users\"), r"C:\Users\");
    }

    #[test]
    fn account_ids_are_replaced() {
        let r = r();
        assert_eq!(
            r.clean("user S-1-5-21-1004336348-1177238915-682003330-1001 ok"),
            "user [sid] ok"
        );
        assert_eq!(r.clean("(S-1-5-21-1-2-3)"), "([sid])");
        assert_eq!(r.clean("s-1-5-21-1-2-3-500"), "[sid]");
        assert_eq!(r.clean("S-1-5-21-1-2-3-500\nnext"), "[sid]\nnext");
        assert_eq!(r.clean("MS-1-5-21-1-2-3"), "MS-1-5-21-1-2-3");
        assert_eq!(r.clean("S-1-5-21-1-2-3x"), "S-1-5-21-1-2-3x");
        assert_eq!(r.clean("S-1-5"), "S-1-5");
    }

    #[test]
    fn the_computer_and_user_names_are_replaced_as_whole_words() {
        let r = r();
        assert_eq!(
            r.clean("host marias-pc owned by maria lopez."),
            "host [computer] owned by [user]."
        );
        assert_eq!(r.clean(r"\\MARIAS-PC\share"), r"\\[computer]\share");
        assert_eq!(r.clean("MARIAS-PCS"), "MARIAS-PCS");
        assert_eq!(r.clean("xMARIAS-PC"), "xMARIAS-PC");
    }

    #[test]
    fn very_short_names_are_not_used_for_matching() {
        let r = Redactor::new("a", "");
        assert_eq!(r.clean("a cat sat"), "a cat sat");
    }

    #[test]
    fn text_without_personal_parts_is_unchanged() {
        let r = r();
        for text in [
            "Secblitz 1.0.0 on Windows 11 Pro 24H2 (26100)",
            "defender.realtime: protected",
            "Microsoft.BingNews_8wekyb3d8bbwe",
            "ünïcode İstanbul ok",
            "",
        ] {
            assert_eq!(r.clean(text), text);
        }
    }

    #[test]
    fn site_names_are_hidden_but_file_names_and_versions_stay() {
        assert_eq!(
            hide_sites("Could not reach shop.example.com and ads.tracker.net."),
            "Could not reach [site] and [site]."
        );
        assert_eq!(
            hide_sites("Visit https://evil.example/path now"),
            "Visit https://[site]/path now"
        );
        assert_eq!(
            hide_sites("Version 1.0.0 in gui-prefs.json and checks.jsonl, unins000.exe"),
            "Version 1.0.0 in gui-prefs.json and checks.jsonl, unins000.exe"
        );
        assert_eq!(
            hide_sites("Port in use. No lists."),
            "Port in use. No lists."
        );
    }
}
