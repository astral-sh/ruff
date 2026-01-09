use ruff_diagnostics::{Applicability, Fix};
use rustc_hash::FxHashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_ast::comparable::HashableExpr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for set literals that contain duplicate items.
///
/// ## Why is this bad?
/// In Python, sets are unordered collections of unique elements. Including a
/// duplicate item in a set literal is redundant, as the duplicate item will be
/// replaced with a single item at runtime.
///
/// ## Example
/// ```python
/// {1, 2, 3, 1}
/// ```
///
/// Use instead:
/// ```python
/// {1, 2, 3}
/// ```
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the
/// original expression, potentially losing important context or documentation.
///
/// For example:
/// ```python
/// {
///     1,
///     2,
///     # Comment
///     1,
/// }
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
pub(crate) struct DuplicateValue {
    value: String,
    existing: String,
}

impl Violation for DuplicateValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValue { value, existing } = self;
        if value == existing {
            format!("Sets should not contain duplicate item `{value}`")
        } else {
            format!(
                "Sets should not contain duplicate items, but `{existing}` and `{value}` has the same value"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate item".to_string())
    }
}

/// B033
pub(crate) fn duplicate_value(checker: &Checker, set: &ast::ExprSet) {
    let mut seen_values: FxHashMap<HashableExpr, &Expr> = FxHashMap::default();
    for (index, value) in set.iter().enumerate() {
        if value.is_literal_expr() {
            if let Some(existing) = seen_values.insert(HashableExpr::from(value), value) {
                let mut diagnostic = checker.report_diagnostic(
                    DuplicateValue {
                        value: checker.generator().expr(value),
                        existing: checker.generator().expr(existing),
                    },
                    value.range(),
                );
                diagnostic.try_set_fix(|| {
                    edits::remove_member(&set.elts, index, checker.locator().contents()).map(
                        |edit| {
                            let applicability = if checker.comment_ranges().intersects(edit.range())
                            {
                                Applicability::Unsafe
                            } else {
                                Applicability::Safe
                            };
                            Fix::applicable_edit(edit, applicability)
                        },
                    )
                });
            }
        }
    }
}
