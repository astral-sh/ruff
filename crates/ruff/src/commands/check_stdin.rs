use std::path::Path;

use anyhow::Result;
use ruff_db::diagnostic::Diagnostic;
use ruff_linter::package::PackageRoot;
use ruff_linter::packaging;
use ruff_linter::settings::flags;
use ruff_workspace::resolver::{PyprojectConfig, Resolver, match_exclusion, project_file_at_path};
use ruff_workspace::uv::UvImportDiscovery;

use crate::args::ConfigArguments;
use crate::diagnostics::{Diagnostics, lint_stdin};
use crate::stdin::{parrot_stdin, read_from_stdin};

/// Run the linter over a single file, read from `stdin`.
pub(crate) fn check_stdin(
    filename: Option<&Path>,
    pyproject_config: PyprojectConfig,
    overrides: &ConfigArguments,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
) -> Result<Diagnostics> {
    let uv = overrides.use_uv.then(UvImportDiscovery::default);
    let directory = filename
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let pyproject_config = PyprojectConfig {
        settings: pyproject_config
            .settings
            .with_uv_imports(uv.as_ref(), directory),
        ..pyproject_config
    };
    let mut resolver = Resolver::new(&pyproject_config);

    if resolver.force_exclude() {
        if let Some(filename) = filename {
            if !project_file_at_path(filename, &mut resolver, overrides)? {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }

            if filename.file_name().is_some_and(|name| {
                match_exclusion(filename, name, &resolver.base_settings().linter.exclude)
            }) {
                if fix_mode.is_apply() {
                    parrot_stdin()?;
                }
                return Ok(Diagnostics::default());
            }
        }
    }
    let stdin = read_from_stdin()?;
    let package_root = filename.and_then(Path::parent).and_then(|path| {
        packaging::detect_package_root(path, &resolver.base_settings().linter.namespace_packages)
            .map(PackageRoot::root)
    });
    let mut diagnostics = lint_stdin(
        filename,
        package_root,
        stdin,
        resolver.base_settings(),
        noqa,
        fix_mode,
    )?;
    diagnostics
        .inner
        .sort_unstable_by(Diagnostic::ruff_start_ordering);
    Ok(diagnostics)
}
