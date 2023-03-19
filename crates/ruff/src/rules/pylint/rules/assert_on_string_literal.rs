use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `assert` statements that use a string literal as the first
/// argument.
///
/// ## Why is this bad?
/// An `assert` on a non-empty string literal will always pass.
/// An 'assert' on an emtpy string literal will always fail.
///
/// ## Example
/// ```python
/// assert "always true"
/// ```
#[violation]
pub struct AssertOnStringLiteral {
    length: usize,
    f_type: bool,
}

impl Violation for AssertOnStringLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertOnStringLiteral { length, f_type } = self;
        if *f_type {
            format!("Asserting on a string literal may have unintended results")
        } else {
            if *length == 0 {
                format!("Asserting on an empty string literal will always pass")
            } else {
                format!("Asserting on a non-empty string literal will always pass")
            }
        }
    }
}

/// PLW0129
pub fn assert_on_string_literal(checker: &mut Checker, test: &Expr) {
    match &test.node {
        ExprKind::Constant { value, .. } => match value {
            Constant::Str(s, ..) => {
                checker.diagnostics.push(Diagnostic::new(
                    AssertOnStringLiteral {
                        length: s.len(),
                        f_type: false,
                    },
                    Range::from(test),
                ));
            }
            Constant::Bytes(b) => {
                checker.diagnostics.push(Diagnostic::new(
                    AssertOnStringLiteral {
                        length: b.len(),
                        f_type: false,
                    },
                    Range::from(test),
                ));
            }
            _ => {}
        },
        ExprKind::JoinedStr { .. } => {
            checker.diagnostics.push(Diagnostic::new(
                AssertOnStringLiteral {
                    length: 0,
                    f_type: true,
                },
                Range::from(test),
            ));
        }
        _ => {}
    }
}
