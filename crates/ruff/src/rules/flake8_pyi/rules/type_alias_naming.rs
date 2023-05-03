use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

static CAMEL_CASE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^_?[a-z]").unwrap());

// PYI043: Error for alias names in "T"
// (plus possibly a single digit afterwards), but only if:
//
// - The name starts with "_"
// - The penultimate character in the name is an ASCII-lowercase letter
static T_SUFFIXED_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^_.*[a-z]T\d?$").unwrap());

#[violation]
pub struct SnakeCaseTypeAlias {
    pub name: String,
}

impl Violation for SnakeCaseTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Type alias `{name}` should be CamelCase")
    }
}

#[violation]
pub struct TSuffixedTypeAlias {
    pub name: String,
}

impl Violation for TSuffixedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Private type alias `{name}` should not be suffixed with `T` (the `T` suffix implies that an object is a TypeVar)")
    }
}

pub fn snake_case_type_alias(checker: &mut Checker, target: &Expr) {
    if let ExprKind::Name { id, .. } = target.node() {
        if !CAMEL_CASE_REGEX.is_match(id) {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            SnakeCaseTypeAlias {
                name: id.to_string(),
            },
            target.range(),
        ));
    }
}

pub fn t_suffixed_type_alias(checker: &mut Checker, target: &Expr) {
    if let ExprKind::Name { id, .. } = target.node() {
        if !T_SUFFIXED_REGEX.is_match(id) {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            TSuffixedTypeAlias {
                name: id.to_string(),
            },
            target.range(),
        ));
    }
}
