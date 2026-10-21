use std::slice::Iter;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for extraneous parentheses.
///
/// ## Why is this bad?
/// Extraneous parentheses are redundant, and can be removed to improve
/// readability while retaining identical semantics.
///
/// ## Example
/// ```python
/// print(("Hello, world"))
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.228", category = Category::Complexity)]
pub(crate) struct ExtraneousParentheses;

impl AlwaysFixableViolation for ExtraneousParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid extraneous parentheses".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove extraneous parentheses".to_string()
    }
}

// See: https://github.com/asottile/pyupgrade/blob/97ed6fb3cf2e650d4f762ba231c3f04c41797710/pyupgrade/_main.py#L148
fn match_extraneous_parentheses(
    tokens: &mut Iter<'_, Token>,
    is_class_def: bool,
) -> Option<(TextRange, TextRange)> {
    // Store the location of the extraneous opening parenthesis.
    let start_range = loop {
        let token = tokens.next()?;

        match token.kind() {
            TokenKind::Comment | TokenKind::NonLogicalNewline => {
                continue;
            }
            TokenKind::Lpar => {
                break token.range();
            }
            _ => {
                return None;
            }
        }
    };

    // Verify that we're not in an empty tuple.
    let mut empty_tuple = true;

    // Verify that we're not in a tuple or coroutine.
    let mut depth = 1u32;
    let mut has_for_at_depth_1 = false;

    // Store the location of the extraneous closing parenthesis.
    let end_range = loop {
        let token = tokens.next()?;

        match token.kind() {
            // If we find a comma or a yield at depth 1 or 2, it's a tuple or coroutine.
            TokenKind::Comma | TokenKind::Yield if depth == 1 => return None,
            TokenKind::For if depth == 1 => {
                has_for_at_depth_1 = true;
            }
            TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb => {
                depth = depth.saturating_add(1);
            }
            TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }

        if depth == 0 {
            break token.range();
        }

        if !matches!(
            token.kind(),
            TokenKind::Comment | TokenKind::NonLogicalNewline
        ) {
            empty_tuple = false;
        }
    };

    if empty_tuple {
        return None;
    }

    // In a class definition (`class Foo((base for base in bases)):`), the outer parentheses
    // enclose base classes, which cannot be a bare generator expression without parentheses.
    // See: https://github.com/astral-sh/ruff/issues/29422
    if is_class_def && has_for_at_depth_1 {
        return None;
    }

    // Find the next non-coding token.
    let token = loop {
        let token = tokens.next()?;

        match token.kind() {
            TokenKind::Comment | TokenKind::NonLogicalNewline => continue,
            _ => {
                break token;
            }
        }
    };

    if matches!(token.kind(), TokenKind::Rpar) {
        Some((start_range, end_range))
    } else {
        None
    }
}

/// Returns true if the opening parenthesis at `lpar_index` is the argument list of a class definition.
fn is_class_def_lpar(tokens: &[Token], lpar_index: usize) -> bool {
    let mut iter = tokens[..lpar_index].iter().rev();

    // Skip non-coding tokens between the class name (or type params) and the opening parenthesis.
    let mut prev = loop {
        match iter.next() {
            Some(token)
                if matches!(
                    token.kind(),
                    TokenKind::Comment | TokenKind::NonLogicalNewline
                ) =>
            {
                continue;
            }
            Some(token) => break token,
            None => return false,
        }
    };

    // If type parameters are present (e.g. `class Foo[T](...)`), skip backwards over `[...]`.
    if matches!(prev.kind(), TokenKind::Rsqb) {
        let mut depth = 1u32;
        prev = loop {
            match iter.next() {
                Some(token) => match token.kind() {
                    TokenKind::Comment | TokenKind::NonLogicalNewline => continue,
                    TokenKind::Rsqb => depth = depth.saturating_add(1),
                    TokenKind::Lsqb => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            break loop {
                                match iter.next() {
                                    Some(tok)
                                        if matches!(
                                            tok.kind(),
                                            TokenKind::Comment | TokenKind::NonLogicalNewline
                                        ) =>
                                    {
                                        continue;
                                    }
                                    Some(tok) => break tok,
                                    None => return false,
                                }
                            };
                        }
                    }
                    _ => {}
                },
                None => return false,
            }
        };
    }

    // The token preceding the parentheses (or `[...]`) must be the class name.
    if !matches!(prev.kind(), TokenKind::Identifier) {
        return false;
    }

    // The token preceding the class name must be `class`.
    let class_token = loop {
        match iter.next() {
            Some(token)
                if matches!(
                    token.kind(),
                    TokenKind::Comment | TokenKind::NonLogicalNewline
                ) =>
            {
                continue;
            }
            Some(token) => break token,
            None => return false,
        }
    };

    matches!(class_token.kind(), TokenKind::Class)
}

/// UP034
pub(crate) fn extraneous_parentheses(context: &LintContext, tokens: &Tokens, locator: &Locator) {
    let mut token_iter = tokens.iter();
    while let Some(token) = token_iter.next() {
        if !matches!(token.kind(), TokenKind::Lpar) {
            continue;
        }

        let current_index = tokens.len() - token_iter.len() - 1;
        let is_class_def = is_class_def_lpar(tokens, current_index);

        let Some((start_range, end_range)) =
            match_extraneous_parentheses(&mut token_iter, is_class_def)
        else {
            continue;
        };

        if let Some(mut diagnostic) = context.report_diagnostic_if_enabled(
            ExtraneousParentheses,
            TextRange::new(start_range.start(), end_range.end()),
        ) {
            let contents = locator.slice(TextRange::new(start_range.start(), end_range.end()));
            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                contents[1..contents.len() - 1].to_string(),
                start_range.start(),
                end_range.end(),
            )));
        }
    }
}
