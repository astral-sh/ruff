use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{
    Arguments, Expr, ExprAttribute, ExprBinOp, ExprCall, ExprStringLiteral, ExprSubscript,
    ExprUnaryOp, Number, Operator, UnaryOp,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for `datetime.fromisoformat()` calls
/// where the only argument is an inline replacement
/// of `Z` with a zero offset timezone.
///
/// ## Why is this bad?
/// On Python 3.11 and later, `datetime.fromisoformat()`
/// can handle most [ISO 8601][iso-8601] formats,
/// including ones affixed with `Z`,
/// so such an operation is unnecessary.
///
/// ## Example
///
/// ```python
/// from datetime import datetime
///
///
/// date = "2025-01-01T00:00:00Z"
///
/// datetime.fromisoformat(date.replace("Z", "+00:00"))
/// datetime.fromisoformat(date[:-1] + "-00")
/// datetime.fromisoformat(date.strip("Z", "-0000"))
/// datetime.fromisoformat(date.rstrip("Z", "-00:00"))
/// ```
///
/// Use instead:
///
/// ```python
/// from datetime import datetime
///
///
/// date = "2025-01-01T00:00:00Z"
///
/// datetime.fromisoformat(date)
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe if it might remove comments.
///
/// ## References
/// * [What’s New In Python 3.11 &sect; `datetime`](https://docs.python.org/3/whatsnew/3.11.html#datetime)
///
/// [iso-8601]: https://www.iso.org/obp/ui/#iso:std:iso:8601
#[derive(ViolationMetadata)]
pub(crate) struct FromisoformatReplaceZ;

impl AlwaysFixableViolation for FromisoformatReplaceZ {
    #[derive_message_formats]
    fn message(&self) -> String {
        r#"Unnecessary timezone monkeypatching"#.to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `.replace()` call".to_string()
    }
}

/// FURB162
pub(crate) fn fromisoformat_replace_z(checker: &Checker, call: &ExprCall) {
    if checker.settings.target_version < PythonVersion::Py311 {
        return;
    }

    let (func, arguments) = (&*call.func, &call.arguments);

    if !arguments.keywords.is_empty() {
        return;
    }

    let [argument] = &*arguments.args else {
        return;
    };

    if !func_is_fromisoformat(func, checker.semantic()) {
        return;
    }

    let Some(range_to_remove) = range_to_remove(argument, checker) else {
        return;
    };

    let applicability = if checker.comment_ranges().intersects(range_to_remove) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let edit = Edit::range_deletion(range_to_remove);
    let fix = Fix::applicable_edit(edit, applicability);

    let diagnostic = Diagnostic::new(FromisoformatReplaceZ, argument.range());

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

fn func_is_fromisoformat(func: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    if !matches!(
        qualified_name.segments(),
        ["datetime", "datetime", "fromisoformat"]
    ) {
        return false;
    }

    true
}

fn range_to_remove(expr: &Expr, checker: &Checker) -> Option<TextRange> {
    let (date, parent, zero_offset) = match expr {
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => replace_z_date_parent_and_offset(func, arguments)?,

        Expr::BinOp(ExprBinOp {
            left, op, right, ..
        }) => {
            if *op != Operator::Add {
                return None;
            }

            let (date, parent) = match &**left {
                Expr::Call(call) => strip_z_date_and_parent(call)?,
                Expr::Subscript(subscript) => (slice_minus_1_date(subscript)?, &**left),
                _ => return None,
            };

            (date, parent, right.as_string_literal_expr()?)
        }

        _ => return None,
    };

    if !is_zero_offset_timezone(zero_offset.value.to_str()) {
        return None;
    }

    let comment_ranges = checker.comment_ranges();
    let source = checker.source();
    let value_full_range = parenthesized_range(date.into(), parent.into(), comment_ranges, source)
        .unwrap_or(date.range());

    Some(TextRange::new(value_full_range.end(), expr.end()))
}

fn replace_z_date_parent_and_offset<'a>(
    func: &'a Expr,
    arguments: &'a Arguments,
) -> Option<(&'a Expr, &'a Expr, &'a ExprStringLiteral)> {
    if !arguments.keywords.is_empty() {
        return None;
    };

    let ExprAttribute { value, attr, .. } = func.as_attribute_expr()?;

    if attr != "replace" {
        return None;
    }

    let [z, Expr::StringLiteral(zero_offset)] = &*arguments.args else {
        return None;
    };

    if !is_upper_case_z_string(z) {
        return None;
    }

    Some((&**value, func, zero_offset))
}

fn strip_z_date_and_parent(call: &ExprCall) -> Option<(&Expr, &Expr)> {
    let ExprCall {
        func, arguments, ..
    } = call;

    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &**func else {
        return None;
    };

    if !matches!(attr.as_str(), "strip" | "rstrip") {
        return None;
    }

    if !arguments.keywords.is_empty() {
        return None;
    }

    let [z] = &*arguments.args else {
        return None;
    };

    if !is_upper_case_z_string(z) {
        return None;
    }

    Some((value, func))
}

fn slice_minus_1_date(subscript: &ExprSubscript) -> Option<&Expr> {
    let ExprSubscript { value, slice, .. } = subscript;
    let slice = slice.as_slice_expr()?;

    if slice.lower.is_some() || slice.step.is_some() {
        return None;
    }

    let ExprUnaryOp { operand, op, .. } = slice.upper.as_ref()?.as_unary_op_expr()?;

    let Number::Int(int) = &operand.as_number_literal_expr()?.value else {
        return None;
    };

    if *op != UnaryOp::USub || !matches!(int.as_u8(), Some(1)) {
        return None;
    }

    Some(value)
}

fn is_upper_case_z_string(expr: &Expr) -> bool {
    let Expr::StringLiteral(string) = expr else {
        return false;
    };

    string.value.to_str() == "Z"
}

fn is_zero_offset_timezone(value: &str) -> bool {
    matches!(
        value,
        "+00:00" | "+0000" | "+00" | "-00:00" | "-0000" | "-00"
    )
}
