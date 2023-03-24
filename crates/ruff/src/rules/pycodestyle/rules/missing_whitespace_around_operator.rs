#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::helpers::{
    is_arithmetic_token, is_keyword_token, is_op_token, is_singleton_token, is_skip_comment_token,
    is_soft_keyword_token, is_unary_token, is_ws_needed_token, is_ws_optional_token,
};

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
#[cfg(debug_assertions)]
#[allow(clippy::if_same_then_else)]
pub fn missing_whitespace_around_operator(
    tokens: &[(Location, &Tok, Location)],
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];

    let mut needs_space_main: Option<bool> = Some(false);
    let mut needs_space_aux: Option<bool> = None;
    let mut prev_end_aux: Option<&Location> = None;
    let mut parens = 0;
    let mut prev_type: Option<&Tok> = None;
    let mut prev_end: Option<&Location> = None;

    for (start, token, end) in tokens {
        if is_skip_comment_token(token) {
            continue;
        }
        if **token == Tok::Lpar || **token == Tok::Lambda {
            parens += 1;
        } else if **token == Tok::Rpar {
            parens -= 1;
        }
        let needs_space = (needs_space_main.is_some() && needs_space_main.unwrap())
            || needs_space_aux.is_some()
            || prev_end_aux.is_some();
        if needs_space {
            if Some(start) != prev_end {
                if !(needs_space_main.is_some() && needs_space_main.unwrap())
                    && (needs_space_aux.is_none() || !needs_space_aux.unwrap())
                {
                    diagnostics.push((
                        *(prev_end_aux.unwrap()),
                        MissingWhitespaceAroundOperator.into(),
                    ));
                }
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if **token == Tok::Greater
                && (prev_type == Some(&Tok::Less) || prev_type == Some(&Tok::Minus))
            {
                // Tolerate the "<>" operator, even if running Python 3
                // Deal with Python 3's annotated return value "->"
            } else if prev_type == Some(&Tok::Slash)
                && (**token == Tok::Comma || **token == Tok::Rpar || **token == Tok::Colon)
                || (prev_type == Some(&Tok::Rpar) && **token == Tok::Colon)
            {
                // Tolerate the "/" operator in function definition
                // For more info see PEP570
            } else {
                if (needs_space_main.is_some() && needs_space_main.unwrap())
                    || (needs_space_aux.is_some() && needs_space_aux.unwrap())
                {
                    diagnostics
                        .push((*(prev_end.unwrap()), MissingWhitespaceAroundOperator.into()));
                } else if prev_type != Some(&Tok::DoubleStar) {
                    if prev_type == Some(&Tok::Percent) {
                        diagnostics.push((
                            *(prev_end_aux.unwrap()),
                            MissingWhitespaceAroundModuloOperator.into(),
                        ));
                    } else if !is_arithmetic_token(prev_type.unwrap()) {
                        diagnostics.push((
                            *(prev_end_aux.unwrap()),
                            MissingWhitespaceAroundBitwiseOrShiftOperator.into(),
                        ));
                    } else {
                        diagnostics.push((
                            *(prev_end_aux.unwrap()),
                            MissingWhitespaceAroundArithmeticOperator.into(),
                        ));
                    }
                }
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            }
        } else if (is_op_token(token) || matches!(token, Tok::Name { .. })) && prev_end.is_some() {
            if **token == Tok::Equal && parens > 0 {
                // Allow keyword args or defaults: foo(bar=None).
            } else if is_ws_needed_token(token) {
                needs_space_main = Some(true);
                needs_space_aux = None;
                prev_end_aux = None;
            } else if is_unary_token(token) {
                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if (prev_type.is_some()
                    && is_op_token(prev_type.unwrap())
                    && (prev_type == Some(&Tok::Rpar)
                        || prev_type == Some(&Tok::Rsqb)
                        || prev_type == Some(&Tok::Rbrace)))
                    || (!is_op_token(prev_type.unwrap()) && !is_keyword_token(prev_type.unwrap()))
                        && (!is_soft_keyword_token(prev_type.unwrap()))
                {
                    needs_space_main = None;
                    needs_space_aux = None;
                    prev_end_aux = None;
                }
            } else if is_ws_optional_token(token) {
                needs_space_main = None;
                needs_space_aux = None;
                prev_end_aux = None;
            }

            if needs_space_main.is_none() {
                // Surrounding space is optional, but ensure that
                // trailing space matches opening space
                needs_space_main = None;
                prev_end_aux = prev_end;
                needs_space_aux = Some(Some(start) != prev_end_aux);
            } else if needs_space_main.is_some()
                && needs_space_main.unwrap()
                && Some(start) == prev_end_aux
            {
                // A needed opening space was not found
                diagnostics.push((*(prev_end.unwrap()), MissingWhitespaceAroundOperator.into()));
                needs_space_main = Some(false);
                needs_space_aux = None;
                prev_end_aux = None;
            }
        }
        prev_type = Some(*token);
        prev_end = Some(end);
    }

    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn missing_whitespace_around_operator(
    _tokens: &[(Location, &Tok, Location)],
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
