//! Writing requests out for other tools.

pub mod codegen;
pub mod openapi;
pub mod postman;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::workspace::{Node, Workspace};

/// A file an exporter wrote, and what it could not carry over.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Exported {
    /// A file name to save it as, such as `orders.postman_collection.json`.
    pub file_name: String,
    /// The file's text.
    pub content: String,
    /// Things that could not be carried over exactly.
    pub warnings: Vec<String>,
}

/// The items of a folder, or of the whole workspace for `""`.
pub(crate) fn items_of(workspace: &Workspace, folder: &str) -> Result<Vec<Node>> {
    fn find(nodes: Vec<Node>, path: &str) -> Option<Vec<Node>> {
        for node in nodes {
            if let Node::Folder {
                path: node_path,
                children,
                ..
            } = node
            {
                if node_path == path {
                    return Some(children);
                }
                if path.starts_with(&format!("{node_path}/")) {
                    return find(children, path);
                }
            }
        }
        None
    }
    let tree = workspace.tree()?;
    if folder.is_empty() {
        return Ok(tree);
    }
    find(tree, folder).ok_or_else(|| Error::NotFound(format!("folder {folder}")))
}

/// The settings of a folder and every folder around it, from the
/// workspace down.
pub(crate) fn chain_of(workspace: &Workspace, folder: &str) -> Result<Vec<crate::Folder>> {
    if folder.is_empty() {
        return Ok(vec![workspace.read_folder("")?]);
    }
    // A request path inside the folder gives the folder's own chain.
    workspace.folder_chain(&format!("{folder}/_"))
}
