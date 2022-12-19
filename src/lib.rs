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
use path_absolutize::path_dedot;
use rustpython_helpers::tokenize;
use rustpython_parser::lexer::LexResult;
use settings::{pyproject, Settings};

use crate::checks::Check;
use crate::linter::check_path;
use crate::resolver::Relativity;
use crate::settings::configuration::Configuration;
use crate::settings::flags;
use crate::source_code_locator::SourceCodeLocator;

mod ast;
pub mod autofix;
pub mod cache;
mod checkers;
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
mod flake8_datetimez;
mod flake8_debugger;
pub mod flake8_errmsg;
mod flake8_import_conventions;
mod flake8_print;
pub mod flake8_quotes;
mod flake8_return;
mod flake8_simplify;
pub mod flake8_tidy_imports;
mod flake8_unused_arguments;
pub mod fs;
mod isort;
pub mod iterators;
mod lex;
pub mod linter;
pub mod logging;
pub mod mccabe;
pub mod message;
mod noqa;
mod packages;
mod pandas_vet;
pub mod pep8_naming;
pub mod printer;
mod pycodestyle;
mod pydocstyle;
mod pyflakes;
mod pygrep_hooks;
mod pylint;
mod python;
mod pyupgrade;
pub mod resolver;
mod ruff;
mod rustpython_helpers;
pub mod settings;
pub mod source_code_locator;
#[cfg(feature = "update-informer")]
pub mod updates;
mod vendored;
pub mod visibility;

/// Load the relevant `Settings` for a given `Path`.
fn resolve(path: &Path) -> Result<Settings> {
    if let Some(pyproject) = pyproject::find_pyproject_toml(path)? {
        // First priority: `pyproject.toml` in the current `Path`.
        resolver::resolve_settings(&pyproject, &Relativity::Parent, None)
    } else if let Some(pyproject) = pyproject::find_user_pyproject_toml() {
        // Second priority: user-specific `pyproject.toml`.
        resolver::resolve_settings(&pyproject, &Relativity::Cwd, None)
    } else {
        // Fallback: default settings.
        Settings::from_configuration(Configuration::default(), &path_dedot::CWD)
    }
}

/// Run Ruff over Python source code directly.
pub fn check(path: &Path, contents: &str, autofix: bool) -> Result<Vec<Check>> {
    // Load the relevant `Settings` for the given `Path`.
    let settings = resolve(path)?;

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
        packages::detect_package_root(path),
        contents,
        tokens,
        &locator,
        &directives,
        &settings,
        autofix.into(),
        flags::Noqa::Enabled,
    )?;

    Ok(checks)
}
