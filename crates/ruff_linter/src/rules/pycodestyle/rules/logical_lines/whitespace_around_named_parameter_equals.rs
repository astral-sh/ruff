use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::{LogicalLine, LogicalLineToken};

/// ## What it does
/// Checks for missing whitespace around the equals sign in an unannotated
/// function keyword parameter.
///
/// ## Why is this bad?
/// According to [PEP 8], there should be no spaces around the equals sign in a
/// keyword parameter, if it is unannotated:
///
/// > Don’t use spaces around the = sign when used to indicate a keyword
/// > argument, or when used to indicate a default value for an unannotated
/// > function parameter.
///
/// ## Example
/// ```python
/// def add(a = 0) -> int:
///     return a + 1
/// ```
///
/// Use instead:
/// ```python
/// def add(a=0) -> int:
///     return a + 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[violation]
pub struct UnexpectedSpacesAroundKeywordParameterEquals;

impl Violation for UnexpectedSpacesAroundKeywordParameterEquals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected spaces around keyword / parameter equals")
    }
}

/// ## What it does
/// Checks for missing whitespace around the equals sign in an annotated
/// function keyword parameter.
///
/// ## Why is this bad?
/// According to [PEP 8], the spaces around the equals sign in a keyword
/// parameter should only be omitted when the parameter is unannotated:
///
/// > Don’t use spaces around the = sign when used to indicate a keyword
/// > argument, or when used to indicate a default value for an unannotated
/// > function parameter.
///
/// ## Example
/// ```python
/// def add(a: int=0) -> int:
///     return a + 1
/// ```
///
/// Use instead:
/// ```python
/// def add(a: int = 0) -> int:
///     return a + 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[violation]
pub struct MissingWhitespaceAroundParameterEquals;

impl Violation for MissingWhitespaceAroundParameterEquals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around parameter equals")
    }
}

fn is_in_def(tokens: &[LogicalLineToken]) -> bool {
    for token in tokens {
        match token.kind() {
            TokenKind::Async | TokenKind::Indent | TokenKind::Dedent => continue,
            TokenKind::Def => return true,
            _ => return false,
        }
    }

    false
}

/// E251, E252
pub(crate) fn whitespace_around_named_parameter_equals(
    line: &LogicalLine,
    context: &mut LogicalLinesContext,
) {
    let mut parens = 0u32;
    let mut fstrings = 0u32;
    let mut annotated_func_arg = false;
    let mut prev_end = TextSize::default();

    let in_def = is_in_def(line.tokens());
    let mut iter = line.tokens().iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();

        if kind == TokenKind::NonLogicalNewline {
            continue;
        }

        match kind {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            TokenKind::Lpar | TokenKind::Lsqb => {
                parens = parens.saturating_add(1);
            }
            TokenKind::Rpar | TokenKind::Rsqb => {
                parens = parens.saturating_sub(1);
                if parens == 0 {
                    annotated_func_arg = false;
                }
            }

            TokenKind::Colon if parens == 1 && in_def => {
                annotated_func_arg = true;
            }
            TokenKind::Comma if parens == 1 => {
                annotated_func_arg = false;
            }
            TokenKind::Equal if parens > 0 && fstrings == 0 => {
                if annotated_func_arg && parens == 1 {
                    let start = token.start();
                    if start == prev_end && prev_end != TextSize::new(0) {
                        context.push(MissingWhitespaceAroundParameterEquals, token.range());
                    }

                    while let Some(next) = iter.peek() {
                        if next.kind() == TokenKind::NonLogicalNewline {
                            iter.next();
                        } else {
                            let next_start = next.start();

                            if next_start == token.end() {
                                context.push(MissingWhitespaceAroundParameterEquals, token.range());
                            }
                            break;
                        }
                    }
                } else {
                    if token.start() != prev_end {
                        context.push(
                            UnexpectedSpacesAroundKeywordParameterEquals,
                            TextRange::new(prev_end, token.start()),
                        );
                    }

                    while let Some(next) = iter.peek() {
                        if next.kind() == TokenKind::NonLogicalNewline {
                            iter.next();
                        } else {
                            if next.start() != token.end() {
                                context.push(
                                    UnexpectedSpacesAroundKeywordParameterEquals,
                                    TextRange::new(token.end(), next.start()),
                                );
                            }
                            break;
                        }
                    }
                }
            }
            _ => {}
        }

        prev_end = token.end();
    }
}
