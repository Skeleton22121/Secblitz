//! WinGet app updates, run by the unelevated launcher for a short allowlist of apps.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    pub id: &'static str,
    pub name: &'static str,
}

pub const APPS: [App; 13] = [
    App {
        id: "Google.Chrome",
        name: "Google Chrome",
    },
    App {
        id: "Mozilla.Firefox",
        name: "Firefox",
    },
    App {
        id: "Brave.Brave",
        name: "Brave",
    },
    App {
        id: "Opera.Opera",
        name: "Opera",
    },
    App {
        id: "Vivaldi.Vivaldi",
        name: "Vivaldi",
    },
    App {
        id: "Oracle.JavaRuntimeEnvironment",
        name: "Java",
    },
    App {
        id: "Adobe.Acrobat.Reader.64-bit",
        name: "Adobe Acrobat Reader",
    },
    App {
        id: "7zip.7zip",
        name: "7-Zip",
    },
    App {
        id: "RARLab.WinRAR",
        name: "WinRAR",
    },
    App {
        id: "Zoom.Zoom",
        name: "Zoom",
    },
    App {
        id: "VideoLAN.VLC",
        name: "VLC media player",
    },
    App {
        id: "Notepad++.Notepad++",
        name: "Notepad++",
    },
    App {
        id: "Adobe.Acrobat.Reader.32-bit",
        name: "Adobe Acrobat Reader (32-bit)",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Available,
    NothingToDo,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    Apps([AppState; APPS.len()]),
    Unreadable,
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 8 && t.bytes().all(|b| b == b'-')
}

fn is_noise(line: &str) -> bool {
    line.trim()
        .chars()
        .all(|c| matches!(c, '-' | '\\' | '|' | '/' | ' ' | '█' | '▒'))
}

pub fn parse_upgrades(output: &str, exit: Option<u32>) -> Scan {
    let normalised = output.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalised.lines().collect();
    let Some(rule) = lines.iter().position(|l| is_rule(l)) else {
        let words = lines.iter().filter(|l| !is_noise(l)).count();
        return if exit == Some(0) && (1..=3).contains(&words) {
            Scan::Apps([AppState::NothingToDo; APPS.len()])
        } else {
            Scan::Unreadable
        };
    };
    if rule == 0 || is_noise(lines[rule - 1]) {
        return Scan::Unreadable;
    }
    let mut states = [AppState::NothingToDo; APPS.len()];
    for line in &lines[rule + 1..] {
        for token in line.split_whitespace() {
            for (i, app) in APPS.iter().enumerate() {
                if token.eq_ignore_ascii_case(app.id) {
                    states[i] = AppState::Available;
                } else if states[i] != AppState::Available {
                    let cut = token
                        .strip_suffix('…')
                        .or_else(|| token.strip_suffix("..."));
                    if let Some(prefix) = cut {
                        if prefix.len() >= 3
                            && app
                                .id
                                .get(..prefix.len())
                                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
                        {
                            states[i] = AppState::Unknown;
                        }
                    }
                }
            }
        }
    }
    Scan::Apps(states)
}

static LAST: Mutex<Option<[AppState; APPS.len()]>> = Mutex::new(None);

pub fn remember(states: [AppState; APPS.len()]) {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(states);
}

pub fn forget() {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn remembered(index: usize) -> Option<AppState> {
    LAST.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|s| s.get(index).copied())
}

pub fn list_args() -> Vec<String> {
    [
        "upgrade",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn upgrade_args(index: usize) -> Option<Vec<String>> {
    let app = APPS.get(index)?;
    Some(
        [
            "upgrade",
            "--id",
            app.id,
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    )
}

#[cfg(windows)]
pub use run::run_winget;

#[cfg(windows)]
mod run {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    pub struct WingetRun {
        pub code: Option<u32>,
        pub output: String,
    }

    pub fn run_winget(args: &[String], limit: Duration) -> WingetRun {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(winget) = secblitz::tools::winget_path() else {
            return WingetRun {
                code: None,
                output: String::new(),
            };
        };
        let spawned = Command::new(winget)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            return WingetRun {
                code: None,
                output: String::new(),
            };
        };
        let reader = child.stdout.take().map(|mut out| {
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                let mut chunk = [0u8; 8192];
                while let Ok(n) = out.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let room = (256 * 1024usize).saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                }
                kept
            })
        });
        let deadline = Instant::now() + limit;
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code().map(|c| c as u32),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(250))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let bytes = reader.and_then(|h| h.join().ok()).unwrap_or_default();
        WingetRun {
            code,
            output: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str =
        "Name                  Id                         Version   Available Source\r\n\
-----------------------------------------------------------------------------\r\n\
Google Chrome         Google.Chrome              120.0.1   121.0.2   winget\r\n\
7-Zip                 7zip.7zip                  23.01     24.08     winget\r\n\
Some Other App        Vendor.Other               1.0       2.0       winget\r\n\
2 upgrades available.\r\n";

    fn states(scan: Scan) -> [AppState; APPS.len()] {
        match scan {
            Scan::Apps(s) => s,
            Scan::Unreadable => panic!("unreadable"),
        }
    }

    #[test]
    fn allowlist_ids_are_unique_plain_and_cover_the_research_list() {
        let mut seen = std::collections::HashSet::new();
        for app in APPS {
            assert!(seen.insert(app.id.to_ascii_lowercase()), "{}", app.id);
            assert!(
                app.id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')),
                "{}",
                app.id
            );
            assert!(app.id.contains('.') && !app.name.is_empty());
        }
        for needle in [
            "Google.Chrome",
            "Mozilla.Firefox",
            "Oracle.JavaRuntimeEnvironment",
            "Adobe.Acrobat.Reader.64-bit",
            "7zip.7zip",
            "RARLab.WinRAR",
            "Zoom.Zoom",
            "VideoLAN.VLC",
            "Notepad++.Notepad++",
        ] {
            assert!(APPS.iter().any(|a| a.id == needle), "{needle}");
        }
    }

    #[test]
    fn finds_allowlisted_rows_and_ignores_the_rest() {
        let s = states(parse_upgrades(TABLE, Some(0)));
        assert_eq!(s[0], AppState::Available);
        assert_eq!(s[7], AppState::Available);
        assert_eq!(s.iter().filter(|x| **x == AppState::Available).count(), 2);
        assert_eq!(s[1], AppState::NothingToDo);
    }

    #[test]
    fn matching_is_exact_per_word_not_substring() {
        let text = "Name Id Version Available Source\n-----------\nFoo Vendor.Google.Chrome.Beta 1 2 winget\nBar Zoom.ZoomPlugin 1 2 winget\n";
        let s = states(parse_upgrades(text, Some(0)));
        assert!(s.iter().all(|x| *x == AppState::NothingToDo));
    }

    #[test]
    fn cut_off_ids_are_unknown_never_current() {
        let text = "Name Id Version Available Source\n-----------\nJava Oracle.JavaRuntimeEnv… 1 2 winget\n";
        let s = states(parse_upgrades(text, Some(0)));
        assert_eq!(s[5], AppState::Unknown);
        assert_eq!(s[0], AppState::NothingToDo);
        let ascii =
            "Name Id Version Available Source\n-----------\nJava Oracle.Java... 1 2 winget\n";
        assert_eq!(states(parse_upgrades(ascii, Some(0)))[5], AppState::Unknown);
        let stub = "Name Id Version Available Source\n-----------\nX G… 1 2 winget\n";
        assert_eq!(
            states(parse_upgrades(stub, Some(0)))[0],
            AppState::NothingToDo
        );
    }

    #[test]
    fn spinner_noise_before_the_table_is_ignored() {
        let text = "   - \r   \\ \r   | \r\nName Id Version Available Source\r\n-----------\r\nZoom Zoom.Zoom 1 2 winget\r\n";
        assert_eq!(
            states(parse_upgrades(text, Some(0)))[9],
            AppState::Available
        );
    }

    #[test]
    fn short_sentence_with_success_means_nothing_to_update() {
        let s = states(parse_upgrades(
            "No installed package found matching input criteria.\n",
            Some(0),
        ));
        assert!(s.iter().all(|x| *x == AppState::NothingToDo));
    }

    #[test]
    fn doubt_is_unreadable() {
        for (text, code) in [
            ("", Some(0)),
            ("", None),
            (
                "No installed package found matching input criteria.\n",
                Some(1),
            ),
            (
                "No installed package found matching input criteria.\n",
                None,
            ),
            ("one\ntwo\nthree\nfour\n", Some(0)),
            ("-----------\nGoogle.Chrome\n", Some(0)),
            ("   -\n-----------\nGoogle.Chrome\n", Some(0)),
        ] {
            assert_eq!(
                parse_upgrades(text, code),
                Scan::Unreadable,
                "{text:?} {code:?}"
            );
        }
    }

    #[test]
    fn upgrade_arguments_are_fixed_and_exact() {
        let args = upgrade_args(0).unwrap();
        assert_eq!(&args[..4], ["upgrade", "--id", "Google.Chrome", "--exact"]);
        assert!(args.contains(&"--silent".to_string()));
        assert!(args.contains(&"--accept-package-agreements".to_string()));
        assert!(args.contains(&"--accept-source-agreements".to_string()));
        assert!(!args.iter().any(|a| a.contains(' ')));
        assert!(upgrade_args(APPS.len()).is_none());
        assert!(list_args().contains(&"upgrade".to_string()));
    }

    #[test]
    fn remembered_scan_is_per_index_and_clearable() {
        forget();
        assert_eq!(remembered(0), None);
        let mut s = [AppState::NothingToDo; APPS.len()];
        s[3] = AppState::Available;
        remember(s);
        assert_eq!(remembered(3), Some(AppState::Available));
        assert_eq!(remembered(0), Some(AppState::NothingToDo));
        assert_eq!(remembered(APPS.len()), None);
        forget();
        assert_eq!(remembered(3), None);
    }
}
