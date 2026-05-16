//! Compile-time catalog of registry entries, embedded by `build.rs` (US-005).
//!
//! The build script reads every `manifest.yml` under `assets/skills/<name>/` and
//! `assets/agents.md/<name>/` at the workspace root and writes a generated file into
//! `OUT_DIR` containing two constants — `SKILLS` and `AGENTS_MD` — each a
//! slice of [`CatalogEntry`]. We `include!` that file below so the catalog is
//! baked into the binary and works fully offline.

#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub harness_compatibility: &'static [&'static str],
    /// Path of the entry inside the registry repo (e.g. `assets/skills/foo`). Kept
    /// for provenance / future debugging — not consumed by install paths
    /// today.
    #[allow(dead_code)]
    pub source_path: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/catalog_generated.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest_validation::is_allowed_harness;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct ManifestUnderTest {
        name: String,
        description: String,
        version: String,
        #[serde(default)]
        harness_compatibility: Vec<String>,
    }

    #[test]
    fn catalog_sections_are_non_empty() {
        assert!(!SKILLS.is_empty(), "SKILLS catalog should not be empty");
        assert!(
            !AGENTS_MD.is_empty(),
            "AGENTS_MD catalog should not be empty"
        );
    }

    #[test]
    fn manifest_with_codebuff_harness_parses_and_validates() {
        let yaml = r#"
name: my-skill
description: A skill that targets Codebuff.
version: 0.1.0
harness_compatibility:
  - codebuff
"#;
        let m: ManifestUnderTest =
            serde_yaml::from_str(yaml).expect("manifest YAML with codebuff should parse");
        assert_eq!(m.name, "my-skill");
        assert_eq!(m.description, "A skill that targets Codebuff.");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.harness_compatibility, vec!["codebuff"]);
        for h in &m.harness_compatibility {
            assert!(
                is_allowed_harness(h),
                "codebuff must be accepted by the build-time validator"
            );
        }
    }
}
