use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::{self as ast, CmpOp, Expr, helpers::is_empty_f_string};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for membership tests on empty collections (such as `list`, `tuple`, `set`, or `dict`).
///
/// ## Why is this bad?
/// If the collection is always empty, the check is unnecessary and can be removed.
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
#[violation_metadata(stable_since = "0.15.0", category = Category::Correctness)]
pub(crate) struct InEmptyCollection;

impl Violation for InEmptyCollection {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary membership test on empty collection".to_string()
    }
}

/// RUF060
pub(crate) fn in_empty_collection(checker: &Checker, compare: flat::View<'_, ast::ExprCompare>) {
    let Some((_, CmpOp::In | CmpOp::NotIn, right)) = compare.as_single() else {
        return;
    };

    let semantic = checker.semantic();

    if is_empty(right, semantic) {
        checker.report_diagnostic(InEmptyCollection, compare.range());
    }
}

fn is_empty(expr: flat::View<'_, Expr>, semantic: &SemanticModel) -> bool {
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

    match expr.classify() {
        flat::ExprKind::List(expr_list) => {
            let elts = expr_list.elts();
            elts.is_empty()
        }
        flat::ExprKind::Tuple(expr_tuple) => {
            let elts = expr_tuple.elts();
            elts.is_empty()
        }
        flat::ExprKind::Set(expr_set) => {
            let elts = expr_set.elts();
            elts.is_empty()
        }
        flat::ExprKind::Dict(expr_dict) => {
            let items = expr_dict.items();
            items.is_empty()
        }
        flat::ExprKind::BytesLiteral(expr_bytes_literal) => {
            let value = expr_bytes_literal.value();

            value.is_empty()
        }
        flat::ExprKind::StringLiteral(expr_string_literal) => {
            let value = expr_string_literal.value();

            value.is_empty()
        }
        flat::ExprKind::FString(s) => is_empty_f_string(s),
        flat::ExprKind::Call(expr_call) => {
            let func = expr_call.func();
            let arguments = expr_call.arguments();

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
