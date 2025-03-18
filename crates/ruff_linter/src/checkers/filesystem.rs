use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::PythonVersion;
use ruff_python_trivia::CommentRanges;

use crate::package::PackageRoot;
use crate::registry::Rule;
use crate::rules::flake8_builtins::rules::stdlib_module_shadowing;
use crate::rules::flake8_no_pep420::rules::implicit_namespace_package;
use crate::rules::pep8_naming::rules::invalid_module_name;
use crate::settings::LinterSettings;
use crate::Locator;

pub(crate) fn check_file_path(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
    target_version: PythonVersion,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // flake8-no-pep420
    if settings.rules.enabled(Rule::ImplicitNamespacePackage) {
        if let Some(diagnostic) = implicit_namespace_package(
            path,
            package,
            locator,
            comment_ranges,
            &settings.project_root,
            &settings.src,
            settings.preview,
        ) {
            diagnostics.push(diagnostic);
        }
    }

    // pep8-naming
    if settings.rules.enabled(Rule::InvalidModuleName) {
        if let Some(diagnostic) =
            invalid_module_name(path, package, &settings.pep8_naming.ignore_names)
        {
            diagnostics.push(diagnostic);
        }
    }

    // flake8-builtins
    if settings.rules.enabled(Rule::StdlibModuleShadowing) {
        if let Some(diagnostic) = stdlib_module_shadowing(path, settings, target_version) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
