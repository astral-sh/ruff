use rustpython_parser::ast::Location;
use std::path::Path;

use rustpython_parser::parser;
use wasm_bindgen::prelude::*;

use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::checks::{Check, CheckCode, CheckKind, ALL_CHECK_CODES};
use crate::settings::Settings;

mod ast;
mod autofix;
pub mod check_ast;
mod check_lines;
pub mod checks;
mod python;
pub mod settings;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Message {
    code: CheckCode,
    message: String,
    location: Location,
}

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);
}

#[wasm_bindgen]
pub fn check(contents: &str) -> String {
    let settings = Settings::for_rules(ALL_CHECK_CODES.to_vec());
    let autofix = autofix::fixer::Mode::None;

    // Aggregate all checks.
    let mut checks: Vec<Check> = vec![];

    // Run the AST-based checks.
    match parser::parse_program(contents, "<filename>") {
        Ok(python_ast) => checks.extend(check_ast(
            &python_ast,
            contents,
            &settings,
            &autofix,
            Path::new("<filename>"),
        )),
        Err(parse_error) => {
            if settings.select.contains(&CheckCode::E999) {
                checks.push(Check::new(
                    CheckKind::SyntaxError(parse_error.error.to_string()),
                    parse_error.location,
                ))
            }
        }
    }

    // Run the lines-based checks.
    check_lines(&mut checks, contents, &settings);

    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            code: check.kind.code().clone(),
            message: check.kind.body(),
            location: check.location,
        })
        .collect();

    serde_json::to_string(&messages).unwrap()
}
