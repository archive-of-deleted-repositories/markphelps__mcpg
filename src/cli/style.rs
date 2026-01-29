use console::{Emoji, style};

// Emoji with fallbacks for non-emoji terminals
pub static CHECK: Emoji<'_, '_> = Emoji("✓ ", "+ ");
#[allow(dead_code)]
pub static CROSS: Emoji<'_, '_> = Emoji("✗ ", "x ");
pub static ARROW: Emoji<'_, '_> = Emoji("→ ", "-> ");
pub static WARN: Emoji<'_, '_> = Emoji("⚠ ", "! ");

/// Format a success message (green with checkmark)
pub fn success(msg: &str) -> String {
    format!("{}{}", style(CHECK).green(), style(msg).green())
}

/// Format an error message (red with X)
#[allow(dead_code)]
pub fn error(msg: &str) -> String {
    format!("{}{}", style(CROSS).red(), style(msg).red().bold())
}

/// Format an info message (cyan with arrow)
pub fn info(msg: &str) -> String {
    format!("{}{}", style(ARROW).cyan(), msg)
}

/// Format a warning message (yellow with warning sign)
pub fn warning(msg: &str) -> String {
    format!("{}{}", style(WARN).yellow(), style(msg).yellow())
}

/// Format dimmed text (for secondary information)
pub fn dim(msg: &str) -> String {
    style(msg).dim().to_string()
}

/// Format a header/title (bold)
pub fn header(msg: &str) -> String {
    style(msg).bold().to_string()
}

/// Format a key in a key-value display (cyan)
pub fn key(msg: &str) -> String {
    style(msg).cyan().to_string()
}

/// Create a styled addition indicator
pub fn addition(msg: &str) -> String {
    format!("  {} {}", style("+").green(), msg)
}

/// Create a horizontal separator line
pub fn separator(width: usize) -> String {
    style("─".repeat(width.min(70))).dim().to_string()
}

/// Return the colorful theme for dialoguer prompts
pub fn theme() -> dialoguer::theme::ColorfulTheme {
    dialoguer::theme::ColorfulTheme::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_contains_message() {
        let result = success("test message");
        assert!(result.contains("test message"));
    }

    #[test]
    fn test_error_contains_message() {
        let result = error("error occurred");
        assert!(result.contains("error occurred"));
    }

    #[test]
    fn test_separator_respects_max_width() {
        let sep = separator(100);
        // Strip ANSI codes to check length
        let stripped = console::strip_ansi_codes(&sep);
        assert_eq!(stripped.chars().count(), 70);
    }

    #[test]
    fn test_separator_uses_provided_width_when_smaller() {
        let sep = separator(30);
        let stripped = console::strip_ansi_codes(&sep);
        assert_eq!(stripped.chars().count(), 30);
    }
}
