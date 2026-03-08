use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_python_trivia::leading_indentation;
use ruff_text_size::Ranged;

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::LintContext;

/// ## What it does
/// Checks for indentation mismatch between lines with corresponding opening and closing brackets.
/// This effectively disallows usage of visual indentation (vertical alignment),
/// in favor of hanging indentation.
///
/// ## Why is this bad?
/// Although vertical alignment is allowed as one of the indentation styles in [PEP 8],
/// it decreases usable horizontal space, leads to larger diffs when new elements are added,
/// and requires realigning all lines upon changing the length of the initial expression.
/// Using hanging indentation as the only style improves consistency within a project.
///
/// ## Example
/// ```python
/// function(arg1,
///          arg2=[{f"""
///                 data{i}
///                 """} for i in range(5)])  # line indent = 16, not matching the previous lines
/// ```
///
/// Use instead:
/// ```python
/// function(
///     arg1,
///     arg2=[{f"""
///         data{i}
///     """} for i in range(5)],  # line indent = 4, matching the line with opening brackets
/// )  # line indent = 0, matching the line with opening parenthesis
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct BracketIndentationMismatch;

impl Violation for BracketIndentationMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Closing bracket line indent doesn't match the opening bracket line indent".to_string()
    }
}

pub(crate) fn bracket_indentation_mismatch(
    context: &LintContext,
    tokens: &Tokens,
    locator: &Locator,
) {
    let get_token_indent =
        |token: &Token| leading_indentation(locator.line_str(token.range().start()));
    let mut bracket_stack: Vec<usize> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                bracket_stack.push(index);
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                if let Some(prev_index) = bracket_stack.pop() {
                    if get_token_indent(&tokens[prev_index]) != get_token_indent(token) {
                        context.report_diagnostic_if_enabled(
                            BracketIndentationMismatch,
                            token.range(),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
