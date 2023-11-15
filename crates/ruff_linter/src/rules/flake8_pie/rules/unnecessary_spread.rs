use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

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
#[violation]
pub struct UnnecessarySpread;

impl Violation for UnnecessarySpread {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary spread `**`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove unnecessary dict"))
    }
}

/// PIE800
pub(crate) fn unnecessary_spread(checker: &mut Checker, dict: &ast::ExprDict) {
    let mut prev_end = dict.range.start() + TextSize::from(1);
    for item in dict.keys.iter().zip(dict.values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let Expr::Dict(inner) = value {
                let mut diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                if checker.settings.preview.is_enabled() {
                    diagnostic.set_fix(unnecessary_spread_fix(checker, inner, prev_end));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        prev_end = item.1.end();
    }
}

fn unnecessary_spread_fix(checker: &mut Checker, inner: &ast::ExprDict, prev_end: TextSize) -> Fix {
    let tokenizer = SimpleTokenizer::starts_at(prev_end, checker.locator().contents());
    let mut start = None;
    for tok in tokenizer {
        if let SimpleTokenKind::DoubleStar = tok.kind() {
            start = Some(tok.range.start());
            break;
        }
    }
    // unwrap is ok, b/c item.0 can't be None without a DoubleStar
    let doublestar = start.unwrap();

    if let Some(last) = inner.values.last() {
        let tokenizer =
            SimpleTokenizer::starts_at(last.range().end(), checker.locator().contents());
        let mut edits = vec![];
        for tok in tokenizer.skip_trivia() {
            match tok.kind() {
                SimpleTokenKind::Comma => {
                    edits.push(Edit::range_deletion(tok.range()));
                }
                SimpleTokenKind::RBrace => {
                    edits.push(Edit::range_deletion(tok.range));
                    break;
                }
                _ => {}
            }
        }
        Fix::safe_edits(
            // Delete the first `**{`
            Edit::deletion(doublestar, inner.start() + TextSize::from(1)),
            // Delete the trailing `}`
            edits,
        )
    } else {
        // Can just delete the entire thing
        Fix::safe_edit(Edit::deletion(doublestar, inner.end()))
    }
}
