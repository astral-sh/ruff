use std::slice::Iter;

use ruff_python_parser::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

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
#[violation]
pub struct ExtraneousParentheses;

impl AlwaysFixableViolation for ExtraneousParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid extraneous parentheses")
    }

    fn fix_title(&self) -> String {
        "Remove extraneous parentheses".to_string()
    }
}

// See: https://github.com/asottile/pyupgrade/blob/97ed6fb3cf2e650d4f762ba231c3f04c41797710/pyupgrade/_main.py#L148
fn match_extraneous_parentheses(tokens: &mut Iter<'_, Token>) -> Option<(TextRange, TextRange)> {
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

    // Store the location of the extraneous closing parenthesis.
    let end_range = loop {
        let token = tokens.next()?;

        match token.kind() {
            // If we find a comma or a yield at depth 1 or 2, it's a tuple or coroutine.
            TokenKind::Comma | TokenKind::Yield if depth == 1 => return None,
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

/// UP034
pub(crate) fn extraneous_parentheses(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &Tokens,
    locator: &Locator,
) {
    let mut token_iter = tokens.iter();
    while let Some(token) = token_iter.next() {
        if !matches!(token.kind(), TokenKind::Lpar) {
            continue;
        }

        let Some((start_range, end_range)) = match_extraneous_parentheses(&mut token_iter) else {
            continue;
        };

        let mut diagnostic = Diagnostic::new(
            ExtraneousParentheses,
            TextRange::new(start_range.start(), end_range.end()),
        );
        let contents = locator.slice(TextRange::new(start_range.start(), end_range.end()));
        diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
            contents[1..contents.len() - 1].to_string(),
            start_range.start(),
            end_range.end(),
        )));
        diagnostics.push(diagnostic);
    }
}
