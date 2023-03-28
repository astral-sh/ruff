use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::helpers::{
    is_arithmetic_token, is_keyword_token, is_op_token, is_skip_comment_token,
    is_soft_keyword_token, is_unary_token, is_ws_needed_token, is_ws_optional_token,
};
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
    let mut prev_type: Option<&Tok> = None;
    let mut prev_end: Option<Location> = None;

    for token in tokens {
        let kind = token.kind();

        if is_skip_comment_token(kind) {
            continue;
        }
        match kind {
            Tok::Lpar | Tok::Lambda => parens += 1,
            Tok::Rpar => parens -= 1,
            _ => {}
        };

        let needs_space = (needs_space_main.is_some() && needs_space_main.unwrap())
            || needs_space_aux.is_some()
            || prev_end_aux.is_some();
        if needs_space {
            if Some(token.start()) != prev_end {
                if !(needs_space_main.is_some() && needs_space_main.unwrap())
                    && (needs_space_aux.is_none() || !needs_space_aux.unwrap())
                {
                    diagnostics.push((
                        prev_end_aux.unwrap(),
                        MissingWhitespaceAroundOperator.into(),
                    ));
                }
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if kind == &Tok::Greater && matches!(prev_type, Some(Tok::Less | Tok::Minus)) {
                // Tolerate the "<>" operator, even if running Python 3
                // Deal with Python 3's annotated return value "->"
            } else if prev_type == Some(&Tok::Slash)
                && matches!(kind, Tok::Comma | Tok::Rpar | Tok::Colon)
                || (prev_type == Some(&Tok::Rpar) && kind == &Tok::Colon)
            {
                // Tolerate the "/" operator in function definition
                // For more info see PEP570
            } else {
                if (needs_space_main.is_some() && needs_space_main.unwrap())
                    || (needs_space_aux.is_some() && needs_space_aux.unwrap())
                {
                    diagnostics.push((prev_end.unwrap(), MissingWhitespaceAroundOperator.into()));
                } else if prev_type != Some(&Tok::DoubleStar) {
                    if prev_type == Some(&Tok::Percent) {
                        diagnostics.push((
                            prev_end_aux.unwrap(),
                            MissingWhitespaceAroundModuloOperator.into(),
                        ));
                    } else if !is_arithmetic_token(prev_type.unwrap()) {
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
        } else if (is_op_token(kind) || matches!(kind, Tok::Name { .. })) && prev_end.is_some() {
            if kind == &Tok::Equal && parens > 0 {
                // Allow keyword args or defaults: foo(bar=None).
            } else if is_ws_needed_token(kind) {
                needs_space_main = Some(true);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if is_unary_token(kind) {
                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if let Some(prev_type) = prev_type {
                    if (matches!(prev_type, Tok::Rpar | Tok::Rsqb | Tok::Rbrace))
                        || (!is_op_token(prev_type) && !is_keyword_token(prev_type))
                            && (!is_soft_keyword_token(prev_type))
                    {
                        needs_space_main = None;
                        needs_space_aux = None;
                        prev_end_aux = None;
                    }
                }
            } else if is_ws_optional_token(kind) {
                needs_space_main = None;
                needs_space_aux = None;
                prev_end_aux = None;
            }

            if needs_space_main.is_none() {
                // Surrounding space is optional, but ensure that
                // trailing space matches opening space
                needs_space_main = None;
                prev_end_aux = prev_end;
                needs_space_aux = Some(Some(token.start()) != prev_end_aux);
            } else if needs_space_main.is_some()
                && needs_space_main.unwrap()
                && Some(token.start()) == prev_end_aux
            {
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
