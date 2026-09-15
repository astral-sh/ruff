use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::Result;
use ruff_db::system::OsSystem;
use ty_project::ProjectMetadata;
use ty_site_packages::PythonEnvironment;

use crate::args::{ServerCommand, TerminalColor};
use crate::logging::{VerbosityLevel, setup_tracing};
use crate::printer::Printer;
use crate::{ExitStatus, current_directory};

/// Unix execute-permission bits for the file owner, group, and others.
#[cfg(unix)]
const EXECUTE_BITS: u32 = 0o111;

pub(crate) fn run(args: &ServerCommand) -> Result<ExitStatus> {
    if args.find_executable {
        return find_executable();
    }
    ty_server::run_server()?;
    Ok(ExitStatus::Success)
}

fn find_executable() -> Result<ExitStatus> {
    let verbosity = VerbosityLevel::Quiet;
    let printer = Printer::new(verbosity, true);
    let _guard = setup_tracing(verbosity, TerminalColor::default())?;
    let cwd = current_directory()?;
    let system = OsSystem::new(&cwd);

    let environment = match ProjectMetadata::discover(&cwd, &system) {
        Ok(project) => match project.to_merged_options().python_environment(&system) {
            Ok(None) => PythonEnvironment::discover(Some(project.root()), &system)
                .map_err(anyhow::Error::from),
            configured => configured,
        },
        Err(error) => {
            tracing::debug!("Failed to discover a project, falling back to `{cwd}`: {error}");
            PythonEnvironment::discover(Some(&cwd), &system).map_err(anyhow::Error::from)
        }
    };

    let environment = match environment {
        Ok(Some(environment)) => environment,
        Ok(None) => return Ok(ExitStatus::Failure),
        Err(error) => {
            tracing::debug!("Failed to discover a Python environment: {error}");
            return Ok(ExitStatus::Failure);
        }
    };

    let candidate = environment.sys_prefix().join(if cfg!(windows) {
        "Scripts/ty.exe"
    } else {
        "bin/ty"
    });

    let Ok(metadata) = fs::metadata(candidate.as_std_path()).inspect_err(|error| {
        tracing::debug!("Failed to read file metadata for `{candidate}`: {error}");
    }) else {
        return Ok(ExitStatus::Failure);
    };

    if !metadata.is_file() {
        return Ok(ExitStatus::Failure);
    }

    #[cfg(unix)]
    if metadata.permissions().mode() & EXECUTE_BITS == 0 {
        return Ok(ExitStatus::Failure);
    }

    writeln!(printer.stream_for_requested_summary().lock(), "{candidate}")?;

    Ok(ExitStatus::Success)
}
