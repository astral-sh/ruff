use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    DictItem, Expr, ExprAttribute, ExprCall, ExprDict, ExprNumberLiteral, ExprStringLiteral,
    ExprSubscript, ExprUnaryOp, Keyword, Number, UnaryOp,
};
use ruff_python_semantic::{SemanticModel, analyze::typing};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

/// ## What it does
/// Checks for access to the first or last element of `str.split()` or `str.rsplit()` without
/// `maxsplit=1`
///
/// ## Why is this bad?
/// Calling `str.split()` or `str.rsplit()` without passing `maxsplit=1` splits on every delimiter in the
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
///
/// To access the last element, use `str.rsplit()` instead of `str.split()`:
/// ```python
/// url = "www.example.com"
/// suffix = url.rsplit(".", maxsplit=1)[-1]
/// ```
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe for `split()`/`rsplit()` calls that contain `*args` or `**kwargs` arguments, as
/// adding a `maxsplit` argument to such a call may lead to duplicated arguments.
#[derive(ViolationMetadata)]
pub(crate) struct MissingMaxsplitArg {
    actual_split_type: String,
    suggested_split_type: String,
}

/// Represents the index of the slice used for this rule (which can only be 0 or -1)
enum SliceBoundary {
    First,
    Last,
}

impl AlwaysFixableViolation for MissingMaxsplitArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingMaxsplitArg {
            actual_split_type: _,
            suggested_split_type,
        } = self;

        format!("Replace with `{suggested_split_type}(..., maxsplit=1)`.")
    }

    fn fix_title(&self) -> String {
        let MissingMaxsplitArg {
            actual_split_type,
            suggested_split_type,
        } = self;

        if actual_split_type == suggested_split_type {
            format!("Pass `maxsplit=1` into `str.{actual_split_type}()`")
        } else {
            format!("Use `str.{suggested_split_type}()` and pass `maxsplit=1`")
        }
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

    let slice_boundary = match index {
        Some(0) => SliceBoundary::First,
        Some(-1) => SliceBoundary::Last,
        _ => return,
    };

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    // Check the function is "split" or "rsplit"
    let actual_split_type = attr.as_str();
    if !matches!(actual_split_type, "split" | "rsplit") {
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

    let suggested_split_type = match slice_boundary {
        SliceBoundary::First => "split",
        SliceBoundary::Last => "rsplit",
    };

    let maxsplit_argument_edit = fix::edits::add_argument(
        "maxsplit=1",
        arguments,
        checker.comment_ranges(),
        checker.locator().contents(),
    );

    // Only change `actual_split_type` if it doesn't match `suggested_split_type`
    let split_type_edit: Option<Edit> = if actual_split_type == suggested_split_type {
        None
    } else {
        Some(Edit::range_replacement(
            suggested_split_type.to_string(),
            attr.range(),
        ))
    };

    let mut diagnostic = checker.report_diagnostic(
        MissingMaxsplitArg {
            actual_split_type: actual_split_type.to_string(),
            suggested_split_type: suggested_split_type.to_string(),
        },
        expr.range(),
    );

    diagnostic.set_fix(Fix::applicable_edits(
        maxsplit_argument_edit,
        split_type_edit,
        // Mark the fix as unsafe, if there are `*args` or `**kwargs`
        if arguments.args.iter().any(Expr::is_starred_expr)
            || arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        },
    ));
}
