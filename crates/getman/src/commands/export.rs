//! `getman export`: print a request as code, or export a folder, the
//! workspace or an environment for other tools.

use std::process::ExitCode;

use getman_core::Workspace;
use getman_core::execute::{Context, Executor};
use getman_core::export::{Exported, codegen, openapi, postman};
use getman_core::model::{Request, RequestKind};
use getman_core::vars::missing_reason;

use super::Outcome;
use crate::cli::ExportArgs;
use crate::style;
use crate::workspaces;

pub(crate) fn export(args: &ExportArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let target = match args.request.as_deref() {
        Some(given) => Some(workspaces::item_path(&workspace, given)?),
        None => None,
    };
    let is_folder = |path: &str| workspace.resolve(path).is_ok_and(|p| p.is_dir());
    let exported = match args.language.as_str() {
        "postman" if args.globals => {
            postman::globals(&workspace, args.with_secrets).map_err(|e| e.to_string())?
        }
        "postman" if target.is_none() && args.vars.env.is_some() => {
            let id =
                workspaces::environment(&workspace, args.vars.env.as_deref())?.unwrap_or_default();
            postman::environment(&workspace, &id, args.with_secrets).map_err(|e| e.to_string())?
        }
        format @ ("postman" | "openapi") => {
            let folder = target.unwrap_or_default();
            if !is_folder(&folder) {
                return Err(format!(
                    "{folder} is not a folder; export a folder or the workspace as {format}, or a request as code"
                ));
            }
            if format == "postman" {
                postman::collection(&workspace, &folder)
            } else {
                let yaml = args.output.as_ref().is_some_and(|o| {
                    o.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml")
                    })
                });
                openapi::document(&workspace, &folder, yaml)
            }
            .map_err(|e| e.to_string())?
        }
        language => {
            let Some(path) = target.filter(|p| !is_folder(p)) else {
                return Err(format!(
                    "give a request to write as {language}; folders export with --to postman or --to openapi"
                ));
            };
            code(workspace, &path, language, args)?
        }
    };
    for warning in &exported.warnings {
        style::warn(warning);
    }
    match &args.output {
        Some(file) => {
            std::fs::write(file, &exported.content)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            style::note(&format!("Wrote {}", file.display()));
        }
        None => println!("{}", exported.content),
    }
    Ok(ExitCode::SUCCESS)
}

fn code(
    workspace: Workspace,
    path: &str,
    language: &str,
    args: &ExportArgs,
) -> Result<Exported, String> {
    let environment = workspaces::environment(&workspace, args.vars.env.as_deref())?;
    let request = workspace.read_request(path).map_err(|e| e.to_string())?;
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
    if request.kind.is_realtime() {
        return Err(format!(
            "{} requests are not written as code; connect to them with getman send",
            request.kind.label()
        ));
    }
    if request.kind == RequestKind::Grpc || language == "grpcurl" {
        return export_grpc(&executor, path, &request, &context, language);
    }
    let prepared = if args.keep_vars {
        executor
            .prepare_unresolved(Some(path), &request)
            .map_err(|e| e.to_string())?
    } else {
        executor
            .prepare(Some(path), &request, &context)
            .map_err(|e| e.to_string())?
    };
    for name in &prepared.missing {
        style::warn(&missing_reason(name));
    }
    let mut outgoing = prepared.outgoing;
    codegen::relativize(&mut outgoing, executor.workspace.root());
    let snippet = codegen::generate(language, &outgoing)
        .ok_or_else(|| format!("{language} is not a language Getman writes"))?;
    Ok(Exported {
        content: snippet,
        ..Exported::default()
    })
}

/// A gRPC request as a `grpcurl` command, its only form; `curl`, the
/// default, means the same.
fn export_grpc(
    executor: &Executor,
    path: &str,
    request: &Request,
    context: &Context,
    language: &str,
) -> Result<Exported, String> {
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
    Ok(Exported {
        content: code,
        ..Exported::default()
    })
}
