use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::Locator;

/// ## What it does
/// Checks for unnecessary dictionary unpacking operators (`**`).
///
/// ## Why is this bad?
/// Unpacking a dictionary into another dictionary is redundant. The unpacking
/// operator can be removed, making the code more readable.
///
/// ## Example
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, **{"C": 3}}
/// ```
///
/// Use instead:
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, "C": 3}
/// ```
///
/// ## References
/// - [Python documentation: Dictionary displays](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessarySpread;

impl Violation for UnnecessarySpread {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary spread `**`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary dict".to_string())
    }
}

/// PIE800
pub(crate) fn unnecessary_spread(checker: &mut Checker, dict: &ast::ExprDict) {
    // The first "end" is the start of the dictionary, immediately following the open bracket.
    let mut prev_end = dict.start() + TextSize::from(1);
    for ast::DictItem { key, value } in dict {
        if key.is_none() {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let Expr::Dict(inner) = value {
                let mut diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                if let Some(fix) = unnecessary_spread_fix(inner, prev_end, checker.locator()) {
                    diagnostic.set_fix(fix);
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        prev_end = value.end();
    }
}

/// Generate a [`Fix`] to remove an unnecessary dictionary spread.
fn unnecessary_spread_fix(
    dict: &ast::ExprDict,
    prev_end: TextSize,
    locator: &Locator,
) -> Option<Fix> {
    // Find the `**` token preceding the spread.
    let doublestar = SimpleTokenizer::starts_at(prev_end, locator.contents())
        .find(|tok| matches!(tok.kind(), SimpleTokenKind::DoubleStar))?;

    if let Some(last) = dict.iter_values().last() {
        // Ex) `**{a: 1, b: 2}`
        let mut edits = vec![];
        let mut open_parens: u32 = 0;

        for tok in SimpleTokenizer::starts_at(doublestar.end(), locator.contents()).skip_trivia() {
            match tok.kind() {
                SimpleTokenKind::LParen => {
                    edits.push(Edit::range_deletion(tok.range()));
                    open_parens += 1;
                }
                SimpleTokenKind::LBrace => {
                    edits.push(Edit::range_deletion(tok.range()));
                    break;
                }
                _ => {
                    // Unexpected token, bail
                    return None;
                }
            }
        }

        let mut found_r_curly = false;
        for tok in SimpleTokenizer::starts_at(last.end(), locator.contents()).skip_trivia() {
            if found_r_curly && open_parens == 0 {
                break;
            }

            match tok.kind() {
                SimpleTokenKind::Comma => {
                    edits.push(Edit::range_deletion(tok.range()));
                }
                SimpleTokenKind::RParen => {
                    if found_r_curly {
                        edits.push(Edit::range_deletion(tok.range()));
                        open_parens -= 1;
                    }
                }
                SimpleTokenKind::RBrace => {
                    edits.push(Edit::range_deletion(tok.range()));
                    found_r_curly = true;
                }
                _ => {
                    // Unexpected token, bail
                    return None;
                }
            }
        }

        Some(Fix::safe_edits(
            Edit::range_deletion(doublestar.range()),
            edits,
        ))
    } else {
        // Ex) `**{}`
        Some(Fix::safe_edit(Edit::deletion(
            doublestar.start(),
            dict.end(),
        )))
    }
}
