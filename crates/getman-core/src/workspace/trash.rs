//! Deleted requests and folders, kept in the git-ignored state directory
//! so they can be put back.
//!
//! ```text
//! .getman/trash/
//!   1761590400000-3f2a9c1e/
//!     entry.json             where it was, its name, when it went
//!     create-user.yaml       the request file, or the folder itself
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    FOLDER_FILE, Workspace, copy_dir, item_key, join, read_yaml, request_stem, split, unique_stem,
};
use crate::error::{Error, Result};
use crate::model::{Folder, Request};

/// Directory of deleted items, inside the state directory.
pub const TRASH_DIR: &str = "trash";
const ENTRY_FILE: &str = "entry.json";

/// What kind of item went to the trash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    /// A request file.
    Request,
    /// A folder and everything in it.
    Folder,
}

/// An item in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashItem {
    /// How the item is addressed in the trash.
    pub id: String,
    /// Where the item was, relative to the workspace root.
    pub path: String,
    /// A request or a folder.
    pub kind: TrashKind,
    /// Display name.
    pub name: String,
    /// The request's method; empty for folders.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// Milliseconds since the Unix epoch.
    pub deleted_at: i64,
    /// Whether the item was in its folder's `order`.
    #[serde(default)]
    pub ordered: bool,
    /// The item before it in that order, so it goes back in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Creates `dir`, Getman's state directory, with a `.gitignore` that keeps
/// everything in it out of git, whatever the repository's own ignore rules
/// say. Everything that writes there goes through this.
pub fn create_state_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        fs::write(&ignore, "# Getman's local state\n*\n").map_err(|e| Error::io(&ignore, e))?;
    }
    Ok(())
}

impl Workspace {
    /// The directory of Getman's state, created with a `.gitignore` that
    /// keeps everything in it out of git, whatever the repository's own
    /// ignore rules say.
    pub fn ensure_state_dir(&self) -> Result<PathBuf> {
        let dir = self.state_dir();
        create_state_dir(&dir)?;
        Ok(dir)
    }

    fn trash_dir(&self) -> PathBuf {
        self.state_dir().join(TRASH_DIR)
    }

    /// Moves a request, or a folder and everything in it, to the trash.
    pub fn trash(&self, rel: &str) -> Result<TrashItem> {
        if rel.is_empty() {
            return Err(Error::Invalid("the workspace cannot be deleted".into()));
        }
        let (parent, file) = split(rel);
        let key = item_key(file).to_owned();
        let (kind, source, name, method) = if request_stem(file).is_some() {
            let source = self.request_path(rel)?;
            if !source.is_file() {
                return Err(Error::NotFound(rel.to_owned()));
            }
            // A file that no longer parses can still be thrown away.
            let (name, method) = match read_yaml::<Request>(&source) {
                Ok(request) if !request.name.is_empty() => (request.name, request.method),
                Ok(request) => (key.clone(), request.method),
                Err(_) => (key.clone(), String::new()),
            };
            (TrashKind::Request, source, name, method)
        } else {
            let source = self.folder_dir(rel)?;
            let name = read_yaml::<Folder>(&source.join(FOLDER_FILE))
                .ok()
                .map(|meta| meta.name)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| key.clone());
            (TrashKind::Folder, source, name, String::new())
        };

        let order = self.read_folder(parent)?.order;
        let at = order.iter().position(|k| *k == key);
        let deleted_at = crate::time::now_millis();
        let random = crate::dynamic::uuid_v4();
        let id = format!("{deleted_at:013}-{}", &random[..8]);
        let item = TrashItem {
            id: id.clone(),
            path: rel.to_owned(),
            kind,
            name,
            method,
            deleted_at,
            ordered: at.is_some(),
            after: at.and_then(|i| i.checked_sub(1)).map(|i| order[i].clone()),
        };

        self.ensure_state_dir()?;
        let slot = self.trash_dir().join(&id);
        fs::create_dir_all(&slot).map_err(|e| Error::io(&slot, e))?;
        let entry = slot.join(ENTRY_FILE);
        let text = serde_json::to_string_pretty(&item)
            .map_err(|e| Error::Invalid(format!("could not record the deleted item: {e}")))?;
        fs::write(&entry, text).map_err(|e| Error::io(&entry, e))?;
        if let Err(e) = move_path(&source, &slot.join(file)) {
            let _ = fs::remove_dir_all(&slot);
            return Err(e);
        }
        self.remove_from_order(parent, &key)?;
        Ok(item)
    }

    /// The items in the trash, most recently deleted first.
    pub fn trash_list(&self) -> Result<Vec<TrashItem>> {
        let dir = self.trash_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut items: Vec<TrashItem> = entries
            .flatten()
            .filter_map(|entry| fs::read_to_string(entry.path().join(ENTRY_FILE)).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect();
        items.sort_by(|a, b| {
            b.deleted_at
                .cmp(&a.deleted_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(items)
    }

    /// Puts an item from the trash back where it was and returns its path,
    /// which gets a number if another item has taken the name meanwhile.
    pub fn restore(&self, id: &str) -> Result<String> {
        let slot = self.trash_slot(id)?;
        let text = fs::read_to_string(slot.join(ENTRY_FILE))
            .map_err(|_| Error::NotFound(format!("{id} in the trash")))?;
        let item: TrashItem = serde_json::from_str(&text)
            .map_err(|e| Error::Invalid(format!("the trash entry {id} is damaged: {e}")))?;
        // The entry is only ever written by Getman, but check the path anyway.
        self.resolve(&item.path)?;
        let (parent, file) = split(&item.path);
        let source = slot.join(file);
        if !source.exists() {
            return Err(Error::NotFound(format!("{} in the trash", item.path)));
        }
        // The folder it was in may have gone too; bring the path back.
        let dir = self.resolve(parent)?;
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let key = unique_stem(&dir, item_key(file), None);
        // A request keeps its extension, `.yaml` or `.yml`.
        let extension = &file[request_stem(file).map_or(file.len(), str::len)..];
        let new_file = format!("{key}{extension}");
        move_path(&source, &dir.join(&new_file))?;
        let _ = fs::remove_dir_all(&slot);
        if item.ordered {
            let mut meta = self.read_folder(parent)?;
            if !meta.order.is_empty() && !meta.order.contains(&key) {
                let at = match &item.after {
                    None => 0,
                    Some(after) => meta
                        .order
                        .iter()
                        .position(|k| k == after)
                        .map_or(meta.order.len(), |i| i + 1),
                };
                meta.order.insert(at, key);
                self.write_folder(parent, &meta)?;
            }
        }
        Ok(join(parent, &new_file))
    }

    /// Deletes everything in the trash for good.
    pub fn empty_trash(&self) -> Result<()> {
        let dir = self.trash_dir();
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        }
        Ok(())
    }

    fn trash_slot(&self, id: &str) -> Result<PathBuf> {
        let valid = !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if !valid {
            return Err(Error::InvalidPath(id.to_owned()));
        }
        Ok(self.trash_dir().join(id))
    }
}

/// Moves a file or directory, copying it when a rename is not possible.
fn move_path(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        return Err(Error::AlreadyExists(to.display().to_string()));
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        if let Err(e) = fs::remove_dir_all(from) {
            let _ = fs::remove_dir_all(to);
            return Err(Error::io(from, e));
        }
    } else {
        fs::copy(from, to).map_err(|e| Error::io(from, e))?;
        if let Err(e) = fs::remove_file(from) {
            let _ = fs::remove_file(to);
            return Err(Error::io(from, e));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Node;

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
    fn deleted_requests_wait_in_the_trash_and_come_back_in_place() {
        let (dir, ws) = workspace();
        ws.create_request("", "One", &get("1")).unwrap();
        ws.create_request("", "Two", &get("2")).unwrap();
        ws.create_request("", "Three", &get("3")).unwrap();
        ws.set_order("", vec!["one".into(), "two".into(), "three".into()])
            .unwrap();

        let item = ws.trash("two.yaml").unwrap();
        assert_eq!(item.name, "Two");
        assert_eq!(item.kind, TrashKind::Request);
        assert_eq!(item.after.as_deref(), Some("one"));
        assert_eq!(paths(&ws.tree().unwrap()), ["one.yaml", "three.yaml"]);
        assert_eq!(ws.read_folder("").unwrap().order, ["one", "three"]);
        assert_eq!(ws.trash_list().unwrap(), std::slice::from_ref(&item));
        // The trash is out of git whatever the repository ignores.
        let ignore = fs::read_to_string(dir.path().join(".getman/.gitignore")).unwrap();
        assert!(ignore.lines().any(|line| line == "*"));

        assert_eq!(ws.restore(&item.id).unwrap(), "two.yaml");
        assert_eq!(ws.read_request("two.yaml").unwrap().name, "Two");
        assert_eq!(ws.read_folder("").unwrap().order, ["one", "two", "three"]);
        assert!(ws.trash_list().unwrap().is_empty());
    }

    #[test]
    fn folders_go_to_the_trash_with_everything_in_them() {
        let (_dir, ws) = workspace();
        let folder = ws.create_folder("", "Users").unwrap();
        ws.create_request(&folder, "Me", &get("me")).unwrap();
        let item = ws.trash(&folder).unwrap();
        assert_eq!(item.kind, TrashKind::Folder);
        assert_eq!(item.name, "Users");
        assert!(ws.tree().unwrap().is_empty());

        // Something new took the name meanwhile; the old one gets a number.
        ws.create_folder("", "Users").unwrap();
        assert_eq!(ws.restore(&item.id).unwrap(), "users-2");
        assert_eq!(ws.read_request("users-2/me.yaml").unwrap().name, "Me");
    }

    #[test]
    fn a_request_comes_back_even_if_its_folder_is_gone() {
        let (_dir, ws) = workspace();
        let folder = ws.create_folder("", "Users").unwrap();
        let request = ws.create_request(&folder, "Me", &get("me")).unwrap();
        let item = ws.trash(&request).unwrap();
        ws.delete(&folder).unwrap();
        assert_eq!(ws.restore(&item.id).unwrap(), "users/me.yaml");
        assert!(ws.read_request("users/me.yaml").is_ok());
    }

    #[test]
    fn the_trash_empties_and_refuses_odd_ids() {
        let (_dir, ws) = workspace();
        ws.create_request("", "One", &get("1")).unwrap();
        let item = ws.trash("one.yaml").unwrap();
        assert!(matches!(ws.restore("../x"), Err(Error::InvalidPath(_))));
        assert!(matches!(
            ws.restore("0000000000000-00000000"),
            Err(Error::NotFound(_))
        ));
        ws.empty_trash().unwrap();
        assert!(ws.trash_list().unwrap().is_empty());
        assert!(ws.restore(&item.id).is_err());
        assert!(matches!(ws.trash(""), Err(Error::Invalid(_))));
        assert!(matches!(ws.trash("nope.yaml"), Err(Error::NotFound(_))));
    }
}
