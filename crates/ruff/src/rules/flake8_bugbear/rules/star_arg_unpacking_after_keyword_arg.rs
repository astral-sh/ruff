//! Checks for `f(x=0, *(1, 2))`.
//!
//! ## Why is this bad?
//!
//! Star-arg unpacking after a keyword argument is strongly discouraged. It only
//! works when the keyword parameter is declared after all parameters supplied
//! by the unpacked sequence, and this change of ordering can surprise and
//! mislead readers.

use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct StarArgUnpackingAfterKeywordArg;
);
impl Violation for StarArgUnpackingAfterKeywordArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Star-arg unpacking after a keyword argument is strongly discouraged")
    }
}

/// B026
pub fn star_arg_unpacking_after_keyword_arg(
    checker: &mut Checker,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(keyword) = keywords.first() else {
        return;
    };
    for arg in args {
        let ExprKind::Starred { .. } = arg.node else {
            continue;
        };
        if arg.location <= keyword.location {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            StarArgUnpackingAfterKeywordArg,
            Range::from_located(arg),
        ));
    }
}
