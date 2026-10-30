use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{Number, UnaryOp};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for unnecessary subscript reversal of iterable.
///
/// ## Why is this bad?
/// It's unnecessary to reverse the order of an iterable when passing it
/// into `reversed()`, `set()` or `sorted()` functions as they will change
/// the order of the elements again.
///
/// ## Example
/// ```python
/// sorted(iterable[::-1])
/// set(iterable[::-1])
/// reversed(iterable[::-1])
/// ```
///
/// Use instead:
/// ```python
/// sorted(iterable)
/// set(iterable)
/// iterable
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.64", category = Category::Complexity)]
pub(crate) struct UnnecessarySubscriptReversal {
    func: String,
}

impl Violation for UnnecessarySubscriptReversal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessarySubscriptReversal { func } = self;
        format!("Unnecessary subscript reversal of iterable within `{func}()`")
    }
}

/// C415
pub(crate) fn unnecessary_subscript_reversal(checker: &Checker, call: &ast::ExprCall<'_>) {
    let Some(first_arg) = call.arguments.args().first() else {
        return;
    };
    let flat::ExprKind::Subscript(subscript_node) = first_arg.classify() else {
        return;
    };
    let slice = subscript_node.slice();
    let flat::ExprKind::Slice(slice_node) = slice.classify() else {
        return;
    };
    let lower = slice_node.lower();
    let upper = slice_node.upper();
    let step = slice_node.step();
    if lower.is_some() || upper.is_some() {
        return;
    }
    let Some(step) = step else {
        return;
    };
    let ast::Expr::UnaryOp(ast::ExprUnaryOp {
        op: UnaryOp::USub,
        operand,
        range: _,
        node: _,
    }) = step.fields()
    else {
        return;
    };
    let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(val),
        ..
    }) = operand.fields()
    else {
        return;
    };
    if *val != 1 {
        return;
    }
    let Some(function_name) = checker.semantic().resolve_builtin_symbol(call.func) else {
        return;
    };
    if !matches!(function_name, "reversed" | "set" | "sorted") {
        return;
    }
    checker.report_diagnostic(
        UnnecessarySubscriptReversal {
            func: function_name.to_string(),
        },
        call.range,
    );
}
