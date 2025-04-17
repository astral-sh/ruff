use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for membership tests on empty collections (such as `list`, `tuple`, `set` or `dict`).
///
/// ## Why is this bad?
/// If the collection is always empty, the check is unnecessary, and can be removed.

#[derive(ViolationMetadata)]
pub(crate) struct InEmptyCollection;

impl Violation for InEmptyCollection {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary membership test on empty collection".to_string()
    }
}

/// PLR6202
pub(crate) fn in_empty_collection(checker: &Checker, compare: &ast::ExprCompare) {
    let [op] = &*compare.ops else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = &*compare.comparators else {
        return;
    };

    let is_relevant_and_empty = match right {
        Expr::List(ast::ExprList { elts, .. }) => elts.is_empty(),
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.is_empty(),
        Expr::Set(ast::ExprSet { elts, .. }) => elts.is_empty(),
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) => {
            if let Expr::Name(ast::ExprName {
                id,
                ctx: _,
                range: _,
            }) = func.as_ref()
            {
                id == "set" && arguments.is_empty()
            } else {
                false
            }
        }
        Expr::Dict(ast::ExprDict { range: _, items }) => items.is_empty(),
        _ => false,
    };

    if is_relevant_and_empty {
        checker.report_diagnostic(Diagnostic::new(InEmptyCollection, compare.range()));
    }
}
