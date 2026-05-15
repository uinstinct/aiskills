//! Per-project .instinctagents state file. Implemented in US-009.

#![allow(dead_code)]

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const STATE_FILE_NAME: &str = ".instinctagents";
const TMP_SUFFIX: &str = ".tmp";

// Field order matters for TOML serialization: scalar/optional fields must
// appear before vectors (arrays-of-tables) at the top level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_known_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub installed_skills: Vec<InstalledItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub installed_agents_md: Vec<InstalledItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledItem {
    pub name: String,
    pub version: String,
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter_id: Option<String>,
}

#[derive(Debug)]
pub enum StateError {
    Io(io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Io(e) => write!(f, "failed to read or write .instinctagents: {e}"),
            StateError::Parse(e) => write!(f, "failed to parse .instinctagents: {e}"),
            StateError::Serialize(e) => write!(f, "failed to serialize .instinctagents: {e}"),
        }
    }
}

impl std::error::Error for StateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StateError::Io(e) => Some(e),
            StateError::Parse(e) => Some(e),
            StateError::Serialize(e) => Some(e),
        }
    }
}

impl From<io::Error> for StateError {
    fn from(e: io::Error) -> Self {
        StateError::Io(e)
    }
}

impl ProjectState {
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(STATE_FILE_NAME)
    }

    pub fn load(project_root: &Path) -> Result<Self, StateError> {
        let path = Self::path(project_root);
        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).map_err(StateError::Parse),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(StateError::Io(e)),
        }
    }

    pub fn save(&self, project_root: &Path) -> Result<(), StateError> {
        let path = Self::path(project_root);
        let tmp = project_root.join(format!("{STATE_FILE_NAME}{TMP_SUFFIX}"));
        let serialized = toml::to_string(self).map_err(StateError::Serialize)?;
        fs::write(&tmp, serialized)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn skill_item() -> InstalledItem {
        InstalledItem {
            name: "storytelling-mastery-skill".to_string(),
            version: "0.1.0".to_string(),
            source_url: "https://github.com/uinstinct/aiskills".to_string(),
            install_path: Some(".claude/skills/storytelling-mastery-skill/".to_string()),
            delimiter_id: None,
        }
    }

    fn agents_md_item() -> InstalledItem {
        InstalledItem {
            name: "grill-me".to_string(),
            version: "0.1.0".to_string(),
            source_url: "https://github.com/uinstinct/aiskills".to_string(),
            install_path: None,
            delimiter_id: Some("grill-me".to_string()),
        }
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = TempDir::new().unwrap();
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state, ProjectState::default());
        assert!(state.installed_skills.is_empty());
        assert!(state.installed_agents_md.is_empty());
        assert!(state.last_update_check.is_none());
        assert!(state.latest_known_version.is_none());
        assert!(!ProjectState::path(dir.path()).exists());
    }

    #[test]
    fn load_existing_parses_full_file() {
        let dir = TempDir::new().unwrap();
        let content = r#"
last_update_check = "2026-05-15T12:34:56Z"
latest_known_version = "0.2.0"

[[installed_skills]]
name = "storytelling-mastery-skill"
version = "0.1.0"
source_url = "https://github.com/uinstinct/aiskills"
install_path = ".claude/skills/storytelling-mastery-skill/"

[[installed_agents_md]]
name = "grill-me"
version = "0.1.0"
source_url = "https://github.com/uinstinct/aiskills"
delimiter_id = "grill-me"
"#;
        fs::write(ProjectState::path(dir.path()), content).unwrap();

        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_skills, vec![skill_item()]);
        assert_eq!(state.installed_agents_md, vec![agents_md_item()]);
        assert_eq!(state.latest_known_version.as_deref(), Some("0.2.0"));
        assert_eq!(
            state.last_update_check,
            Some(Utc.with_ymd_and_hms(2026, 5, 15, 12, 34, 56).unwrap())
        );
    }

    #[test]
    fn save_and_roundtrip_preserves_state() {
        let dir = TempDir::new().unwrap();
        let original = ProjectState {
            last_update_check: Some(Utc.with_ymd_and_hms(2026, 5, 15, 12, 34, 56).unwrap()),
            latest_known_version: Some("0.2.0".to_string()),
            installed_skills: vec![skill_item()],
            installed_agents_md: vec![agents_md_item()],
        };
        original.save(dir.path()).unwrap();

        assert!(ProjectState::path(dir.path()).exists());
        let tmp = dir.path().join(format!("{STATE_FILE_NAME}{TMP_SUFFIX}"));
        assert!(!tmp.exists(), "temp file should be renamed away");

        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn save_empty_state_roundtrips() {
        let dir = TempDir::new().unwrap();
        let original = ProjectState::default();
        original.save(dir.path()).unwrap();
        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn save_overwrites_existing_file_atomically() {
        let dir = TempDir::new().unwrap();
        let mut state = ProjectState {
            latest_known_version: Some("0.1.0".to_string()),
            ..Default::default()
        };
        state.save(dir.path()).unwrap();

        state.latest_known_version = Some("0.2.0".to_string());
        state.installed_skills.push(skill_item());
        state.save(dir.path()).unwrap();

        let loaded = ProjectState::load(dir.path()).unwrap();
        assert_eq!(loaded.latest_known_version.as_deref(), Some("0.2.0"));
        assert_eq!(loaded.installed_skills, vec![skill_item()]);
    }

    #[test]
    fn load_invalid_toml_returns_parse_error() {
        let dir = TempDir::new().unwrap();
        fs::write(ProjectState::path(dir.path()), "this is = not = valid").unwrap();
        let err = ProjectState::load(dir.path()).unwrap_err();
        assert!(matches!(err, StateError::Parse(_)));
    }

    #[test]
    fn path_joins_state_filename() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            ProjectState::path(dir.path()),
            dir.path().join(".instinctagents")
        );
    }
}
