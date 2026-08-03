use crate::command::CommandReport;
use crate::output::lines::Line as OutputLine;
use serde::Serialize;

/// A single action upgrade in the report.
///
/// `from`/`to` are the **resolved versions** (e.g. `v6.0.1` → `v6.0.3`), not the
/// manifest range — so a downstream PR body reads correctly and can link a diff.
/// `compare` is present only when both sides are real version tags.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpgradeEntry {
    /// The action identifier (e.g. `actions/checkout`).
    pub action: String,
    /// The old resolved version, from the lock before the upgrade.
    pub from: String,
    /// The new resolved version.
    pub to: String,
    /// Whether the upgrade stayed within the manifest range (lock-only advance).
    pub in_range: bool,
    /// A GitHub compare URL for the transition, when both versions are tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<String>,
}

/// Report from the upgrade command.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// Actions that were upgraded.
    pub upgrades: Vec<UpgradeEntry>,
    /// Actions that were skipped: (action, reason).
    #[serde(serialize_with = "serialize_skipped")]
    pub skipped: Vec<(String, String)>,
    /// Warnings encountered during upgrade.
    pub warnings: Vec<String>,
    /// Number of managed files updated — workflows and composite action definitions.
    /// The name is fixed by the `--json` contract.
    pub workflows_updated: usize,
    /// True if everything was already up to date.
    pub up_to_date: bool,
}

/// Serialize skipped `(action, reason)` pairs as objects so the JSON contract is
/// self-describing rather than positional tuples.
fn serialize_skipped<S: serde::Serializer>(
    skipped: &[(String, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq as _;
    let mut seq = serializer.serialize_seq(Some(skipped.len()))?;
    for (action, reason) in skipped {
        seq.serialize_element(&SkippedEntry { action, reason })?;
    }
    seq.end()
}

/// A skipped action rendered as a self-describing JSON object.
#[derive(Serialize)]
struct SkippedEntry<'entry> {
    /// The action identifier that was skipped.
    action: &'entry str,
    /// Why the action was skipped.
    reason: &'entry str,
}

impl Report {
    /// Render the report as a single JSON object — the stable, machine-readable
    /// contract for unattended consumers (e.g. a scheduled PR-opening workflow).
    ///
    /// # Panics
    ///
    /// Panics only if the report contains a value serde cannot serialize, which
    /// cannot happen for this all-owned-strings type.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if self.up_to_date
            || (self.upgrades.is_empty() && self.skipped.is_empty() && self.warnings.is_empty())
        {
            return vec![OutputLine::Summary {
                text: "All actions up to date".to_owned(),
            }];
        }

        let mut lines = Vec::new();

        for entry in &self.upgrades {
            // The compare link is intentionally omitted from the terminal line —
            // it is verbose. It lives in the `--json` contract and the log file.
            lines.push(OutputLine::Upgraded {
                action: entry.action.clone(),
                from: entry.from.clone(),
                to: entry.to.clone(),
            });
        }

        for (action, reason) in &self.skipped {
            lines.push(OutputLine::Skipped {
                action: action.clone(),
                reason: reason.clone(),
            });
        }

        for message in &self.warnings {
            lines.push(OutputLine::Warning {
                message: message.clone(),
            });
        }

        lines.push(OutputLine::Blank);

        let upgrade_count = self.upgrades.len();
        let wf = self.workflows_updated;
        let summary = format!(
            "{} upgraded · {} file{}",
            upgrade_count,
            wf,
            if wf == 1 { "" } else { "s" }
        );
        lines.push(OutputLine::Summary { text: summary });

        lines
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{CommandReport as _, OutputLine, Report, UpgradeEntry};

    fn entry(action: &str, from: &str, to: &str) -> UpgradeEntry {
        UpgradeEntry {
            action: action.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            in_range: true,
            compare: None,
        }
    }

    #[test]
    fn render_upgrade_up_to_date() {
        let report = Report {
            up_to_date: true,
            ..Default::default()
        };
        let lines = report.render();
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "All actions up to date")
        );
    }

    #[test]
    fn render_upgrade_with_upgrades() {
        let report = Report {
            upgrades: vec![
                entry("actions/checkout", "v6", "v6.0.2"),
                entry("jdx/mise-action", "v3", "v3.6.2"),
            ],
            workflows_updated: 1,
            ..Default::default()
        };
        let lines = report.render();

        assert!(lines.contains(&OutputLine::Upgraded {
            action: "actions/checkout".to_owned(),
            from: "v6".to_owned(),
            to: "v6.0.2".to_owned(),
        }));
        assert!(lines.contains(&OutputLine::Upgraded {
            action: "jdx/mise-action".to_owned(),
            from: "v3".to_owned(),
            to: "v3.6.2".to_owned(),
        }));
        assert!(lines.contains(&OutputLine::Summary {
            text: "2 upgraded · 1 file".to_owned(),
        }));
    }

    #[test]
    fn to_json_uses_resolved_versions_and_compare() {
        let report = Report {
            upgrades: vec![UpgradeEntry {
                compare: Some(
                    "https://github.com/actions/checkout/compare/v6.0.1...v6.0.3".to_owned(),
                ),
                ..entry("actions/checkout", "v6.0.1", "v6.0.3")
            }],
            workflows_updated: 1,
            ..Default::default()
        };
        let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        let upgrade = &value["upgrades"][0];
        // `from` is the OLD resolved version, never the `^6` range.
        assert_eq!(upgrade["from"], "v6.0.1");
        assert_eq!(upgrade["to"], "v6.0.3");
        assert_eq!(upgrade["in_range"], true);
        assert_eq!(
            upgrade["compare"],
            "https://github.com/actions/checkout/compare/v6.0.1...v6.0.3"
        );
        assert_eq!(value["workflows_updated"], 1);
        assert_eq!(value["up_to_date"], false);
    }

    #[test]
    fn to_json_up_to_date_has_empty_upgrades() {
        let report = Report {
            up_to_date: true,
            ..Default::default()
        };
        let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(value["up_to_date"], true);
        assert_eq!(value["upgrades"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn to_json_omits_compare_when_absent() {
        let report = Report {
            upgrades: vec![entry("actions/setup-node", "v4.0.0", "v4.1.0")],
            ..Default::default()
        };
        let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        // A missing compare link is absent, not `null`, keeping the contract tidy.
        assert!(value["upgrades"][0].get("compare").is_none());
    }
}
