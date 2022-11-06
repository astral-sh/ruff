#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]

use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::lexer::LexResult;
use settings::{pyproject, Settings};

use crate::autofix::fixer::Mode;
use crate::checks::Check;
use crate::linter::{check_path, tokenize};
use crate::settings::configuration::Configuration;
use crate::source_code_locator::SourceCodeLocator;

mod ast;
pub mod autofix;
pub mod cache;
pub mod check_ast;
mod check_lines;
mod check_tokens;
pub mod checks;
pub mod checks_gen;
pub mod cli;
pub mod code_gen;
mod cst;
mod docstrings;
pub mod flake8_annotations;
mod flake8_bugbear;
mod flake8_builtins;
mod flake8_comprehensions;
mod flake8_print;
pub mod flake8_quotes;
pub mod fs;
mod import_tracking;
mod lex;
pub mod linter;
pub mod logging;
pub mod message;
mod noqa;
pub mod pep8_naming;
pub mod printer;
mod pycodestyle;
mod pydocstyle;
mod pyflakes;
mod python;
mod pyupgrade;
mod rules;
pub mod settings;
pub mod source_code_locator;
pub mod visibility;

/// Run Ruff over Python source code directly.
pub fn check(path: &Path, contents: &str, autofix: bool) -> Result<Vec<Check>> {
    // Find the project root and pyproject.toml.
    let project_root = pyproject::find_project_root(&[path.to_path_buf()]);
    match &project_root {
        Some(path) => debug!("Found project root at: {:?}", path),
        None => debug!("Unable to identify project root; assuming current directory..."),
    };
    let pyproject = pyproject::find_pyproject_toml(&project_root);
    match &pyproject {
        Some(path) => debug!("Found pyproject.toml at: {:?}", path),
        None => debug!("Unable to find pyproject.toml; using default settings..."),
    };

    let settings =
        Settings::from_configuration(Configuration::from_pyproject(&pyproject, &project_root)?);

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(contents);

    // Determine the noqa line for every line in the source.
    let noqa_line_for = noqa::extract_noqa_line_for(&tokens);

    // Generate checks.
    let checks = check_path(
        path,
        contents,
        tokens,
        &locator,
        &noqa_line_for,
        &settings,
        &if autofix { Mode::Generate } else { Mode::None },
    )?;

    Ok(checks)
}
