//! Repository state lookups over GitHub's REST API.
//!
//! A check deciding "archived or not" must be testable offline, hence the seam:
//! [`RepoMetadata`] is what checks depend on, [`RestRepoMetadata`] is the real adapter, and
//! `CannedRepos` below is the double. Same shape as [`super::advisory`].
//!
//! Separate from the advisory seam because it answers a different question against a
//! different endpoint: merging them would force each caller to depend on the half it never
//! calls.

use super::Error;
use super::Registry;
use super::resolve::GITHUB_API_BASE;
use serde::Deserialize;

/// The state of a repository an action is published from, normalized at the integration
/// edge so check logic never touches raw JSON.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoMeta {
    /// Whether GitHub reports the repository as archived — accepting no commits, so no
    /// future version can be published while archived.
    pub archived: bool,
    /// When the repository was last pushed to, as GitHub sends it.
    ///
    /// Kept as the raw string: it is only ever displayed, never compared or sorted, so
    /// parsing it would add a failure mode without buying any behavior.
    pub pushed_at: String,
}

/// Source of repository state for an action.
///
/// Implemented by [`RestRepoMetadata`] (real) and `CannedRepos` (tests).
pub trait RepoMetadata {
    /// The state of `repo`, an `owner/repo` slug.
    ///
    /// Any failure to establish that state is an `Err`, never a default [`RepoMeta`], so a
    /// caller cannot mistake "could not check" for "not archived".
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the request fails, is rejected, or cannot be parsed.
    fn metadata(&self, repo: &str) -> Result<RepoMeta, Error>;
}

/// Real adapter: reads repository state over the shared blocking HTTP client.
pub struct RestRepoMetadata {
    /// The authenticated client. Reused from [`Registry`] so timeout, user-agent, and
    /// token handling stay in one place.
    registry: Registry,
}

/// Wire identity is checked at the edge, not exposed to audit checks.
#[derive(Deserialize)]
struct RepositoryResponse {
    /// GitHub's canonical owner/repository slug.
    full_name: String,
    /// Only the state consumed by checks.
    #[serde(flatten)]
    metadata: RepoMeta,
}

impl RestRepoMetadata {
    /// Wrap an authenticated registry as a repository-state source.
    #[must_use]
    pub const fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// Fetch from the supplied endpoint; kept separate so HTTP behavior is tested offline.
    fn metadata_at(&self, url: &str, repo: &str) -> Result<RepoMeta, Error> {
        let response: RepositoryResponse = self.registry.get_json(url, "repository metadata")?;
        if !response.full_name.eq_ignore_ascii_case(repo) {
            return Err(Error::RepositoryIdentityChanged {
                requested: repo.to_owned(),
                actual: response.full_name,
            });
        }
        Ok(response.metadata)
    }
}

impl RepoMetadata for RestRepoMetadata {
    fn metadata(&self, repo: &str) -> Result<RepoMeta, Error> {
        let url = format!("{GITHUB_API_BASE}/repos/{repo}");
        self.metadata_at(&url, repo)
    }
}

/// The [`RepoMetadata`] test double.
///
/// Lives in this file rather than its own because `src/infra/github/` is at its 8-file
/// budget, which is also why [`super::advisory`] keeps its fake inline.
#[cfg(test)]
pub mod fake {
    use super::{Error, GITHUB_API_BASE, RepoMeta, RepoMetadata};
    use std::cell::RefCell;
    use std::collections::HashSet;

    /// Returns pre-seeded repository state without issuing any request, so the check that
    /// judges whether an action is archived can be unit-tested with no network.
    pub struct CannedRepos {
        /// What every lookup returns. `Err` models a failed request so callers can be
        /// tested on the path where reporting "not archived" would be a lie.
        result: Result<RepoMeta, ()>,
        /// Repositories whose lookups fail instead of returning the default result.
        failures: HashSet<String>,
        /// Slugs passed to `metadata`, in call order. Asserting on these is the only way
        /// to observe that a subpath action was looked up by its base repository.
        pub seen: RefCell<Vec<String>>,
    }

    impl CannedRepos {
        /// A source reporting every repository as archived at `pushed_at`.
        #[must_use]
        pub fn archived(pushed_at: &str) -> Self {
            Self::returning(RepoMeta {
                archived: true,
                pushed_at: pushed_at.to_owned(),
            })
        }

        /// A source reporting every repository as active.
        #[must_use]
        pub fn active() -> Self {
            Self::returning(RepoMeta {
                archived: false,
                pushed_at: "2026-08-01T00:00:00Z".to_owned(),
            })
        }

        /// A source returning `meta` for every lookup.
        #[must_use]
        pub fn returning(meta: RepoMeta) -> Self {
            Self {
                result: Ok(meta),
                failures: HashSet::new(),
                seen: RefCell::new(Vec::new()),
            }
        }

        /// A source whose every lookup fails.
        #[must_use]
        pub fn failing() -> Self {
            Self {
                result: Err(()),
                failures: HashSet::new(),
                seen: RefCell::new(Vec::new()),
            }
        }

        /// Fail this repository's lookup while preserving other repositories' results.
        #[must_use]
        pub fn with_failure(mut self, repo: &str) -> Self {
            self.failures.insert(repo.to_owned());
            self
        }
    }

    impl RepoMetadata for CannedRepos {
        fn metadata(&self, repo: &str) -> Result<RepoMeta, Error> {
            self.seen.borrow_mut().push(repo.to_owned());
            let result = if self.failures.contains(repo) {
                Err(())
            } else {
                self.result.clone()
            };
            result.map_err(|()| Error::NotFound {
                url: format!("{GITHUB_API_BASE}/repos/{repo}"),
            })
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Error, Registry, RepoMeta, RestRepoMetadata};
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    /// Bound local client/server waits so a missing request fails rather than hanging.
    const HTTP_TEST_TIMEOUT: Duration = Duration::from_secs(5);

    /// Serve a fixed response sequence, including redirects, with no external network.
    #[expect(
        clippy::unwrap_in_result,
        reason = "test server setup failures must panic"
    )]
    fn lookup(responses: &[(&str, &str)], repo: &str) -> Result<RepoMeta, Error> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let wire_responses: Vec<String> = responses
            .iter()
            .map(|(status, body)| {
                format!(
                    "HTTP/1.1 {status}\r\nLocation: {base}/repos/new/repo\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            })
            .collect();
        let server = std::thread::spawn(move || {
            for response in wire_responses {
                let started = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) => {
                            assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
                            assert!(
                                started.elapsed() < HTTP_TEST_TIMEOUT,
                                "metadata request did not reach the local server"
                            );
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                };
                stream.set_read_timeout(Some(HTTP_TEST_TIMEOUT)).unwrap();
                stream.set_write_timeout(Some(HTTP_TEST_TIMEOUT)).unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.extend_from_slice(&byte);
                }
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let adapter = RestRepoMetadata::new(Registry {
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(HTTP_TEST_TIMEOUT)
                .build()
                .unwrap(),
            token: None,
        });
        let result = adapter.metadata_at(&format!("{base}/repos/{repo}"), repo);
        server.join().unwrap();
        result
    }

    #[test]
    fn renamed_repository_redirect_is_not_active_metadata() {
        let result = lookup(
            &[
                ("301 Moved Permanently", ""),
                (
                    "200 OK",
                    r#"{"full_name":"new/repo","archived":false,"pushed_at":"2026-08-01T09:15:00Z"}"#,
                ),
            ],
            "old/repo",
        );
        let error = result.unwrap_err();
        assert!(matches!(
            &error,
            Error::RepositoryIdentityChanged { requested, actual }
                if requested == "old/repo" && actual == "new/repo"
        ));
        assert_eq!(
            error.to_string(),
            "GitHub repository old/repo was renamed or transferred to new/repo"
        );
    }

    #[test]
    fn matching_repository_identity_accepts_case_only_changes() {
        for repo in ["actions/checkout", "Actions/CheckOut"] {
            let meta = lookup(
                &[("200 OK", r#"{"full_name":"actions/checkout","archived":false,"pushed_at":"2026-08-01T09:15:00Z"}"#)],
                repo,
            )
            .unwrap();
            assert!(!meta.archived);
            assert_eq!(meta.pushed_at, "2026-08-01T09:15:00Z");
        }
    }

    #[test]
    fn unsuccessful_or_invalid_http_payloads_are_errors() {
        for (status, body) in [
            ("404 Not Found", "{}"),
            ("401 Unauthorized", "{}"),
            ("200 OK", "not JSON"),
            ("200 OK", r#"{"archived":false,"pushed_at":"date"}"#),
            ("200 OK", r#"{"full_name":"old/repo","pushed_at":"date"}"#),
        ] {
            lookup(&[(status, body)], "old/repo").unwrap_err();
        }
    }

    #[test]
    fn parses_the_documented_repository_payload() {
        // Field names asserted against a literal payload in GitHub's documented shape, so
        // a rename breaks this test rather than production. Extra keys are ignored, which
        // is why only the two the check reads appear beyond the noise.
        let raw = r#"{
          "id": 12345,
          "full_name": "actions/setup-ruby",
          "archived": true,
          "disabled": false,
          "pushed_at": "2021-04-14T18:22:31Z"
        }"#;
        let meta: RepoMeta = serde_json::from_str(raw).unwrap();

        assert!(meta.archived);
        assert_eq!(meta.pushed_at, "2021-04-14T18:22:31Z");
    }

    #[test]
    fn parses_an_active_repository() {
        let raw = r#"{"archived": false, "pushed_at": "2026-08-01T09:15:00Z"}"#;
        let meta: RepoMeta = serde_json::from_str(raw).unwrap();

        assert!(!meta.archived);
        assert_eq!(meta.pushed_at, "2026-08-01T09:15:00Z");
    }

    #[test]
    fn a_payload_missing_archived_does_not_decode_as_not_archived() {
        // Defaulting the field would turn an unexpected response into a silent "clean",
        // which is the failure mode this check exists to prevent.
        let raw = r#"{"pushed_at": "2021-04-14T18:22:31Z"}"#;
        serde_json::from_str::<RepoMeta>(raw).unwrap_err();
    }
}
