//! Command-line arguments.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// A local-first API client. Requests are plain YAML files in your
/// repository; no account, no cloud.
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
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run a folder or the whole workspace and report the results, for CI.
    Run(RunArgs),
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
