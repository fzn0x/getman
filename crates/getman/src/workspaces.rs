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

/// The workspace to open the UI on: the one containing `dir` or the
/// current directory, else the personal workspace, created on first use.
pub(crate) fn find_or_personal(dir: Option<&Path>) -> Result<Workspace, String> {
    if let Some(dir) = dir {
        return open_or_create(dir);
    }
    if let Ok(workspace) = find(None) {
        return Ok(workspace);
    }
    personal()
}

/// The workspace containing `dir`, or a new one created in it.
pub fn open_or_create(dir: &Path) -> Result<Workspace, String> {
    let dir = absolute(dir);
    if let Some(root) = Workspace::discover(&dir) {
        return Workspace::open(root).map_err(|e| e.to_string());
    }
    let name = dir
        .file_name()
        .map_or_else(|| "My API".to_owned(), |n| n.to_string_lossy().into_owned());
    let workspace = Workspace::init(&dir, &name).map_err(|e| e.to_string())?;
    crate::style::note(&format!("Created a workspace in {}", dir.display()));
    Ok(workspace)
}

/// The personal workspace, created with example requests on first use.
pub fn personal() -> Result<Workspace, String> {
    let personal =
        personal_dir().ok_or("cannot find your data directory for the personal workspace")?;
    if Workspace::discover(&personal).is_some() {
        return Workspace::open(&personal).map_err(|e| e.to_string());
    }
    let workspace = Workspace::init(&personal, "My workspace").map_err(|e| e.to_string())?;
    crate::examples::add(&workspace).map_err(|e| e.to_string())?;
    crate::style::note(&format!(
        "Created your personal workspace in {}",
        personal.display()
    ));
    Ok(workspace)
}

/// Where the personal workspace lives.
pub(crate) fn personal_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("getman").join("workspace"))
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
