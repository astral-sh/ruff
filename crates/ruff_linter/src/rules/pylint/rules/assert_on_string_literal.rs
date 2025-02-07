use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

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
#[derive(ViolationMetadata)]
pub(crate) struct AssertOnStringLiteral {
    kind: Kind,
}

impl Violation for AssertOnStringLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            Kind::Empty => "Asserting on an empty string literal will never pass".to_string(),
            Kind::NonEmpty => {
                "Asserting on a non-empty string literal will always pass".to_string()
            }
            Kind::Unknown => {
                "Asserting on a string literal may have unintended results".to_string()
            }
        }
    }
}

/// PLW0129
pub(crate) fn assert_on_string_literal(checker: &Checker, test: &Expr) {
    match test {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            checker.report_diagnostic(Diagnostic::new(
                AssertOnStringLiteral {
                    kind: if value.is_empty() {
                        Kind::Empty
                    } else {
                        Kind::NonEmpty
                    },
                },
                test.range(),
            ));
        }
        Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
            checker.report_diagnostic(Diagnostic::new(
                AssertOnStringLiteral {
                    kind: if value.is_empty() {
                        Kind::Empty
                    } else {
                        Kind::NonEmpty
                    },
                },
                test.range(),
            ));
        }
        Expr::FString(ast::ExprFString { value, .. }) => {
            let kind = if value.iter().all(|f_string_part| match f_string_part {
                ast::FStringPart::Literal(literal) => literal.is_empty(),
                ast::FStringPart::FString(f_string) => {
                    f_string.elements.iter().all(|element| match element {
                        ast::FStringElement::Literal(ast::FStringLiteralElement {
                            value, ..
                        }) => value.is_empty(),
                        ast::FStringElement::Expression(_) => false,
                    })
                }
            }) {
                Kind::Empty
            } else if value.iter().any(|f_string_part| match f_string_part {
                ast::FStringPart::Literal(literal) => !literal.is_empty(),
                ast::FStringPart::FString(f_string) => {
                    f_string.elements.iter().any(|element| match element {
                        ast::FStringElement::Literal(ast::FStringLiteralElement {
                            value, ..
                        }) => !value.is_empty(),
                        ast::FStringElement::Expression(_) => false,
                    })
                }
            }) {
                Kind::NonEmpty
            } else {
                Kind::Unknown
            };
            checker.report_diagnostic(Diagnostic::new(
                AssertOnStringLiteral { kind },
                test.range(),
            ));
        }
        _ => {}
    }
}
