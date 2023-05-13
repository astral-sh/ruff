use std::collections::HashSet;

use rustpython_parser::ast::{self, Expr, ExprKind, Identifier};

use crate::checkers::ast::Checker;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct DuplicateBases {
    name: String,
}

impl Violation for DuplicateBases {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateBases { name } = self;
        format!("Duplicate bases for class `{name}`")
    }
}

/// PLE0241
pub(crate) fn duplicate_bases(checker: &mut Checker, name: &str, bases: &[Expr]) {
    let mut unique_bases: HashSet<&Identifier> = HashSet::new();

    for base in bases {
        if let ExprKind::Name(ast::ExprName { id, .. }) = &base.node {
            if unique_bases.contains(id) {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateBases {
                        name: name.to_string(),
                    },
                    base.range(),
                ))
            }
            unique_bases.insert(id);
        }
    }
}
