use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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

fn is_t_suffixed_type_alias(name: &str) -> bool {
    // A T suffixed, private type alias must begin with an underscore.
    if !name.starts_with('_') {
        return false;
    }

    // It must end in a lowercase letter, followed by `T`, and (optionally) a digit.
    let mut revchars = name.chars().rev();
    matches!(
        (revchars.next(), revchars.next(), revchars.next()), // note this is end -> beginning
        (Some('0'..='9'), Some('T'), Some('a'..='z')) | (Some('T'), Some('a'..='z'), _)
    )
}

fn is_snake_case_type_alias(name: &str) -> bool {
    // The first letter must be alphabetic, optionally preceded by an '_'.
    let mut chars = name.chars();
    let Some(mut first) = chars.next() else { return false; };

    if first == '_' {
        if let Some(c) = chars.next() {
            first = c;
        } else {
            return false;
        }
    }

    // There should be at least one other underscore in the name.
    first.is_ascii_alphabetic() && chars.any(|c| c == '_')
}

pub fn snake_case_type_alias(checker: &mut Checker, target: &Expr) {
    if let ExprKind::Name { id, .. } = target.node() {
        if !is_snake_case_type_alias(id) {
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
        if !is_t_suffixed_type_alias(id) {
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
