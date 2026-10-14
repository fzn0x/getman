//! Finding the workspace a command works on.

use std::path::{Path, PathBuf};

use getman_core::Workspace;
use getman_core::workspace::slugify;

/// The workspace in `dir`, or the one containing the current directory.
pub(crate) fn find(dir: Option<&Path>) -> Result<Workspace, String> {
    let start = match dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|e| format!("cannot read the current directory: {e}"))?,
    };
    let root = Workspace::discover(&absolute(&start)).ok_or_else(|| {
        format!(
            "no workspace here: {} has no getman.yaml, nor does any directory above it. Create one with `getman init`.",
            start.display()
        )
    })?;
    Workspace::open(root).map_err(|e| e.to_string())
}

/// Turns an item given on the command line, as a path in the workspace or
/// a file or directory on disk, into its path in the workspace.
pub(crate) fn item_path(workspace: &Workspace, given: &str) -> Result<String, String> {
    let on_disk = Path::new(given);
    if on_disk.exists() {
        let path = on_disk
            .canonicalize()
            .map_err(|e| format!("{given}: {e}"))?;
        let root = workspace
            .root()
            .canonicalize()
            .map_err(|e| format!("{}: {e}", workspace.root().display()))?;
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| format!("{given} is not inside the workspace at {}", root.display()))?;
        return Ok(relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"));
    }
    Ok(given.replace('\\', "/").trim_matches('/').to_owned())
}

/// Finds an environment by id or display name.
pub(crate) fn environment(
    workspace: &Workspace,
    given: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(given) = given.filter(|g| !g.is_empty()) else {
        return Ok(None);
    };
    let environments = workspace.environments().map_err(|e| e.to_string())?;
    let found = environments
        .iter()
        .find(|env| env.id == given)
        .or_else(|| {
            environments
                .iter()
                .find(|env| env.name.eq_ignore_ascii_case(given))
        })
        .or_else(|| environments.iter().find(|env| env.id == slugify(given)));
    match found {
        Some(env) => Ok(Some(env.id.clone())),
        None => {
            let names: Vec<_> = environments.iter().map(|e| e.id.as_str()).collect();
            Err(if names.is_empty() {
                format!("there is no environment {given:?}; this workspace has none")
            } else {
                format!(
                    "there is no environment {given:?}; choose one of {}",
                    names.join(", ")
                )
            })
        }
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
