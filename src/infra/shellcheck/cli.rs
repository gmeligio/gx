//! The real [`ShellChecker`] adapter: spawns the `shellcheck` binary and parses its
//! `-f json` output into [`Finding`]s. This is the only place in gx that runs an external
//! process, so all process I/O and JSON shape live here behind the trait.

use super::{Finding, Severity, Sh, ShellChecker};
use serde::Deserialize;
use std::io::Write as _;
use std::process::{Command, Stdio};

/// shellcheck codes excluded on every invocation. These fire as artifacts of gx's
/// preprocessing rather than real issues in the workflow author's script:
///
/// - `SC1091` source-not-found — gx never resolves `source`d files in the CI env.
/// - `SC2050`/`SC2157` constant-expression / always-false `-z` — the `${{ }}` → `____`
///   substitution turns interpolated conditionals into literals.
/// - `SC2153`/`SC2154` possible-misspelling / referenced-but-not-assigned — `run:` blocks
///   read variables defined in `env:`/`${{ }}`, which shellcheck cannot see.
/// - `SC2194` word-is-constant `case` — another underscore-substitution artifact.
/// - `SC2043` `for` loops once — fires on single-item lists that were `${{ }}` expansions.
///
/// Matches actionlint's exclusion set.
const EXCLUDED_CODES: &str = "SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043";

/// Adapter that runs the system `shellcheck`.
pub struct ShellcheckCli {
    /// The binary name/path to invoke. Always `"shellcheck"` today; a field so a future
    /// config could point at a pinned path without touching call sites.
    program: String,
}

impl ShellcheckCli {
    /// Construct an adapter targeting `shellcheck` on `PATH`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            program: "shellcheck".to_owned(),
        }
    }

    /// True if `shellcheck --version` runs. Used by `Availability::probe` to decide
    /// whether the rule can run at all.
    #[must_use]
    pub fn is_available() -> bool {
        Command::new("shellcheck")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Spawn shellcheck, feed `input` on stdin, and capture stdout. Returns `None` on any
    /// spawn or I/O error.
    fn run(&self, input: &str, shell: Sh) -> Option<String> {
        let mut child = Command::new(&self.program)
            .args([
                "--norc",
                "-f",
                "json",
                "-x",
                "--shell",
                shell.flag(),
                "-e",
                EXCLUDED_CODES,
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        child.stdin.take()?.write_all(input.as_bytes()).ok()?;
        let output = child.wait_with_output().ok()?;
        // shellcheck exits 1 when it finds issues; that is success for our purposes. Only
        // a failure to produce parseable stdout matters, handled by the parser.
        String::from_utf8(output.stdout).ok()
    }
}

impl Default for ShellcheckCli {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellChecker for ShellcheckCli {
    fn check(&self, script: &str, shell: Sh) -> Vec<Finding> {
        // Prepend the runtime setup line GitHub uses, so masked-pipeline findings match
        // real behavior. Reported lines are then offset by -1 to undo the prepend.
        let mut input = String::with_capacity(script.len().saturating_add(20));
        input.push_str(shell.setup_line());
        input.push('\n');
        input.push_str(script);

        let Some(stdout) = self.run(&input, shell) else {
            // Spawn/IO failure → behave as "no findings" so a transient error never fails
            // the lint run. Binary-absent is handled earlier via `Availability`.
            return Vec::new();
        };

        parse_findings(&stdout)
    }
}

/// One entry of shellcheck's `-f json` array. Only the fields gx maps are deserialized;
/// the rest (`endLine`, `endColumn`, `fix`, ...) are ignored.
#[derive(Debug, Deserialize)]
struct RawComment {
    /// 1-based line within the script fed on stdin (includes the prepended setup line).
    line: i64,
    /// 1-based column within the line.
    column: i64,
    /// Severity string: `error`/`warning`/`info`/`style`.
    level: String,
    /// Numeric `SCxxxx` code without the `SC` prefix.
    code: u16,
    /// Human-readable description of the finding.
    message: String,
}

/// Parse shellcheck's JSON array into findings, offsetting line numbers by -1 to undo the
/// prepended setup line. A line that maps to 0 or below (i.e. a finding on the injected
/// `set -e` line itself) is clamped to 1. Unparseable output yields no findings.
fn parse_findings(stdout: &str) -> Vec<Finding> {
    let Ok(raw) = serde_json::from_str::<Vec<RawComment>>(stdout) else {
        return Vec::new();
    };
    raw.into_iter()
        .map(|c| Finding {
            code: c.code,
            severity: parse_severity(&c.level),
            line: c.line.saturating_sub(1).max(1),
            column: c.column,
            message: c.message,
        })
        .collect()
}

/// Map shellcheck's `level` string to [`Severity`]. Unknown values fall back to `Info`.
fn parse_severity(level: &str) -> Severity {
    match level {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        "style" => Severity::Style,
        _ => Severity::Info,
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "tests index into findings freely")]
mod tests {
    use super::*;

    // A captured `shellcheck --norc -f json --shell bash -` sample for two SC2086 findings.
    const SAMPLE: &str = r#"[{"file":"-","line":2,"endLine":2,"column":6,"endColumn":10,"level":"info","code":2086,"message":"Double quote to prevent globbing and word splitting."},{"file":"-","line":3,"endLine":3,"column":4,"endColumn":8,"level":"warning","code":2086,"message":"Double quote to prevent globbing and word splitting."}]"#;

    #[test]
    fn parses_sample_offsetting_lines() {
        let findings = parse_findings(SAMPLE);
        assert_eq!(findings.len(), 2);
        // line 2 in the prepended script → line 1 of the user's body.
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].code, 2086);
        assert_eq!(findings[0].severity, Severity::Info);
        assert_eq!(findings[0].column, 6);
        assert_eq!(findings[1].line, 2);
        assert_eq!(findings[1].severity, Severity::Warning);
    }

    #[test]
    fn empty_array_is_clean() {
        assert!(parse_findings("[]").is_empty());
    }

    #[test]
    fn garbage_output_yields_no_findings() {
        assert!(parse_findings("not json").is_empty());
        assert!(parse_findings("").is_empty());
    }

    #[test]
    fn finding_on_setup_line_is_clamped_to_one() {
        let raw = r#"[{"file":"-","line":1,"column":1,"level":"error","code":2148,"message":"x"}]"#;
        let findings = parse_findings(raw);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn severity_mapping() {
        assert_eq!(parse_severity("error"), Severity::Error);
        assert_eq!(parse_severity("warning"), Severity::Warning);
        assert_eq!(parse_severity("style"), Severity::Style);
        assert_eq!(parse_severity("info"), Severity::Info);
        assert_eq!(parse_severity("mystery"), Severity::Info);
    }
}
