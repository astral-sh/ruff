use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
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
            if let Expr::Dict(_) = value {
                let mut diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                if checker.settings.preview.is_enabled() {
                    let range = value.range();
                    // unwrap -- `item.0 == None` iff this is a spread operator
                    // which means there *must* be a `**` here
                    let doublestar = checker.locator().up_to(range.start()).rfind("**").unwrap();
                    diagnostic.set_fix(Fix::safe_edits(
                        // delete the `**{`
                        Edit::deletion(
                            TextSize::from(doublestar as u32),
                            range.start() + TextSize::from(1),
                        ),
                        // delete the `}`
                        [Edit::deletion(range.end() - TextSize::from(1), range.end())],
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
