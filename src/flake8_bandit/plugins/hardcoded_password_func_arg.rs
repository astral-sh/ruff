use rustpython_ast::Keyword;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_bandit::helpers::{matches_password_name, string_literal};

/// S106
pub fn hardcoded_password_func_arg(keywords: &[Keyword]) -> Vec<Check> {
    keywords
        .iter()
        .filter_map(|keyword| {
            if let Some(string) = string_literal(&keyword.node.value) {
                if let Some(arg) = &keyword.node.arg {
                    if matches_password_name(arg) {
                        return Some(Check::new(
                            CheckKind::HardcodedPasswordFuncArg(string.to_string()),
                            Range::from_located(keyword),
                        ));
                    }
                }
            }
            None
        })
        .collect()
}
