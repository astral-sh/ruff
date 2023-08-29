use std::path::Path;

use anyhow::Result;

use ruff::packaging;
use ruff::settings::flags;
use ruff_workspace::resolver::{python_file_at_path, PyprojectConfig};

use crate::args::Overrides;
use crate::diagnostics::{lint_stdin, Diagnostics};
use crate::stdin::read_from_stdin;

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    pyproject_config: &PyprojectConfig,
    overrides: &Overrides,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    if let Some(filename) = filename {
        if !python_file_at_path(filename, pyproject_config, overrides)? {
            return Ok(Diagnostics::default());
        }
    }
    let package_root = filename.and_then(Path::parent).and_then(|path| {
        packaging::detect_package_root(path, &pyproject_config.settings.lib.namespace_packages)
    });
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(
        filename,
        package_root,
        stdin,
        &pyproject_config.settings.lib,
        noqa,
        autofix,
    )?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}
