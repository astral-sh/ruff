use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::lexer::LexResult;

use crate::autofix::fixer::Mode;
use crate::linter::{check_path, tokenize};
use crate::message::Message;
use crate::settings::{RawSettings, Settings};

mod ast;
mod autofix;
pub mod cache;
pub mod check_ast;
mod check_lines;
pub mod checks;
pub mod cli;
pub mod code_gen;
pub mod fs;
pub mod linter;
pub mod logging;
pub mod message;
mod noqa;
mod plugins;
pub mod printer;
pub mod pyproject;
mod python;
pub mod settings;

/// Run ruff over Python source code directly.
pub fn check(path: &Path, contents: &str) -> Result<Vec<Message>> {
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

    let settings = Settings::from_raw(RawSettings::from_pyproject(&pyproject, &project_root)?);

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Determine the noqa line for every line in the source.
    let noqa_line_for = noqa::extract_noqa_line_for(&tokens);

    // Generate checks.
    let checks = check_path(
        path,
        contents,
        tokens,
        &noqa_line_for,
        &settings,
        &Mode::None,
    )?;

    // Convert to messages.
    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            fixed: check.fix.map(|fix| fix.applied).unwrap_or_default(),
            location: check.location,
            end_location: check.end_location,
            filename: path.to_string_lossy().to_string(),
        })
        .collect();

    Ok(messages)
}
