use std::path::Path;

use ruff_python_ast::PythonVersion;
use ruff_python_trivia::CommentRanges;

use crate::Locator;
use crate::checkers::ast::LintContext;
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
    context: &LintContext,
) {
    // flake8-no-pep420
    if context.is_rule_enabled(Rule::ImplicitNamespacePackage) {
        let allow_nested_roots = is_allow_nested_roots_enabled(settings);
        implicit_namespace_package(
            path,
            package,
            locator,
            comment_ranges,
            &settings.project_root,
            &settings.src,
            allow_nested_roots,
            context,
        );
    }

    // pep8-naming
    if context.is_rule_enabled(Rule::InvalidModuleName) {
        invalid_module_name(path, package, &settings.pep8_naming.ignore_names, context);
    }

    // flake8-builtins
    if context.is_rule_enabled(Rule::StdlibModuleShadowing) {
        stdlib_module_shadowing(path, settings, target_version, context);
    }
}
