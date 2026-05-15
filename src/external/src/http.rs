//! HTTP layer (ureq + rustls) for fetching release assets. Implemented in US-008.

#![allow(dead_code)]

use std::fmt;
use std::io::Read;
use std::time::Duration;

/// Default base URL for GitHub release downloads. Production code calls
/// [`download_release_asset`], which fixes this; tests use
/// [`download_release_asset_from`] to redirect at a mock server.
const GITHUB_BASE_URL: &str = "https://github.com";

/// Default base URL for the GitHub REST API. Used by
/// [`fetch_latest_release_json`] for the update-check probe (US-015).
const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// User-Agent header the GitHub API requires. GitHub rejects requests
/// without a UA; this is the public identifier of the running binary.
const USER_AGENT: &str =
    concat!("instinctagents/", env!("CARGO_PKG_VERSION"), " (+https://github.com/uinstinct/aiskills)");

/// Errors returned by the HTTP layer.
#[derive(Debug)]
pub enum HttpError {
    /// The asset path returned a 404 from the server.
    AssetNotFound { url: String },
    /// Network-level failure (DNS, connection refused, timeout, TLS).
    NetworkUnreachable { url: String },
    /// Non-success status that isn't 404 (e.g. 5xx, 403).
    UnexpectedStatus { url: String, status: u16 },
    /// I/O error reading the response body after the request succeeded.
    BodyRead { url: String },
    /// Response was 200 OK but the body didn't match the expected shape
    /// (e.g. GitHub API JSON without a `tag_name` field).
    Malformed { url: String },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::AssetNotFound { url } => {
                write!(f, "asset not found: {url}")
            }
            HttpError::NetworkUnreachable { url } => {
                write!(f, "failed to reach github.com ({url})")
            }
            HttpError::UnexpectedStatus { url, status } => {
                write!(f, "unexpected HTTP status {status} for {url}")
            }
            HttpError::BodyRead { url } => {
                write!(f, "failed to read response body from {url}")
            }
            HttpError::Malformed { url } => {
                write!(f, "unexpected response body shape from {url}")
            }
        }
    }
}

impl std::error::Error for HttpError {}

/// Download a release asset from the canonical GitHub Releases URL.
///
/// `repo` is `"<owner>/<repo>"` (e.g. `"uinstinct/aiskills"`). The fetched URL
/// is `https://github.com/<repo>/releases/download/<tag>/<asset_name>`.
pub fn download_release_asset(
    repo: &str,
    tag: &str,
    asset_name: &str,
) -> Result<Vec<u8>, HttpError> {
    download_release_asset_from(GITHUB_BASE_URL, repo, tag, asset_name)
}

/// Same as [`download_release_asset`] but with the base URL injected, for
/// pointing at a mock server in tests.
pub fn download_release_asset_from(
    base_url: &str,
    repo: &str,
    tag: &str,
    asset_name: &str,
) -> Result<Vec<u8>, HttpError> {
    let url = format!(
        "{base}/{repo}/releases/download/{tag}/{asset_name}",
        base = base_url.trim_end_matches('/'),
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .build();

    match agent.get(&url).call() {
        Ok(resp) => {
            let mut buf = Vec::new();
            resp.into_reader()
                .read_to_end(&mut buf)
                .map_err(|_| HttpError::BodyRead { url: url.clone() })?;
            Ok(buf)
        }
        Err(ureq::Error::Status(404, _)) => Err(HttpError::AssetNotFound { url }),
        Err(ureq::Error::Status(status, _)) => {
            Err(HttpError::UnexpectedStatus { url, status })
        }
        Err(ureq::Error::Transport(_)) => Err(HttpError::NetworkUnreachable { url }),
    }
}

/// Fetch the JSON body of `GET <api>/repos/<repo>/releases/latest`.
///
/// `repo` is `"<owner>/<repo>"` (e.g. `"uinstinct/aiskills"`). The body is
/// returned as a UTF-8 string; the caller (US-015's `update::parse_tag_name`)
/// is responsible for extracting `tag_name`.
pub fn fetch_latest_release_json(repo: &str) -> Result<String, HttpError> {
    fetch_latest_release_json_from(GITHUB_API_BASE_URL, repo)
}

/// Same as [`fetch_latest_release_json`] but with the base URL injected, for
/// pointing at a mock server in tests.
pub fn fetch_latest_release_json_from(base_url: &str, repo: &str) -> Result<String, HttpError> {
    let url = format!(
        "{base}/repos/{repo}/releases/latest",
        base = base_url.trim_end_matches('/'),
    );

    // Shorter timeouts here than the asset-download path: this is a passive
    // probe that runs on every TUI launch and must not block the UI when
    // the network is slow.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build();

    match agent
        .get(&url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(resp) => resp
            .into_string()
            .map_err(|_| HttpError::BodyRead { url: url.clone() }),
        Err(ureq::Error::Status(404, _)) => Err(HttpError::AssetNotFound { url }),
        Err(ureq::Error::Status(status, _)) => {
            Err(HttpError::UnexpectedStatus { url, status })
        }
        Err(ureq::Error::Transport(_)) => Err(HttpError::NetworkUnreachable { url }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_asset_successfully() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                "/uinstinct/aiskills/releases/download/v0.1.0/foo.tar.gz",
            )
            .with_status(200)
            .with_body(b"hello-release-asset")
            .create();

        let bytes = download_release_asset_from(
            &server.url(),
            "uinstinct/aiskills",
            "v0.1.0",
            "foo.tar.gz",
        )
        .expect("download should succeed");

        assert_eq!(bytes, b"hello-release-asset");
        mock.assert();
    }

    #[test]
    fn missing_asset_returns_asset_not_found() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock(
                "GET",
                "/uinstinct/aiskills/releases/download/v0.1.0/missing.tar.gz",
            )
            .with_status(404)
            .create();

        let err = download_release_asset_from(
            &server.url(),
            "uinstinct/aiskills",
            "v0.1.0",
            "missing.tar.gz",
        )
        .expect_err("download should fail with 404");

        assert!(
            matches!(err, HttpError::AssetNotFound { .. }),
            "expected AssetNotFound, got {err:?}"
        );
        assert!(
            err.to_string().contains("asset not found"),
            "Display should mention 'asset not found', got: {err}"
        );
    }

    #[test]
    fn url_is_constructed_correctly_from_components() {
        let mut server = mockito::Server::new();
        // Mock matches only this exact path; if the URL is built wrong, the
        // request will 501 and we'll fail.
        let mock = server
            .mock(
                "GET",
                "/owner/repo/releases/download/v1.2.3/asset-with-dashes.bin",
            )
            .with_status(200)
            .with_body(&[0u8, 1, 2, 3, 4][..])
            .create();

        let bytes = download_release_asset_from(
            &server.url(),
            "owner/repo",
            "v1.2.3",
            "asset-with-dashes.bin",
        )
        .expect("download should succeed");

        assert_eq!(bytes, vec![0u8, 1, 2, 3, 4]);
        mock.assert();
    }

    #[test]
    fn trailing_slash_on_base_url_is_tolerated() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                "/uinstinct/aiskills/releases/download/v0.1.0/asset.bin",
            )
            .with_status(200)
            .with_body(b"ok")
            .create();

        let with_slash = format!("{}/", server.url());
        let bytes =
            download_release_asset_from(&with_slash, "uinstinct/aiskills", "v0.1.0", "asset.bin")
                .expect("download should succeed");

        assert_eq!(bytes, b"ok");
        mock.assert();
    }

    #[test]
    fn fetch_latest_release_json_returns_body() {
        let mut server = mockito::Server::new();
        let body = r#"{"tag_name":"v0.2.0","name":"Release 0.2.0"}"#;
        let mock = server
            .mock("GET", "/repos/uinstinct/aiskills/releases/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();

        let got = fetch_latest_release_json_from(&server.url(), "uinstinct/aiskills")
            .expect("github api call should succeed");
        assert_eq!(got, body);
        mock.assert();
    }

    #[test]
    fn fetch_latest_release_json_propagates_404() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/repos/unknown/unknown/releases/latest")
            .with_status(404)
            .create();

        let err = fetch_latest_release_json_from(&server.url(), "unknown/unknown")
            .expect_err("404 must surface as AssetNotFound");
        assert!(matches!(err, HttpError::AssetNotFound { .. }));
    }

    #[test]
    fn fetch_latest_release_json_propagates_5xx() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/repos/u/r/releases/latest")
            .with_status(500)
            .create();

        let err = fetch_latest_release_json_from(&server.url(), "u/r")
            .expect_err("5xx must surface as UnexpectedStatus");
        match err {
            HttpError::UnexpectedStatus { status, .. } => assert_eq!(status, 500),
            other => panic!("expected UnexpectedStatus, got {other:?}"),
        }
    }
}
