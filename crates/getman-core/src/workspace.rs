//! A workspace on disk: a directory with `getman.yaml` at its root.
//!
//! ```text
//! my-api/
//!   getman.yaml              workspace settings: variables, auth, scripts
//!   getman.local.yaml        git-ignored values for this machine
//!   environments/
//!     staging.yaml           shared values, committed
//!     staging.local.yaml     git-ignored values: secrets, tokens
//!   users/
//!     _folder.yaml           folder settings (optional)
//!     create-user.yaml       a request
//! ```
//!
//! Items are addressed by their path relative to the root with `/`
//! separators: `users` is a folder, `users/create-user.yaml` a request and
//! the empty string the workspace itself.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::lenient;
use crate::model::{Environment, Folder, Request, Vars};

/// File that marks the root of a workspace and holds its settings.
pub const WORKSPACE_FILE: &str = "getman.yaml";
/// File that holds a folder's settings.
pub const FOLDER_FILE: &str = "_folder.yaml";
/// Directory of environment files, at the workspace root.
pub const ENVIRONMENTS_DIR: &str = "environments";
/// Directory of Getman's own state, such as history. Git-ignored.
pub const STATE_DIR: &str = ".getman";

const LOCAL_SUFFIX: &str = ".local.yaml";
const LOCAL_WORKSPACE_FILE: &str = "getman.local.yaml";
const SKIPPED_DIRS: &[&str] = &["node_modules", "target"];
const MAX_DEPTH: usize = 32;
const GITIGNORE: &str =
    "# Values for this machine only, and Getman's local state\n*.local.yaml\n.getman/\n";

/// A workspace rooted at a directory.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

/// An item in the workspace tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    /// A folder and its items, in order.
    Folder {
        /// Path relative to the workspace root.
        path: String,
        /// Display name.
        name: String,
        /// Items in the folder, in order.
        children: Vec<Node>,
    },
    /// A request.
    Request {
        /// Path relative to the workspace root.
        path: String,
        /// Display name.
        name: String,
        /// HTTP method.
        method: String,
        /// Why the file could not be read, if it could not.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl Node {
    /// Path of the item relative to the workspace root.
    pub fn path(&self) -> &str {
        match self {
            Self::Folder { path, .. } | Self::Request { path, .. } => path,
        }
    }

    /// Display name of the item.
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } | Self::Request { name, .. } => name,
        }
    }
}

/// An environment's name as listed, without its values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvironmentSummary {
    /// File name without extension; how the environment is addressed.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// An environment's shared file and the values kept on this machine only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentFiles {
    /// The committed file.
    pub shared: Environment,
    /// Values from the git-ignored `<id>.local.yaml`. They take precedence
    /// over the shared values.
    #[serde(deserialize_with = "lenient::vars")]
    pub local: Vars,
}

impl EnvironmentFiles {
    /// The values requests see: shared values overridden by local ones.
    pub fn effective(&self) -> Vars {
        let mut vars = self.shared.variables.clone();
        for (name, value) in &self.local {
            vars.insert(name.clone(), value.clone());
        }
        vars
    }
}

/// Values for the whole workspace kept on this machine only, in
/// `getman.local.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalState {
    /// Overrides of the workspace variables.
    #[serde(
        skip_serializing_if = "Vars::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub variables: Vars,
    /// Global variables, the lowest-precedence scope.
    #[serde(
        skip_serializing_if = "Vars::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub globals: Vars,
}

#[derive(Debug, Default, Deserialize)]
struct LocalEnvironment {
    #[serde(default, deserialize_with = "lenient::vars")]
    variables: Vars,
}

#[derive(Serialize)]
struct LocalEnvironmentRef<'a> {
    variables: &'a Vars,
}

impl Workspace {
    /// Opens the workspace rooted at `root`, which must hold `getman.yaml`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let file = root.join(WORKSPACE_FILE);
        if !file.is_file() {
            return Err(Error::NotFound(file.display().to_string()));
        }
        Ok(Self { root })
    }

    /// Finds the workspace containing `start`: the nearest ancestor holding
    /// `getman.yaml`, `start` included.
    pub fn discover(start: &Path) -> Option<PathBuf> {
        start
            .ancestors()
            .find(|dir| dir.join(WORKSPACE_FILE).is_file())
            .map(Path::to_path_buf)
    }

    /// Creates a workspace in `root`, which may already hold other files,
    /// and git-ignores its machine-only files.
    pub fn init(root: impl Into<PathBuf>, name: &str) -> Result<Self> {
        let root = root.into();
        let file = root.join(WORKSPACE_FILE);
        if file.exists() {
            return Err(Error::AlreadyExists(file.display().to_string()));
        }
        fs::create_dir_all(&root).map_err(|e| Error::io(&root, e))?;
        write_yaml(
            &file,
            &Folder {
                name: name.to_owned(),
                ..Folder::default()
            },
        )?;
        let ignore = root.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, GITIGNORE).map_err(|e| Error::io(&ignore, e))?;
        }
        Ok(Self { root })
    }

    /// The workspace's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory for Getman's own state, such as history.
    pub fn state_dir(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    /// The workspace's display name.
    pub fn name(&self) -> String {
        match self.read_folder("") {
            Ok(meta) if !meta.name.is_empty() => meta.name,
            _ => dir_name(&self.root),
        }
    }

    /// Resolves a path relative to the root, refusing anything that could
    /// leave the workspace or name a hidden file.
    pub fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let mut path = self.root.clone();
        if rel.is_empty() {
            return Ok(path);
        }
        for part in rel.split('/') {
            if part.is_empty()
                || part.starts_with('.')
                || part.contains(['\\', ':', '\0'])
                || part.len() > 255
            {
                return Err(Error::InvalidPath(rel.to_owned()));
            }
            path.push(part);
        }
        Ok(path)
    }

    // ---- Tree -----------------------------------------------------------

    /// All folders and requests, in order.
    pub fn tree(&self) -> Result<Vec<Node>> {
        let meta = self.read_folder("")?;
        Ok(Self::children(&self.root, "", &meta.order, 0))
    }

    fn children(dir: &Path, rel: &str, order: &[String], depth: usize) -> Vec<Node> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut items: Vec<(String, Node)> = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_name) = entry.file_name().into_string() else {
                continue;
            };
            if file_name.starts_with(['.', '_']) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                if (rel.is_empty() && file_name == ENVIRONMENTS_DIR)
                    || SKIPPED_DIRS.contains(&file_name.as_str())
                    || file_name.ends_with(crate::examples::DIR_SUFFIX)
                    || depth >= MAX_DEPTH
                {
                    continue;
                }
                let meta_path = path.join(FOLDER_FILE);
                let has_meta = meta_path.is_file();
                let meta: Folder = if has_meta {
                    read_yaml(&meta_path).unwrap_or_default()
                } else {
                    Folder::default()
                };
                let child_rel = join(rel, &file_name);
                let children = Self::children(&path, &child_rel, &meta.order, depth + 1);
                if !has_meta && children.is_empty() {
                    continue;
                }
                let name = if meta.name.is_empty() {
                    file_name.clone()
                } else {
                    meta.name
                };
                items.push((
                    file_name,
                    Node::Folder {
                        path: child_rel,
                        name,
                        children,
                    },
                ));
            } else if file_type.is_file() {
                if rel.is_empty() && file_name == WORKSPACE_FILE {
                    continue;
                }
                let Some(stem) = request_stem(&file_name) else {
                    continue;
                };
                let stem = stem.to_owned();
                let node = match peek_request(&path) {
                    Ok(Some(request)) => Node::Request {
                        path: join(rel, &file_name),
                        name: if request.name.is_empty() {
                            stem.clone()
                        } else {
                            request.name
                        },
                        method: request.method,
                        error: None,
                    },
                    Ok(None) => continue,
                    Err(message) => Node::Request {
                        path: join(rel, &file_name),
                        name: stem.clone(),
                        method: String::new(),
                        error: Some(message),
                    },
                };
                items.push((stem, node));
            }
        }
        items.sort_by_cached_key(|(key, node)| {
            (
                order.iter().position(|o| o == key).unwrap_or(usize::MAX),
                matches!(node, Node::Request { .. }),
                key.to_lowercase(),
            )
        });
        items.into_iter().map(|(_, node)| node).collect()
    }

    // ---- Requests -------------------------------------------------------

    /// Reads a request.
    pub fn read_request(&self, rel: &str) -> Result<Request> {
        let path = self.request_path(rel)?;
        if !path.is_file() {
            return Err(Error::NotFound(rel.to_owned()));
        }
        let mut request: Request = read_yaml(&path)?;
        if request.name.is_empty() {
            request.name = request_stem(file_name(rel)).unwrap_or(rel).to_owned();
        }
        Ok(request)
    }

    /// Saves an existing request.
    pub fn write_request(&self, rel: &str, request: &Request) -> Result<()> {
        let path = self.request_path(rel)?;
        if !path.is_file() {
            return Err(Error::NotFound(rel.to_owned()));
        }
        write_yaml(&path, request)
    }

    /// Creates a request named `name` in a folder and returns its path.
    pub fn create_request(&self, folder: &str, name: &str, request: &Request) -> Result<String> {
        let dir = self.folder_dir(folder)?;
        let stem = unique_stem(&dir, &slugify(name), None);
        let request = Request {
            name: name.to_owned(),
            ..request.clone()
        };
        write_yaml(&dir.join(format!("{stem}.yaml")), &request)?;
        self.add_to_order(folder, &stem, None)?;
        Ok(join(folder, &format!("{stem}.yaml")))
    }

    fn request_path(&self, rel: &str) -> Result<PathBuf> {
        let (parent, file) = split(rel);
        let reserved = file == FOLDER_FILE
            || (parent.is_empty() && file == WORKSPACE_FILE)
            || is_environments(parent)
            || request_stem(file).is_none();
        if reserved {
            return Err(Error::InvalidPath(rel.to_owned()));
        }
        self.resolve(rel)
    }

    // ---- Folders --------------------------------------------------------

    /// Reads a folder's settings; the empty path reads the workspace's.
    pub fn read_folder(&self, rel: &str) -> Result<Folder> {
        let dir = self.folder_dir(rel)?;
        let file = folder_file(&dir, rel);
        if file.is_file() {
            read_yaml(&file)
        } else {
            Ok(Folder {
                name: dir_name(&dir),
                ..Folder::default()
            })
        }
    }

    /// Saves a folder's settings; the empty path saves the workspace's.
    pub fn write_folder(&self, rel: &str, folder: &Folder) -> Result<()> {
        let dir = self.folder_dir(rel)?;
        write_yaml(&folder_file(&dir, rel), folder)
    }

    /// Creates a folder named `name` inside `parent` and returns its path.
    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String> {
        let dir = self.folder_dir(parent)?;
        let stem = unique_stem(&dir, &slugify(name), None);
        let path = dir.join(&stem);
        fs::create_dir(&path).map_err(|e| Error::io(&path, e))?;
        write_yaml(
            &path.join(FOLDER_FILE),
            &Folder {
                name: name.to_owned(),
                ..Folder::default()
            },
        )?;
        self.add_to_order(parent, &stem, None)?;
        Ok(join(parent, &stem))
    }

    /// The settings of every folder enclosing a request, from the
    /// workspace down to the request's own folder.
    pub fn folder_chain(&self, rel: &str) -> Result<Vec<Folder>> {
        let (parent, _) = split(rel);
        let mut chain = vec![self.read_folder("")?];
        let mut current = String::new();
        if !parent.is_empty() {
            for part in parent.split('/') {
                current = join(&current, part);
                chain.push(self.read_folder(&current)?);
            }
        }
        Ok(chain)
    }

    /// Sets the order of a folder's items, by file or directory name
    /// without extension.
    pub fn set_order(&self, folder: &str, order: Vec<String>) -> Result<()> {
        let mut meta = self.read_folder(folder)?;
        meta.order = order;
        self.write_folder(folder, &meta)
    }

    fn folder_dir(&self, rel: &str) -> Result<PathBuf> {
        if is_environments(rel) {
            return Err(Error::InvalidPath(rel.to_owned()));
        }
        let dir = self.resolve(rel)?;
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(Error::NotFound(rel.to_owned()))
        }
    }

    // ---- Any item -------------------------------------------------------

    /// Renames a request or folder, renaming its file to match, and
    /// returns its new path. The empty path renames the workspace.
    pub fn rename(&self, rel: &str, name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a name cannot be empty".into()));
        }
        if rel.is_empty() {
            let mut meta = self.read_folder("")?;
            meta.name = name.to_owned();
            self.write_folder("", &meta)?;
            return Ok(String::new());
        }
        let (parent, file) = split(rel);
        let parent_dir = self.folder_dir(parent)?;
        let old_key = item_key(file).to_owned();
        let slug = slugify(name);
        let is_request = request_stem(file).is_some();
        let new_key = if slug == old_key {
            slug
        } else {
            unique_stem(&parent_dir, &slug, Some(file))
        };
        if is_request {
            let mut request = self.read_request(rel)?;
            request.name = name.to_owned();
            let new_rel = join(parent, &format!("{new_key}.yaml"));
            write_yaml(&self.resolve(&new_rel)?, &request)?;
            if new_rel != rel {
                let old = self.resolve(rel)?;
                fs::remove_file(&old).map_err(|e| Error::io(&old, e))?;
                move_examples(&old, &self.resolve(&new_rel)?)?;
            }
            self.replace_in_order(parent, &old_key, &new_key)?;
            Ok(new_rel)
        } else {
            let mut meta = self.read_folder(rel)?;
            meta.name = name.to_owned();
            self.write_folder(rel, &meta)?;
            let new_rel = join(parent, &new_key);
            if new_rel != rel {
                let (old, new) = (self.resolve(rel)?, self.resolve(&new_rel)?);
                fs::rename(&old, &new).map_err(|e| Error::io(&old, e))?;
            }
            self.replace_in_order(parent, &old_key, &new_key)?;
            Ok(new_rel)
        }
    }

    /// Moves a request or folder into another folder and returns its new
    /// path.
    pub fn move_item(&self, rel: &str, to_folder: &str) -> Result<String> {
        if rel.is_empty() {
            return Err(Error::Invalid("the workspace cannot be moved".into()));
        }
        let (parent, file) = split(rel);
        if parent == to_folder {
            return Ok(rel.to_owned());
        }
        let is_request = request_stem(file).is_some();
        if !is_request && (to_folder == rel || to_folder.starts_with(&format!("{rel}/"))) {
            return Err(Error::Invalid(
                "a folder cannot be moved into itself".into(),
            ));
        }
        let source = if is_request {
            self.request_path(rel)?
        } else {
            self.folder_dir(rel)?
        };
        if !source.exists() {
            return Err(Error::NotFound(rel.to_owned()));
        }
        let dest_dir = self.folder_dir(to_folder)?;
        let key = item_key(file);
        let new_key = unique_stem(&dest_dir, key, None);
        let new_file = if is_request {
            format!("{new_key}.yaml")
        } else {
            new_key.clone()
        };
        let dest = dest_dir.join(&new_file);
        fs::rename(&source, &dest).map_err(|e| Error::io(&source, e))?;
        if is_request {
            move_examples(&source, &dest)?;
        }
        self.remove_from_order(parent, key)?;
        self.add_to_order(to_folder, &new_key, None)?;
        Ok(join(to_folder, &new_file))
    }

    /// Deletes a request, or a folder and everything in it.
    pub fn delete(&self, rel: &str) -> Result<()> {
        if rel.is_empty() {
            return Err(Error::Invalid("the workspace cannot be deleted".into()));
        }
        let (parent, file) = split(rel);
        if request_stem(file).is_some() {
            let path = self.request_path(rel)?;
            fs::remove_file(&path).map_err(|e| not_found_or_io(rel, &path, e))?;
            let examples = crate::examples::dir_for(&path);
            if examples.is_dir() {
                fs::remove_dir_all(&examples).map_err(|e| Error::io(&examples, e))?;
            }
        } else {
            let path = self.folder_dir(rel)?;
            fs::remove_dir_all(&path).map_err(|e| Error::io(&path, e))?;
        }
        self.remove_from_order(parent, item_key(file))
    }

    /// Copies a request or folder next to the original, named
    /// "<name> copy", and returns the copy's path.
    pub fn duplicate(&self, rel: &str) -> Result<String> {
        let (parent, file) = split(rel);
        let key = item_key(file).to_owned();
        if request_stem(file).is_some() {
            let request = self.read_request(rel)?;
            let name = format!("{} copy", request.name);
            let dir = self.folder_dir(parent)?;
            let stem = unique_stem(&dir, &slugify(&name), None);
            let copy = dir.join(format!("{stem}.yaml"));
            write_yaml(&copy, &Request { name, ..request })?;
            let examples = crate::examples::dir_for(&self.resolve(rel)?);
            if examples.is_dir() {
                copy_dir(&examples, &crate::examples::dir_for(&copy))?;
            }
            self.add_to_order(parent, &stem, Some(&key))?;
            Ok(join(parent, &format!("{stem}.yaml")))
        } else {
            let source = self.folder_dir(rel)?;
            let meta = self.read_folder(rel)?;
            let name = format!("{} copy", meta.name);
            let dir = self.folder_dir(parent)?;
            let stem = unique_stem(&dir, &slugify(&name), None);
            let dest = dir.join(&stem);
            copy_dir(&source, &dest)?;
            write_yaml(&dest.join(FOLDER_FILE), &Folder { name, ..meta })?;
            self.add_to_order(parent, &stem, Some(&key))?;
            Ok(join(parent, &stem))
        }
    }

    fn add_to_order(&self, folder: &str, key: &str, after: Option<&str>) -> Result<()> {
        let mut meta = self.read_folder(folder)?;
        if meta.order.is_empty() {
            return Ok(());
        }
        let at = after
            .and_then(|after| meta.order.iter().position(|k| k == after))
            .map_or(meta.order.len(), |i| i + 1);
        meta.order.insert(at, key.to_owned());
        self.write_folder(folder, &meta)
    }

    fn remove_from_order(&self, folder: &str, key: &str) -> Result<()> {
        let mut meta = self.read_folder(folder)?;
        let before = meta.order.len();
        meta.order.retain(|k| k != key);
        if meta.order.len() == before {
            return Ok(());
        }
        self.write_folder(folder, &meta)
    }

    fn replace_in_order(&self, folder: &str, old: &str, new: &str) -> Result<()> {
        if old == new {
            return Ok(());
        }
        let mut meta = self.read_folder(folder)?;
        let Some(slot) = meta.order.iter_mut().find(|k| *k == old) else {
            return Ok(());
        };
        new.clone_into(slot);
        self.write_folder(folder, &meta)
    }

    // ---- Environments ---------------------------------------------------

    /// The workspace's environments, by name.
    pub fn environments(&self) -> Result<Vec<EnvironmentSummary>> {
        let dir = self.root.join(ENVIRONMENTS_DIR);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut list = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_name) = entry.file_name().into_string() else {
                continue;
            };
            if file_name.starts_with(['.', '_']) {
                continue;
            }
            let Some(id) = request_stem(&file_name) else {
                continue;
            };
            let env: Environment = read_yaml(&entry.path()).unwrap_or_default();
            list.push(EnvironmentSummary {
                id: id.to_owned(),
                name: if env.name.is_empty() {
                    id.to_owned()
                } else {
                    env.name
                },
            });
        }
        list.sort_by_cached_key(|env| env.name.to_lowercase());
        Ok(list)
    }

    /// Reads an environment's shared and local values.
    pub fn read_environment(&self, id: &str) -> Result<EnvironmentFiles> {
        let (shared_path, local_path) = self.environment_paths(id)?;
        if !shared_path.is_file() {
            return Err(Error::NotFound(format!("environment {id}")));
        }
        let mut shared: Environment = read_yaml(&shared_path)?;
        if shared.name.is_empty() {
            id.clone_into(&mut shared.name);
        }
        let local = if local_path.is_file() {
            read_yaml::<LocalEnvironment>(&local_path)?.variables
        } else {
            Vars::new()
        };
        Ok(EnvironmentFiles { shared, local })
    }

    /// Saves an environment. Values of secret variables never reach the
    /// shared file: they are moved to the local one.
    pub fn write_environment(&self, id: &str, files: &EnvironmentFiles) -> Result<()> {
        let (shared_path, local_path) = self.environment_paths(id)?;
        if !shared_path.is_file() {
            return Err(Error::NotFound(format!("environment {id}")));
        }
        let mut shared = files.shared.clone();
        let mut local = files.local.clone();
        for secret in &shared.secrets {
            if let Some(value) = shared.variables.get_mut(secret)
                && !value.is_empty()
            {
                local.entry(secret.clone()).or_insert_with(|| value.clone());
                value.clear();
            }
        }
        if shared.name == id {
            shared.name.clear();
        }
        write_yaml(&shared_path, &shared)?;
        if local.is_empty() {
            if local_path.exists() {
                fs::remove_file(&local_path).map_err(|e| Error::io(&local_path, e))?;
            }
            Ok(())
        } else {
            write_yaml(&local_path, &LocalEnvironmentRef { variables: &local })
        }
    }

    /// Sets variables in an environment's local file, keeping its other
    /// values. This is where scripts' `gm.environment.set` lands.
    pub fn set_local_environment_vars(&self, id: &str, vars: &Vars) -> Result<()> {
        if vars.is_empty() {
            return Ok(());
        }
        let mut files = self.read_environment(id)?;
        for (name, value) in vars {
            files.local.insert(name.clone(), value.clone());
        }
        let (_, local_path) = self.environment_paths(id)?;
        write_yaml(
            &local_path,
            &LocalEnvironmentRef {
                variables: &files.local,
            },
        )
    }

    /// Creates an empty environment named `name` and returns its id.
    pub fn create_environment(&self, name: &str) -> Result<String> {
        let dir = self.root.join(ENVIRONMENTS_DIR);
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let id = unique_stem(&dir, &slugify(name), None);
        let env = Environment {
            name: if slugify(name) == id && name == id {
                String::new()
            } else {
                name.to_owned()
            },
            ..Environment::default()
        };
        write_yaml(&dir.join(format!("{id}.yaml")), &env)?;
        Ok(id)
    }

    /// Renames an environment, renaming its files to match, and returns its
    /// new id.
    pub fn rename_environment(&self, id: &str, name: &str) -> Result<String> {
        let mut files = self.read_environment(id)?;
        let dir = self.root.join(ENVIRONMENTS_DIR);
        let slug = slugify(name);
        let new_id = if slug == id {
            slug
        } else {
            unique_stem(&dir, &slug, Some(&format!("{id}.yaml")))
        };
        name.clone_into(&mut files.shared.name);
        if new_id != id {
            let (old_shared, old_local) = self.environment_paths(id)?;
            let (new_shared, new_local) = self.environment_paths(&new_id)?;
            fs::rename(&old_shared, &new_shared).map_err(|e| Error::io(&old_shared, e))?;
            if old_local.exists() {
                fs::rename(&old_local, &new_local).map_err(|e| Error::io(&old_local, e))?;
            }
        }
        self.write_environment(&new_id, &files)?;
        Ok(new_id)
    }

    /// Deletes an environment and its local values.
    pub fn delete_environment(&self, id: &str) -> Result<()> {
        let (shared, local) = self.environment_paths(id)?;
        fs::remove_file(&shared).map_err(|e| not_found_or_io(id, &shared, e))?;
        if local.exists() {
            fs::remove_file(&local).map_err(|e| Error::io(&local, e))?;
        }
        Ok(())
    }

    fn environment_paths(&self, id: &str) -> Result<(PathBuf, PathBuf)> {
        let local_like = Path::new(id)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("local"));
        if id.is_empty() || id.contains('/') || local_like {
            return Err(Error::InvalidPath(id.to_owned()));
        }
        let dir = self.resolve(ENVIRONMENTS_DIR)?;
        let shared = self.resolve(&format!("{ENVIRONMENTS_DIR}/{id}.yaml"))?;
        Ok((shared, dir.join(format!("{id}{LOCAL_SUFFIX}"))))
    }

    // ---- Local state ----------------------------------------------------

    /// Reads the workspace's machine-only values.
    pub fn read_local_state(&self) -> Result<LocalState> {
        let path = self.root.join(LOCAL_WORKSPACE_FILE);
        if path.is_file() {
            read_yaml(&path)
        } else {
            Ok(LocalState::default())
        }
    }

    /// Saves the workspace's machine-only values.
    pub fn write_local_state(&self, state: &LocalState) -> Result<()> {
        let path = self.root.join(LOCAL_WORKSPACE_FILE);
        if *state == LocalState::default() {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
            }
            return Ok(());
        }
        write_yaml(&path, state)
    }
}

/// Turns a display name into a file name: lowercase letters and digits
/// joined by dashes.
pub fn slugify(name: &str) -> String {
    // Names Windows reserves for devices, whatever the extension.
    const RESERVED: &[&str] = &["con", "prn", "aux", "nul"];

    let mut slug = String::new();
    let mut gap = false;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !slug.is_empty() {
                slug.push('-');
            }
            gap = false;
            slug.push(c);
        } else {
            gap = true;
        }
        if slug.chars().count() >= 60 {
            break;
        }
    }
    if slug.is_empty() {
        return "untitled".to_owned();
    }
    let device = RESERVED.contains(&slug.as_str())
        || ((slug.starts_with("com") || slug.starts_with("lpt"))
            && slug.len() == 4
            && slug.as_bytes()[3].is_ascii_digit());
    if device {
        slug.push_str("-item");
    }
    slug
}

/// Reads and parses a YAML file.
pub(crate) fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    if text.trim().is_empty() {
        return serde_yaml_ng::from_str("{}").map_err(|source| Error::Yaml {
            path: path.to_owned(),
            source,
        });
    }
    serde_yaml_ng::from_str(&text).map_err(|source| Error::Yaml {
        path: path.to_owned(),
        source,
    })
}

/// Writes a YAML file atomically, so a crash never leaves half a file.
pub(crate) fn write_yaml<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    let text = serde_yaml_ng::to_string(value).map_err(|source| Error::Yaml {
        path: path.to_owned(),
        source,
    })?;
    let tmp = path.with_file_name(format!(".{}.tmp", file_name_of(path)));
    fs::write(&tmp, text).map_err(|e| Error::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(path, e)
    })
}

/// Reads a file if it is a request: a YAML mapping with a `url` or
/// `method`. Other YAML files are not requests and give `None`.
fn peek_request(path: &Path) -> Result<Option<Request>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).map_err(|e| e.to_string())?;
    let is_request = value
        .as_mapping()
        .is_some_and(|map| map.contains_key("url") || map.contains_key("method"));
    if !is_request {
        return Ok(None);
    }
    serde_yaml_ng::from_value(value)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn request_stem(file_name: &str) -> Option<&str> {
    if file_name.ends_with(LOCAL_SUFFIX) || file_name == FOLDER_FILE {
        return None;
    }
    file_name
        .strip_suffix(".yaml")
        .or_else(|| file_name.strip_suffix(".yml"))
        .filter(|stem| !stem.is_empty())
}

/// The name an item has in its folder's `order`: a request's file name
/// without extension, or a folder's directory name.
fn item_key(file_name: &str) -> &str {
    request_stem(file_name).unwrap_or(file_name)
}

/// A name for a new item in `dir` based on `stem` that no other item uses.
/// `ignore` is a file that may keep its own name.
fn unique_stem(dir: &Path, stem: &str, ignore: Option<&str>) -> String {
    let taken = |candidate: &str| {
        [
            candidate.to_owned(),
            format!("{candidate}.yaml"),
            format!("{candidate}.yml"),
        ]
        .iter()
        .any(|name| Some(name.as_str()) != ignore && dir.join(name).exists())
    };
    if !taken(stem) {
        return stem.to_owned();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn folder_file(dir: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        dir.join(WORKSPACE_FILE)
    } else {
        dir.join(FOLDER_FILE)
    }
}

fn is_environments(rel: &str) -> bool {
    rel == ENVIRONMENTS_DIR || rel.starts_with(&format!("{ENVIRONMENTS_DIR}/"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to).map_err(|e| Error::io(to, e))?;
    for entry in fs::read_dir(from).map_err(|e| Error::io(from, e))? {
        let entry = entry.map_err(|e| Error::io(from, e))?;
        let target = to.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| Error::io(entry.path(), e))?;
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target).map_err(|e| Error::io(&target, e))?;
        }
    }
    Ok(())
}

/// Moves the directory of example bodies that belongs to a request file
/// whose file moved from `from` to `to`.
fn move_examples(from: &Path, to: &Path) -> Result<()> {
    let (old, new) = (crate::examples::dir_for(from), crate::examples::dir_for(to));
    if !old.is_dir() || old == new {
        return Ok(());
    }
    if new.exists() {
        fs::remove_dir_all(&new).map_err(|e| Error::io(&new, e))?;
    }
    fs::rename(&old, &new).map_err(|e| Error::io(&old, e))
}

fn not_found_or_io(rel: &str, path: &Path, e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        Error::NotFound(rel.to_owned())
    } else {
        Error::io(path, e)
    }
}

fn split(rel: &str) -> (&str, &str) {
    rel.rsplit_once('/').unwrap_or(("", rel))
}

fn file_name(rel: &str) -> &str {
    split(rel).1
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

fn dir_name(dir: &Path) -> String {
    dir.file_name().map_or_else(
        || "workspace".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyValue;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Test API").unwrap();
        (dir, ws)
    }

    fn get(name: &str) -> Request {
        Request {
            url: format!("https://example.com/{name}"),
            ..Request::default()
        }
    }

    fn paths(nodes: &[Node]) -> Vec<String> {
        let mut out = Vec::new();
        for node in nodes {
            out.push(node.path().to_owned());
            if let Node::Folder { children, .. } = node {
                out.extend(paths(children));
            }
        }
        out
    }

    #[test]
    fn init_writes_the_workspace_file_and_ignores_local_files() {
        let (dir, ws) = workspace();
        assert_eq!(ws.name(), "Test API");
        let ignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(ignore.contains("*.local.yaml"));
        assert!(Workspace::init(dir.path(), "again").is_err());
        assert_eq!(
            Workspace::discover(&dir.path().join("a/b")).as_deref(),
            Some(dir.path())
        );
    }

    #[test]
    fn the_tree_lists_folders_first_then_requests() {
        let (_dir, ws) = workspace();
        ws.create_request("", "Zebra", &get("z")).unwrap();
        ws.create_request("", "Apple", &get("a")).unwrap();
        let users = ws.create_folder("", "Users").unwrap();
        ws.create_request(&users, "List users", &get("users"))
            .unwrap();
        let tree = ws.tree().unwrap();
        assert_eq!(
            paths(&tree),
            ["users", "users/list-users.yaml", "apple.yaml", "zebra.yaml"]
        );
        assert_eq!(tree[0].name(), "Users");
    }

    #[test]
    fn the_folder_order_comes_first() {
        let (_dir, ws) = workspace();
        ws.create_request("", "One", &get("1")).unwrap();
        ws.create_request("", "Two", &get("2")).unwrap();
        ws.create_request("", "Three", &get("3")).unwrap();
        ws.set_order("", vec!["two".into(), "one".into()]).unwrap();
        assert_eq!(
            paths(&ws.tree().unwrap()),
            ["two.yaml", "one.yaml", "three.yaml"]
        );
        // New items join the end of an explicit order.
        ws.create_request("", "Four", &get("4")).unwrap();
        assert_eq!(ws.read_folder("").unwrap().order, ["two", "one", "four"]);
    }

    #[test]
    fn other_yaml_files_are_not_requests() {
        let (dir, ws) = workspace();
        fs::write(dir.path().join("compose.yaml"), "services: {}\n").unwrap();
        fs::write(dir.path().join("broken.yaml"), "url: [\n").unwrap();
        fs::create_dir(dir.path().join("empty")).unwrap();
        let tree = ws.tree().unwrap();
        assert_eq!(paths(&tree), ["broken.yaml"]);
        let Node::Request { error, .. } = &tree[0] else {
            panic!()
        };
        assert!(error.is_some());
    }

    #[test]
    fn requests_round_trip() {
        let (_dir, ws) = workspace();
        let path = ws.create_request("", "Get thing", &get("x")).unwrap();
        let mut request = ws.read_request(&path).unwrap();
        assert_eq!(request.name, "Get thing");
        request.headers.push(KeyValue::new("Accept", "text/plain"));
        ws.write_request(&path, &request).unwrap();
        assert_eq!(ws.read_request(&path).unwrap(), request);
        assert!(matches!(
            ws.write_request("nope.yaml", &request),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn paths_cannot_leave_the_workspace() {
        let (_dir, ws) = workspace();
        for bad in [
            "../x.yaml",
            "a/../../x.yaml",
            ".git/config",
            "a//b.yaml",
            "C:/x.yaml",
            "a\\b.yaml",
        ] {
            assert!(ws.read_request(bad).is_err(), "{bad}");
        }
        assert!(matches!(
            ws.read_request("getman.yaml"),
            Err(Error::InvalidPath(_))
        ));
        assert!(matches!(
            ws.read_request("environments/prod.yaml"),
            Err(Error::InvalidPath(_))
        ));
    }

    #[test]
    fn names_that_collide_get_a_number() {
        let (_dir, ws) = workspace();
        assert_eq!(
            ws.create_request("", "Login", &get("a")).unwrap(),
            "login.yaml"
        );
        assert_eq!(
            ws.create_request("", "login!", &get("b")).unwrap(),
            "login-2.yaml"
        );
        assert_eq!(ws.create_folder("", "Login").unwrap(), "login-3");
    }

    #[test]
    fn rename_moves_the_file_and_keeps_the_order() {
        let (_dir, ws) = workspace();
        ws.create_request("", "A", &get("a")).unwrap();
        ws.create_request("", "B", &get("b")).unwrap();
        ws.set_order("", vec!["b".into(), "a".into()]).unwrap();
        let path = ws.rename("a.yaml", "Alpha").unwrap();
        assert_eq!(path, "alpha.yaml");
        assert_eq!(ws.read_request(&path).unwrap().name, "Alpha");
        assert_eq!(ws.read_folder("").unwrap().order, ["b", "alpha"]);
        // Changing only the capitalisation keeps the file.
        assert_eq!(ws.rename("alpha.yaml", "ALPHA").unwrap(), "alpha.yaml");

        let folder = ws.create_folder("", "Users").unwrap();
        ws.create_request(&folder, "Me", &get("me")).unwrap();
        let renamed = ws.rename(&folder, "People").unwrap();
        assert_eq!(renamed, "people");
        assert_eq!(ws.read_folder("people").unwrap().name, "People");
        assert!(ws.read_request("people/me.yaml").is_ok());
    }

    #[test]
    fn items_move_between_folders() {
        let (_dir, ws) = workspace();
        let a = ws.create_folder("", "A").unwrap();
        let b = ws.create_folder("", "B").unwrap();
        let req = ws.create_request(&a, "Ping", &get("ping")).unwrap();
        assert_eq!(ws.move_item(&req, &b).unwrap(), "b/ping.yaml");
        assert!(ws.move_item("a", "a").is_err());
        let nested = ws.create_folder(&a, "Inner").unwrap();
        assert!(ws.move_item("a", &nested).is_err());
        assert_eq!(ws.move_item(&nested, "").unwrap(), "inner");
    }

    #[test]
    fn delete_and_duplicate() {
        let (_dir, ws) = workspace();
        let folder = ws.create_folder("", "Users").unwrap();
        let req = ws.create_request(&folder, "Me", &get("me")).unwrap();
        let copy = ws.duplicate(&req).unwrap();
        assert_eq!(copy, "users/me-copy.yaml");
        assert_eq!(ws.read_request(&copy).unwrap().name, "Me copy");
        let folder_copy = ws.duplicate(&folder).unwrap();
        assert_eq!(folder_copy, "users-copy");
        assert!(ws.read_request("users-copy/me.yaml").is_ok());
        ws.delete(&req).unwrap();
        assert!(matches!(ws.read_request(&req), Err(Error::NotFound(_))));
        ws.delete(&folder).unwrap();
        assert_eq!(
            paths(&ws.tree().unwrap()),
            [
                "users-copy",
                "users-copy/me.yaml",
                "users-copy/me-copy.yaml"
            ]
        );
    }

    #[test]
    fn folder_chain_runs_from_the_workspace_down() {
        let (_dir, ws) = workspace();
        let outer = ws.create_folder("", "Outer").unwrap();
        let inner = ws.create_folder(&outer, "Inner").unwrap();
        let req = ws.create_request(&inner, "Deep", &get("deep")).unwrap();
        let names: Vec<_> = ws
            .folder_chain(&req)
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, ["Test API", "Outer", "Inner"]);
    }

    #[test]
    fn secrets_stay_out_of_the_shared_environment_file() {
        let (dir, ws) = workspace();
        let id = ws.create_environment("Staging").unwrap();
        assert_eq!(id, "staging");
        let mut files = ws.read_environment(&id).unwrap();
        files
            .shared
            .variables
            .insert("baseUrl".into(), "https://staging.example.com".into());
        files
            .shared
            .variables
            .insert("token".into(), "s3cret".into());
        files.shared.secrets.push("token".into());
        ws.write_environment(&id, &files).unwrap();

        let shared = fs::read_to_string(dir.path().join("environments/staging.yaml")).unwrap();
        assert!(!shared.contains("s3cret"), "{shared}");
        let local = fs::read_to_string(dir.path().join("environments/staging.local.yaml")).unwrap();
        assert!(local.contains("s3cret"), "{local}");

        let read = ws.read_environment(&id).unwrap();
        assert_eq!(read.effective()["token"], "s3cret");
        assert_eq!(read.effective()["baseUrl"], "https://staging.example.com");
        assert_eq!(
            ws.environments().unwrap(),
            [EnvironmentSummary {
                id: "staging".into(),
                name: "Staging".into()
            }]
        );
    }

    #[test]
    fn environments_rename_with_their_local_values() {
        let (_dir, ws) = workspace();
        let id = ws.create_environment("dev").unwrap();
        ws.set_local_environment_vars(
            &id,
            &[("token".to_owned(), "t".to_owned())].into_iter().collect(),
        )
        .unwrap();
        let new_id = ws.rename_environment(&id, "Development").unwrap();
        assert_eq!(new_id, "development");
        assert_eq!(ws.read_environment(&new_id).unwrap().local["token"], "t");
        ws.delete_environment(&new_id).unwrap();
        assert_eq!(ws.environments().unwrap(), []);
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slugify("Create user (v2)!"), "create-user-v2");
        assert_eq!(slugify("  "), "untitled");
        assert_eq!(slugify("CON"), "con-item");
        assert_eq!(slugify("Über Straße"), "über-straße");
        assert_eq!(slugify(&"x".repeat(100)).len(), 60);
    }
}
