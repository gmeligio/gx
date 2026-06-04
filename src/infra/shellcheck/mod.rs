//! Shellcheck integration seam. The `run-shellcheck` lint rule depends on the
//! [`ShellChecker`] trait, not on `std::process::Command` directly, so the rule's logic
//! (expression sanitization, shell resolution, finding → diagnostic mapping) stays pure
//! and unit-testable with no `shellcheck` binary on `PATH`.
//!
//! - [`ShellcheckCli`] is the real adapter: it spawns `shellcheck` and parses its JSON.
//! - [`FakeChecker`] is the test double: it returns canned [`Finding`]s.
//! - [`Availability`] models "is the binary present" as a type, probed once per lint run,
//!   so a missing binary is a graceful skip rather than a hard error.

#![expect(
    clippy::pub_use,
    clippy::module_name_repetitions,
    reason = "reexport adapters to the lint rule; ShellcheckCli is the public adapter name"
)]

mod cli;
#[cfg(test)]
mod fake;

pub use cli::ShellcheckCli;
#[cfg(test)]
pub use fake::FakeChecker;

/// The shell dialect a `run:` body is analyzed as. Only the two POSIX-family shells
/// shellcheck understands are modeled; other shells (`pwsh`, `python`, ...) are skipped
/// by the rule before a checker is ever invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sh {
    Bash,
    Sh,
}

impl Sh {
    /// Parse an effective-shell token into a `Sh`. Returns `None` for non-POSIX shells,
    /// which the caller treats as "skip this step".
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "bash" => Some(Self::Bash),
            "sh" => Some(Self::Sh),
            _ => None,
        }
    }

    /// The `--shell` argument value.
    fn flag(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Sh => "sh",
        }
    }

    /// The runtime setup line GitHub prepends before running the body, mirrored so that
    /// masked-pipeline findings (e.g. SC2086 in a pipe) match real GitHub behavior.
    fn setup_line(self) -> &'static str {
        match self {
            Self::Bash => "set -eo pipefail",
            Self::Sh => "set -e",
        }
    }
}

/// shellcheck severity, as reported in its JSON `level` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Style,
}

/// A single shellcheck finding, normalized at the integration edge so rule logic never
/// touches the raw JSON. `line`/`column` are positions within the analyzed script (1-based),
/// already adjusted for the prepended setup line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The numeric part of the `SCxxxx` code (e.g. `2086`).
    pub code: u16,
    pub severity: Severity,
    pub line: i64,
    pub column: i64,
    pub message: String,
}

/// Static analyzer over a single shell script. Implemented by [`ShellcheckCli`] (real)
/// and `FakeChecker` (tests).
pub trait ShellChecker {
    /// Analyze `script` as `shell` and return all findings. An empty vec means "clean".
    /// Invocation errors are surfaced as an empty result by the CLI adapter — the rule
    /// degrades gracefully rather than failing the lint run.
    fn check(&self, script: &str, shell: Sh) -> Vec<Finding>;
}

/// Whether `shellcheck` is usable on this host, probed once per lint run. Keeping
/// "present" and "absent" as distinct variants (rather than an `Option` threaded
/// everywhere) lets the rule emit a single, explicit skip diagnostic when absent.
pub enum Availability {
    Present(ShellcheckCli),
    Absent,
}

impl Availability {
    /// Probe `PATH` for a runnable `shellcheck` once. Runs `shellcheck --version`; any
    /// spawn failure (not installed, not executable) resolves to `Absent`.
    #[must_use]
    pub fn probe() -> Self {
        if ShellcheckCli::is_available() {
            Self::Present(ShellcheckCli::new())
        } else {
            Self::Absent
        }
    }
}

/// Replace every `${{ ... }}` GitHub Actions expression in `script` with an equal-length
/// run of underscores, preserving byte length and therefore every column position after
/// the expression. Without this, `$` inside `${{ }}` collides with shell variable syntax
/// and shellcheck reports false positives on any workflow that interpolates expressions.
///
/// Ported from actionlint's `rule_shellcheck.go`. The closing `}}` is matched to the first
/// occurrence after the opener, matching actionlint and GitHub's own lexer. Newlines inside
/// an expression are preserved so line numbering is unaffected; every other byte of the
/// span (including multi-byte UTF-8) becomes one `_` per byte to keep byte-columns stable.
#[must_use]
pub fn sanitize_expressions(script: &str) -> String {
    // Collect the byte span of each complete `${{ ... }}` expression. An unterminated
    // opener contributes no span and is therefore copied through verbatim.
    let spans = expression_spans(script);
    if spans.is_empty() {
        return script.to_owned();
    }
    let mut out = String::with_capacity(script.len());
    let mut remaining = spans.into_iter().peekable();
    for (offset, ch) in script.char_indices() {
        // Drop any span we have already moved past so the current span is always at the front.
        while remaining.peek().is_some_and(|&(_, end)| offset >= end) {
            remaining.next();
        }
        if remaining
            .peek()
            .is_some_and(|&(start, end)| offset >= start && offset < end)
        {
            blank_char(&mut out, ch);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Byte spans `[start, end)` of every complete `${{ ... }}` expression, in order. Walks
/// `char_indices` so no byte indexing is needed; an unterminated `${{` yields no span.
fn expression_spans(script: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut iter = script.char_indices().peekable();
    while let Some((offset, ch)) = iter.next() {
        if ch != '$' {
            continue;
        }
        // Need two `{` immediately after.
        let mut probe = iter.clone();
        if probe.next().map(|(_, c)| c) != Some('{') || probe.next().map(|(_, c)| c) != Some('{') {
            continue;
        }
        // Scan forward for the first `}}`.
        if let Some(end) = find_close(&mut probe) {
            spans.push((offset, end));
            // Resume scanning after the close.
            iter = probe;
        }
    }
    spans
}

/// Given an iterator positioned just past `${{`, return the byte offset just after the
/// first `}}`, advancing the iterator to that point. Returns `None` (leaving the iterator
/// consumed) if no close is found.
fn find_close(
    iter: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Option<usize> {
    while let Some((_, ch)) = iter.next() {
        if ch == '}' && iter.peek().map(|&(_, c)| c) == Some('}') {
            let (close_offset, _) = iter.next()?;
            return close_offset.checked_add(1);
        }
    }
    None
}

/// Append the blanked form of one expression-interior char: a newline stays a newline so
/// line counts are preserved; anything else becomes one `_` per UTF-8 byte so byte-columns
/// after the expression are unchanged.
fn blank_char(out: &mut String, ch: char) {
    if ch == '\n' {
        out.push('\n');
    } else {
        for _ in 0..ch.len_utf8() {
            out.push('_');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_from_token() {
        assert_eq!(Sh::from_token("bash"), Some(Sh::Bash));
        assert_eq!(Sh::from_token("sh"), Some(Sh::Sh));
        assert_eq!(Sh::from_token("pwsh"), None);
        assert_eq!(Sh::from_token("python"), None);
    }

    #[test]
    fn sanitize_blanks_expression_preserving_columns() {
        let got = sanitize_expressions("echo '${{ matrix.os }}'");
        // The text after the expression keeps its position; the span is all underscores.
        assert_eq!(got, "echo '________________'");
        assert_eq!(got.len(), "echo '${{ matrix.os }}'".len());
    }

    #[test]
    fn sanitize_preserves_newlines_inside_expression() {
        let src = "echo ${{\n  github.sha\n}}";
        let got = sanitize_expressions(src);
        assert_eq!(got.len(), src.len());
        // Newlines survive so line numbering is unchanged.
        assert_eq!(got.matches('\n').count(), 2);
        assert!(got.starts_with("echo _"));
    }

    #[test]
    fn sanitize_leaves_plain_shell_untouched() {
        let src = "rm $RUNNER_TEMP/file";
        assert_eq!(sanitize_expressions(src), src);
    }

    #[test]
    fn sanitize_leaves_unterminated_expression_untouched() {
        let src = "echo ${{ broken";
        assert_eq!(sanitize_expressions(src), src);
    }

    #[test]
    fn sanitize_handles_multiple_expressions() {
        let got = sanitize_expressions("${{ a }}-${{ b }}");
        assert_eq!(got, "________-________");
    }
}
