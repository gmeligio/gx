#![expect(clippy::pub_use, reason = "reexport Trigger from extracted submodule")]

use super::workflow_actions::WorkflowPath;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_saphyr::Commented;
use std::collections::BTreeMap;
use std::fmt;

mod de;
mod trigger;

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

/// Access level for a single permission scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    Write,
    None,
}

/// A workflow's `permissions:` block, in one of GitHub's three shapes:
/// `read-all`, `write-all`, or a per-scope map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permissions {
    ReadAll,
    WriteAll,
    /// Empty `permissions: {}` — drops all defaults.
    Empty,
    Specific(BTreeMap<String, Access>),
}

impl Permissions {
    /// True when this block grants anything broader than `contents: read`.
    #[must_use]
    pub fn is_excessive(&self) -> bool {
        match self {
            Self::WriteAll | Self::ReadAll => true,
            Self::Empty => false,
            Self::Specific(map) => map.iter().any(|(scope, access)| {
                !(scope == "contents" && matches!(access, Access::Read | Access::None))
            }),
        }
    }

    /// True when this block grants any write scope.
    #[must_use]
    pub fn has_write(&self) -> bool {
        match self {
            Self::WriteAll => true,
            Self::ReadAll | Self::Empty => false,
            Self::Specific(map) => map.values().any(|a| matches!(a, Access::Write)),
        }
    }
}

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Permissions;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"read-all\", \"write-all\", or a per-scope map")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Permissions, E> {
                match v {
                    "read-all" => Ok(Permissions::ReadAll),
                    "write-all" => Ok(Permissions::WriteAll),
                    other => Err(E::custom(format!("unknown permissions shorthand: {other}"))),
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Permissions, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((k, v)) = map.next_entry::<String, Access>()? {
                    out.insert(k, v);
                }
                if out.is_empty() {
                    Ok(Permissions::Empty)
                } else {
                    Ok(Permissions::Specific(out))
                }
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
/// `"bash"`/`"sh"` directly. The fourth GitHub source — the runner-OS default — is a
/// documented non-goal for this cut; an absent shell is treated as `bash`.
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

/// A single step within a job, with the structural fields rule logic needs.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    #[serde(default)]
    pub id: Option<String>,
    /// The step's `uses:` reference with its inline version comment. Read via
    /// [`Step::uses_ref`] (bare reference) and [`Step::uses_comment`] (comment).
    #[serde(default)]
    pub uses: Option<Commented<String>>,
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
        self.uses.as_ref().map(|c| c.0.as_str())
    }

    /// The step's inline `uses:` version comment (e.g. `v4`), if any. saphyr yields an
    /// empty string for no comment; this normalizes that to `None`.
    #[must_use]
    pub fn uses_comment(&self) -> Option<&str> {
        self.uses
            .as_ref()
            .map(|c| c.1.as_str())
            .filter(|s| !s.is_empty())
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

/// Top-level workflow parse. Structural fields only — `name`, `defaults`, `env`, `runs-on`
/// and friends are intentionally not captured.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub path: WorkflowPath,
    pub on: Vec<Trigger>,
    pub permissions: Option<Permissions>,
    pub concurrency: Option<Concurrency>,
    /// The workflow-level `defaults:` block. Lowest-precedence source for a step's
    /// effective shell (below the step's own `shell:` and the job's `defaults`).
    pub defaults: Option<Defaults>,
    pub jobs: Vec<Job>,
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
}

impl Parsed {
    /// Parse a workflow YAML string into structural data. The `path` is supplied by the
    /// caller because it is not present in the YAML itself.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_saphyr` error if the YAML cannot be deserialized.
    pub fn from_yaml(path: WorkflowPath, content: &str) -> Result<Self, Box<serde_saphyr::Error>> {
        let wire: WireWorkflow = serde_saphyr::from_str(content).map_err(Box::new)?;
        let jobs = wire
            .jobs
            .into_iter()
            .map(|(id, mut job)| {
                job.id = id;
                job
            })
            .collect();
        Ok(Self {
            path,
            on: wire.on.unwrap_or_default(),
            permissions: wire.permissions,
            concurrency: wire.concurrency,
            defaults: wire.defaults,
            jobs,
        })
    }

    /// True if any trigger in `on` matches.
    #[must_use]
    pub fn has_trigger(&self, t: &Trigger) -> bool {
        self.on.iter().any(|x| x == t)
    }
}

#[cfg(test)]
mod tests;
