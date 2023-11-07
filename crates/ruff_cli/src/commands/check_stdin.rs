use std::path::Path;

use anyhow::Result;

use ruff_linter::packaging;
use ruff_linter::settings::flags;
use ruff_linter::settings::types::ExtensionMapping;
use ruff_workspace::resolver::{match_exclusion, python_file_at_path, PyprojectConfig};

use crate::args::CliOverrides;
use crate::diagnostics::{lint_stdin, Diagnostics};
use crate::stdin::read_from_stdin;

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    pyproject_config: &PyprojectConfig,
    overrides: &CliOverrides,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
    extension_override: &ExtensionMapping,
) -> Result<Diagnostics> {
    if let Some(filename) = filename {
        if !python_file_at_path(filename, pyproject_config, overrides)? {
            return Ok(Diagnostics::default());
        }

        let lint_settings = &pyproject_config.settings.linter;
        if filename
            .file_name()
            .is_some_and(|name| match_exclusion(filename, name, &lint_settings.exclude))
        {
            return Ok(Diagnostics::default());
        }
    }
    let package_root = filename.and_then(Path::parent).and_then(|path| {
        packaging::detect_package_root(path, &pyproject_config.settings.linter.namespace_packages)
    });
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(
        filename,
        package_root,
        stdin,
        &pyproject_config.settings,
        noqa,
        fix_mode,
        extension_override,
    )?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}
