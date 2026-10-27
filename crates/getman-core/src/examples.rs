//! Saved responses, called examples, and where their bodies live.
//!
//! An example is stored in its request's file, under `examples:`. A body
//! that is text of at most [`INLINE_LIMIT`] bytes is written inline. Any
//! other body, longer or binary, goes to a file of its own in the
//! `<request>.examples` directory beside the request file, and the
//! example names it in `body_file`:
//!
//! ```text
//! orders/
//!   create-order.yaml           examples: [{ name: Created, body_file: created.json }]
//!   create-order.examples/
//!     created.json
//! ```
//!
//! The directory moves, is copied and is deleted together with its
//! request.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{Example, Request};
use crate::workspace::{Workspace, slugify};

/// Longest body, in bytes, kept inline in the request file.
pub const INLINE_LIMIT: usize = 32 * 1024;

/// Ending of the directory that holds a request's example bodies.
pub const DIR_SUFFIX: &str = ".examples";

/// The directory beside a request file that holds its example bodies:
/// `orders/create.yaml` keeps them in `orders/create.examples`.
pub fn dir_for(request_file: &Path) -> PathBuf {
    let stem = request_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    request_file.with_file_name(format!("{stem}{DIR_SUFFIX}"))
}

/// Whether a body is kept inline: UTF-8 text without control characters
/// other than tabs and line breaks, and short.
pub fn fits_inline(body: &[u8]) -> bool {
    body.len() <= INLINE_LIMIT
        && std::str::from_utf8(body).is_ok_and(|text| {
            !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
        })
}

/// The bytes of an example's body, from the request file or its own file.
pub fn body(workspace: &Workspace, request: &str, example: &Example) -> Result<Vec<u8>> {
    match &example.body_file {
        Some(name) => {
            let path = body_path(workspace, request, name)?;
            fs::read(&path).map_err(|e| Error::io(&path, e))
        }
        None => Ok(example.body.clone().into_bytes()),
    }
}

/// Saves an example to a request, replacing any example of the same name,
/// with `body` inline or in a file as [`fits_inline`] decides. Returns the
/// example as stored.
pub fn save(
    workspace: &Workspace,
    request: &str,
    mut example: Example,
    body: &[u8],
) -> Result<Example> {
    let name = example.name.trim().to_owned();
    if name.is_empty() {
        return Err(Error::Invalid("an example needs a name".into()));
    }
    example.name = name;
    let mut saved = workspace.read_request(request)?;
    let replaced = saved.examples.iter().position(|e| e.name == example.name);
    if let Some(at) = replaced {
        let old = saved.examples.remove(at);
        remove_body_file(workspace, request, &saved, &old)?;
        place_body(workspace, request, &saved, &mut example, body)?;
        saved.examples.insert(at, example.clone());
    } else {
        place_body(workspace, request, &saved, &mut example, body)?;
        saved.examples.push(example.clone());
    }
    workspace.write_request(request, &saved)?;
    Ok(example)
}

/// Renames an example. Its body file keeps its name.
pub fn rename(workspace: &Workspace, request: &str, from: &str, to: &str) -> Result<()> {
    let to = to.trim();
    if to.is_empty() {
        return Err(Error::Invalid("an example needs a name".into()));
    }
    let mut saved = workspace.read_request(request)?;
    if from != to && saved.examples.iter().any(|e| e.name == to) {
        return Err(Error::AlreadyExists(format!("an example named {to:?}")));
    }
    let example = saved
        .examples
        .iter_mut()
        .find(|e| e.name == from)
        .ok_or_else(|| Error::NotFound(format!("the example {from:?}")))?;
    to.clone_into(&mut example.name);
    workspace.write_request(request, &saved)
}

/// Deletes an example and its body file.
pub fn delete(workspace: &Workspace, request: &str, name: &str) -> Result<()> {
    let mut saved = workspace.read_request(request)?;
    let at = saved
        .examples
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| Error::NotFound(format!("the example {name:?}")))?;
    let old = saved.examples.remove(at);
    remove_body_file(workspace, request, &saved, &old)?;
    workspace.write_request(request, &saved)
}

/// Moves inline bodies that are too long for the request file into files,
/// for requests written with examples already in them, such as imports.
/// Returns whether anything moved.
pub fn place_long_bodies(workspace: &Workspace, request: &str) -> Result<bool> {
    let mut saved = workspace.read_request(request)?;
    let mut moved = false;
    for at in 0..saved.examples.len() {
        let example = &saved.examples[at];
        if example.body_file.is_some() || fits_inline(example.body.as_bytes()) {
            continue;
        }
        let mut example = example.clone();
        let body = std::mem::take(&mut example.body).into_bytes();
        place_body(workspace, request, &saved, &mut example, &body)?;
        saved.examples[at] = example;
        moved = true;
    }
    if moved {
        workspace.write_request(request, &saved)?;
    }
    Ok(moved)
}

/// Puts a body inline or in a new file in the request's examples
/// directory, and points the example at it.
fn place_body(
    workspace: &Workspace,
    request: &str,
    saved: &Request,
    example: &mut Example,
    body: &[u8],
) -> Result<()> {
    if fits_inline(body) {
        example.body = String::from_utf8_lossy(body).into_owned();
        example.body_file = None;
        return Ok(());
    }
    let dir = dir_for(&workspace.resolve(request)?);
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let content_type = example
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.as_str());
    let extension = extension_for(content_type, body);
    let stem = slugify(&example.name);
    let taken = |file: &str| {
        dir.join(file).exists()
            || saved
                .examples
                .iter()
                .any(|e| e.body_file.as_deref() == Some(file))
    };
    let mut file = format!("{stem}.{extension}");
    let mut n = 2;
    while taken(&file) {
        file = format!("{stem}-{n}.{extension}");
        n += 1;
    }
    let path = dir.join(&file);
    fs::write(&path, body).map_err(|e| Error::io(&path, e))?;
    example.body = String::new();
    example.body_file = Some(file);
    Ok(())
}

/// Deletes the body file of an example that is going away, unless another
/// example still uses it, and the directory once it is empty.
fn remove_body_file(
    workspace: &Workspace,
    request: &str,
    remaining: &Request,
    old: &Example,
) -> Result<()> {
    let Some(name) = &old.body_file else {
        return Ok(());
    };
    if remaining
        .examples
        .iter()
        .any(|e| e.body_file.as_deref() == Some(name))
    {
        return Ok(());
    }
    let path = body_path(workspace, request, name)?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io(&path, e)),
    }
    if let Some(dir) = path.parent() {
        // Fails while other bodies are left, which is fine.
        let _ = fs::remove_dir(dir);
    }
    Ok(())
}

/// Where a body file is, refusing names that would leave the directory.
fn body_path(workspace: &Workspace, request: &str, name: &str) -> Result<PathBuf> {
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && name.len() <= 255;
    if !plain {
        return Err(Error::InvalidPath(name.to_owned()));
    }
    Ok(dir_for(&workspace.resolve(request)?).join(name))
}

/// A file extension for a body, from its content type. Never `yaml`, so
/// a body file is not mistaken for a request.
fn extension_for(content_type: Option<&str>, body: &[u8]) -> String {
    let essence = content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let known = match essence.as_str() {
        "application/json" | "application/problem+json" => Some("json"),
        "application/xml" | "text/xml" => Some("xml"),
        "text/html" => Some("html"),
        "text/plain" => Some("txt"),
        "text/csv" => Some("csv"),
        "text/css" => Some("css"),
        "application/javascript" | "text/javascript" => Some("js"),
        "application/pdf" => Some("pdf"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "application/zip" => Some("zip"),
        _ => None,
    };
    if let Some(extension) = known {
        return extension.to_owned();
    }
    if essence.ends_with("+json") {
        return "json".to_owned();
    }
    if essence.ends_with("+xml") {
        return "xml".to_owned();
    }
    if !essence.contains("yaml")
        && let Some(guess) = mime_guess::get_mime_extensions_str(&essence)
            .and_then(|list| list.first())
            .filter(|ext| !ext.starts_with("yaml") && !ext.starts_with("yml"))
    {
        return (*guess).to_owned();
    }
    if std::str::from_utf8(body).is_ok() {
        "txt".to_owned()
    } else {
        "bin".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KeyValue;

    fn setup() -> (tempfile::TempDir, Workspace, String) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let folder = ws.create_folder("", "Orders").unwrap();
        let path = ws
            .create_request(
                &folder,
                "Create order",
                &Request {
                    method: "POST".into(),
                    url: "{{baseUrl}}/orders".into(),
                    ..Request::default()
                },
            )
            .unwrap();
        (dir, ws, path)
    }

    fn example(name: &str, content_type: &str) -> Example {
        Example {
            name: name.into(),
            status: 201,
            headers: vec![KeyValue::new("Content-Type", content_type)],
            ..Example::default()
        }
    }

    #[test]
    fn short_text_stays_inline_and_the_rest_gets_a_file() {
        let (dir, ws, path) = setup();
        let stored = save(
            &ws,
            &path,
            example("Created", "application/json"),
            b"{\"id\":1}",
        )
        .unwrap();
        assert_eq!(stored.body, "{\"id\":1}");
        assert_eq!(stored.body_file, None);

        let big = vec![b'x'; INLINE_LIMIT + 1];
        let stored = save(
            &ws,
            &path,
            example("Big", "text/plain; charset=utf-8"),
            &big,
        )
        .unwrap();
        assert_eq!(stored.body_file.as_deref(), Some("big.txt"));
        let file = dir.path().join("orders/create-order.examples/big.txt");
        assert_eq!(fs::read(&file).unwrap().len(), INLINE_LIMIT + 1);

        let png = [0x89, b'P', b'N', b'G', 0, 1, 2];
        let stored = save(&ws, &path, example("Logo", "image/png"), &png).unwrap();
        assert_eq!(stored.body_file.as_deref(), Some("logo.png"));
        assert_eq!(body(&ws, &path, &stored).unwrap(), png);

        let yaml = save(&ws, &path, example("Spec", "application/yaml"), &[0xff; 3]).unwrap();
        assert_eq!(yaml.body_file.as_deref(), Some("spec.bin"));

        let request = ws.read_request(&path).unwrap();
        let names: Vec<_> = request.examples.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Created", "Big", "Logo", "Spec"]);
        // The directory of bodies is not a folder of requests.
        let tree = serde_json::to_string(&ws.tree().unwrap()).unwrap();
        assert!(!tree.contains(".examples"), "{tree}");
    }

    #[test]
    fn saving_a_name_again_replaces_it() {
        let (dir, ws, path) = setup();
        let big = vec![b'y'; INLINE_LIMIT + 10];
        save(&ws, &path, example("Created", "text/plain"), &big).unwrap();
        save(&ws, &path, example("Created", "text/plain"), b"small").unwrap();
        let request = ws.read_request(&path).unwrap();
        assert_eq!(request.examples.len(), 1);
        assert_eq!(request.examples[0].body, "small");
        assert!(!dir.path().join("orders/create-order.examples").exists());
    }

    #[test]
    fn examples_rename_and_delete_with_their_files() {
        let (dir, ws, path) = setup();
        let big = vec![b'z'; INLINE_LIMIT + 1];
        save(&ws, &path, example("One", "text/plain"), &big).unwrap();
        save(&ws, &path, example("Two", "text/plain"), b"2").unwrap();
        assert!(rename(&ws, &path, "One", "Two").is_err());
        assert!(rename(&ws, &path, "Nope", "Three").is_err());
        rename(&ws, &path, "One", "First").unwrap();
        let request = ws.read_request(&path).unwrap();
        assert_eq!(request.examples[0].name, "First");
        assert_eq!(request.examples[0].body_file.as_deref(), Some("one.txt"));

        delete(&ws, &path, "First").unwrap();
        assert!(!dir.path().join("orders/create-order.examples").exists());
        assert!(delete(&ws, &path, "First").is_err());
        assert_eq!(ws.read_request(&path).unwrap().examples.len(), 1);
    }

    #[test]
    fn body_files_cannot_point_outside_their_directory() {
        let (_dir, ws, path) = setup();
        for name in ["../getman.yaml", "a/b", ".hidden", "C:x", ""] {
            let example = Example {
                body_file: Some(name.into()),
                ..Example::default()
            };
            assert!(body(&ws, &path, &example).is_err(), "{name}");
        }
    }

    #[test]
    fn bodies_follow_their_request() {
        let (dir, ws, path) = setup();
        let big = vec![b'q'; INLINE_LIMIT + 1];
        save(&ws, &path, example("Big", "text/plain"), &big).unwrap();

        let renamed = ws.rename(&path, "Place order").unwrap();
        let stored = &ws.read_request(&renamed).unwrap().examples[0];
        assert_eq!(body(&ws, &renamed, stored).unwrap().len(), big.len());
        assert!(!dir.path().join("orders/create-order.examples").exists());

        let copy = ws.duplicate(&renamed).unwrap();
        let stored = &ws.read_request(&copy).unwrap().examples[0];
        assert_eq!(body(&ws, &copy, stored).unwrap().len(), big.len());

        let moved = ws.move_item(&copy, "").unwrap();
        let stored = &ws.read_request(&moved).unwrap().examples[0];
        assert_eq!(body(&ws, &moved, stored).unwrap().len(), big.len());

        ws.delete(&moved).unwrap();
        assert!(!dir.path().join("place-order-copy.examples").exists());
    }

    #[test]
    fn long_inline_bodies_move_to_files() {
        let (_dir, ws, path) = setup();
        let mut request = ws.read_request(&path).unwrap();
        request.examples.push(Example {
            name: "Long".into(),
            body: "l".repeat(INLINE_LIMIT + 1),
            ..Example::default()
        });
        ws.write_request(&path, &request).unwrap();
        assert!(place_long_bodies(&ws, &path).unwrap());
        let stored = &ws.read_request(&path).unwrap().examples[0];
        assert_eq!(stored.body, "");
        assert_eq!(stored.body_file.as_deref(), Some("long.txt"));
        assert!(!place_long_bodies(&ws, &path).unwrap());
    }
}
