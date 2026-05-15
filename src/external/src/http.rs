//! HTTP layer (ureq + rustls) for fetching release assets. Implemented in US-008.

#![allow(dead_code)]

use std::fmt;
use std::io::Read;
use std::time::Duration;

/// Default base URL for GitHub release downloads. Production code calls
/// [`download_release_asset`], which fixes this; tests use
/// [`download_release_asset_from`] to redirect at a mock server.
const GITHUB_BASE_URL: &str = "https://github.com";

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
}
