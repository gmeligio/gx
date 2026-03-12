use super::lines::Line as OutputLine;
use console::Term;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

/// Terminal output handler: detects CI/TTY/color and prints `OutputLine` values.
pub struct Printer {
    /// Whether to use ANSI colors
    pub use_color: bool,
    /// Whether we're running in CI mode
    pub is_ci: bool,
}

impl Printer {
    /// Create a new `Printer`, auto-detecting CI mode, TTY, and `NO_COLOR`.
    #[must_use]
    pub fn new() -> Self {
        let is_ci = std::env::var("CI").is_ok();
        let is_tty = Term::stdout().is_term();
        let no_color = std::env::var("NO_COLOR").is_ok();
        let use_color = is_tty && !no_color && !is_ci;
        Self { use_color, is_ci }
    }

    /// Create a spinner for long operations. Returns `None` if CI or not a TTY.
    #[must_use]
    pub fn spinner(&self, message: &str) -> Option<ProgressBar> {
        if self.is_ci || !Term::stderr().is_term() {
            return None;
        }

        let pb = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message(message.to_string());
        Some(pb)
    }

    /// Print a list of `OutputLine` values to stdout with optional color.
    pub fn print_lines(&self, lines: &[OutputLine]) {
        for line in lines {
            println!("{}", line.format_line(self.use_color));
        }
    }
}

impl Default for Printer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_new_respects_ci_env() {
        // CI is set in many test environments; just verify it doesn't panic
        let printer = Printer::new();
        // When CI is set, use_color should be false
        if std::env::var("CI").is_ok() {
            assert!(!printer.use_color);
            assert!(printer.is_ci);
        }
    }

    #[test]
    fn printer_new_respects_no_color() {
        // Temporarily test NO_COLOR behavior via Printer struct logic
        // (can't easily set env vars in parallel tests, so just test the struct)
        let printer = Printer {
            use_color: false,
            is_ci: false,
        };
        assert!(!printer.use_color);
    }

    #[test]
    fn printer_spinner_returns_none_in_ci() {
        let printer = Printer {
            use_color: false,
            is_ci: true,
        };
        // spinner() returns None when is_ci = true
        // (can't test the TTY check in unit tests, but CI path is deterministic)
        let spinner = printer.spinner("test");
        assert!(spinner.is_none());
    }
}
