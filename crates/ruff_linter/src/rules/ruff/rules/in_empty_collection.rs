use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr, helpers::is_empty_f_string};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
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
#[violation_metadata(preview_since = "0.11.9")]
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
        checker.report_diagnostic(InEmptyCollection, compare.range());
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
        Expr::FString(s) => is_empty_f_string(s),
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
            node_index: _,
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
