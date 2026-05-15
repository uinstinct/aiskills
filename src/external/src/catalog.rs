//! Compile-time catalog of registry entries, embedded by `build.rs` (US-005).
//!
//! The build script reads every `manifest.yml` under `skills/<name>/` and
//! `agents.md/<name>/` at the workspace root and writes a generated file into
//! `OUT_DIR` containing two constants — `SKILLS` and `AGENTS_MD` — each a
//! slice of [`CatalogEntry`]. We `include!` that file below so the catalog is
//! baked into the binary and works fully offline.

#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub harness_compatibility: &'static [&'static str],
    /// Path of the entry inside the registry repo (e.g. `skills/foo`). Kept
    /// for provenance / future debugging — not consumed by install paths
    /// today.
    #[allow(dead_code)]
    pub source_path: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/catalog_generated.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_sections_are_non_empty() {
        assert!(!SKILLS.is_empty(), "SKILLS catalog should not be empty");
        assert!(
            !AGENTS_MD.is_empty(),
            "AGENTS_MD catalog should not be empty"
        );
    }
}
