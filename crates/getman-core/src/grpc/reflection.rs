//! Asking a server for its schema through gRPC server reflection, version
//! `v1` or the older `v1alpha`.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use prost::Message as _;
use prost_reflect::DescriptorPool;
use prost_types::FileDescriptorProto;

use super::status::Code;
use super::transport::{Call, Connection};
use crate::error::{Error, Result};
use crate::http::Header;

const V1: &str = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";
const V1ALPHA: &str = "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo";

/// Most files a server's schema may have.
const MAX_FILES: usize = 2000;
/// Most bytes of descriptors read from a server.
const MAX_BYTES: usize = 64 * 1024 * 1024;

// The reflection messages, the same in v1 and v1alpha. Only the parts
// Getman uses are declared; the rest is skipped when decoding.

#[derive(Clone, PartialEq, prost::Message)]
struct ReflectionRequest {
    #[prost(string, tag = "1")]
    host: String,
    #[prost(oneof = "Ask", tags = "3, 4, 7")]
    ask: Option<Ask>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
enum Ask {
    #[prost(string, tag = "3")]
    FileByFilename(String),
    #[prost(string, tag = "4")]
    FileContainingSymbol(String),
    #[prost(string, tag = "7")]
    ListServices(String),
}

#[derive(Clone, PartialEq, prost::Message)]
struct ReflectionResponse {
    #[prost(oneof = "Answer", tags = "4, 6, 7")]
    answer: Option<Answer>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
enum Answer {
    #[prost(message, tag = "4")]
    Files(FileDescriptors),
    #[prost(message, tag = "6")]
    Services(ServiceList),
    #[prost(message, tag = "7")]
    Error(ReflectionError),
}

#[derive(Clone, PartialEq, prost::Message)]
struct FileDescriptors {
    #[prost(bytes = "vec", repeated, tag = "1")]
    file_descriptor_proto: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ServiceList {
    #[prost(message, repeated, tag = "1")]
    service: Vec<ServiceName>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ServiceName {
    #[prost(string, tag = "1")]
    name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ReflectionError {
    #[prost(int32, tag = "1")]
    error_code: i32,
    #[prost(string, tag = "2")]
    error_message: String,
}

/// Reads the server's schema: every service it lists, the files that
/// define them and the files those import. `metadata` gives the metadata
/// to send to a reflection method, by its path.
pub(crate) async fn reflect(
    connection: &mut Connection,
    metadata: &(dyn Fn(&str) -> Result<Vec<Header>> + Sync),
    timeout: Duration,
) -> Result<DescriptorPool> {
    for path in [V1, V1ALPHA] {
        let work = reflect_with(connection, metadata, timeout, path);
        let found = tokio::time::timeout(timeout, work).await.map_err(|_| {
            Error::Http(format!(
                "The server did not describe itself within {} s",
                timeout.as_secs()
            ))
        })??;
        if let Some(pool) = found {
            return Ok(pool);
        }
    }
    Err(Error::Http(
        "The server does not offer reflection (grpc.reflection.v1 or v1alpha); choose .proto files for this request instead".into(),
    ))
}

/// The schema through one version of reflection, or `None` when the
/// server does not implement that version.
async fn reflect_with(
    connection: &mut Connection,
    metadata: &(dyn Fn(&str) -> Result<Vec<Header>> + Sync),
    timeout: Duration,
    path: &str,
) -> Result<Option<DescriptorPool>> {
    let metadata = metadata(path)?;
    let mut call = connection.start(path, &metadata, Some(timeout)).await?;
    let services = match ask(&mut call, Ask::ListServices("*".into())).await? {
        Some(Answer::Services(list)) => {
            list.service.into_iter().map(|s| s.name).collect::<Vec<_>>()
        }
        Some(Answer::Error(e)) => {
            return Err(Error::Http(format!(
                "The server's reflection could not list its services: {}",
                e.error_message
            )));
        }
        Some(Answer::Files(_)) => {
            return Err(Error::Http(
                "The server's reflection answered a list of services with files".into(),
            ));
        }
        None => {
            let status = call.status();
            if status.code == Code::Unimplemented as u32 {
                return Ok(None);
            }
            return Err(Error::Http(format!(
                "Reflection failed: {} {}",
                status.name, status.message
            )));
        }
    };

    let mut files: HashMap<String, FileDescriptorProto> = HashMap::new();
    let mut bytes = 0;
    for service in services {
        let answer = ask(&mut call, Ask::FileContainingSymbol(service.clone())).await?;
        take_files(answer, &service, &mut files, &mut bytes)?;
    }
    loop {
        let missing: BTreeSet<String> = files
            .values()
            .flat_map(|f| f.dependency.iter())
            .filter(|d| !files.contains_key(*d))
            .cloned()
            .collect();
        if missing.is_empty() {
            break;
        }
        for name in missing {
            if let Some(file) = DescriptorPool::global().get_file_by_name(&name) {
                files.insert(name, file.file_descriptor_proto().clone());
                continue;
            }
            let answer = ask(&mut call, Ask::FileByFilename(name.clone())).await?;
            take_files(answer, &name, &mut files, &mut bytes)?;
            if !files.contains_key(&name) {
                return Err(Error::Http(format!(
                    "The server's reflection does not have {name}, which its schema imports"
                )));
            }
        }
    }
    call.end();
    let mut pool = DescriptorPool::new();
    pool.add_file_descriptor_protos(files.into_values())
        .map_err(|e| Error::Invalid(format!("The server's schema is not valid: {e}")))?;
    Ok(Some(pool))
}

/// Sends one question and reads its answer; `None` when the server ended
/// the stream instead.
async fn ask(call: &mut Call, question: Ask) -> Result<Option<Answer>> {
    let request = ReflectionRequest {
        host: String::new(),
        ask: Some(question),
    };
    call.send(&request.encode_to_vec())?;
    let Some(bytes) = call.next().await? else {
        return Ok(None);
    };
    let response = ReflectionResponse::decode(bytes)
        .map_err(|e| Error::Http(format!("The server's reflection answer is not valid: {e}")))?;
    Ok(response.answer)
}

fn take_files(
    answer: Option<Answer>,
    asked: &str,
    files: &mut HashMap<String, FileDescriptorProto>,
    bytes: &mut usize,
) -> Result<()> {
    match answer {
        Some(Answer::Files(found)) => {
            for encoded in found.file_descriptor_proto {
                *bytes += encoded.len();
                if *bytes > MAX_BYTES || files.len() >= MAX_FILES {
                    return Err(Error::Http(format!(
                        "The server's schema has more than {MAX_FILES} files or {} MB",
                        MAX_BYTES / 1024 / 1024
                    )));
                }
                let file = FileDescriptorProto::decode(encoded.as_slice()).map_err(|e| {
                    Error::Http(format!(
                        "The server sent a file descriptor that is not valid: {e}"
                    ))
                })?;
                files.entry(file.name().to_owned()).or_insert(file);
            }
            Ok(())
        }
        Some(Answer::Error(e)) => Err(Error::Http(format!(
            "The server's reflection could not describe {asked}: {}",
            e.error_message
        ))),
        Some(Answer::Services(_)) => Err(Error::Http(format!(
            "The server's reflection answered a question about {asked} with a list of services"
        ))),
        None => Err(Error::Http(format!(
            "The server ended reflection while describing {asked}"
        ))),
    }
}
