//! Named sets of headers people add to requests in one go, kept per
//! workspace in the git-ignored state directory.
//!
//! They live in `.getman/header-presets.yaml` rather than in the browser
//! or in a committed file: the browser's storage is tied to the port the
//! UI happened to get and vanishes with it, and a committed file would
//! share header values such as tokens with everyone who clones the
//! repository. The file is plain YAML, so it can be edited by hand or
//! copied to another workspace.

use std::fs;
use std::path::PathBuf;

use getman_core::model::KeyValue;
use getman_core::{Error, Workspace};
use serde::{Deserialize, Serialize};

const FILE: &str = "header-presets.yaml";

/// A named set of headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Preset {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) headers: Vec<KeyValue>,
}

fn file(workspace: &Workspace) -> PathBuf {
    workspace.state_dir().join(FILE)
}

/// The workspace's presets; none when the file does not exist.
pub(crate) fn read(workspace: &Workspace) -> Result<Vec<Preset>, Error> {
    let path = file(workspace);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(Error::Io { path, source }),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    getman_core::yaml::from_str(&text).map_err(|source| Error::Yaml { path, source })
}

/// Replaces the workspace's presets.
pub(crate) fn write(workspace: &Workspace, presets: &[Preset]) -> Result<(), Error> {
    for preset in presets {
        if preset.name.trim().is_empty() {
            return Err(Error::Invalid("a header preset needs a name".into()));
        }
    }
    workspace.ensure_state_dir()?;
    let path = file(workspace);
    let text = serde_yaml_ng::to_string(presets).map_err(|source| Error::Yaml {
        path: path.clone(),
        source,
    })?;
    fs::write(&path, text).map_err(|source| Error::Io { path, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_and_need_names() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        assert!(read(&workspace).unwrap().is_empty());
        let presets = vec![Preset {
            name: "Internal".into(),
            headers: vec![KeyValue::new("X-Api-Version", "2")],
        }];
        write(&workspace, &presets).unwrap();
        assert_eq!(read(&workspace).unwrap(), presets);
        assert!(dir.path().join(".getman/header-presets.yaml").is_file());

        let unnamed = vec![Preset {
            name: " ".into(),
            headers: Vec::new(),
        }];
        assert!(write(&workspace, &unnamed).is_err());
    }

    #[test]
    fn hand_written_values_keep_their_text() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        workspace.ensure_state_dir().unwrap();
        fs::write(
            file(&workspace),
            "- name: Versioned\n  headers:\n    - name: X-Api-Version\n      value: 2.0\n",
        )
        .unwrap();
        assert_eq!(read(&workspace).unwrap()[0].headers[0].value, "2.0");
    }
}
