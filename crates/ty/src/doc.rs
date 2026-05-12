use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig, DisplayDiagnostics};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ty_doc::{GenerationOptions, generate};
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::settings::TerminalSettings;
use ty_project::{Db as _, ProjectDatabase, ProjectMetadata};

use crate::args::DocCommand;
use crate::logging::{VerbosityLevel, setup_tracing};
use crate::printer::Printer;
use crate::{ExitStatus, set_colored_override, version};

pub(crate) fn run(args: DocCommand) -> Result<ExitStatus> {
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    set_colored_override(args.color);

    let verbosity = args.verbosity.level();
    let _guard = setup_tracing(verbosity, args.color.unwrap_or_default())?;
    let printer = Printer::new(verbosity, args.no_progress);

    tracing::debug!("Version: {}", version::version());

    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd).map_err(|path| {
            anyhow!(
                "The current working directory `{}` contains non-Unicode characters. ty only supports Unicode paths.",
                path.display()
            )
        })?
    };

    let project_path = args
        .project
        .as_ref()
        .map(|project| {
            if project.as_std_path().is_dir() {
                Ok(SystemPath::absolute(project, &cwd))
            } else {
                Err(anyhow!(
                    "Provided project path `{project}` is not a directory"
                ))
            }
        })
        .transpose()?
        .unwrap_or_else(|| cwd.clone());

    let doc_paths: Vec<_> = args
        .paths
        .iter()
        .map(|path| SystemPath::absolute(path, &cwd))
        .collect();
    let output_dir = args
        .output_dir
        .as_ref()
        .map(|path| SystemPath::absolute(path, &cwd));
    let open = args.open;
    let document_private_items = args.document_private_items;
    let config_file = args
        .config_file
        .as_ref()
        .map(|path| SystemPath::absolute(path, &cwd));
    let force_exclude = args.force_exclude();

    let system = OsSystem::new(&cwd);
    let mut project_metadata = match &config_file {
        Some(config_file) => {
            ProjectMetadata::from_config_file(config_file.clone(), &project_path, &system)?
        }
        None => ProjectMetadata::discover(&project_path, &system)?,
    };

    project_metadata.apply_configuration_files(&system)?;

    let project_options_overrides = ProjectOptionsOverrides::new(config_file, args.into_options());
    project_metadata.apply_overrides(&project_options_overrides);

    let mut db = ProjectDatabase::fallible(project_metadata, system)?;
    let project = db.project();

    project.set_verbose(&mut db, verbosity >= VerbosityLevel::Verbose);
    project.set_force_exclude(&mut db, force_exclude);

    let default_selection = doc_paths.is_empty();
    if !doc_paths.is_empty() {
        project.set_included_paths(&mut db, doc_paths);
    }

    let output_dir =
        output_dir.unwrap_or_else(|| project.root(&db).join(SystemPath::new("target/doc")));
    let terminal_settings = project.settings(&db).terminal();
    let is_human_readable = terminal_settings.output_format.is_human_readable();

    if is_human_readable {
        writeln!(
            printer.stream_for_details().lock(),
            " Documenting {} ({})",
            project.name(&db),
            project.root(&db)
        )?;
    }

    let started = ruff_db::Instant::now();
    let generated = generate(
        &db,
        output_dir.as_std_path(),
        GenerationOptions {
            document_private_items,
            default_selection,
            generator_version: version::version().to_string(),
        },
    )?;

    if generated.documented_files == 0 {
        tracing::warn!("No python files found under the given path(s)");
    }

    write_doc_warnings(printer, &db, terminal_settings, &generated.warnings)?;

    if is_human_readable {
        if !generated.warnings.is_empty() {
            writeln!(
                printer.stream_for_failure_summary(),
                "warning: `{}` (doc) generated {} warning{}",
                generated.project_name,
                generated.warnings.len(),
                if generated.warnings.len() == 1 {
                    ""
                } else {
                    "s"
                }
            )?;
        }

        writeln!(
            printer.stream_for_success_summary(),
            "    Finished documentation in {:.3}s",
            started.elapsed().as_secs_f64()
        )?;
        writeln!(
            printer.stream_for_success_summary(),
            "   Generated {}",
            generated.index_path.display()
        )?;
    }

    if open {
        open_documentation(&generated.index_path)?;
    }

    Ok(ExitStatus::Success)
}

fn write_doc_warnings(
    printer: Printer,
    db: &ProjectDatabase,
    terminal_settings: &TerminalSettings,
    diagnostics: &[Diagnostic],
) -> Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    let mut stdout = printer.stream_for_details().lock();
    if stdout.is_enabled() {
        let display_config = DisplayDiagnosticConfig::new("ty")
            .format(terminal_settings.output_format.into())
            .color(colored::control::SHOULD_COLORIZE.should_colorize())
            .context(0);

        write!(
            stdout,
            "{}",
            DisplayDiagnostics::new(db, &display_config, diagnostics)
        )?;
    }

    Ok(())
}

fn open_documentation(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    let status = command
        .status()
        .with_context(|| format!("Failed to open `{}`", path.display()))?;
    if !status.success() {
        return Err(anyhow!(
            "Failed to open `{}`: opener exited with {status}",
            path.display()
        ));
    }

    Ok(())
}
