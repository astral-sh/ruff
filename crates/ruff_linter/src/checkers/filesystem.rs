use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::PythonVersion;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::SourceFile;

use crate::Locator;
use crate::package::PackageRoot;
use crate::preview::is_allow_nested_roots_enabled;
use crate::registry::Rule;
use crate::rules::flake8_builtins::rules::stdlib_module_shadowing;
use crate::rules::flake8_no_pep420::rules::implicit_namespace_package;
use crate::rules::pep8_naming::rules::invalid_module_name;
use crate::settings::LinterSettings;

pub(crate) fn check_file_path(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
    target_version: PythonVersion,
    source_file: SourceFile,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    // flake8-no-pep420
    if settings.rules.enabled(Rule::ImplicitNamespacePackage) {
        let allow_nested_roots = is_allow_nested_roots_enabled(settings);
        if let Some(diagnostic) = implicit_namespace_package(
            path,
            package,
            locator,
            comment_ranges,
            &settings.project_root,
            &settings.src,
            allow_nested_roots,
            source_file.clone(),
        ) {
            diagnostics.push(diagnostic);
        }
    }

    // pep8-naming
    if settings.rules.enabled(Rule::InvalidModuleName) {
        if let Some(diagnostic) = invalid_module_name(
            path,
            package,
            &settings.pep8_naming.ignore_names,
            source_file.clone(),
        ) {
            diagnostics.push(diagnostic);
        }
    }

    // flake8-builtins
    if settings.rules.enabled(Rule::StdlibModuleShadowing) {
        if let Some(diagnostic) =
            stdlib_module_shadowing(path, settings, target_version, source_file)
        {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
