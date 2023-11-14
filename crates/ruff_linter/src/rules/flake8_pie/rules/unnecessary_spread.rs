use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::{BackwardsTokenizer, SimpleTokenKind};
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
pub(crate) fn unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let Expr::Dict(dict) = value {
                let mut diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                if checker.settings.preview.is_enabled() {
                    // Delete the `**{`
                    let tokenizer = BackwardsTokenizer::up_to(
                        dict.range.start(),
                        checker.locator().contents(),
                        &[],
                    );
                    let mut start = None;
                    for tok in tokenizer {
                        if let SimpleTokenKind::DoubleStar = tok.kind() {
                            start = Some(tok.range.start());
                            break;
                        }
                    }
                    // unwrap is ok, b/c item.0 can't be None without a DoubleStar
                    let first =
                        Edit::deletion(start.unwrap(), dict.range.start() + TextSize::from(1));

                    // Delete the `}` (and possibly a trailing comma) but preserve comments
                    let mut edits = Vec::with_capacity(1);
                    let mut end = dict.range.end();

                    let tokenizer = BackwardsTokenizer::up_to(
                        dict.range.end() - TextSize::from(1),
                        checker.locator().contents(),
                        &[],
                    );
                    for tok in tokenizer {
                        match tok.kind() {
                            SimpleTokenKind::Comment => {
                                if tok.range.end() != end {
                                    edits.push(Edit::deletion(tok.range.end(), end));
                                }
                                end = tok.range.start();
                            }
                            SimpleTokenKind::Comma
                            | SimpleTokenKind::Whitespace
                            | SimpleTokenKind::Newline
                            | SimpleTokenKind::Continuation => {}
                            _ => {
                                if tok.range.end() != end {
                                    edits.push(Edit::deletion(tok.range.end(), end));
                                }
                                break;
                            }
                        }
                    }
                    diagnostic.set_fix(Fix::safe_edits(first, edits));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
