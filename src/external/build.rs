//! Build script for US-005.
//!
//! Walks `skills/<name>/manifest.yml` and `agents.md/<name>/manifest.yml` from
//! the workspace root and emits a generated Rust file in `OUT_DIR` that
//! `catalog.rs` pulls in via `include!`. Any missing or malformed manifest
//! aborts the build with a message naming the offending file.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    description: String,
    version: String,
    #[serde(default)]
    harness_compatibility: Vec<String>,
}

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"),
    );
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("CARGO_MANIFEST_DIR must live at <repo>/src/external")
        .to_path_buf();

    let skills = scan_dir(&repo_root, "skills");
    let agents_md = scan_dir(&repo_root, "agents.md");

    let mut out = String::new();
    render_section(&mut out, "SKILLS", &skills);
    out.push('\n');
    render_section(&mut out, "AGENTS_MD", &agents_md);

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir).join("catalog_generated.rs");
    fs::write(&out_path, out)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));

    println!("cargo:rerun-if-changed=build.rs");
}

fn scan_dir(repo_root: &Path, label: &str) -> Vec<(String, Manifest)> {
    let dir = repo_root.join(label);
    println!("cargo:rerun-if-changed={}", dir.display());

    let read_dir = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read directory {}: {}", dir.display(), e));

    let mut entries: BTreeMap<String, Manifest> = BTreeMap::new();
    for entry in read_dir {
        let entry = entry.expect("read_dir entry");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let folder_name = path
            .file_name()
            .expect("dir entry has file_name")
            .to_string_lossy()
            .into_owned();

        let manifest_path = path.join("manifest.yml");
        println!("cargo:rerun-if-changed={}", manifest_path.display());

        if !manifest_path.exists() {
            panic!("missing manifest.yml at {}", manifest_path.display());
        }

        let text = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", manifest_path.display(), e));

        let manifest: Manifest = serde_yaml::from_str(&text).unwrap_or_else(|e| {
            panic!("malformed manifest at {}: {}", manifest_path.display(), e)
        });

        if manifest.name != folder_name {
            panic!(
                "manifest name {:?} does not match folder name {:?} at {}",
                manifest.name,
                folder_name,
                manifest_path.display()
            );
        }

        for h in &manifest.harness_compatibility {
            if !matches!(h.as_str(), "claude-code" | "codex" | "opencode") {
                panic!(
                    "manifest at {} has invalid harness_compatibility entry {:?} (allowed: claude-code, codex, opencode)",
                    manifest_path.display(),
                    h
                );
            }
        }

        entries.insert(folder_name, manifest);
    }

    entries.into_iter().collect()
}

fn render_section(out: &mut String, const_name: &str, items: &[(String, Manifest)]) {
    writeln!(out, "pub const {const_name}: &[CatalogEntry] = &[").unwrap();
    for (folder, m) in items {
        let label = if const_name == "SKILLS" {
            "skills"
        } else {
            "agents.md"
        };
        let source_path = format!("{label}/{folder}");
        let hc = m
            .harness_compatibility
            .iter()
            .map(|h| format!("{h:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            out,
            "    CatalogEntry {{ name: {:?}, description: {:?}, version: {:?}, harness_compatibility: &[{}], source_path: {:?} }},",
            m.name, m.description, m.version, hc, source_path
        )
        .unwrap();
    }
    out.push_str("];\n");
}
