//! Text from other programs (PowerShell errors and the like), made safe to show.

/// Strips control and invisible formatting characters (bidi overrides, zero-width marks) so error text can't reorder what people read.
pub fn excerpt(raw: &str, max: usize) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control() && !is_invisible_format(*c))
        .take(max)
        .collect()
}

fn is_invisible_format(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{FEFF}'
        | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206F}'
        | '\u{FFF9}'..='\u{FFFB}'
        | '\u{E0001}' | '\u{E0020}'..='\u{E007F}')
}

#[cfg(test)]
mod tests {
    use super::excerpt;

    #[test]
    fn keeps_one_plain_line() {
        assert_eq!(
            excerpt("  Access\r\n  is\tdenied.\u{0007}  ", 300),
            "Access is denied."
        );
    }

    #[test]
    fn drops_characters_that_reorder_or_hide_text() {
        let spoofed = "update\u{202E}exe.fdp\u{202C} failed\u{200B}\u{2066}!\u{2069}\u{FEFF}";
        assert_eq!(excerpt(spoofed, 300), "updateexe.fdp failed!");
    }

    #[test]
    fn caps_the_length_by_characters() {
        assert_eq!(excerpt(&"é".repeat(500), 300).chars().count(), 300);
        assert_eq!(excerpt("", 300), "");
    }
}
