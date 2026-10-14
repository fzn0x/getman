//! Command-line arguments.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// A local-first API client. Requests are plain YAML files in your
/// repository; no account, no cloud.
///
/// Without a command, Getman opens its browser UI on the workspace that
/// contains the current directory.
///
#[derive(Debug, Parser)]
#[command(
    name = "getman",
    version,
    propagate_version = true,
    max_term_width = 100
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    #[command(flatten)]
    pub(crate) open: OpenArgs,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Open the browser UI (the default).
    Open(OpenArgs),
    /// Run a folder or the whole workspace and report the results, for CI.
    Run(RunArgs),
    /// Send one request and print the response body.
    Send(SendArgs),
    /// Create a workspace.
    Init(InitArgs),
    /// Import a Postman collection, environment or globals file, or a cURL
    /// command.
    Import(ImportArgs),
    /// Print a request as code: cURL, HTTPie, PowerShell, fetch, Python,
    /// Go, Rust or Java.
    Export(ExportArgs),
}

#[derive(Debug, Args, Default)]
pub(crate) struct OpenArgs {
    /// Workspace directory. Defaults to the workspace containing the current
    /// directory, or your personal workspace.
    pub(crate) dir: Option<PathBuf>,

    /// Port to serve the UI on. Defaults to 7878, or the next free port.
    #[arg(long, short)]
    pub(crate) port: Option<u16>,

    /// Print the address instead of opening a browser.
    #[arg(long)]
    pub(crate) no_open: bool,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspaceArgs {
    /// Workspace directory. Defaults to the workspace containing the current
    /// directory.
    #[arg(long, short = 'w', global = true, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct VarArgs {
    /// Environment to use, by file name or display name.
    #[arg(long, short = 'e', value_name = "NAME", env = "GETMAN_ENVIRONMENT")]
    pub(crate) env: Option<String>,

    /// Set a variable for this run, overriding every other value. Repeat
    /// for more.
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_var)]
    pub(crate) vars: Vec<(String, String)>,

    /// Seconds a script may run before it is stopped.
    #[arg(long, value_name = "SECONDS", default_value_t = 10.0)]
    pub(crate) script_timeout: f64,
}

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// Folder or request to run, as a path in the workspace or a path on
    /// disk. Runs the whole workspace when left out.
    pub(crate) target: Option<String>,

    #[command(flatten)]
    pub(crate) workspace: WorkspaceArgs,

    #[command(flatten)]
    pub(crate) vars: VarArgs,

    /// Number of iterations. Defaults to the number of data rows, or 1.
    #[arg(long, short = 'n')]
    pub(crate) iterations: Option<usize>,

    /// CSV or JSON file with one row of variables per iteration.
    #[arg(long, short = 'd', value_name = "FILE")]
    pub(crate) data: Option<PathBuf>,

    /// Milliseconds to wait between requests.
    #[arg(long, value_name = "MS", default_value_t = 0)]
    pub(crate) delay: u64,

    /// Stop at the first failure.
    #[arg(long)]
    pub(crate) bail: bool,

    /// Reporters: `cli` (the default), `json` and `junit`, each optionally
    /// followed by `=PATH` to write to a file. Repeat for more.
    #[arg(long = "reporter", short = 'r', value_name = "NAME[=PATH]", value_parser = parse_reporter)]
    pub(crate) reporters: Vec<Reporter>,

    /// Print every response's status line, headers and body.
    #[arg(long, short)]
    pub(crate) verbose: bool,

    /// Save variables set during the run to the git-ignored local files,
    /// as the UI does.
    #[arg(long)]
    pub(crate) persist: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reporter {
    pub(crate) kind: ReporterKind,
    pub(crate) path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ReporterKind {
    Cli,
    Json,
    Junit,
}

#[derive(Debug, Args)]
pub(crate) struct SendArgs {
    /// The request: a path in the workspace or a path on disk.
    pub(crate) request: String,

    #[command(flatten)]
    pub(crate) workspace: WorkspaceArgs,

    #[command(flatten)]
    pub(crate) vars: VarArgs,

    /// Print the status line and headers before the body.
    #[arg(long, short = 'i')]
    pub(crate) include: bool,

    /// Print everything that happened as JSON instead of the body.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Directory to create the workspace in. Defaults to the current one.
    pub(crate) dir: Option<PathBuf>,

    /// Workspace name. Defaults to the directory's name.
    #[arg(long)]
    pub(crate) name: Option<String>,

    /// Add example requests against a public test API.
    #[arg(long)]
    pub(crate) example: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ImportArgs {
    /// File to import, or `-` to read standard input. A cURL command can
    /// be given directly with --curl instead.
    #[arg(required_unless_present = "curl")]
    pub(crate) file: Option<PathBuf>,

    /// A cURL command to import as a request.
    #[arg(long, conflicts_with = "file")]
    pub(crate) curl: Option<String>,

    #[command(flatten)]
    pub(crate) workspace: WorkspaceArgs,

    /// Folder of the workspace to import into.
    #[arg(long, default_value = "")]
    pub(crate) folder: String,

    /// Make a collection the workspace itself instead of a folder in it.
    #[arg(long)]
    pub(crate) root: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ExportArgs {
    /// The request: a path in the workspace or a path on disk.
    pub(crate) request: String,

    /// Language or tool to write.
    #[arg(long = "to", short = 't', default_value = "curl", value_parser = clap::builder::PossibleValuesParser::new(
        getman_core::export::codegen::LANGUAGES.iter().map(|l| l.id)
    ))]
    pub(crate) language: String,

    #[command(flatten)]
    pub(crate) workspace: WorkspaceArgs,

    #[command(flatten)]
    pub(crate) vars: VarArgs,
}

fn parse_var(text: &str) -> Result<(String, String), String> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| format!("{text:?} should look like NAME=VALUE"))?;
    if name.trim().is_empty() {
        return Err("the variable name is empty".into());
    }
    Ok((name.trim().to_owned(), value.to_owned()))
}

fn parse_reporter(text: &str) -> Result<Reporter, String> {
    let (kind, path) = match text.split_once('=') {
        Some((kind, path)) => (kind, Some(PathBuf::from(path))),
        None => (text, None),
    };
    let kind = ReporterKind::from_str(kind, true)
        .map_err(|_| format!("{kind:?} is not a reporter; use cli, json or junit"))?;
    Ok(Reporter { kind, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn reporters_and_vars_parse() {
        assert_eq!(
            parse_reporter("junit=out/report.xml").unwrap(),
            Reporter {
                kind: ReporterKind::Junit,
                path: Some("out/report.xml".into())
            }
        );
        assert!(parse_reporter("html").is_err());
        assert_eq!(parse_var("a=b=c").unwrap(), ("a".into(), "b=c".into()));
        assert!(parse_var("novalue").is_err());
    }

    #[test]
    fn the_default_command_opens_the_ui() {
        let cli = Cli::try_parse_from(["getman", "--port", "9000"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.open.port, Some(9000));
        let cli = Cli::try_parse_from(["getman", "open", "api", "--no-open"]).unwrap();
        let Some(Command::Open(args)) = cli.command else {
            panic!()
        };
        assert!(args.no_open);
    }

    #[test]
    fn run_takes_a_target_environment_and_reporters() {
        let cli = Cli::try_parse_from([
            "getman",
            "run",
            "users",
            "-e",
            "staging",
            "-r",
            "junit=r.xml",
        ])
        .unwrap();
        let Some(Command::Run(args)) = cli.command else {
            panic!()
        };
        assert_eq!(args.target.as_deref(), Some("users"));
        assert_eq!(args.vars.env.as_deref(), Some("staging"));
        assert_eq!(args.reporters[0].kind, ReporterKind::Junit);
    }
}
