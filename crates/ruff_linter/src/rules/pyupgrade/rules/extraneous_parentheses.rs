use std::slice::Iter;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

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
    tokens: &Tokens,
    iter: &mut Iter<'_, Token>,
) -> Option<(TextRange, TextRange)> {
    // Store the location of the extraneous opening parenthesis.
    let start_range = loop {
        let token = iter.next()?;

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

    // Track whether the parenthesized content is a generator expression.
    // A `for` at depth 1 can only be the `for` of a generator expression,
    // since any other comprehension needs its own brackets.
    let mut is_generator = false;

    // Store the location of the extraneous closing parenthesis.
    let end_range = loop {
        let token = iter.next()?;

        match token.kind() {
            // If we find a comma or a yield at depth 1 or 2, it's a tuple or coroutine.
            TokenKind::Comma | TokenKind::Yield if depth == 1 => return None,
            TokenKind::For if depth == 1 => is_generator = true,
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

    // A generator expression needs its parentheses as a class base, since
    // `class Foo(base for base in bases)` does not parse. The parentheses
    // are extraneous everywhere else, e.g. as the sole argument of a call,
    // where `f(x for x in y)` is valid.
    if is_generator && is_class_base(tokens, start_range.start()) {
        return None;
    }

    // Find the next non-coding token.
    let token = loop {
        let token = iter.next()?;

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

/// Returns the index of the previous non-trivia token before `index`, or
/// `None` if there is none.
fn prev_significant_token(tokens: &[Token], index: usize) -> Option<usize> {
    let mut index = index;
    while index > 0 {
        index -= 1;
        if !matches!(
            tokens[index].kind(),
            TokenKind::Comment | TokenKind::NonLogicalNewline
        ) {
            return Some(index);
        }
    }
    None
}

/// Returns `true` if the parenthesized expression starting at `start` is a
/// base of a class definition, e.g. the `(base for base in bases)` in
/// `class Foo((base for base in bases))`.
fn is_class_base(tokens: &Tokens, start: TextSize) -> bool {
    let before = tokens.before(start);

    // Find the `(` that opens the group directly containing these
    // parentheses, e.g. the class argument list in
    // `class Foo(object, (base for base in bases))`.
    let mut depth = 0u32;
    let mut index = before.len();
    let mut open_paren = None;
    while let Some(prev) = prev_significant_token(before, index) {
        index = prev;
        match before[index].kind() {
            TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb => {
                depth = depth.saturating_add(1);
            }
            TokenKind::Lpar => {
                if depth == 0 {
                    open_paren = Some(index);
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            TokenKind::Lbrace | TokenKind::Lsqb => {
                if depth == 0 {
                    // The parentheses sit inside a list, set, or dict
                    // literal, not a class base list.
                    return false;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    let Some(mut index) = open_paren else {
        return false;
    };

    // The token before the enclosing `(` is the class name, or the `]` of a
    // type parameter list, as in `class Foo[T]((base for base in bases))`.
    let Some(prev) = prev_significant_token(before, index) else {
        return false;
    };
    index = prev;
    if before[index].kind() == TokenKind::Rsqb {
        // Walk back over the type parameter list to its `[`.
        let mut depth = 1u32;
        while let Some(prev) = prev_significant_token(before, index) {
            index = prev;
            match before[index].kind() {
                TokenKind::Rsqb | TokenKind::Rpar | TokenKind::Rbrace => {
                    depth = depth.saturating_add(1);
                }
                TokenKind::Lsqb | TokenKind::Lpar | TokenKind::Lbrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return false;
        }
        let Some(prev) = prev_significant_token(before, index) else {
            return false;
        };
        index = prev;
    }

    if before[index].kind() != TokenKind::Identifier {
        return false;
    }

    let Some(index) = prev_significant_token(before, index) else {
        return false;
    };
    before[index].kind() == TokenKind::Class
}

/// UP034
pub(crate) fn extraneous_parentheses(context: &LintContext, tokens: &Tokens, locator: &Locator) {
    let mut token_iter = tokens.iter();
    while let Some(token) = token_iter.next() {
        if !matches!(token.kind(), TokenKind::Lpar) {
            continue;
        }

        let Some((start_range, end_range)) = match_extraneous_parentheses(tokens, &mut token_iter)
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
