//! GitHub security advisory lookups for GitHub Actions.
//!
//! Advisories are the one piece of gx's knowledge that changes without the repository
//! changing, so the checks that consume them must be testable without the network — a
//! check that decides "vulnerable or not" and is only ever exercised against the live API
//! is a check nobody can trust. Hence the seam:
//!
//! - [`AdvisoryQuery`] is what checks depend on.
//! - [`GraphQlAdvisories`] is the real adapter, querying GitHub's GraphQL API.
//! - `FakeAdvisories` is the `#[cfg(test)]` double returning canned advisories.
//!
//! Modeled on [`crate::infra::shellcheck`], which establishes the same trait + adapter +
//! fake shape.
//!
//! GraphQL rather than REST because `securityVulnerabilities` accepts an ecosystem filter
//! and returns affected version ranges in one round trip. It is also why `gx audit`
//! requires a token: this endpoint rejects unauthenticated requests outright.

use super::Error;
use super::Registry;
use serde::{Deserialize, Serialize};

/// GitHub's GraphQL endpoint.
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// The advisory query, parameterized by the action's repository slug.
///
/// `ACTIONS` is GitHub's ecosystem name for GitHub Actions. Note that filtering by version
/// is deliberately NOT done server-side: the caller compares the locked version against
/// `vulnerableVersionRange` itself, because the API's version filter has a history of
/// returning empty results — a false-negative that would silently report "clean".
const ADVISORY_QUERY: &str = "\
query($package: String!) {
  securityVulnerabilities(ecosystem: ACTIONS, package: $package, first: 100) {
    nodes {
      vulnerableVersionRange
      firstPatchedVersion { identifier }
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
    /// All advisories affecting `package`, a `owner/repo` slug.
    ///
    /// An empty vec means "no known advisories" — a positive statement that the lookup
    /// happened and found nothing. Any failure to establish that is an `Err`, never an
    /// empty vec, so a caller cannot mistake "could not check" for "clean".
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the request fails, is rejected, or cannot be parsed.
    fn advisories(&self, package: &str) -> Result<Vec<Advisory>, Error>;
}

/// The GraphQL request body: a query plus its variables.
#[derive(Serialize)]
struct Request<'req> {
    /// The GraphQL query document.
    query: &'req str,
    /// Bound variables for the query.
    variables: Variables<'req>,
}

/// Variables bound into [`ADVISORY_QUERY`].
#[derive(Serialize)]
struct Variables<'vars> {
    /// The `owner/repo` slug to look up.
    package: &'vars str,
}

/// Top-level GraphQL response envelope.
#[derive(Deserialize)]
struct Response {
    /// The `data` half; absent when the query itself failed.
    data: Option<ResponseData>,
    /// Query-level errors. GraphQL reports these with HTTP 200, so they must be checked
    /// explicitly or a failed query reads as an empty — and therefore clean — result.
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
    /// The advisory this vulnerability belongs to.
    advisory: AdvisoryNode,
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
        // GraphQL signals query failures with HTTP 200 and an `errors` array. Treating
        // that as "no advisories" is exactly the silent false-clean this command exists
        // to prevent, so it is an error.
        if let Some(first) = response.errors.first() {
            return Err(Error::ApiError {
                status: 200,
                url: format!("{GRAPHQL_URL} ({})", first.message),
            });
        }
        let Some(data) = response.data else {
            return Err(Error::ApiError {
                status: 200,
                url: format!("{GRAPHQL_URL} (response contained no data)"),
            });
        };
        Ok(data
            .security_vulnerabilities
            .nodes
            .into_iter()
            .map(|node| Advisory {
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
    fn advisories(&self, package: &str) -> Result<Vec<Advisory>, Error> {
        let body = Request {
            query: ADVISORY_QUERY,
            variables: Variables { package },
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
mod fake {
    use super::{Advisory, AdvisoryQuery, Error, GRAPHQL_URL};
    use std::cell::RefCell;

    /// Returns pre-seeded advisories without issuing any request, so checks that judge
    /// whether an action is vulnerable can be unit-tested with no network and fully
    /// deterministic data.
    pub struct FakeAdvisories {
        /// What every lookup returns. `Err` models a failed query so callers can be
        /// tested on the path where the lookup did not succeed — the path where
        /// reporting "clean" would be a lie.
        result: Result<Vec<Advisory>, ()>,
        /// Packages passed to `advisories`, in call order, so a test can assert which
        /// actions were actually looked up.
        pub seen: RefCell<Vec<String>>,
    }

    impl FakeAdvisories {
        /// A source that returns `advisories` for every lookup.
        pub fn new(advisories: Vec<Advisory>) -> Self {
            Self {
                result: Ok(advisories),
                seen: RefCell::new(Vec::new()),
            }
        }

        /// A source whose every lookup fails.
        pub fn failing() -> Self {
            Self {
                result: Err(()),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl AdvisoryQuery for FakeAdvisories {
        fn advisories(&self, package: &str) -> Result<Vec<Advisory>, Error> {
            self.seen.borrow_mut().push(package.to_owned());
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
        Severity, Variables,
    };

    #[test]
    fn request_body_carries_query_and_variables() {
        let body = Request {
            query: ADVISORY_QUERY,
            variables: Variables {
                package: "actions/checkout",
            },
        };
        let json: serde_json::Value = serde_json::to_value(&body).unwrap();

        // The two keys GitHub's GraphQL endpoint requires.
        assert!(json.get("query").is_some());
        assert_eq!(json["variables"]["package"], "actions/checkout");
        // The ecosystem filter is what scopes this to GitHub Actions at all.
        assert!(
            json["query"]
                .as_str()
                .unwrap()
                .contains("ecosystem: ACTIONS")
        );
    }

    #[test]
    fn parses_a_vulnerability_payload() {
        let raw = r#"{
          "data": {
            "securityVulnerabilities": {
              "nodes": [{
                "vulnerableVersionRange": "< 45.0.7",
                "firstPatchedVersion": { "identifier": "45.0.7" },
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
        assert_eq!(advisories[0].ghsa_id, "GHSA-mrrh-fwg8-r2c3");
        assert_eq!(advisories[0].severity, Severity::High);
        assert_eq!(advisories[0].vulnerable_range, "< 45.0.7");
        assert_eq!(advisories[0].first_patched.as_deref(), Some("45.0.7"));
    }

    #[test]
    fn parses_an_empty_result_as_no_advisories() {
        let raw = r#"{"data": {"securityVulnerabilities": {"nodes": []}}}"#;
        let decoded: Response = serde_json::from_str(raw).unwrap();
        assert!(GraphQlAdvisories::interpret(decoded).unwrap().is_empty());
    }

    #[test]
    fn advisory_without_patch_parses() {
        let raw = r#"{
          "data": {
            "securityVulnerabilities": {
              "nodes": [{
                "vulnerableVersionRange": ">= 0",
                "firstPatchedVersion": null,
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
    fn fake_satisfies_the_trait_without_network() {
        // The seam's whole purpose: an advisory-consuming check can be exercised with no
        // network and fully deterministic data.
        let advisory = Advisory {
            ghsa_id: "GHSA-test".to_owned(),
            summary: "test".to_owned(),
            severity: Severity::High,
            permalink: "https://example.invalid".to_owned(),
            vulnerable_range: "< 2.0.0".to_owned(),
            first_patched: Some("2.0.0".to_owned()),
        };
        let fake = FakeAdvisories::new(vec![advisory.clone()]);

        let got = fake.advisories("actions/checkout").unwrap();

        assert_eq!(got, vec![advisory]);
        assert_eq!(fake.seen.borrow().as_slice(), ["actions/checkout"]);
    }

    #[test]
    fn fake_can_report_a_failed_lookup() {
        // Checks must be able to test their behavior when the lookup fails, not only
        // when it succeeds.
        let fake = FakeAdvisories::failing();
        fake.advisories("actions/checkout").unwrap_err();
    }
}
