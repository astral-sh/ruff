use rustpython_ast::{KeywordData, Located};

use super::super::helpers::{matches_password_name, string_literal};
use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// S106
pub fn hardcoded_password_funcarg(keywords: &Vec<Located<KeywordData>>) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();

    for keyword in keywords {
        if let Some(string) = string_literal(&keyword.node.value) {
            if let Some(arg) = &keyword.node.arg {
                if matches_password_name(arg) {
                    checks.push(Check::new(
                        CheckKind::HardcodedPasswordFuncArg(string.to_string()),
                        Range::from_located(&keyword),
                    ));
                }
            }
        }
    }
    checks
}
