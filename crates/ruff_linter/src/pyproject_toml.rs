use std::path::Path;

use ruff_db::diagnostic::Diagnostic;

use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::rules::ruff::rules::invalid_pyproject_toml;
use crate::settings::LinterSettings;

pub fn lint_pyproject_toml(
    path: &Path,
    contents: &str,
    settings: &LinterSettings,
) -> Vec<Diagnostic> {
    let context = LintContext::new(path, contents, settings);

    if context.is_rule_enabled(Rule::InvalidPyprojectToml) {
        invalid_pyproject_toml(&context);
    }

    context.into_diagnostics()
}
