use anyhow::{Context, Result};
use pyproject_toml::PyProjectToml;
use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::SourceFile;

use crate::message::Message;
use crate::rules::ruff::rules::InvalidPyprojectToml;

pub fn lint_pyproject_toml(source_file: SourceFile) -> Result<Vec<Message>> {
    let err = match toml::from_str::<PyProjectToml>(source_file.source_text()) {
        Ok(_) => return Ok(Vec::default()),
        Err(err) => err,
    };

    let range = match err.span() {
        // This is bad but sometimes toml and/or serde just don't give us spans
        // TODO(konstin,micha): https://github.com/charliermarsh/ruff/issues/4571
        None => TextRange::default(),
        Some(range) => {
            let expect_err = "pyproject.toml should be smaller than 4GB";
            TextRange::new(
                TextSize::try_from(range.start).context(expect_err)?,
                TextSize::try_from(range.end).context(expect_err)?,
            )
        }
    };

    let toml_err = err.message().to_string();
    let diagnostic = Diagnostic::new(InvalidPyprojectToml { message: toml_err }, range);
    Ok(vec![Message::from_diagnostic(
        diagnostic,
        source_file,
        TextSize::default(),
    )])
}
