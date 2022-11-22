#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]
#![deny(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cloned_instead_of_copied,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::explicit_deref_methods,
    clippy::explicit_iter_loop,
    clippy::from_iter_instead_of_collect,
    clippy::if_not_else,
    clippy::inefficient_to_string,
    clippy::items_after_statements,
    clippy::let_underscore_drop,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_bool,
    clippy::mut_mut,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::unreadable_literal
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
mod cst;
mod directives;
mod docstrings;
mod flake8_2020;
pub mod flake8_annotations;
pub mod flake8_bandit;
mod flake8_blind_except;
pub mod flake8_boolean_trap;
pub mod flake8_bugbear;
mod flake8_builtins;
mod flake8_comprehensions;
mod flake8_print;
pub mod flake8_quotes;
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
mod python;
mod pyupgrade;
mod rules;
mod rustpython_helpers;
pub mod settings;
pub mod source_code_locator;
#[cfg(feature = "update-informer")]
pub mod updates;
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

    let settings = Settings::from_configuration(Configuration::from_pyproject(
        pyproject.as_ref(),
        project_root.as_ref(),
    )?);

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
    )?;

    Ok(checks)
}
