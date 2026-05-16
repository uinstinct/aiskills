// Shared manifest validation primitives.
//
// Included by `build.rs` (via `include!`) so the same allow-list governs
// both build-time validation and runtime unit tests.

pub const ALLOWED_HARNESS_COMPATIBILITY: &[&str] =
    &["claude-code", "codex", "opencode", "codebuff"];

pub fn is_allowed_harness(value: &str) -> bool {
    ALLOWED_HARNESS_COMPATIBILITY.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_four_harness_tags() {
        assert!(is_allowed_harness("claude-code"));
        assert!(is_allowed_harness("codex"));
        assert!(is_allowed_harness("opencode"));
        assert!(is_allowed_harness("codebuff"));
    }

    #[test]
    fn rejects_unknown_harness_tags() {
        assert!(!is_allowed_harness("cursor"));
        assert!(!is_allowed_harness(""));
        assert!(!is_allowed_harness("Claude-Code"));
    }
}
