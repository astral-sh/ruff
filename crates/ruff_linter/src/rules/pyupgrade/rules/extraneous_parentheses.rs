use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::tokens::SpannedKind;
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
fn match_extraneous_parentheses(tokens: &[SpannedKind], mut i: usize) -> Option<(usize, usize)> {
    i += 1;

    loop {
        if i >= tokens.len() {
            return None;
        }
        let tok = &tokens[i].kind();
        match tok {
            TokenKind::Comment | TokenKind::NonLogicalNewline => {
                i += 1;
            }
            TokenKind::Lpar => {
                break;
            }
            _ => {
                return None;
            }
        }
    }

    // Store the location of the extraneous opening parenthesis.
    let start = i;

    // Verify that we're not in a tuple or coroutine.
    let mut depth = 1u32;
    while depth > 0 {
        i += 1;
        if i >= tokens.len() {
            return None;
        }
        let tok = &tokens[i].kind();

        // If we find a comma or a yield at depth 1 or 2, it's a tuple or coroutine.
        if depth == 1 && matches!(tok, TokenKind::Comma | TokenKind::Yield) {
            return None;
        } else if matches!(tok, TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb) {
            depth = depth.saturating_add(1);
        } else if matches!(tok, TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb) {
            depth = depth.saturating_sub(1);
        }
    }

    // Store the location of the extraneous closing parenthesis.
    let end = i;

    // Verify that we're not in an empty tuple.
    if (start + 1..i).all(|i| {
        matches!(
            tokens[i].kind(),
            TokenKind::Comment | TokenKind::NonLogicalNewline
        )
    }) {
        return None;
    }

    // Find the next non-coding token.
    i += 1;
    loop {
        if i >= tokens.len() {
            return None;
        }
        let tok = &tokens[i].kind();
        match tok {
            TokenKind::Comment | TokenKind::NonLogicalNewline => {
                i += 1;
            }
            _ => {
                break;
            }
        }
    }

    if i >= tokens.len() {
        return None;
    }
    let tok = &tokens[i].kind();
    if matches!(tok, TokenKind::Rpar) {
        Some((start, end))
    } else {
        None
    }
}

/// UP034
pub(crate) fn extraneous_parentheses(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &[SpannedKind],
    locator: &Locator,
) {
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i].kind(), TokenKind::Lpar) {
            if let Some((start, end)) = match_extraneous_parentheses(tokens, i) {
                i = end + 1;
                let start = &tokens[start];
                let end = &tokens[end];
                let mut diagnostic = Diagnostic::new(
                    ExtraneousParentheses,
                    TextRange::new(start.start(), end.end()),
                );
                let contents = locator.slice(TextRange::new(start.start(), end.end()));
                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    contents[1..contents.len() - 1].to_string(),
                    start.start(),
                    end.end(),
                )));
                diagnostics.push(diagnostic);
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}
