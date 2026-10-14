//! Errors returned by the core.

use std::io;
use std::path::PathBuf;

/// Something that went wrong in the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// What the operating system reported.
        source: io::Error,
    },
    /// A file is not valid YAML, or not the expected shape.
    #[error("{}: {source}", path.display())]
    Yaml {
        /// The file involved.
        path: PathBuf,
        /// What the parser reported.
        source: serde_yaml_ng::Error,
    },
    /// An item that was asked for does not exist.
    #[error("{0} does not exist")]
    NotFound(String),
    /// An item already exists where one was to be created.
    #[error("{0} already exists")]
    AlreadyExists(String),
    /// A path names something outside the workspace or is malformed.
    #[error("invalid path {0:?}")]
    InvalidPath(String),
    /// A request or input could not be used as given.
    #[error("{0}")]
    Invalid(String),
    /// Sending a request failed.
    #[error("{0}")]
    Http(String),
    /// A script failed to run.
    #[error("{0}")]
    Script(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Result type of the core.
pub type Result<T, E = Error> = std::result::Result<T, E>;
