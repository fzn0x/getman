//! The engine behind Getman: the workspace format, variables, sending
//! requests, scripts, checks and the collection runner.
//!
//! Getman's front ends (the browser UI, the terminal UI and the command
//! line) are thin layers over this crate.

mod error;
mod lenient;
pub mod model;
pub mod workspace;

pub use error::{Error, Result};
pub use model::{Auth, Body, Environment, Folder, KeyValue, Request, Settings, Vars};
pub use workspace::{Node, Workspace};
