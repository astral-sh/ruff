use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_text_size::Ranged;

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

impl Violation for DuplicateValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValue { value } = self;
        format!("Sets should not contain duplicate item `{value}`")
    }
}

/// B033
pub(crate) fn duplicate_value(checker: &mut Checker, elts: &Vec<Expr>) {
    let mut seen_values: FxHashSet<ComparableExpr> = FxHashSet::default();
    for elt in elts {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = elt {
            let comparable_value: ComparableExpr = elt.into();

            if !seen_values.insert(comparable_value) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateValue {
                        value: checker.generator().constant(value),
                    },
                    elt.range(),
                ));
            }
        };
    }
}
