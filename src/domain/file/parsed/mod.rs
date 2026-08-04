#![expect(
    clippy::pub_use,
    reason = "reexport Trigger and Permissions from extracted submodules"
)]
#![expect(
    clippy::module_name_repetitions,
    reason = "ParsedWorkflow/ParsedAction name the variants of Parsed; the shared prefix is \
              the point, and callers import them from this module's public surface"
)]
#![expect(
    clippy::multiple_inherent_impl,
    reason = "parsing constructors need WireWorkflow, which lives here; the accessors live \
              beside the type in file.rs"
)]

use super::site::WorkflowPath;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_saphyr::{Commented, Spanned};
use std::collections::BTreeMap;
use std::fmt;

mod de;
mod file;
mod kind;
mod permissions;
mod trigger;

pub use file::{Parsed, ParsedAction, ParsedWorkflow};
pub use kind::FileKind;
pub use permissions::{Access, Permissions};
pub use trigger::Trigger;

use de::deserialize_needs;
use trigger::parse_triggers_opt;

/// A scalar value that accepts strings, numbers, bools, or null and stores them as `String`.
///
/// GitHub Actions `with:` and `env:` values are stringified at runtime regardless of how
/// the YAML scalar is written. Capturing them as `String` lets the security rules text-scan
/// for `secrets.*` references without choosing between `with: { foo: 42 }` (int) and
/// `with: { foo: "42" }` (string) at deserialization time.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct AnyScalar(pub String);

impl AnyScalar {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AnyScalar {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = AnyScalar;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a YAML scalar")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v.to_owned()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v.to_string()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v.to_string()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v.to_string()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<AnyScalar, E> {
                Ok(AnyScalar(v.to_string()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<AnyScalar, E> {
                Ok(AnyScalar(String::new()))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<AnyScalar, E> {
                Ok(AnyScalar(String::new()))
            }
        }
        de.deserialize_any(V)
    }
}

/// `concurrency:` block. Captures the structural fields rules care about; everything else
/// is ignored on parse.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Concurrency {
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default, rename = "cancel-in-progress")]
    pub cancel_in_progress: Option<bool>,
}

/// A `defaults:` block. Only `run.shell` is captured — it is the one field the
/// `run-shellcheck` rule needs to resolve a step's effective shell. Both levels are
/// optional, so an absent `defaults:` or absent `defaults.run:` deserializes to `None`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Defaults {
    #[serde(default)]
    pub run: Option<RunDefaults>,
}

/// The `defaults.run:` block. Captures only `shell`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct RunDefaults {
    #[serde(default)]
    pub shell: Option<String>,
}

impl Defaults {
    /// The `run.shell` value, if both `defaults:` and `defaults.run:` are present.
    fn run_shell(&self) -> Option<&str> {
        self.run.as_ref().and_then(|r| r.shell.as_deref())
    }
}

/// Resolve a step's effective shell from the three GitHub Actions sources, in precedence
/// order: the step's own `shell:`, then the job's `defaults.run.shell`, then the
/// workflow's `defaults.run.shell`. When none is set, GitHub's default on Linux/macOS
/// runners is `bash`, which this returns as the floor.
///
/// The returned token is normalized: a `shell:` value carrying a flag/template form
/// (`bash -e {0}`, `sh -e {0}`) is reduced to its leading word, so callers can match on
/// `"bash"`/`"sh"` directly. GitHub's runner-OS default is not consulted.
#[must_use]
pub fn effective_shell(
    step_shell: Option<&str>,
    job_defaults: Option<&Defaults>,
    workflow_defaults: Option<&Defaults>,
) -> String {
    let raw = step_shell
        .or_else(|| job_defaults.and_then(Defaults::run_shell))
        .or_else(|| workflow_defaults.and_then(Defaults::run_shell))
        .unwrap_or("bash");
    normalize_shell(raw)
}

/// Reduce a `shell:` value to its leading word. GitHub allows custom command templates
/// like `bash -e {0}` or `perl {0}`; the first whitespace-delimited token is the shell
/// name. An empty or whitespace-only value falls back to `bash`.
fn normalize_shell(raw: &str) -> String {
    raw.split_whitespace().next().unwrap_or("bash").to_owned()
}

/// A single step of a job or a composite action, with the structural fields rule
/// logic needs.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    #[serde(default)]
    pub id: Option<String>,
    /// The step's `uses:` reference with its inline version comment and source location.
    /// The nested tuple is opaque; read it through the `uses_*` accessors.
    #[serde(default)]
    pub uses: Option<Spanned<Commented<String>>>,
    #[serde(default, rename = "if")]
    pub if_cond: Option<String>,
    #[serde(default)]
    pub with: BTreeMap<String, AnyScalar>,
    #[serde(default)]
    pub env: BTreeMap<String, AnyScalar>,
    #[serde(default)]
    pub run: Option<String>,
    /// The step's `shell:`, if declared. The `run-shellcheck` rule uses this (with
    /// `defaults.run.shell` as fallback) to decide whether the body is bash/sh.
    #[serde(default)]
    pub shell: Option<String>,
}

impl Step {
    /// The step's `uses:` action reference without its version comment, if present.
    #[must_use]
    pub fn uses_ref(&self) -> Option<&str> {
        self.uses.as_ref().map(|s| s.value.0.as_str())
    }

    /// The step's inline `uses:` version comment (e.g. `v4`), if any. saphyr yields an
    /// empty string for no comment; this normalizes that to `None`.
    #[must_use]
    pub fn uses_comment(&self) -> Option<&str> {
        self.uses
            .as_ref()
            .map(|s| s.value.1.as_str())
            .filter(|c| !c.is_empty())
    }

    /// The 1-based source line of the step's `uses:` scalar, if present.
    ///
    /// saphyr reports line 0 for an unknown location; this normalizes that to `None`.
    #[must_use]
    pub fn uses_line(&self) -> Option<u32> {
        self.uses
            .as_ref()
            .map(|s| s.referenced.line())
            .filter(|&line| line != 0)
            .and_then(|line| u32::try_from(line).ok())
    }

    /// All scalar text owned by this step (concatenated `with` values, `env` values, and
    /// `run` body). Rules text-scan this for expression references like `secrets.NAME`.
    #[must_use]
    pub fn scalar_text(&self) -> String {
        let mut out = String::new();
        for v in self.with.values() {
            out.push_str(v.as_str());
            out.push('\n');
        }
        for v in self.env.values() {
            out.push_str(v.as_str());
            out.push('\n');
        }
        if let Some(run) = &self.run {
            out.push_str(run);
        }
        out
    }
}

/// A job within a workflow.
#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    #[serde(skip)]
    pub id: String,
    #[serde(default)]
    pub permissions: Option<Permissions>,
    #[serde(default, rename = "if")]
    pub if_cond: Option<String>,
    /// Jobs this one depends on. Accepts the scalar (`needs: build`) and sequence
    /// (`needs: [build, test]`) forms; absent → empty. The validity rules read this.
    #[serde(default, deserialize_with = "deserialize_needs")]
    pub needs: Vec<String>,
    /// The job's inline `outputs:` map. The `invalid-expression` rule reads the key
    /// set to validate `needs.<job>.outputs.<key>` references. A `uses:` reusable-workflow
    /// job has no inline outputs here (they live in the called file) → empty.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub secrets: Option<JobSecrets>,
    /// The job's `defaults:` block. Supplies the `run.shell` fallback for steps in this
    /// job that omit a step-level `shell:`.
    #[serde(default)]
    pub defaults: Option<Defaults>,
}

/// The `secrets:` field on a reusable-workflow call. Captures only the `inherit` shape;
/// per-key maps are treated as `Explicit` for rule logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobSecrets {
    Inherit,
    Explicit,
}

impl<'de> Deserialize<'de> for JobSecrets {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = JobSecrets;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"inherit\" or a secrets map")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<JobSecrets, E> {
                Ok(if v == "inherit" {
                    JobSecrets::Inherit
                } else {
                    JobSecrets::Explicit
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JobSecrets, A::Error> {
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(JobSecrets::Explicit)
            }
        }
        de.deserialize_any(V)
    }
}

/// Wire-format struct used only as a serde target. Public surface is `Parsed`.
#[derive(Debug, Deserialize)]
struct WireWorkflow {
    /// The `on:` block, parsed into a list of triggers; absent when no `on:` key is present.
    #[serde(default, deserialize_with = "parse_triggers_opt")]
    on: Option<Vec<Trigger>>,
    /// The workflow-level `permissions:` block, if declared.
    #[serde(default)]
    permissions: Option<Permissions>,
    /// The workflow-level `concurrency:` block, if declared.
    #[serde(default)]
    concurrency: Option<Concurrency>,
    /// The workflow-level `defaults:` block, if declared.
    #[serde(default)]
    defaults: Option<Defaults>,
    /// The workflow's jobs, keyed by job id.
    #[serde(default)]
    jobs: BTreeMap<String, Job>,
    /// The `runs:` block of an action definition. Absent in a workflow.
    #[serde(default)]
    runs: Option<Runs>,
}

/// The `runs:` block of an action definition. Only the fields needed to decide
/// whether the file is composite and to reach its steps are captured.
#[derive(Debug, Deserialize)]
struct Runs {
    /// The action's implementation kind: `composite`, `node20`, `docker`, …
    #[serde(default)]
    using: Option<String>,
    /// The composite steps. Present only when `using` is `composite`; a `node20`
    /// or `docker` action has `main`/`image` instead.
    #[serde(default)]
    steps: Vec<Step>,
}

impl Parsed {
    /// Parse a workflow YAML string, skipping the kind dispatch [`Parsed::parse`] does.
    ///
    /// Test-only: production always knows the kind discovery assigned, so it goes through
    /// `parse`. This exists so a rule test can write a workflow body and get a
    /// [`ParsedWorkflow`] without naming a kind the test has no say in.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_saphyr` error if the YAML cannot be deserialized.
    #[cfg(test)]
    pub fn from_yaml(
        path: WorkflowPath,
        content: &str,
    ) -> Result<ParsedWorkflow, Box<serde_saphyr::Error>> {
        let wire = Self::deserialize(content)?;
        Ok(Self::into_workflow(path, wire))
    }

    /// Parse a managed file into structural data. The `kind` is supplied by the caller
    /// because a workflow and an action definition are distinguished by where the file
    /// was found, not by its contents — sniffing the YAML shape would misattribute a
    /// malformed file to the wrong schema, and a path says only where a file sits.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_saphyr` error if the YAML cannot be deserialized.
    pub fn parse(
        path: WorkflowPath,
        kind: FileKind,
        content: &str,
    ) -> Result<Self, Box<serde_saphyr::Error>> {
        let wire = Self::deserialize(content)?;
        Ok(match kind {
            FileKind::Workflow => Self::Workflow(Self::into_workflow(path, wire)),
            FileKind::ActionDefinition => Self::Action(ParsedAction {
                path,
                // A non-composite `using` is a legitimate action with no steps to manage,
                // so it yields an empty list rather than an error.
                steps: wire
                    .runs
                    .filter(|runs| runs.using.as_deref() == Some("composite"))
                    .map(|runs| runs.steps)
                    .unwrap_or_default(),
            }),
        })
    }

    /// Deserialize the shared wire form. Both schemas are accepted by one serde target;
    /// which half is read is decided by the caller-supplied kind, not by what is present.
    fn deserialize(content: &str) -> Result<WireWorkflow, Box<serde_saphyr::Error>> {
        serde_saphyr::from_str(content).map_err(Box::new)
    }

    /// Build the workflow view from the wire form, keying each job by its `jobs:` map key.
    fn into_workflow(path: WorkflowPath, wire: WireWorkflow) -> ParsedWorkflow {
        ParsedWorkflow {
            path,
            on: wire.on.unwrap_or_default(),
            permissions: wire.permissions,
            concurrency: wire.concurrency,
            defaults: wire.defaults,
            jobs: wire
                .jobs
                .into_iter()
                .map(|(id, mut job)| {
                    job.id = id;
                    job
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
