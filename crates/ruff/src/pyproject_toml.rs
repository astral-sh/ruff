use std::fs;
use std::path::Path;

use anyhow::Result;
use pyproject_toml::PyProjectToml;
use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::SourceFileBuilder;

use crate::message::Message;
use crate::rules::ruff::rules::InvalidPyprojectToml;

pub fn lint_pyproject_toml(path: &Path) -> Result<Vec<Message>> {
    let contents = fs::read_to_string(path)?;
    let source_file = SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
    let err = match toml::from_str::<PyProjectToml>(source_file.source_text()) {
        Ok(_) => return Ok(Vec::default()),
        Err(err) => err,
    };

    let range = match err.span() {
        // This is bad but sometimes toml and/or serde just don't give us spans
        None => TextRange::default(),
        Some(range) => {
            let expect_err = "pyproject.toml file be smaller than 4GB";
            TextRange::new(
                TextSize::try_from(range.start).expect(expect_err),
                TextSize::try_from(range.end).expect(expect_err),
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
