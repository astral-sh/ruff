use std::io::{self, Read};
use std::path::Path;

use anyhow::Result;

use ruff::resolver::PyprojectDiscovery;
use ruff::settings::flags;
use ruff::{packaging, resolver};

use crate::args::Overrides;
use crate::diagnostics::{lint_stdin, Diagnostics};

/// Read a `String` from `stdin`.
fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Run the linter over a single file, read from `stdin`.
pub fn run_stdin(
    filename: Option<&Path>,
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    if let Some(filename) = filename {
        if !resolver::python_file_at_path(filename, pyproject_strategy, overrides)? {
            return Ok(Diagnostics::default());
        }
    }
    let settings = pyproject_strategy.top_level_settings();
    let package_root = filename
        .and_then(Path::parent)
        .and_then(|path| packaging::detect_package_root(path, &settings.lib.namespace_packages));
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(filename, package_root, &stdin, &settings.lib, noqa, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}
