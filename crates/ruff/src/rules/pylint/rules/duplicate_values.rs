use ruff_python_ast::comparable::ComparableExpr;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct DuplicateValues {
    value: String,
}

impl Violation for DuplicateValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValues { value } = self;
        format!("Duplicate value `{value}` in set.")
    }
}

/// PLW0130
/// "This message is emitted when a set contains the same value two or more times.",
pub(crate) fn duplicate_values(checker: &mut Checker, elts: &Vec<Expr>) {
    let mut seen_values: FxHashSet<ComparableExpr> = FxHashSet::default();

    for elt in elts {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = elt {
            let comparable_value: ComparableExpr = elt.into();

            if !seen_values.insert(comparable_value) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateValues {
                        value: checker.generator().constant(value),
                    },
                    elt.range(),
                ));
            }
        };
    }
}
