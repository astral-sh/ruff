use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;

use crate::registry::Rule;
use crate::rules::flake8_builtins::rules::builtin_module_shadowing;
use crate::rules::flake8_no_pep420::rules::implicit_namespace_package;
use crate::rules::pep8_naming::rules::invalid_module_name;
use crate::settings::LinterSettings;

pub(crate) fn check_file_path(
    path: &Path,
    package: Option<&Path>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
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
    if settings.rules.enabled(Rule::BuiltinModuleShadowing) {
        if let Some(diagnostic) = builtin_module_shadowing(
            path,
            package,
            &settings.flake8_builtins.builtins_allowed_modules,
            settings.target_version,
        ) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
