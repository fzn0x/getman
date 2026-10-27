//! `getman export`: print a request as code.

use std::process::ExitCode;

use getman_core::execute::{Context, Executor};
use getman_core::export::codegen;
use getman_core::model::{Request, RequestKind};
use getman_core::vars::missing_reason;

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
    if !request.kind.is_http() || args.language == "grpcurl" {
        return export_grpc(&executor, &path, &request, &context, &args.language);
    }
    let prepared = executor
        .prepare(Some(&path), &request, &context)
        .map_err(|e| e.to_string())?;
    for name in &prepared.missing {
        style::warn(&missing_reason(name));
    }
    let mut outgoing = prepared.outgoing;
    codegen::relativize(&mut outgoing, executor.workspace.root());
    let code = codegen::generate(&args.language, &outgoing)
        .ok_or_else(|| format!("{} is not a language Getman writes", args.language))?;
    println!("{code}");
    Ok(ExitCode::SUCCESS)
}

/// A gRPC request as a `grpcurl` command, its only form; `curl`, the
/// default, means the same.
fn export_grpc(
    executor: &Executor,
    path: &str,
    request: &Request,
    context: &Context,
    language: &str,
) -> Outcome {
    if request.kind != RequestKind::Grpc {
        return Err("grpcurl writes gRPC requests only; choose another language".into());
    }
    if !matches!(language, "curl" | "grpcurl") {
        return Err(format!(
            "gRPC requests are written as grpcurl commands, not {language}"
        ));
    }
    let (code, missing) = getman_core::grpc::grpcurl(executor, Some(path), request, context)
        .map_err(|e| e.to_string())?;
    for name in &missing {
        style::warn(&missing_reason(name));
    }
    println!("{code}");
    Ok(ExitCode::SUCCESS)
}
