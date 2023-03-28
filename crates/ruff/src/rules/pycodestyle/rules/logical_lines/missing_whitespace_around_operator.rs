use rustpython_parser::ast::Location;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

use crate::rules::pycodestyle::rules::logical_lines::LogicalLineTokens;

// E225
#[violation]
pub struct MissingWhitespaceAroundOperator;

impl Violation for MissingWhitespaceAroundOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around operator")
    }
}

// E226
#[violation]
pub struct MissingWhitespaceAroundArithmeticOperator;

impl Violation for MissingWhitespaceAroundArithmeticOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around arithmetic operator")
    }
}

// E227
#[violation]
pub struct MissingWhitespaceAroundBitwiseOrShiftOperator;

impl Violation for MissingWhitespaceAroundBitwiseOrShiftOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around bitwise or shift operator")
    }
}

// E228
#[violation]
pub struct MissingWhitespaceAroundModuloOperator;

impl Violation for MissingWhitespaceAroundModuloOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around modulo operator")
    }
}

/// E225, E226, E227, E228
#[allow(clippy::if_same_then_else)]
pub(crate) fn missing_whitespace_around_operator(
    tokens: &LogicalLineTokens,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];

    let mut needs_space_main: Option<bool> = Some(false);
    let mut needs_space_aux: Option<bool> = None;
    let mut prev_end_aux: Option<Location> = None;
    let mut parens = 0u32;
    let mut prev_type: Option<TokenKind> = None;
    let mut prev_end: Option<Location> = None;

    for token in tokens {
        let kind = token.kind();

        if kind.is_skip_comment() {
            continue;
        }

        match kind {
            TokenKind::Lpar | TokenKind::Lambda => parens += 1,
            TokenKind::Rpar => parens -= 1,
            _ => {}
        };

        let needs_space =
            needs_space_main == Some(true) || needs_space_aux.is_some() || prev_end_aux.is_some();
        if needs_space {
            if Some(token.start()) != prev_end {
                if needs_space_main != Some(true) && needs_space_aux != Some(true) {
                    diagnostics.push((
                        prev_end_aux.unwrap(),
                        MissingWhitespaceAroundOperator.into(),
                    ));
                }
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if kind == TokenKind::Greater
                && matches!(prev_type, Some(TokenKind::Less | TokenKind::Minus))
            {
                // Tolerate the "<>" operator, even if running Python 3
                // Deal with Python 3's annotated return value "->"
            } else if prev_type == Some(TokenKind::Slash)
                && matches!(kind, TokenKind::Comma | TokenKind::Rpar | TokenKind::Colon)
                || (prev_type == Some(TokenKind::Rpar) && kind == TokenKind::Colon)
            {
                // Tolerate the "/" operator in function definition
                // For more info see PEP570
            } else {
                if needs_space_main == Some(true) || needs_space_aux == Some(true) {
                    diagnostics.push((prev_end.unwrap(), MissingWhitespaceAroundOperator.into()));
                } else if prev_type != Some(TokenKind::DoubleStar) {
                    if prev_type == Some(TokenKind::Percent) {
                        diagnostics.push((
                            prev_end_aux.unwrap(),
                            MissingWhitespaceAroundModuloOperator.into(),
                        ));
                    } else if !prev_type.unwrap().is_arithmetic() {
                        diagnostics.push((
                            prev_end_aux.unwrap(),
                            MissingWhitespaceAroundBitwiseOrShiftOperator.into(),
                        ));
                    } else {
                        diagnostics.push((
                            prev_end_aux.unwrap(),
                            MissingWhitespaceAroundArithmeticOperator.into(),
                        ));
                    }
                }
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            }
        } else if (kind.is_operator() || matches!(kind, TokenKind::Name)) && prev_end.is_some() {
            if kind == TokenKind::Equal && parens > 0 {
                // Allow keyword args or defaults: foo(bar=None).
            } else if kind.is_whitespace_needed() {
                needs_space_main = Some(true);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if kind.is_unary() {
                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if let Some(prev_type) = prev_type {
                    if (matches!(
                        prev_type,
                        TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                    )) || (!prev_type.is_operator() && !prev_type.is_keyword())
                        && (!prev_type.is_soft_keyword())
                    {
                        needs_space_main = None;
                        needs_space_aux = None;
                        prev_end_aux = None;
                    }
                }
            } else if kind.is_whitespace_optional() {
                needs_space_main = None;
                needs_space_aux = None;
                prev_end_aux = None;
            }

            if needs_space_main.is_none() {
                // Surrounding space is optional, but ensure that
                // trailing space matches opening space
                prev_end_aux = prev_end;
                needs_space_aux = Some(Some(token.start()) != prev_end_aux);
            } else if needs_space_main == Some(true) && Some(token.start()) == prev_end_aux {
                // A needed opening space was not found
                diagnostics.push((prev_end.unwrap(), MissingWhitespaceAroundOperator.into()));
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            }
        }
        prev_type = Some(kind);
        prev_end = Some(token.end());
    }

    diagnostics
}
