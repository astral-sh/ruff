use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    DictItem, Expr, ExprAttribute, ExprCall, ExprDict, ExprNumberLiteral, ExprStringLiteral,
    ExprSubscript, ExprUnaryOp, Keyword, Number, UnaryOp,
};
use ruff_python_semantic::{SemanticModel, analyze::typing};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for access to the first or last element of `str.split()` without
/// `maxsplit=1`
///
/// ## Why is this bad?
/// Calling `str.split()` without `maxsplit` set splits on every delimiter in the
/// string. When accessing only the first or last element of the result, it
/// would be more efficient to only split once.
///
/// ## Example
/// ```python
/// url = "www.example.com"
/// prefix = url.split(".")[0]
/// ```
///
/// Use instead:
/// ```python
/// url = "www.example.com"
/// prefix = url.split(".", maxsplit=1)[0]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MissingMaxsplitArg;

impl Violation for MissingMaxsplitArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Accessing only the first or last element of `str.split()` without setting `maxsplit=1`"
            .to_string()
    }
}

fn is_string(expr: &Expr, semantic: &SemanticModel) -> bool {
    if let Expr::Name(name) = expr {
        semantic
            .only_binding(name)
            .is_some_and(|binding_id| typing::is_string(semantic.binding(binding_id), semantic))
    } else if let Some(binding_id) = semantic.lookup_attribute(expr) {
        typing::is_string(semantic.binding(binding_id), semantic)
    } else {
        expr.is_string_literal_expr()
    }
}

/// PLC0207
pub(crate) fn missing_maxsplit_arg(checker: &Checker, value: &Expr, slice: &Expr, expr: &Expr) {
    // Check the sliced expression is a function
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value
    else {
        return;
    };

    // Check the slice index is either 0 or -1 (first or last value)
    let index = match slice {
        Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(number_value),
            ..
        }) => number_value.as_i64(),
        Expr::UnaryOp(ExprUnaryOp {
            op: UnaryOp::USub,
            operand,
            ..
        }) => match operand.as_ref() {
            Expr::NumberLiteral(ExprNumberLiteral {
                value: Number::Int(number_value),
                ..
            }) => number_value.as_i64().map(|number| -number),
            _ => return,
        },
        _ => return,
    };

    if !matches!(index, Some(0 | -1)) {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    // Check the function is "split" or "rsplit"
    let attr = attr.as_str();
    if !matches!(attr, "split" | "rsplit") {
        return;
    }

    let mut target_instance = value;
    // a subscripted value could technically be subscripted further ad infinitum, so we
    // recurse into the subscript expressions until we find the value being subscripted
    while let Expr::Subscript(ExprSubscript { value, .. }) = target_instance.as_ref() {
        target_instance = value;
    }

    // Check the function is called on a string
    if !is_string(target_instance, checker.semantic()) {
        return;
    }

    // Check the function does not have maxsplit set
    if arguments.find_argument_value("maxsplit", 1).is_some() {
        return;
    }

    // Check maxsplit kwarg not set via unpacked dict literal
    for keyword in &*arguments.keywords {
        let Keyword { value, .. } = keyword;

        if let Expr::Dict(ExprDict { items, .. }) = value {
            for item in items {
                let DictItem { key, .. } = item;
                if let Some(Expr::StringLiteral(ExprStringLiteral { value, .. })) = key {
                    if value.to_str() == "maxsplit" {
                        return;
                    }
                }
            }
        }
    }

    checker.report_diagnostic(MissingMaxsplitArg, expr.range());
}
