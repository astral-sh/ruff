/*!
A simple command line tool for ad hoc completion benchmarking.
*/

// This is a developer tool and is therefore fine to use `eprintln!`.
#![allow(clippy::print_stderr)]

use std::io::Write;
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use clap::Parser;

use ruff_db::files::system_path_to_file;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ty_ide::Completion;
use ty_project::metadata::Options;
use ty_project::metadata::options::EnvironmentOptions;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::{ProjectDatabase, ProjectMetadata};

#[derive(Debug, clap::Parser)]
#[command(
    author,
    name = "ty_completion_bench",
    about = "Supports ad hoc benchmarking of completions."
)]
struct Cli {
    /// The file path in which to request completions.
    ///
    /// The project directory is discovered automatically by looking
    /// for a sibling `pyproject.toml` in the file's directory or a
    /// parent.
    #[arg(
        help = "The file path in which to request completions.",
        value_name = "FILE"
    )]
    file: SystemPathBuf,
    /// The byte offset at which to request completions.
    ///
    /// i.e., This is where we should consider the cursor to be.
    #[arg(
        help = "The byte offset at which to request completions.",
        value_name = "INTEGER"
    )]
    offset: usize,
    /// The number of times to request completions after the
    /// initial request.
    #[arg(
        long,
        help = "The number of additional times to request completions.",
        value_name = "INTEGER",
        default_value_t = 0
    )]
    iters: u32,
    /// Whether to run the command in quiet mode.
    #[arg(
        long,
        short = 'q',
        help = "When set, don't print completions to stdout.",
        value_name = "BOOLEAN"
    )]
    quiet: bool,
}

fn main() -> anyhow::Result<ExitCode> {
    let args = Cli::parse();
    let project_dir = discover_project_directory(&args.file)?;
    let offset = ruff_text_size::TextSize::try_from(args.offset).with_context(|| {
        format!(
            "failed to convert file offset `{}` to 32-bit integer",
            args.offset
        )
    })?;

    let uv_sync_output = std::process::Command::new("uv")
        .arg("sync")
        .current_dir(&project_dir)
        .output()
        .with_context(|| format!("failed to run `uv sync` in `{project_dir}`"))?;
    if !uv_sync_output.status.success() {
        let code = uv_sync_output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let stderr = bstr::BStr::new(&uv_sync_output.stderr);
        anyhow::bail!("`uv sync` failed to run with exit code `{code}`, stderr: {stderr}")
    }

    let system = OsSystem::new(&project_dir);
    let mut project_metadata = ProjectMetadata::discover(&project_dir, &system)?;
    // Explicitly point ty to the .venv to avoid any set VIRTUAL_ENV variable to take precedence.
    project_metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python: Some(RelativePathBuf::cli(".venv")),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });
    project_metadata.apply_configuration_files(&system)?;
    let db = ProjectDatabase::new(project_metadata, system)?;

    let start = std::time::Instant::now();
    let mut completions = get_completions(&db, &args.file, offset)?;
    let elapsed = std::time::Instant::now().duration_since(start);
    eprintln!("total elapsed for initial completions request: {elapsed:?}");

    if args.iters > 0 {
        let start = std::time::Instant::now();
        for _ in 0..args.iters {
            completions = get_completions(&db, &args.file, offset)?;
        }
        let elapsed = std::time::Instant::now().duration_since(start);
        let per = elapsed / args.iters;
        eprintln!("total elapsed: {elapsed:?}, time per completion request: {per:?}");
    }

    if !args.quiet {
        let mut stdout = std::io::stdout().lock();
        for c in &completions {
            write!(stdout, "{}", c.name.as_str())?;
            if let Some(module_name) = c.module_name {
                write!(stdout, " (module: {module_name})")?;
            }
            writeln!(stdout)?;
        }
        writeln!(stdout, "-----")?;
        writeln!(stdout, "found {} completions", completions.len())?;
    }
    Ok(ExitCode::SUCCESS)
}

fn get_completions<'db>(
    db: &'db ProjectDatabase,
    path: &SystemPath,
    offset: ruff_text_size::TextSize,
) -> anyhow::Result<Vec<Completion<'db>>> {
    let file = system_path_to_file(db, path)
        .with_context(|| format!("failed to get database file for `{path}`"))?;
    let settings = ty_ide::CompletionSettings { auto_import: true };
    Ok(ty_ide::completion(db, &settings, file, offset))
}

fn discover_project_directory(file: &SystemPath) -> anyhow::Result<SystemPathBuf> {
    for ancestor in file.as_std_path().canonicalize()?.ancestors() {
        if ancestor.join("pyproject.toml").exists() {
            return SystemPathBuf::from_path_buf(ancestor.to_path_buf()).map_err(|path| {
                anyhow!(
                    "Detected project directory `{path}` contains non-Unicode characters. \
                     ty only supports Unicode paths.",
                    path = path.display()
                )
            });
        }
    }
    anyhow::bail!("could not find `pyproject.toml` in any ancestor of `{file}`")
}
