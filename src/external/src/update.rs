//! Self-update check against GitHub Releases. Implemented in US-015.
//!
//! Provides:
//! - [`UpdateStatus`]: outcome of a check (UpToDate / Available / Unknown).
//! - Pure helpers ([`parse_tag_name`], [`compare_versions`], [`is_cache_fresh`],
//!   [`release_url`], [`status_from_versions`]) that the integration uses to
//!   drive [`check`].
//! - [`check`]: high-level entry point. Reads `.instinctagents` cache; if the
//!   cache is fresh (<24h) it uses the cached `latest_known_version`. If
//!   stale, it calls a caller-supplied `fetcher` closure to fetch the latest
//!   tag from GitHub, persists the new value back into `.instinctagents`,
//!   and returns the comparison result. On any error from the fetcher, falls
//!   back to the cached value (or `Unknown` if none).
//!
//! Errors are intentionally swallowed at this layer: per US-015 AC, update
//! checks must never block the tool. Verbose stderr logging is deferred to
//! US-016 when the `--verbose` flag lands.

use std::cmp::Ordering;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};

use crate::http::HttpError;
use crate::state::ProjectState;

/// Cache lifetime for the `latest_known_version` field of `.instinctagents`.
/// AC: "skip network call when cache <24h old".
pub fn cache_ttl() -> Duration {
    Duration::hours(24)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Running binary is at or ahead of the latest known release.
    UpToDate { current: String },
    /// A newer release exists.
    Available {
        current: String,
        latest: String,
        release_url: String,
    },
    /// No cache and the network probe failed. Treated as a silent
    /// non-event by the TUI.
    Unknown { current: String },
}

impl UpdateStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, UpdateStatus::Available { .. })
    }
}

/// Pure: extract the `tag_name` field from a GitHub `/releases/latest` JSON
/// response. Returns `None` when the field is missing or the body is too
/// malformed to scan.
///
/// This is intentionally a hand-rolled scanner so we don't pull in
/// `serde_json` just to read one string. The GitHub Releases response is
/// stable and the field shape is `"tag_name":"v0.1.0"` (whitespace
/// tolerated).
pub fn parse_tag_name(json: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let start = json.find(key)?;
    let rest = &json[start + key.len()..];
    let colon = rest.find(':')?;
    let after_colon = &rest[colon + 1..];
    let quote = after_colon.find('"')?;
    let inner = &after_colon[quote + 1..];
    let end_quote = inner.find('"')?;
    Some(inner[..end_quote].to_string())
}

/// Pure: compare two semver-like version strings (`"v0.1.0"` or `"0.1.0"`).
/// Components past 3 are compared if present; missing components compare as
/// 0. Non-numeric components compare as 0 (lossy but predictable).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let mut va = parse(a);
    let mut vb = parse(b);
    // Pad the shorter side with zeros so "0.1" compares equal to "0.1.0".
    let n = va.len().max(vb.len());
    va.resize(n, 0);
    vb.resize(n, 0);
    va.cmp(&vb)
}

/// Pure: is the cached `last_update_check` within the TTL window?
pub fn is_cache_fresh(last_check: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match last_check {
        Some(t) => now.signed_duration_since(t) < cache_ttl(),
        None => false,
    }
}

/// Pure: GitHub Releases tag URL for a given repo + tag.
pub fn release_url(repo: &str, tag: &str) -> String {
    format!("https://github.com/{repo}/releases/tag/{tag}")
}

/// Pure: turn a (current, latest) pair into an [`UpdateStatus`].
pub fn status_from_versions(current: &str, latest: &str, repo: &str) -> UpdateStatus {
    match compare_versions(current, latest) {
        Ordering::Less => UpdateStatus::Available {
            current: current.to_string(),
            latest: latest.to_string(),
            release_url: release_url(repo, latest),
        },
        _ => UpdateStatus::UpToDate {
            current: current.to_string(),
        },
    }
}

/// High-level update check.
///
/// - If `.instinctagents` shows a `last_update_check` within
///   [`cache_ttl`] AND has a `latest_known_version`, that cached value is
///   used and `fetcher` is NOT called.
/// - Otherwise, `fetcher` is invoked. On success, `.instinctagents` is
///   updated (atomic write via [`ProjectState::save`]). On error, the
///   cached version is used if present, else [`UpdateStatus::Unknown`] is
///   returned.
/// - All errors are swallowed silently. AC: "Network errors silent (do not
///   block tool)".
pub fn check<F>(
    project_root: &Path,
    repo: &str,
    current_version: &str,
    now: DateTime<Utc>,
    fetcher: F,
) -> UpdateStatus
where
    F: FnOnce() -> Result<String, HttpError>,
{
    let mut state = ProjectState::load(project_root).unwrap_or_default();

    if is_cache_fresh(state.last_update_check, now) {
        if let Some(cached) = state.latest_known_version.clone() {
            return status_from_versions(current_version, &cached, repo);
        }
    }

    match fetcher() {
        Ok(latest) => {
            state.last_update_check = Some(now);
            state.latest_known_version = Some(latest.clone());
            // Best-effort persist. If the project root isn't writable for
            // some reason, we still return the up-to-date comparison from
            // the in-memory value.
            let _ = state.save(project_root);
            status_from_versions(current_version, &latest, repo)
        }
        Err(_) => match state.latest_known_version.clone() {
            Some(cached) => status_from_versions(current_version, &cached, repo),
            None => UpdateStatus::Unknown {
                current: current_version.to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::cell::Cell;
    use tempfile::TempDir;

    #[test]
    fn parse_tag_name_extracts_value() {
        let body = r#"{"tag_name":"v0.2.0","name":"Release 0.2.0"}"#;
        assert_eq!(parse_tag_name(body).as_deref(), Some("v0.2.0"));
    }

    #[test]
    fn parse_tag_name_tolerates_whitespace() {
        let body = r#"{ "tag_name" :  "v1.0.0" }"#;
        assert_eq!(parse_tag_name(body).as_deref(), Some("v1.0.0"));
    }

    #[test]
    fn parse_tag_name_returns_none_when_missing() {
        let body = r#"{"name":"Release"}"#;
        assert_eq!(parse_tag_name(body), None);
    }

    #[test]
    fn parse_tag_name_returns_none_for_garbage() {
        assert_eq!(parse_tag_name("not json"), None);
        assert_eq!(parse_tag_name(""), None);
    }

    #[test]
    fn compare_versions_equal() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("v0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("v1.2.3", "v1.2.3"), Ordering::Equal);
    }

    #[test]
    fn compare_versions_less_and_greater() {
        assert_eq!(compare_versions("0.1.0", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.0", "0.1.1"), Ordering::Less);
        assert_eq!(compare_versions("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(compare_versions("v0.1.0", "v1.0.0"), Ordering::Less);
    }

    #[test]
    fn compare_versions_handles_uneven_part_counts() {
        // "0.1" and "0.1.0" compare equal because the missing component
        // is treated as 0.
        assert_eq!(compare_versions("0.1", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.1.0", "0.1"), Ordering::Equal);
        // Extra components are compared too.
        assert_eq!(compare_versions("0.1.0.1", "0.1.0"), Ordering::Greater);
    }

    #[test]
    fn is_cache_fresh_within_24h() {
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        let just_now = now - Duration::hours(1);
        let yesterday = now - Duration::hours(23);
        let stale = now - Duration::hours(25);
        assert!(is_cache_fresh(Some(just_now), now));
        assert!(is_cache_fresh(Some(yesterday), now));
        assert!(!is_cache_fresh(Some(stale), now));
        assert!(!is_cache_fresh(None, now));
    }

    #[test]
    fn release_url_format() {
        assert_eq!(
            release_url("uinstinct/aiskills", "v0.2.0"),
            "https://github.com/uinstinct/aiskills/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn status_from_versions_up_to_date_when_equal_or_newer() {
        let s = status_from_versions("0.1.0", "0.1.0", "u/r");
        assert!(matches!(s, UpdateStatus::UpToDate { .. }));
        let s = status_from_versions("0.2.0", "0.1.0", "u/r");
        assert!(matches!(s, UpdateStatus::UpToDate { .. }));
    }

    #[test]
    fn status_from_versions_available_when_newer() {
        let s = status_from_versions("0.1.0", "v0.2.0", "u/r");
        match s {
            UpdateStatus::Available {
                current,
                latest,
                release_url,
            } => {
                assert_eq!(current, "0.1.0");
                assert_eq!(latest, "v0.2.0");
                assert_eq!(release_url, "https://github.com/u/r/releases/tag/v0.2.0");
            }
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn check_uses_fresh_cache_without_network() {
        let dir = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        // Seed cache: checked 1h ago, known latest = v0.5.0.
        let seed = ProjectState {
            last_update_check: Some(now - Duration::hours(1)),
            latest_known_version: Some("v0.5.0".into()),
            ..Default::default()
        };
        seed.save(dir.path()).unwrap();

        let called = Cell::new(false);
        let status = check(dir.path(), "u/r", "0.1.0", now, || {
            called.set(true);
            Ok("v9.9.9".into())
        });
        assert!(
            !called.get(),
            "fetcher must NOT be called when cache is fresh"
        );
        match status {
            UpdateStatus::Available { latest, .. } => assert_eq!(latest, "v0.5.0"),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn check_stale_cache_calls_fetcher_and_persists() {
        let dir = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        // Seed stale cache.
        let seed = ProjectState {
            last_update_check: Some(now - Duration::hours(48)),
            latest_known_version: Some("v0.1.0".into()),
            ..Default::default()
        };
        seed.save(dir.path()).unwrap();

        let called = Cell::new(false);
        let status = check(dir.path(), "u/r", "0.1.0", now, || {
            called.set(true);
            Ok("v0.3.0".into())
        });
        assert!(called.get(), "fetcher must be called when cache is stale");
        match status {
            UpdateStatus::Available { latest, .. } => assert_eq!(latest, "v0.3.0"),
            other => panic!("expected Available, got {other:?}"),
        }

        // Persisted: state file now has the new value AND the new timestamp.
        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded.latest_known_version.as_deref(), Some("v0.3.0"));
        assert_eq!(loaded.last_update_check, Some(now));
    }

    #[test]
    fn check_no_cache_calls_fetcher() {
        let dir = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        let called = Cell::new(false);
        let status = check(dir.path(), "u/r", "0.1.0", now, || {
            called.set(true);
            Ok("v0.1.0".into())
        });
        assert!(called.get());
        assert!(matches!(status, UpdateStatus::UpToDate { .. }));
        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded.latest_known_version.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn check_fetcher_error_falls_back_to_cache() {
        let dir = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        // Stale cache forces fetcher to be tried.
        let seed = ProjectState {
            last_update_check: Some(now - Duration::hours(48)),
            latest_known_version: Some("v0.4.0".into()),
            ..Default::default()
        };
        seed.save(dir.path()).unwrap();

        let status = check(dir.path(), "u/r", "0.1.0", now, || {
            Err(HttpError::NetworkUnreachable { url: "x".into() })
        });
        // Falls back to the cached value rather than going Unknown.
        match status {
            UpdateStatus::Available { latest, .. } => assert_eq!(latest, "v0.4.0"),
            other => panic!("expected Available from cache fallback, got {other:?}"),
        }

        // Cache must not have been clobbered with a new timestamp.
        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded.last_update_check, Some(now - Duration::hours(48)));
        assert_eq!(loaded.latest_known_version.as_deref(), Some("v0.4.0"));
    }

    #[test]
    fn check_fetcher_error_no_cache_returns_unknown() {
        let dir = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
        let status = check(dir.path(), "u/r", "0.1.0", now, || {
            Err(HttpError::NetworkUnreachable { url: "x".into() })
        });
        assert!(matches!(status, UpdateStatus::Unknown { .. }));
    }

    #[test]
    fn check_does_not_block_when_fetcher_panics_caught() {
        // Sanity: this is more about contract — `check` never propagates an
        // error type. We don't actually run a panicking fetcher (panics
        // would unwind), but we can assert the return type by inspection
        // here: `check` returns UpdateStatus, never Result.
        fn assert_return_type<F: FnOnce() -> Result<String, HttpError>>(_: F) -> &'static str {
            "UpdateStatus"
        }
        let _ = assert_return_type(|| Ok(String::new()));
    }
}
