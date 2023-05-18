use std::{collections::HashSet, dbg, hash::BuildHasherDefault, println};

use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct DuplicateValues {
    value: String,
    set: String,
}

impl Violation for DuplicateValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValues { value, set } = self;
        format!("Duplicate value `{value}` in set `{set}`")
    }
}

/// PLW0130
/// "This message is emitted when a set contains the same value two or more times.",
pub(crate) fn duplicate_values(checker: &mut Checker, targets: &[Expr], values: &Expr) {
    if targets.len() != 1 {
        return;
    }
    let target = &targets[0];

    if let Expr::Set(ast::ExprSet { elts, .. }) = values {
        // let mut seen: FxHashSet<&Identifier> =
        //     FxHashSet::with_capacity_and_hasher(bases.len(), BuildHasherDefault::default());
        let mut seen: Vec<&Constant> = Vec::new();
        for elt in elts {
            if let Expr::Constant(ast::ExprConstant { value, .. }) = elt {
                if seen.contains(&value) {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        checker.diagnostics.push(Diagnostic::new(
                            DuplicateValues {
                                value: value.as_str().unwrap().to_string(),
                                set: id.to_string(),
                            },
                            target.range(),
                        ))
                    }
                }
                seen.push(value);
            };
        }
    }
}
