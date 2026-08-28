//! GitHub security advisory lookups for GitHub Actions.
//!
//! A check deciding "vulnerable or not" must be testable offline, hence the seam:
//! [`AdvisoryQuery`] is what checks depend on, [`GraphQlAdvisories`] is the real adapter,
//! and `FakeAdvisories` below is the double. Same shape as [`crate::infra::shellcheck`].
//!
//! GraphQL rather than REST because `securityVulnerabilities` filters by ecosystem and
//! returns affected ranges in one round trip. It is also why `gx audit` needs a token:
//! this endpoint rejects unauthenticated requests.

use super::Error;
use super::Registry;
use serde::{Deserialize, Serialize};

/// GitHub's GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// The HTTP status a failed GraphQL query arrives with.
///
/// GraphQL reports query-level failures — bad credentials, malformed query — in an
/// `errors` array with a 200 response, so a rejected query is indistinguishable from a
/// successful one by status alone. Every failure below therefore carries this status.
const GRAPHQL_ERROR_STATUS: u16 = 200;

/// The advisory query: the whole `ACTIONS` ecosystem in one request.
///
/// Wholesale rather than per package because the set is small — 63 advisories over 47
/// packages when measured — so asking per action would cost 47 requests to learn the same
/// 63 facts, and scale with lock size for nothing.
///
/// Filtering by version is deliberately NOT done server-side: the caller compares the
/// locked version against `vulnerableVersionRange` itself, because the API's version
/// filter has a history of returning empty results — a false-negative that would silently
/// report "clean".
///
/// `totalCount` is selected so a page that could not hold the whole set is detectable.
const ADVISORY_QUERY: &str = "\
query {
  securityVulnerabilities(ecosystem: ACTIONS, first: 100) {
    totalCount
    nodes {
      vulnerableVersionRange
      firstPatchedVersion { identifier }
      package { name }
      advisory { ghsaId summary severity permalink }
    }
  }
}";

/// How severe an advisory is, as GitHub classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Low,
    Moderate,
    High,
    Critical,
}

/// One advisory affecting an action, normalized at the integration edge so check logic
/// never touches raw GraphQL JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advisory {
    /// The package the advisory is published against, e.g. `tj-actions/changed-files`.
    /// Mixed case as GitHub returns it; callers match case-insensitively.
    pub package: String,
    /// The GHSA identifier, e.g. `GHSA-mrrh-fwg8-r2c3`.
    pub ghsa_id: String,
    /// One-line description of the vulnerability.
    pub summary: String,
    /// How severe GitHub rates it.
    pub severity: Severity,
    /// Link to the advisory on github.com.
    pub permalink: String,
    /// The affected range, e.g. `>= 1.0.0, < 1.2.3`. Compared against the locked version
    /// by the caller rather than server-side.
    pub vulnerable_range: String,
    /// The first version containing the fix, when the advisory names one.
    pub first_patched: Option<String>,
}

/// Source of security advisories for an action.
///
/// Implemented by [`GraphQlAdvisories`] (real) and `FakeAdvisories` (tests).
pub trait AdvisoryQuery {
    /// Every advisory published against the `ACTIONS` ecosystem.
    ///
    /// An empty vec means "no known advisories" — a positive statement that the lookup
    /// happened and found nothing. Any failure to establish that is an `Err`, never an
    /// empty vec, so a caller cannot mistake "could not check" for "clean".
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the request fails, is rejected, or cannot be parsed.
    fn all_actions_advisories(&self) -> Result<Vec<Advisory>, Error>;
}

/// The GraphQL request body. The query takes no variables — it asks for the whole
/// ecosystem.
#[derive(Serialize)]
struct Request<'req> {
    /// The GraphQL query document.
    query: &'req str,
}

/// Top-level GraphQL response envelope.
#[derive(Deserialize)]
struct Response {
    /// The `data` half; absent when the query itself failed.
    data: Option<ResponseData>,
    /// Query-level errors. See [`GRAPHQL_ERROR_STATUS`] for why these must be checked
    /// explicitly rather than inferred from the HTTP status.
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

/// One GraphQL query-level error.
#[derive(Deserialize)]
struct GraphQlError {
    /// Human-readable description of what the server rejected.
    message: String,
}

/// The `data` half of a successful response.
#[derive(Deserialize)]
struct ResponseData {
    /// The vulnerability connection.
    #[serde(rename = "securityVulnerabilities")]
    security_vulnerabilities: Vulnerabilities,
}

/// A GraphQL connection wrapping the vulnerability list.
#[derive(Deserialize)]
struct Vulnerabilities {
    /// How many advisories the server holds, which may exceed what one page returns.
    #[serde(rename = "totalCount")]
    total_count: usize,
    /// The vulnerabilities themselves.
    nodes: Vec<VulnerabilityNode>,
}

/// One vulnerability as returned by the API.
#[derive(Deserialize)]
struct VulnerabilityNode {
    /// The affected version range expression.
    #[serde(rename = "vulnerableVersionRange")]
    vulnerable_version_range: String,
    /// The first fixed version, when one exists.
    #[serde(rename = "firstPatchedVersion")]
    first_patched_version: Option<PatchedVersion>,
    /// The package this vulnerability is published against.
    package: PackageNode,
    /// The advisory this vulnerability belongs to.
    advisory: AdvisoryNode,
}

/// The `package` object.
#[derive(Deserialize)]
struct PackageNode {
    /// The package name, an `owner/repo` slug for all but a few oddly-published entries.
    name: String,
}

/// The `firstPatchedVersion` object.
#[derive(Deserialize)]
struct PatchedVersion {
    /// The version string.
    identifier: String,
}

/// The advisory metadata attached to a vulnerability.
#[derive(Deserialize)]
struct AdvisoryNode {
    /// GHSA identifier.
    #[serde(rename = "ghsaId")]
    ghsa_id: String,
    /// One-line description.
    summary: String,
    /// How severe GitHub rates it.
    severity: Severity,
    /// Link to the advisory.
    permalink: String,
}

/// Real adapter: queries GitHub's GraphQL API over the shared blocking HTTP client.
pub struct GraphQlAdvisories {
    /// The authenticated client. Reused from [`Registry`] so timeout, user-agent, and
    /// token handling stay in one place.
    registry: Registry,
}

impl GraphQlAdvisories {
    /// Wrap an authenticated registry as an advisory source.
    #[must_use]
    pub const fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// Convert a decoded response into advisories, or an error if the query itself failed.
    fn interpret(response: Response) -> Result<Vec<Advisory>, Error> {
        // Treating a rejected query as "no advisories" is exactly the silent false-clean
        // this command exists to prevent, so both failure shapes are errors.
        if let Some(first) = response.errors.first() {
            return Err(Error::ApiError {
                status: GRAPHQL_ERROR_STATUS,
                url: format!("{GRAPHQL_URL} ({})", first.message),
            });
        }
        let Some(data) = response.data else {
            return Err(Error::ApiError {
                status: GRAPHQL_ERROR_STATUS,
                url: format!("{GRAPHQL_URL} (response contained no data)"),
            });
        };
        let connection = data.security_vulnerabilities;

        // A page that could not hold the whole set would leave the missing advisories
        // looking like advisories that do not exist — the same silent false-clean, arriving
        // through pagination instead of a rejected query.
        if connection.total_count > connection.nodes.len() {
            return Err(Error::ApiError {
                status: GRAPHQL_ERROR_STATUS,
                url: format!(
                    "{GRAPHQL_URL} (returned {} of {} advisories; the set no longer fits one page)",
                    connection.nodes.len(),
                    connection.total_count
                ),
            });
        }

        Ok(connection
            .nodes
            .into_iter()
            .map(|node| Advisory {
                package: node.package.name,
                ghsa_id: node.advisory.ghsa_id,
                summary: node.advisory.summary,
                severity: node.advisory.severity,
                permalink: node.advisory.permalink,
                vulnerable_range: node.vulnerable_version_range,
                first_patched: node.first_patched_version.map(|v| v.identifier),
            })
            .collect())
    }
}

impl AdvisoryQuery for GraphQlAdvisories {
    fn all_actions_advisories(&self) -> Result<Vec<Advisory>, Error> {
        let body = Request {
            query: ADVISORY_QUERY,
        };

        let request = self.registry.authenticated_post(GRAPHQL_URL).json(&body);

        let response = request.send().map_err(|source| Error::Request {
            operation: "security advisories",
            url: GRAPHQL_URL.to_owned(),
            source,
        })?;

        if !response.status().is_success() {
            return Err(Registry::check_status(&response, GRAPHQL_URL));
        }

        let decoded: Response = response.json().map_err(|source| Error::ParseResponse {
            url: GRAPHQL_URL.to_owned(),
            source,
        })?;

        Self::interpret(decoded)
    }
}

/// The [`AdvisoryQuery`] test double.
///
/// Lives in this file rather than its own so `src/infra/github/` keeps a free slot for the
/// first advisory-consuming check, which is the thing this seam exists to serve. A bottom
/// `#[cfg(test)] mod` satisfies the cfg-at-bottom invariant, which forbids top-level public
/// items after the first `#[cfg(test)]`, not a test module itself.
#[cfg(test)]
pub(crate) mod fake {
    use super::{Advisory, AdvisoryQuery, Error, GRAPHQL_URL};
    use std::cell::Cell;

    /// Returns pre-seeded advisories without issuing any request, so checks that judge
    /// whether an action is vulnerable can be unit-tested with no network and fully
    /// deterministic data.
    pub struct FakeAdvisories {
        /// What every lookup returns. `Err` models a failed query so callers can be
        /// tested on the path where the lookup did not succeed — the path where
        /// reporting "clean" would be a lie.
        result: Result<Vec<Advisory>, ()>,
        /// How many lookups were issued. The whole point of fetching wholesale is that
        /// this stays at one however large the lock, and at zero when it is empty.
        calls: Cell<usize>,
    }

    impl FakeAdvisories {
        /// A source that returns `advisories` for every lookup.
        #[must_use]
        pub fn new(advisories: Vec<Advisory>) -> Self {
            Self {
                result: Ok(advisories),
                calls: Cell::new(0),
            }
        }

        /// A source whose every lookup fails.
        #[must_use]
        pub fn failing() -> Self {
            Self {
                result: Err(()),
                calls: Cell::new(0),
            }
        }

        /// How many lookups have been issued so far.
        pub fn calls(&self) -> usize {
            self.calls.get()
        }
    }

    impl AdvisoryQuery for FakeAdvisories {
        fn all_actions_advisories(&self) -> Result<Vec<Advisory>, Error> {
            self.calls.set(self.calls.get().saturating_add(1));
            self.result.clone().map_err(|()| Error::Unauthorized {
                url: GRAPHQL_URL.to_owned(),
            })
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::fake::FakeAdvisories;
    use super::{
        ADVISORY_QUERY, Advisory, AdvisoryQuery as _, GraphQlAdvisories, Request, Response,
        Severity,
    };

    #[test]
    fn request_body_asks_for_the_whole_ecosystem() {
        let body = Request {
            query: ADVISORY_QUERY,
        };
        let json: serde_json::Value = serde_json::to_value(&body).unwrap();
        let query = json["query"].as_str().unwrap();

        // The ecosystem filter is what scopes this to GitHub Actions at all.
        assert!(query.contains("ecosystem: ACTIONS"));
        // No package variable: one request covers every action, however many are locked.
        assert!(json.get("variables").is_none());
        assert!(!query.contains("$package"));
        // Without totalCount a truncated page is indistinguishable from a complete one.
        assert!(query.contains("totalCount"));
        // The package name is what a finding is matched against.
        assert!(query.contains("package { name }"));
    }

    #[test]
    fn parses_a_vulnerability_payload() {
        let raw = r#"{
          "data": {
            "securityVulnerabilities": {
              "totalCount": 1,
              "nodes": [{
                "vulnerableVersionRange": "< 45.0.7",
                "firstPatchedVersion": { "identifier": "45.0.7" },
                "package": { "name": "tj-actions/changed-files" },
                "advisory": {
                  "ghsaId": "GHSA-mrrh-fwg8-r2c3",
                  "summary": "tj-actions/changed-files leaks secrets",
                  "severity": "HIGH",
                  "permalink": "https://github.com/advisories/GHSA-mrrh-fwg8-r2c3"
                }
              }]
            }
          }
        }"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        let advisories = GraphQlAdvisories::interpret(decoded).unwrap();

        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].package, "tj-actions/changed-files");
        assert_eq!(advisories[0].ghsa_id, "GHSA-mrrh-fwg8-r2c3");
        assert_eq!(advisories[0].severity, Severity::High);
        assert_eq!(advisories[0].vulnerable_range, "< 45.0.7");
        assert_eq!(advisories[0].first_patched.as_deref(), Some("45.0.7"));
    }

    #[test]
    fn parses_an_empty_result_as_no_advisories() {
        let raw = r#"{"data": {"securityVulnerabilities": {"totalCount": 0, "nodes": []}}}"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        assert!(GraphQlAdvisories::interpret(decoded).unwrap().is_empty());
    }

    #[test]
    fn advisory_without_patch_parses() {
        let raw = r#"{
          "data": {
            "securityVulnerabilities": {
              "totalCount": 1,
              "nodes": [{
                "vulnerableVersionRange": ">= 0",
                "firstPatchedVersion": null,
                "package": { "name": "some/action" },
                "advisory": {
                  "ghsaId": "GHSA-xxxx-yyyy-zzzz",
                  "summary": "no fix available",
                  "severity": "CRITICAL",
                  "permalink": "https://example.invalid"
                }
              }]
            }
          }
        }"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        let advisories = GraphQlAdvisories::interpret(decoded).unwrap();
        assert_eq!(advisories[0].first_patched, None);
        assert_eq!(advisories[0].severity, Severity::Critical);
    }

    #[test]
    fn query_level_errors_are_not_a_clean_result() {
        // GraphQL returns errors with HTTP 200. Reading this as "no advisories" would be
        // a silent false-clean, which is the failure mode this command exists to prevent.
        let raw = r#"{"data": null, "errors": [{"message": "Bad credentials"}]}"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        let err = GraphQlAdvisories::interpret(decoded).unwrap_err();
        assert!(
            format!("{err}").contains("Bad credentials"),
            "query errors must not read as clean, and must name the cause: {err}"
        );
    }

    #[test]
    fn missing_data_is_not_a_clean_result() {
        let raw = r"{}";
        let decoded: Response = serde_json::from_str(raw).unwrap();
        GraphQlAdvisories::interpret(decoded).unwrap_err();
    }

    #[test]
    fn a_truncated_page_is_not_a_clean_result() {
        // The set outgrowing one page would otherwise make the advisories that did not
        // fit look like advisories that do not exist — the same false-clean, arriving
        // through pagination rather than a rejected query.
        let raw = r#"{
          "data": {
            "securityVulnerabilities": {
              "totalCount": 120,
              "nodes": [{
                "vulnerableVersionRange": "< 1.0.0",
                "firstPatchedVersion": null,
                "package": { "name": "some/action" },
                "advisory": {
                  "ghsaId": "GHSA-xxxx-yyyy-zzzz",
                  "summary": "one of many",
                  "severity": "LOW",
                  "permalink": "https://example.invalid"
                }
              }]
            }
          }
        }"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        let err = GraphQlAdvisories::interpret(decoded).unwrap_err();
        assert!(
            format!("{err}").contains("120"),
            "the error must say how much was withheld: {err}"
        );
    }

    #[test]
    fn fake_satisfies_the_trait_without_network() {
        // The seam's whole purpose: an advisory-consuming check can be exercised with no
        // network and fully deterministic data.
        let advisory = Advisory {
            package: "actions/checkout".to_owned(),
            ghsa_id: "GHSA-test".to_owned(),
            summary: "test".to_owned(),
            severity: Severity::High,
            permalink: "https://example.invalid".to_owned(),
            vulnerable_range: "< 2.0.0".to_owned(),
            first_patched: Some("2.0.0".to_owned()),
        };
        let fake = FakeAdvisories::new(vec![advisory.clone()]);

        let got = fake.all_actions_advisories().unwrap();

        assert_eq!(got, vec![advisory]);
        assert_eq!(fake.calls(), 1);
    }

    #[test]
    fn fake_can_report_a_failed_lookup() {
        // Checks must be able to test their behavior when the lookup fails, not only
        // when it succeeds.
        let fake = FakeAdvisories::failing();
        fake.all_actions_advisories().unwrap_err();
    }
}
