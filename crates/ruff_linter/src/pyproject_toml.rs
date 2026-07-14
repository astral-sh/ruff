use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::SourceFile;

use crate::rules::ruff::rules::invalid_pyproject_toml;
use crate::settings::LinterSettings;

pub fn lint_pyproject_toml(source_file: &SourceFile, settings: &LinterSettings) -> Vec<Diagnostic> {
    invalid_pyproject_toml(source_file, settings)
}
