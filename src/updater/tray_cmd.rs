//! Pure classification of a Secblitz process command line as the tray agent.
//!
//! The updater ignores tray processes when deciding whether the installed app
//! is busy, but only when the command line is *exactly* `secblitz.exe tray`.
//! Anything else (extra arguments, other subcommands, unusual quoting) stays
//! busy. Kept portable so it is unit-tested on every host.

/// Split a Windows command line into arguments for the simple cases we accept.
/// Returns `None` for anything with escaped quotes or unbalanced quoting, so
/// callers fail closed.
fn split(line: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                // Backslashes before a quote change quoting semantics; refuse.
                current.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if started {
        args.push(current);
    }
    if args.iter().any(|a| a.ends_with('\\') && line.contains("\\\"")) {
        return None;
    }
    Some(args)
}

/// True only for `<exe> tray` (the executable token optionally quoted).
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn is_tray_command_line(line: &str) -> bool {
    matches!(split(line.trim_end_matches('\0')).as_deref(), Some([exe, mode])
        if !exe.is_empty() && mode == "tray")
}

#[cfg(test)]
mod tests {
    use super::is_tray_command_line as tray;

    #[test]
    fn exact_tray_mode_is_recognised() {
        assert!(tray(r#""C:\Program Files\Secblitz\secblitz.exe" tray"#));
        assert!(tray(r"C:\Secblitz\secblitz.exe tray"));
        assert!(tray("\"C:\\Program Files\\Secblitz\\secblitz.exe\"   tray  "));
        assert!(tray("\"C:\\Program Files\\Secblitz\\secblitz.exe\" tray\0"));
    }

    #[test]
    fn everything_else_stays_busy() {
        for line in [
            "",
            r#""C:\Program Files\Secblitz\secblitz.exe""#,
            r#""C:\Program Files\Secblitz\secblitz.exe" tray extra"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" tray --lang en"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" --lang en tray"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" gui"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" "tray extra""#,
            r#""C:\Program Files\Secblitz\secblitz.exe" tray""#,
            r#""C:\Program Files\Secblitz\secblitz.exe tray"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" trayx"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" TRAY"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" update install"#,
            r#"tray"#,
            r#"x\" tray"#,
        ] {
            assert!(!tray(line), "{line:?}");
        }
    }
}
