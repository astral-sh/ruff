use std::path::Path;

use pyproject_toml::PyProjectToml;
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

    if let Err(err) = toml::from_str::<PyProjectToml>(contents) {
        if context.is_rule_enabled(Rule::InvalidPyprojectToml) {
            invalid_pyproject_toml(&context, &err);
        }
    }

    context.into_diagnostics()
}
