#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_helpers::tokenize;
use rustpython_parser::lexer::LexResult;
use settings::{pyproject, Settings};

use crate::checks::Check;
use crate::linter::check_path;
use crate::settings::configuration::Configuration;
use crate::source_code_locator::SourceCodeLocator;

mod ast;
pub mod autofix;
pub mod cache;
pub mod check_ast;
mod check_imports;
mod check_lines;
mod check_tokens;
pub mod checks;
pub mod checks_gen;
pub mod cli;
pub mod code_gen;
pub mod commands;
mod cst;
mod directives;
mod docstrings;
mod eradicate;
mod flake8_2020;
pub mod flake8_annotations;
pub mod flake8_bandit;
mod flake8_blind_except;
pub mod flake8_boolean_trap;
pub mod flake8_bugbear;
mod flake8_builtins;
mod flake8_comprehensions;
mod flake8_debugger;
mod flake8_import_conventions;
mod flake8_print;
pub mod flake8_quotes;
mod flake8_return;
pub mod flake8_tidy_imports;
pub mod fs;
mod isort;
mod lex;
pub mod linter;
pub mod logging;
pub mod mccabe;
pub mod message;
mod noqa;
pub mod pep8_naming;
pub mod printer;
mod pycodestyle;
mod pydocstyle;
mod pyflakes;
mod pygrep_hooks;
mod pylint;
mod python;
mod pyupgrade;
mod ruff;
mod rustpython_helpers;
pub mod settings;
pub mod source_code_locator;
#[cfg(feature = "update-informer")]
pub mod updates;
mod vendored;
pub mod visibility;

/// Run Ruff over Python source code directly.
pub fn check(path: &Path, contents: &str, autofix: bool) -> Result<Vec<Check>> {
    // Find the project root and pyproject.toml.
    let project_root = pyproject::find_project_root(&[path.to_path_buf()]);
    match &project_root {
        Some(path) => debug!("Found project root at: {:?}", path),
        None => debug!("Unable to identify project root; assuming current directory..."),
    };
    let pyproject = pyproject::find_pyproject_toml(project_root.as_ref());
    match &pyproject {
        Some(path) => debug!("Found pyproject.toml at: {:?}", path),
        None => debug!("Unable to find pyproject.toml; using default settings..."),
    };

    let settings = Settings::from_configuration(
        Configuration::from_pyproject(pyproject.as_ref(), project_root.as_ref())?,
        project_root.as_ref(),
    )?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(contents);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        directives::Flags::from_settings(&settings),
    );

    // Generate checks.
    let checks = check_path(
        path,
        contents,
        tokens,
        &locator,
        &directives,
        &settings,
        autofix,
        false,
    )?;

    Ok(checks)
}
