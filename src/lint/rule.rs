//! Lint's rule identity, the `Rule` trait, and the shared checking context. The
//! diagnostic record, ignore matchers, and report aggregation are general and live in
//! [`crate::diagnostic`]; this module supplies only the lint-specific half.

use crate::command::CommandReport;
use crate::config::{Level, Lint as LintConfig};
use crate::diagnostic::{matches_ignore_action, matches_ignore_workflow};
use crate::domain::file::actions::{ActionSet as WorkflowActionSet, Located as LocatedAction};
use crate::domain::file::parsed::ParsedWorkflow;
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::output::lines::Line as OutputLine;

pub use crate::diagnostic::{Diagnostic, Report, RuleName};

/// Text shown when a lint run found nothing.
const NO_ISSUES: &str = "No lint issues found";

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if self.diagnostics.is_empty() {
            return vec![OutputLine::Summary {
                text: NO_ISSUES.to_owned(),
            }];
        }

        let mut lines: Vec<OutputLine> = self
            .diagnostics
            .iter()
            .map(|diag| OutputLine::LintDiag {
                level: diag.level,
                workflow: diag.workflow.as_ref().map(std::string::ToString::to_string),
                line: diag.line,
                rule: diag.rule.to_string(),
                message: diag.message.clone(),
            })
            .collect();

        lines.push(OutputLine::Blank);
        lines.push(OutputLine::Summary {
            text: self.summary(NO_ISSUES),
        });

        lines
    }

    fn exit_code(&self) -> i32 {
        Self::exit_code(self)
    }
}

/// Context shared by all lint rules during checking.
pub struct Context<'ctx> {
    /// The manifest (gx.toml).
    pub manifest: &'ctx Manifest,
    /// The lock file (gx.lock).
    pub lock: &'ctx Lock,
    /// All located actions from scanned workflows.
    pub workflows: &'ctx [LocatedAction],
    /// Structural per-workflow parses, consumed by the workflow-security and
    /// workflow-validity rules.
    ///
    /// Workflow files only — enforced by the type, not by a filter. [`ParsedWorkflow`] is
    /// the workflow variant of [`Parsed`](crate::domain::file::parsed::Parsed), so an
    /// action definition cannot reach this field: it has no `on:`, no top-level
    /// `permissions:`, and no jobs, and every rule reading this would misjudge it. Their
    /// `uses:` references still reach the action-hygiene rules (sha-mismatch, unpinned,
    /// stale-comment, unsynced-manifest) through `workflows` and `action_set`.
    ///
    /// Empty when no workflows were scanned.
    pub workflows_full: &'ctx [ParsedWorkflow],
    /// Aggregated action set from all workflows.
    pub action_set: &'ctx WorkflowActionSet,
}

/// Trait for a lint rule.
pub trait Rule {
    /// Returns the rule's name.
    fn name(&self) -> RuleName;

    /// Returns this rule's default severity level.
    fn default_level(&self) -> Level;

    /// Run the lint check and return all detected diagnostics.
    /// Rules report everything they find; filtering against ignores happens in the orchestrator.
    fn check(&self, ctx: &Context) -> Vec<Diagnostic>;
}

/// Build a `Report` from diagnostics.
#[must_use]
pub fn format_and_report(diagnostics: Vec<Diagnostic>) -> Report {
    Report::from_diagnostics(diagnostics)
}

/// Run a workflow-scoped rule. Filters its diagnostics through the per-rule `ignore`
/// list using the new workflow/job-aware matcher, applies the configured severity, and
/// pushes the survivors onto `out`.
pub(super) fn run_workflow_rule<R: Rule>(
    rule: &R,
    default_level: Level,
    ctx: &Context<'_>,
    lint_config: &LintConfig,
    out: &mut Vec<Diagnostic>,
) {
    let configured = lint_config.get_rule(rule.name(), default_level);
    if configured.level == Level::Off {
        return;
    }
    for mut diag in rule.check(ctx) {
        diag.level = configured.level;
        let ignored = configured
            .ignore
            .iter()
            .any(|target| matches_ignore_workflow(&diag, target));
        if !ignored {
            out.push(diag);
        }
    }
}

/// Check if a per-action diagnostic is ignored via lint config.
pub(super) fn is_ignored(
    diag: &Diagnostic,
    rule_name: RuleName,
    default_level: Level,
    lint_config: &LintConfig,
    action: &LocatedAction,
) -> bool {
    lint_config
        .get_rule(rule_name, default_level)
        .ignore
        .iter()
        .any(|target| matches_ignore_action(diag, target, action))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{CommandReport as _, Diagnostic, Level, OutputLine, Report, Rule as _, RuleName};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::ParsedRef;
    use crate::domain::file::actions::{ActionSet, Located, WorkflowAction};
    use crate::domain::file::site::{Id, JobId, Origin, Slot, StepIndex, WorkflowPath};
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::lint::Context;
    use std::collections::HashMap;
    use std::str::FromStr as _;

    /// Kind-nouns a producer must not use to label an identifier it then names.
    const KIND_NOUNS: [&str; 3] = ["action", "workflow", "component"];

    #[test]
    fn diagnostic_can_be_created() {
        let diag = Diagnostic::new(RuleName::ShaMismatch, Level::Error, "test message");
        assert_eq!(diag.rule, RuleName::ShaMismatch);
        assert_eq!(diag.level, Level::Error);
        assert_eq!(diag.message, "test message");
        assert!(diag.workflow.is_none());
    }

    #[test]
    fn diagnostic_with_workflow() {
        let diag = Diagnostic::new(RuleName::Unpinned, Level::Warn, "test")
            .with_workflow(WorkflowPath::new(".github/workflows/ci.yml"));
        assert_eq!(
            diag.workflow,
            Some(WorkflowPath::new(".github/workflows/ci.yml"))
        );
    }

    /// The name a rule is configured by and the name it is reported by must be the
    /// same string, for every rule, derived from one definition.
    ///
    /// Driven off `RuleName::ALL` rather than a written-out list: a rule added later
    /// is covered without editing this test, which is the property that makes the
    /// single-list guarantee real instead of merely intended.
    #[test]
    fn every_rule_name_agrees_across_config_and_output() {
        for &name in RuleName::ALL {
            let rendered = name.to_string();
            let quoted = serde_json::to_string(&name).unwrap();
            let configured = quoted.trim_matches('"');

            assert_eq!(
                rendered,
                name.as_str(),
                "Display must equal as_str for {name:?}"
            );
            assert_eq!(
                configured, rendered,
                "the config name and the reported name must be one string for {name:?}"
            );
            assert_eq!(
                RuleName::from_str(&rendered),
                Ok(name),
                "the reported name must parse back for {name:?}"
            );
            assert_eq!(
                serde_json::from_str::<RuleName>(&format!("\"{rendered}\"")).unwrap(),
                name,
                "the reported name must deserialize back for {name:?}"
            );
        }
    }

    /// `ALL` must actually enumerate the enum, or every test driven off it is vacuous.
    #[test]
    fn all_covers_every_rule_and_names_are_unique() {
        let names: std::collections::BTreeSet<&str> =
            RuleName::ALL.iter().map(|n| n.as_str()).collect();
        assert_eq!(
            names.len(),
            RuleName::ALL.len(),
            "two rules share a name — config could not distinguish them"
        );
        assert_eq!(RuleName::ALL.len(), 13, "expected 13 lint rules");
    }

    #[test]
    fn rule_name_from_str_invalid() {
        RuleName::from_str("nonexistent-rule").unwrap_err();
    }

    /// Every rule gx can report must be configurable in `[lint.rules]` by the very name
    /// it reports — the maintainer's copy-from-output, paste-into-config workflow.
    ///
    /// Built from `RuleName::ALL`, so this covers a rule added later without edits. It
    /// exercises the real TOML surface, which is where the names are used as map keys.
    #[test]
    fn every_reported_rule_name_is_accepted_in_config() {
        let mut toml_str = String::from("[rules]\n");
        for name in RuleName::ALL {
            use std::fmt::Write as _;
            writeln!(toml_str, "{name} = {{ level = \"warn\" }}").unwrap();
        }

        let config: crate::config::Lint = toml::from_str(&toml_str).unwrap();

        assert_eq!(
            config.rules.len(),
            RuleName::ALL.len(),
            "every implemented rule must be configurable by its reported name"
        );
        for &name in RuleName::ALL {
            assert!(
                config.rules.contains_key(&name),
                "{name} is reported by gx lint but was not accepted in [lint.rules]"
            );
        }
    }

    /// Rule names are `BTreeMap` keys on the manifest *write* path too, so serializing
    /// must emit a plain string in key position and round-trip unchanged.
    #[test]
    fn rule_names_round_trip_as_toml_map_keys() {
        let mut rules = std::collections::BTreeMap::new();
        for &name in RuleName::ALL {
            rules.insert(
                name,
                crate::config::Rule {
                    level: Level::Warn,
                    ignore: Vec::new(),
                },
            );
        }

        let emitted = toml::to_string(&rules).unwrap();
        for &name in RuleName::ALL {
            assert!(
                emitted.contains(name.as_str()),
                "{name} must survive serialization as a map key"
            );
        }

        let reparsed: std::collections::BTreeMap<RuleName, crate::config::Rule> =
            toml::from_str(&emitted).unwrap();
        assert_eq!(reparsed.len(), RuleName::ALL.len());
    }

    /// The zero-config default level of every implemented rule, as the specification
    /// documents it. Pairs `RuleName::ALL` against this table so a rule added later
    /// fails here until its default is written down — the drift that left
    /// `dangling-reference`, `invalid-expression`, and `run-shellcheck` undocumented.
    ///
    /// Only coverage is asserted, not the levels: each rule's own test pins its
    /// `default_level()` (e.g. `sha_mismatch.rs`), and the rules are constructed
    /// per-phase in `command.rs` rather than kept in a list this could iterate.
    #[test]
    fn every_rule_has_a_documented_default_level() {
        let documented: std::collections::BTreeMap<RuleName, Level> = [
            (RuleName::ShaMismatch, Level::Error),
            (RuleName::Unpinned, Level::Error),
            (RuleName::UnsyncedManifest, Level::Error),
            (RuleName::StaleComment, Level::Warn),
            (RuleName::MissingPermissions, Level::Error),
            (RuleName::ExcessivePermissions, Level::Error),
            (RuleName::DangerousTrigger, Level::Error),
            (RuleName::PrHeadCheckout, Level::Error),
            (RuleName::MissingConcurrency, Level::Warn),
            (RuleName::UnprotectedSecrets, Level::Error),
            (RuleName::DanglingReference, Level::Error),
            (RuleName::InvalidExpression, Level::Error),
            (RuleName::RunShellcheck, Level::Warn),
        ]
        .into_iter()
        .collect();

        for &name in RuleName::ALL {
            assert!(
                documented.contains_key(&name),
                "{name} runs but its zero-config default is not documented"
            );
        }
        assert_eq!(
            documented.len(),
            RuleName::ALL.len(),
            "the documented default set must not list rules that do not exist"
        );
    }

    /// A typo'd rule name must be rejected, naming the offending key.
    #[test]
    fn unrecognized_rule_name_is_rejected() {
        let err = serde_json::from_str::<RuleName>("\"sha-missmatch\"").unwrap_err();
        assert!(
            err.to_string().contains("sha-missmatch"),
            "error must name the offending key, got: {err}"
        );
    }

    #[test]
    fn render_lint_clean() {
        let report = Report::default();
        let lines = report.render();
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "No lint issues found")
        );
    }

    #[test]
    fn render_lint_with_violations() {
        let diagnostics = vec![
            Diagnostic::new(
                RuleName::Unpinned,
                Level::Error,
                "actions/checkout@main is not pinned",
            )
            .with_workflow(WorkflowPath::new("ci.yml")),
            Diagnostic::new(
                RuleName::StaleComment,
                Level::Warn,
                "version comment does not match lock",
            )
            .with_workflow(WorkflowPath::new("ci.yml")),
        ];
        let report = Report::from_diagnostics(diagnostics);
        let lines = report.render();

        assert!(lines.iter().any(|l| matches!(
            l,
            OutputLine::LintDiag {
                level: Level::Error,
                ..
            }
        )));
        assert!(lines.iter().any(|l| matches!(
            l,
            OutputLine::LintDiag {
                level: Level::Warn,
                ..
            }
        )));
        assert!(lines.contains(&OutputLine::Summary {
            text: "1 error · 1 warning".to_owned(),
        }));
    }

    /// Build a `Located` for `actions/checkout` with the given reference shape.
    fn located(reference: ParsedRef) -> Located {
        Located {
            action: WorkflowAction {
                id: ActionId::from("actions/checkout"),
                reference,
            },
            site: Id {
                file: WorkflowPath::new(".github/workflows/ci.yml"),
                slot: Slot::WorkflowStep {
                    job: JobId::from("build"),
                    step: StepIndex::from(0_u16),
                },
            },
            origin: Origin { line: Some(7) },
        }
    }

    /// Collect messages the four action-scoped rules actually produce.
    ///
    /// Every string here comes from a rule's own `format!` — never from a literal
    /// written in this test — which is what makes the assertion below a guard
    /// rather than a restatement of what the author typed.
    fn rule_produced_diagnostics() -> Vec<Diagnostic> {
        let sha = CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8");
        let mut out = Vec::new();

        // unpinned: a bare tag ref is not a SHA pin.
        out.extend(crate::lint::unpinned::UnpinnedRule::check_action(&located(
            ParsedRef::Ref(Version::from("v4")),
        )));

        // sha-mismatch: a bare SHA absent from an empty lock.
        let empty_lock = Lock::new(HashMap::new());
        out.extend(crate::lint::sha_mismatch::ShaMismatchRule::check_action(
            &located(ParsedRef::Sha(sha.clone())),
            &empty_lock,
        ));

        // stale-comment: a pinned SHA that disagrees with the locked one.
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let mut lock = Lock::new(HashMap::new());
        lock.set(
            &spec,
            ResolvedRef::Tag(Version::from("v4")),
            Commit {
                sha: CommitSha::from("1111111111111111111111111111111111111111"),
                repository: Repository::from("actions/checkout"),
                ref_type: None,
                date: CommitDate::from("2024-01-01T00:00:00Z"),
            },
        );
        out.extend(crate::lint::stale_comment::StaleCommentRule::check_action(
            &located(ParsedRef::Pinned {
                sha,
                comment: Version::from("v4"),
            }),
            &lock,
        ));

        // unsynced-manifest: both directions — one action only in a workflow,
        // one only in the manifest.
        let mut action_set = ActionSet::new();
        action_set.add(&WorkflowAction {
            id: ActionId::from("actions/only-in-workflow"),
            reference: ParsedRef::Ref(Version::from("v1")),
        });
        let mut manifest = Manifest::new(HashMap::new());
        manifest.set(
            ActionId::from("actions/only-in-manifest"),
            Specifier::from_v1("v1"),
        );
        let ctx = Context {
            manifest: &manifest,
            lock: &empty_lock,
            workflows: &[],
            workflows_full: &[],
            action_set: &action_set,
        };
        out.extend(crate::lint::unsynced_manifest::UnsyncedManifestRule.check(&ctx));

        out
    }

    /// Whether `message` opens with `noun` used as a label on the identifier that
    /// follows it, as in `action actions/checkout uses ...`.
    ///
    /// A noun carrying a sentence that names no identifier — `workflow has no
    /// top-level permissions: block` — is legitimate, so an `owner/repo` shape must
    /// follow for the noun to count as a label.
    fn labels_an_identifier(message: &str, noun: &str) -> bool {
        message
            .strip_prefix(noun)
            .and_then(|rest| rest.strip_prefix(' '))
            .and_then(|rest| rest.split_whitespace().next())
            .is_some_and(|word| word.contains('/'))
    }

    #[test]
    fn rendered_diagnostics_carry_no_kind_noun() {
        // The renderer owns user-facing vocabulary: a rule must not label an
        // identifier with the noun naming its kind, or the noun is fixed upstream
        // where the renderer cannot change it. Sibling of
        // `format_line_lint_diag_renders_location_once` in `output/lines.rs`,
        // which guards the same discipline for the workflow path.
        //
        // Asserted against messages the rules actually produced, so restoring an
        // `action ` prefix in any rule below turns this red.
        let diagnostics = rule_produced_diagnostics();
        assert!(
            diagnostics.len() >= 5,
            "fixture should exercise all four offending rules (unsynced-manifest \
             emits two), got {} diagnostics",
            diagnostics.len()
        );

        let report = Report::from_diagnostics(diagnostics);
        for line in report.render() {
            let OutputLine::LintDiag { message, .. } = line else {
                continue;
            };
            for noun in KIND_NOUNS {
                assert!(
                    !labels_an_identifier(&message, noun),
                    "rule message must not prefix an identifier with `{noun}` — the \
                     renderer owns that vocabulary. Got: {message}"
                );
            }
        }
    }
}
