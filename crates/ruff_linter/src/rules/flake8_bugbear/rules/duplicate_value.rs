use ruff_python_ast::{Expr, ExprSet};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
#[violation]
pub struct DuplicateValue {
    value: String,
}

impl AlwaysFixableViolation for DuplicateValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValue { value } = self;
        format!("Sets should not contain duplicate item `{value}`")
    }

    fn fix_title(&self) -> String {
        let DuplicateValue { value } = self;
        format!("Remove duplicate item `{value}`")
    }
}

/// B033
pub(crate) fn duplicate_value(checker: &mut Checker, elts: &[Expr], range: TextRange) {
    let mut seen_values: FxHashSet<ComparableExpr> = FxHashSet::default();
    for (index, elt) in elts.iter().enumerate() {
        if elt.is_literal_expr() {
            let comparable_value: ComparableExpr = elt.into();

            if !seen_values.insert(comparable_value) {
                let mut diagnostic = Diagnostic::new(
                    DuplicateValue {
                        value: checker.generator().expr(elt),
                    },
                    elt.range(),
                );

                let mut elts_without_duplicate = elts.to_owned();
                elts_without_duplicate.remove(index);

                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    checker.generator().expr(&Expr::Set(ExprSet {
                        elts: elts_without_duplicate,
                        range,
                    })),
                    range,
                )));

                checker.diagnostics.push(diagnostic);
            }
        };
    }
}
