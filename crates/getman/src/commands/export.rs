//! `getman export`: print a request as code.

use std::process::ExitCode;

use getman_core::execute::{Context, Executor};
use getman_core::export::codegen;

use super::Outcome;
use crate::cli::ExportArgs;
use crate::style;
use crate::workspaces;

pub(crate) fn export(args: &ExportArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let path = workspaces::item_path(&workspace, &args.request)?;
    let environment = workspaces::environment(&workspace, args.vars.env.as_deref())?;
    let request = workspace.read_request(&path).map_err(|e| e.to_string())?;
    let executor = Executor::new(workspace);
    let mut context = Context {
        environment,
        ..Context::default()
    };
    for (name, value) in &args.vars.vars {
        context
            .overlay
            .local
            .insert(name.clone(), Some(value.clone()));
    }
    let prepared = executor
        .prepare(Some(&path), &request, &context)
        .map_err(|e| e.to_string())?;
    for name in &prepared.missing {
        style::warn(&format!("{{{{{name}}}}} is not defined"));
    }
    let mut outgoing = prepared.outgoing;
    codegen::relativize(&mut outgoing, executor.workspace.root());
    let code = codegen::generate(&args.language, &outgoing)
        .ok_or_else(|| format!("{} is not a language Getman writes", args.language))?;
    println!("{code}");
    Ok(ExitCode::SUCCESS)
}
