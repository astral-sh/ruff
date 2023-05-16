use std::hash::BuildHasherDefault;

use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Identifier, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct DuplicateBases {
    base: String,
    class: String,
}

impl Violation for DuplicateBases {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateBases { base, class } = self;
        format!("Duplicate base `{base}` for class `{class}`")
    }
}

/// PLE0241
pub(crate) fn duplicate_bases(checker: &mut Checker, name: &str, bases: &[Expr]) {
    let mut seen: FxHashSet<&Identifier> =
        FxHashSet::with_capacity_and_hasher(bases.len(), BuildHasherDefault::default());
    for base in bases {
        if let Expr::Name(ast::ExprName { id, .. }) = base {
            if !seen.insert(id) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateBases {
                        base: id.to_string(),
                        class: name.to_string(),
                    },
                    base.range(),
                ));
            }
        }
    }
}
