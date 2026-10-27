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
    /// Open the terminal UI.
    Tui(TuiArgs),
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
    /// Serve the saved examples of the workspace or a folder as a mock API
    /// on 127.0.0.1.
    Mock(MockArgs),
    /// Let AI agents use the workspace: a Model Context Protocol server on
    /// standard input and output.
    Mcp(McpArgs),
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
pub(crate) struct TuiArgs {
    /// Workspace directory. Defaults to the workspace containing the current
    /// directory, or your personal workspace.
    pub(crate) dir: Option<PathBuf>,
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
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RunArgs {
    /// Folders or requests to run, as paths in the workspace or on disk.
    /// Several run one after another, in the order given. Runs the whole
    /// workspace when left out.
    #[arg(value_name = "TARGET")]
    pub(crate) targets: Vec<String>,

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

    /// Reporters: `cli` (the default), `json`, `junit`, `html`, `csv` and
    /// `xlsx`, each optionally followed by `=PATH` to write to a file
    /// (`xlsx` needs one). Repeat for more.
    #[arg(long = "reporter", short = 'r', value_name = "NAME[=PATH]", value_parser = parse_reporter)]
    pub(crate) reporters: Vec<Reporter>,

    /// Print every response's status line, headers and body.
    #[arg(long, short)]
    pub(crate) verbose: bool,

    /// Save variables set during the run to the git-ignored local files,
    /// as the UI does.
    #[arg(long)]
    pub(crate) persist: bool,

    /// Keep the run in the workspace's git-ignored .getman folder, where
    /// the browser UI's Runner lists past runs. The newest 20 are kept.
    #[arg(long)]
    pub(crate) save: bool,
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
    Html,
    Csv,
    Xlsx,
}

impl ReporterKind {
    /// The file report this reporter writes; `None` for the terminal.
    pub(crate) fn format(self) -> Option<crate::report::Format> {
        use crate::report::Format;
        match self {
            Self::Cli => None,
            Self::Json => Some(Format::Json),
            Self::Junit => Some(Format::Junit),
            Self::Html => Some(Format::Html),
            Self::Csv => Some(Format::Csv),
            Self::Xlsx => Some(Format::Xlsx),
        }
    }
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

#[derive(Debug, Args)]
pub(crate) struct MockArgs {
    /// Folder whose examples to serve, as a path in the workspace or on
    /// disk. Serves every example in the workspace when left out.
    pub(crate) path: Option<String>,

    #[command(flatten)]
    pub(crate) workspace: WorkspaceArgs,

    /// Port to listen on. Defaults to 7979, or the next free port.
    #[arg(long, short)]
    pub(crate) port: Option<u16>,

    /// Milliseconds to wait before each answer.
    #[arg(long, value_name = "MS", default_value_t = 0)]
    pub(crate) delay: u64,
}

#[derive(Debug, Args)]
pub(crate) struct McpArgs {
    /// Workspace directory. Defaults to the workspace containing the
    /// current directory.
    pub(crate) dir: Option<PathBuf>,
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
    let kind = ReporterKind::from_str(kind, true).map_err(|_| {
        format!("{kind:?} is not a reporter; use cli, json, junit, html, csv or xlsx")
    })?;
    if kind == ReporterKind::Xlsx && path.is_none() {
        return Err("the xlsx reporter writes a file; give it one, as in xlsx=report.xlsx".into());
    }
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
        assert_eq!(parse_reporter("HTML").unwrap().kind, ReporterKind::Html);
        assert!(parse_reporter("xlsx").is_err());
        assert!(parse_reporter("xlsx=r.xlsx").is_ok());
        assert!(parse_reporter("pdf").is_err());
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
    fn mock_takes_a_folder_port_and_delay() {
        let cli = Cli::try_parse_from([
            "getman", "mock", "users", "--port", "9001", "--delay", "250",
        ])
        .unwrap();
        let Some(Command::Mock(args)) = cli.command else {
            panic!()
        };
        assert_eq!(args.path.as_deref(), Some("users"));
        assert_eq!((args.port, args.delay), (Some(9001), 250));
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
        assert_eq!(args.targets, ["users"]);
        assert_eq!(args.vars.env.as_deref(), Some("staging"));
        assert_eq!(args.reporters[0].kind, ReporterKind::Junit);

        let cli = Cli::try_parse_from(["getman", "run", "login.yaml", "orders", "--save"]).unwrap();
        let Some(Command::Run(args)) = cli.command else {
            panic!()
        };
        assert_eq!(args.targets, ["login.yaml", "orders"]);
        assert!(args.save);
    }
}
