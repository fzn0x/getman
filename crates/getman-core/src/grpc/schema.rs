//! A gRPC request's schema: the services and messages it can use, from
//! `.proto` files in the workspace or from the server's reflection
//! service.

use std::cell::Cell;
use std::fs;
use std::path::{Component, Path, PathBuf};

use prost_reflect::{DescriptorPool, MethodDescriptor};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use serde::Serialize;
use serde_json::Value;

use super::CallType;
use super::json::example;
use crate::error::{Error, Result};

/// Most `.proto` files read for one schema.
const MAX_PROTO_FILES: usize = 2000;
/// Largest `.proto` file read.
const MAX_PROTO_BYTES: u64 = 4 * 1024 * 1024;
/// Most bytes of `.proto` files read for one schema.
const MAX_PROTO_TOTAL: u64 = 32 * 1024 * 1024;
/// How deep folders are searched for `.proto` files.
const MAX_DEPTH: usize = 16;
/// Folders never searched for `.proto` files.
const SKIPPED_DIRS: &[&str] = &["node_modules", "target"];

/// The services and messages a request can use.
#[derive(Debug, Clone)]
pub struct Schema {
    pool: DescriptorPool,
}

/// A service, for choosing a method.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceInfo {
    /// Full name, such as `shop.Orders`.
    pub name: String,
    /// Its methods, in the order they are declared.
    pub methods: Vec<MethodInfo>,
}

/// A method, for choosing it and starting its message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MethodInfo {
    /// Short name, such as `GetOrder`.
    pub name: String,
    /// What a request's `method` holds: `shop.Orders/GetOrder`.
    pub path: String,
    /// Full name of the message it takes.
    pub input: String,
    /// Full name of the message it returns.
    pub output: String,
    /// Which sides stream.
    pub call_type: CallType,
    /// An example of the message it takes, as JSON.
    pub example: Value,
}

impl Schema {
    /// A schema from descriptors already loaded.
    pub(crate) fn from_pool(pool: DescriptorPool) -> Self {
        Self { pool }
    }

    /// Compiles a `.proto` file, or every `.proto` file in a folder, named
    /// relative to the workspace `root`. Imports are looked up next to the
    /// file (or in the folder) first, then from the workspace root, and
    /// never outside the workspace. The well-known `google/protobuf` types
    /// are built in.
    pub fn from_protos(root: &Path, rel: &str) -> Result<Self> {
        let rel = rel.trim().trim_end_matches(['/', '\\']);
        let canonical_root = root.canonicalize().map_err(|e| Error::io(root, e))?;
        let target = inside(&canonical_root, root, rel)?;
        let (first, names) = if target.is_dir() {
            let mut names = Vec::new();
            collect_protos(&target, "", 0, &mut names);
            if names.is_empty() {
                return Err(Error::Invalid(format!("{rel} holds no .proto files")));
            }
            (target, names)
        } else {
            let is_proto = target
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("proto"));
            if !is_proto {
                return Err(Error::Invalid(format!("{rel} is not a .proto file")));
            }
            let name = target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir = target
                .parent()
                .map_or_else(|| canonical_root.clone(), Path::to_path_buf);
            (dir, vec![name])
        };
        let mut includes = vec![first];
        if includes[0] != canonical_root {
            includes.push(canonical_root.clone());
        }
        let mut resolver = ChainFileResolver::new();
        resolver.add(WorkspaceFiles {
            root: canonical_root,
            includes,
            used: Cell::new((0, 0)),
        });
        resolver.add(GoogleFileResolver::new());
        let mut compiler = protox::Compiler::with_file_resolver(resolver);
        compiler.include_imports(true);
        for name in &names {
            compiler
                .open_file(name)
                .map_err(|e| Error::Invalid(format!("Could not read the .proto files: {e}")))?;
        }
        Ok(Self {
            pool: compiler.descriptor_pool(),
        })
    }

    /// The descriptors.
    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }

    /// Every service and its methods, services by name.
    pub fn services(&self) -> Vec<ServiceInfo> {
        let mut services: Vec<ServiceInfo> = self
            .pool
            .services()
            .map(|service| ServiceInfo {
                name: service.full_name().to_owned(),
                methods: service
                    .methods()
                    .map(|method| MethodInfo {
                        name: method.name().to_owned(),
                        path: format!("{}/{}", service.full_name(), method.name()),
                        input: method.input().full_name().to_owned(),
                        output: method.output().full_name().to_owned(),
                        call_type: CallType::of(&method),
                        example: example(&method.input()),
                    })
                    .collect(),
            })
            .collect();
        services.sort_by(|a, b| a.name.cmp(&b.name));
        services
    }

    /// Finds a method by its full name: `shop.Orders/GetOrder`, with or
    /// without a leading slash, or `shop.Orders.GetOrder`.
    pub fn method(&self, name: &str) -> Result<MethodDescriptor> {
        let name = name.trim().trim_start_matches('/');
        if name.is_empty() {
            return Err(Error::Invalid("choose a method to call".into()));
        }
        let (service_name, method_name) = name
            .rsplit_once('/')
            .or_else(|| name.rsplit_once('.'))
            .ok_or_else(|| {
            Error::Invalid(format!(
                "{name:?} is not a method; write it as package.Service/Method"
            ))
        })?;
        let Some(service) = self.pool.get_service_by_name(service_name) else {
            let known: Vec<String> = self
                .pool
                .services()
                .take(10)
                .map(|s| s.full_name().to_owned())
                .collect();
            return Err(Error::Invalid(if known.is_empty() {
                format!("the schema has no service {service_name}; it has no services at all")
            } else {
                format!(
                    "the schema has no service {service_name}; it has {}",
                    known.join(", ")
                )
            }));
        };
        service
            .methods()
            .find(|m| m.name() == method_name)
            .ok_or_else(|| {
                let known: Vec<String> = service.methods().map(|m| m.name().to_owned()).collect();
                Error::Invalid(format!(
                    "{service_name} has no method {method_name}; it has {}",
                    known.join(", ")
                ))
            })
    }
}

/// Every `.proto` file in the workspace, as paths relative to its root,
/// for choosing a schema.
pub fn proto_files(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    collect_protos(root, "", 0, &mut names);
    names.sort();
    names
}

fn collect_protos(dir: &Path, rel: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if out.len() >= MAX_PROTO_FILES {
            return;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let child = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if file_type.is_dir() {
            if depth < MAX_DEPTH && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect_protos(&entry.path(), &child, depth + 1, out);
            }
        } else if file_type.is_file()
            && Path::new(&name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("proto"))
        {
            out.push(child);
        }
    }
}

/// Resolves `rel` to a canonical path inside the workspace.
fn inside(canonical_root: &Path, root: &Path, rel: &str) -> Result<PathBuf> {
    if rel.is_empty() {
        return Err(Error::Invalid(
            "no .proto file is chosen; choose one, or use server reflection".into(),
        ));
    }
    let path = Path::new(rel);
    let escapes = path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
    if escapes {
        return Err(Error::Invalid(format!(
            "{rel:?} is outside the workspace; .proto files must be inside it"
        )));
    }
    let canonical = root
        .join(path)
        .canonicalize()
        .map_err(|_| Error::NotFound(rel.to_owned()))?;
    if !canonical.starts_with(canonical_root) {
        return Err(Error::Invalid(format!(
            "{rel:?} links outside the workspace; .proto files must be inside it"
        )));
    }
    Ok(canonical)
}

/// Opens `.proto` files from include folders inside the workspace, within
/// the size limits.
struct WorkspaceFiles {
    root: PathBuf,
    includes: Vec<PathBuf>,
    /// Files and bytes read so far.
    used: Cell<(usize, u64)>,
}

impl FileResolver for WorkspaceFiles {
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        let path = Path::new(name);
        let plain =
            !name.contains('\\') && path.components().all(|c| matches!(c, Component::Normal(_)));
        if !plain {
            return Err(protox::Error::file_not_found(name));
        }
        for include in &self.includes {
            let Ok(canonical) = include.join(path).canonicalize() else {
                continue;
            };
            let Ok(metadata) = fs::metadata(&canonical) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if !canonical.starts_with(&self.root) {
                return Err(protox::Error::new(std::io::Error::other(format!(
                    "{name} links outside the workspace; imports must stay inside it"
                ))));
            }
            let size = metadata.len();
            if size > MAX_PROTO_BYTES {
                return Err(protox::Error::new(std::io::Error::other(format!(
                    "{name} is larger than {} MB",
                    MAX_PROTO_BYTES / 1024 / 1024
                ))));
            }
            let (files, bytes) = self.used.get();
            if files + 1 > MAX_PROTO_FILES || bytes + size > MAX_PROTO_TOTAL {
                return Err(protox::Error::new(std::io::Error::other(format!(
                    "the schema needs more than {MAX_PROTO_FILES} files or {} MB of .proto files",
                    MAX_PROTO_TOTAL / 1024 / 1024
                ))));
            }
            self.used.set((files + 1, bytes + size));
            let source = fs::read_to_string(&canonical).map_err(protox::Error::new)?;
            return File::from_source(name, &source);
        }
        Err(protox::Error::file_not_found(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("protos/shop")).unwrap();
        fs::write(
            dir.path().join("protos/shop/common.proto"),
            "syntax = \"proto3\";\npackage shop;\nmessage Money { int64 cents = 1; }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("protos/shop/orders.proto"),
            r#"syntax = "proto3";
package shop;
import "shop/common.proto";
import "google/protobuf/empty.proto";
service Orders {
  rpc Get(OrderId) returns (Order);
  rpc Watch(OrderId) returns (stream Order);
  rpc Upload(stream Order) returns (google.protobuf.Empty);
  rpc Chat(stream Order) returns (stream Order);
}
message OrderId { string id = 1; }
message Order { string id = 1; Money total = 2; }
"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_file_compiles_with_imports_from_the_workspace_root() {
        let dir = workspace();
        let error = Schema::from_protos(dir.path(), "protos/shop/orders.proto").unwrap_err();
        // `shop/common.proto` is relative to `protos`, which is neither the
        // file's folder nor the root.
        assert!(error.to_string().contains("shop/common.proto"), "{error}");

        let schema = Schema::from_protos(dir.path(), "protos").unwrap();
        let services = schema.services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "shop.Orders");
        let kinds: Vec<_> = services[0].methods.iter().map(|m| m.call_type).collect();
        assert_eq!(
            kinds,
            [
                CallType::Unary,
                CallType::ServerStreaming,
                CallType::ClientStreaming,
                CallType::BidiStreaming
            ]
        );
        assert_eq!(services[0].methods[0].path, "shop.Orders/Get");
        assert_eq!(
            services[0].methods[0].example,
            serde_json::json!({"id": ""})
        );
        assert_eq!(schema.method("/shop.Orders/Watch").unwrap().name(), "Watch");
        assert_eq!(schema.method("shop.Orders.Chat").unwrap().name(), "Chat");
        let error = schema.method("shop.Orders/Nope").unwrap_err().to_string();
        assert!(error.contains("it has Get, Watch, Upload, Chat"), "{error}");
        let error = schema.method("shop.Carts/Get").unwrap_err().to_string();
        assert!(error.contains("it has shop.Orders"), "{error}");
    }

    #[test]
    fn files_outside_the_workspace_are_refused() {
        let dir = workspace();
        let inner = dir.path().join("inner");
        fs::create_dir(&inner).unwrap();
        fs::write(
            inner.join("a.proto"),
            "syntax = \"proto3\";\nimport \"protos/shop/common.proto\";\n",
        )
        .unwrap();
        assert!(Schema::from_protos(&inner, "../protos/shop/common.proto").is_err());
        let error = Schema::from_protos(&inner, "a.proto")
            .unwrap_err()
            .to_string();
        assert!(error.contains("common.proto"), "{error}");
        assert!(Schema::from_protos(dir.path(), "").is_err());
        assert!(matches!(
            Schema::from_protos(dir.path(), "missing.proto"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn proto_files_are_listed_from_the_root() {
        let dir = workspace();
        fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
        fs::write(dir.path().join("node_modules/x/skip.proto"), "").unwrap();
        assert_eq!(
            proto_files(dir.path()),
            ["protos/shop/common.proto", "protos/shop/orders.proto"]
        );
    }
}
