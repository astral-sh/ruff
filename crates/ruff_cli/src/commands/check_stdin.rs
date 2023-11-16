use std::path::Path;

use anyhow::Result;

use ruff_linter::packaging;
use ruff_linter::settings::flags;
use ruff_workspace::resolver::{match_exclusion, python_file_at_path, PyprojectConfig};

use crate::args::CliOverrides;
use crate::diagnostics::{lint_stdin, Diagnostics};
use crate::stdin::{parrot_stdin, read_from_stdin};

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    pyproject_config: &PyprojectConfig,
    overrides: &CliOverrides,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
) -> Result<Diagnostics> {
    if pyproject_config.settings.file_resolver.force_exclude {
        if let Some(filename) = filename {
            if !python_file_at_path(filename, pyproject_config, overrides)? {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }

            let lint_settings = &pyproject_config.settings.linter;
            if filename
                .file_name()
                .is_some_and(|name| match_exclusion(filename, name, &lint_settings.exclude))
            {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }
        }
    }
    let stdin = read_from_stdin()?;
    let package_root = filename.and_then(Path::parent).and_then(|path| {
        packaging::detect_package_root(path, &pyproject_config.settings.linter.namespace_packages)
    });
    let mut diagnostics = lint_stdin(
        filename,
        package_root,
        stdin,
        &pyproject_config.settings,
        noqa,
        fix_mode,
    )?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}
