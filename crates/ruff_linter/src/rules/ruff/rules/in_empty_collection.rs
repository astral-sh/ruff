use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for membership tests on empty collections (such as `list`, `tuple`, `set` or `dict`).
///
/// ## Why is this bad?
/// If the collection is always empty, the check is unnecessary, and can be removed.
///
/// ## Example
///
/// ```python
/// if 1 not in set():
///     print("got it!")
/// ```
///
/// Use instead:
///
/// ```python
/// print("got it!")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InEmptyCollection;

impl Violation for InEmptyCollection {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary membership test on empty collection".to_string()
    }
}

/// RUF060
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

    let semantic = checker.semantic();

    if is_empty(right, semantic) {
        checker.report_diagnostic(Diagnostic::new(InEmptyCollection, compare.range()));
    }
}

fn is_empty(expr: &Expr, semantic: &SemanticModel) -> bool {
    let set_methods = ["set", "frozenset"];
    let collection_methods = [
        "list",
        "tuple",
        "set",
        "frozenset",
        "dict",
        "bytes",
        "bytearray",
        "str",
    ];

    match expr {
        Expr::List(ast::ExprList { elts, .. }) => elts.is_empty(),
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.is_empty(),
        Expr::Set(ast::ExprSet { elts, .. }) => elts.is_empty(),
        Expr::Dict(ast::ExprDict { items, .. }) => items.is_empty(),
        Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => value.is_empty(),
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
        Expr::FString(s) => s
            .value
            .elements()
            .all(|elt| elt.as_literal().is_some_and(|elt| elt.is_empty())),
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) => {
            if arguments.is_empty() {
                collection_methods
                    .iter()
                    .any(|s| semantic.match_builtin_expr(func, s))
            } else if let Some(arg) = arguments.find_positional(0) {
                set_methods
                    .iter()
                    .any(|s| semantic.match_builtin_expr(func, s))
                    && is_empty(arg, semantic)
            } else {
                false
            }
        }
        _ => false,
    }
}
