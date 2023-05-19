use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::unparse_constant;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Ranged};
use std::hash::BuildHasherDefault;

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
    let mut seen_values: FxHashSet<ComparableExpr> =
        FxHashSet::with_capacity_and_hasher(elts.len(), BuildHasherDefault::default());

    for elt in elts {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = elt {
            // dbg!(value);
            let comparable_value: ComparableExpr = elt.into();
            // dbg!(&comparable_value);

            if seen_values.contains(&comparable_value) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateValues {
                        value: unparse_constant(value, checker.stylist),
                    },
                    elt.range(),
                ));
            }
            seen_values.insert(comparable_value);
        };
    }
}
