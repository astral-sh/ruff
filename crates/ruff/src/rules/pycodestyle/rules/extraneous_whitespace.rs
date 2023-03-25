#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use crate::rules::pycodestyle::rules::Whitespace;
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for the use of extraneous whitespace after "(".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// spam( ham[1], {eggs: 2})
/// spam(ham[ 1], {eggs: 2})
/// spam(ham[1], { eggs: 2})
/// ```
///
/// Use instead:
/// ```python
/// spam(ham[1], {eggs: 2})
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceAfterOpenBracket;

impl Violation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after '('")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ")".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// spam(ham[1], {eggs: 2} )
/// spam(ham[1 ], {eggs: 2})
/// spam(ham[1], {eggs: 2 })
/// ```
///
/// Use instead:
/// ```python
/// spam(ham[1], {eggs: 2})
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceBeforeCloseBracket;

impl Violation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ')'")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ",", ";" or ":".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// if x == 4: print(x, y); x, y = y , x
/// ```
///
/// Use instead:
/// ```python
/// if x == 4: print(x, y); x, y = y, x
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceBeforePunctuation;

impl Violation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ',', ';', or ':'")
    }
}

/// E201, E202, E203
#[cfg(feature = "logical_lines")]
pub fn extraneous_whitespace(
    tokens: &[(Location, &Tok, Location)],
    locator: &Locator,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut last_token: Option<&Tok> = None;

    for (start, token, end) in tokens {
        match token {
            Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                let after = &locator.contents()[locator.offset(*end)..];

                if !matches!(Whitespace::leading(after), Whitespace::None) {
                    diagnostics.push((
                        Location::new(end.row(), end.column()),
                        WhitespaceAfterOpenBracket.into(),
                    ));
                }
            }
            Tok::Rbrace | Tok::Rpar | Tok::Rsqb | Tok::Comma | Tok::Semi | Tok::Colon => {
                let before = &locator.contents()[..locator.offset(*start)];

                let diagnostic_kind = if matches!(token, Tok::Comma | Tok::Semi | Tok::Colon) {
                    DiagnosticKind::from(WhitespaceBeforePunctuation)
                } else {
                    DiagnosticKind::from(WhitespaceBeforeCloseBracket)
                };

                match Whitespace::trailing(before) {
                    (Whitespace::None, _) => {}
                    (_, offset) => {
                        if !matches!(last_token, Some(Tok::Comma)) {
                            diagnostics.push((
                                Location::new(start.row(), start.column() - offset),
                                diagnostic_kind,
                            ));
                        }
                    }
                }
            }

            _ => {}
        }

        last_token = Some(token);
    }

    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn extraneous_whitespace(
    _tokens: &[(Location, &Tok, Location)],
    _locator: &Locator,
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
