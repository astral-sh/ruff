use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Empty,
    NonEmpty,
    Unknown,
}

/// ## What it does
/// Checks for `assert` statements that use a string literal as the first
/// argument.
///
/// ## Why is this bad?
/// An `assert` on a non-empty string literal will always pass, while an
/// `assert` on an empty string literal will always fail.
///
/// ## Example
/// ```python
/// assert "always true"
/// ```
#[violation]
pub struct AssertOnStringLiteral {
    kind: Kind,
}

impl Violation for AssertOnStringLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertOnStringLiteral { kind } = self;
        match kind {
            Kind::Empty => format!("Asserting on an empty string literal will never pass"),
            Kind::NonEmpty => format!("Asserting on a non-empty string literal will always pass"),
            Kind::Unknown => format!("Asserting on a string literal may have unintended results"),
        }
    }
}

/// PLW0129
pub fn assert_on_string_literal(checker: &mut Checker, test: &Expr) {
    match &test.node {
        ExprKind::Constant { value, .. } => match value {
            Constant::Str(value, ..) => {
                checker.diagnostics.push(Diagnostic::new(
                    AssertOnStringLiteral {
                        kind: if value.is_empty() {
                            Kind::Empty
                        } else {
                            Kind::NonEmpty
                        },
                    },
                    Range::from(test),
                ));
            }
            Constant::Bytes(value) => {
                checker.diagnostics.push(Diagnostic::new(
                    AssertOnStringLiteral {
                        kind: if value.is_empty() {
                            Kind::Empty
                        } else {
                            Kind::NonEmpty
                        },
                    },
                    Range::from(test),
                ));
            }
            _ => {}
        },
        ExprKind::JoinedStr { values } => {
            checker.diagnostics.push(Diagnostic::new(
                AssertOnStringLiteral {
                    kind: if values.iter().all(|value| match &value.node {
                        ExprKind::Constant { value, .. } => match value {
                            Constant::Str(value, ..) => value.is_empty(),
                            Constant::Bytes(value) => value.is_empty(),
                            _ => false,
                        },
                        _ => false,
                    }) {
                        Kind::Empty
                    } else if values.iter().any(|value| match &value.node {
                        ExprKind::Constant { value, .. } => match value {
                            Constant::Str(value, ..) => !value.is_empty(),
                            Constant::Bytes(value) => !value.is_empty(),
                            _ => false,
                        },
                        _ => false,
                    }) {
                        Kind::NonEmpty
                    } else {
                        Kind::Unknown
                    },
                },
                Range::from(test),
            ));
        }
        _ => {}
    }
}
