use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{
    Expr, ExprAttribute, ExprBinOp, ExprCall, ExprStringLiteral, ExprSubscript, ExprUnaryOp,
    Number, Operator, PythonVersion, UnaryOp,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `datetime.fromisoformat()` calls
/// where the only argument is an inline replacement
/// of `Z` with a zero offset timezone.
///
/// ## Why is this bad?
/// On Python 3.11 and later, `datetime.fromisoformat()` can handle most [ISO 8601][iso-8601]
/// formats including ones affixed with `Z`, so such an operation is unnecessary.
///
/// More information on unsupported formats
/// can be found in [the official documentation][fromisoformat].
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
/// The fix is always marked as unsafe,
/// as it might change the program's behaviour.
///
/// For example, working code might become non-working:
///
/// ```python
/// d = "Z2025-01-01T00:00:00Z"  # Note the leading `Z`
///
/// datetime.fromisoformat(d.strip("Z") + "+00:00")  # Fine
/// datetime.fromisoformat(d)  # Runtime error
/// ```
///
/// ## References
/// * [What’s New In Python 3.11 &sect; `datetime`](https://docs.python.org/3/whatsnew/3.11.html#datetime)
/// * [`fromisoformat`](https://docs.python.org/3/library/datetime.html#datetime.date.fromisoformat)
///
/// [iso-8601]: https://www.iso.org/obp/ui/#iso:std:iso:8601
/// [fromisoformat]: https://docs.python.org/3/library/datetime.html#datetime.date.fromisoformat
#[derive(ViolationMetadata)]
pub(crate) struct FromisoformatReplaceZ;

impl AlwaysFixableViolation for FromisoformatReplaceZ {
    #[derive_message_formats]
    fn message(&self) -> String {
        r#"Unnecessary timezone replacement with zero offset"#.to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `.replace()` call".to_string()
    }
}

/// FURB162
pub(crate) fn fromisoformat_replace_z(checker: &Checker, call: &ExprCall) {
    if checker.target_version() < PythonVersion::PY311 {
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

    let Some(replace_time_zone) = ReplaceTimeZone::from_expr(argument) else {
        return;
    };

    if !is_zero_offset_timezone(replace_time_zone.zero_offset.value.to_str()) {
        return;
    }

    let value_full_range = parenthesized_range(
        replace_time_zone.date.into(),
        replace_time_zone.parent.into(),
        checker.comment_ranges(),
        checker.source(),
    )
    .unwrap_or(replace_time_zone.date.range());

    let range_to_remove = TextRange::new(value_full_range.end(), argument.end());

    let diagnostic = Diagnostic::new(FromisoformatReplaceZ, argument.range());
    let fix = Fix::unsafe_edit(Edit::range_deletion(range_to_remove));

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

fn func_is_fromisoformat(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["datetime", "datetime", "fromisoformat"]
            )
        })
}

/// A `datetime.replace` call that replaces the timezone with a zero offset.
struct ReplaceTimeZone<'a> {
    /// The date expression
    date: &'a Expr,
    /// The `date` expression's parent.
    parent: &'a Expr,
    /// The zero offset string literal
    zero_offset: &'a ExprStringLiteral,
}

impl<'a> ReplaceTimeZone<'a> {
    fn from_expr(expr: &'a Expr) -> Option<Self> {
        match expr {
            Expr::Call(call) => Self::from_call(call),
            Expr::BinOp(bin_op) => Self::from_bin_op(bin_op),
            _ => None,
        }
    }

    /// Returns `Some` if the call expression is a call to `str.replace` and matches `date.replace("Z", "+00:00")`
    fn from_call(call: &'a ExprCall) -> Option<Self> {
        let arguments = &call.arguments;

        if !arguments.keywords.is_empty() {
            return None;
        }

        let ExprAttribute { value, attr, .. } = call.func.as_attribute_expr()?;

        if attr != "replace" {
            return None;
        }

        let [z, Expr::StringLiteral(zero_offset)] = &*arguments.args else {
            return None;
        };

        if !is_upper_case_z_string(z) {
            return None;
        }

        Some(Self {
            date: &**value,
            parent: &*call.func,
            zero_offset,
        })
    }

    /// Returns `Some` for binary expressions matching `date[:-1] + "-00"` or
    /// `date.strip("Z") + "+00"`
    fn from_bin_op(bin_op: &'a ExprBinOp) -> Option<Self> {
        let ExprBinOp {
            left, op, right, ..
        } = bin_op;

        if *op != Operator::Add {
            return None;
        }

        let (date, parent) = match &**left {
            Expr::Call(call) => strip_z_date(call)?,
            Expr::Subscript(subscript) => (slice_minus_1_date(subscript)?, &**left),
            _ => return None,
        };

        Some(Self {
            date,
            parent,
            zero_offset: right.as_string_literal_expr()?,
        })
    }
}

/// Returns `Some` if `call` is a call to `date.strip("Z")`.
///
/// It returns the value of the `date` argument and its parent.
fn strip_z_date(call: &ExprCall) -> Option<(&Expr, &Expr)> {
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

/// Returns `Some` if this is a subscript with the form `date[:-1] + "-00"`.
fn slice_minus_1_date(subscript: &ExprSubscript) -> Option<&Expr> {
    let ExprSubscript { value, slice, .. } = subscript;
    let slice = slice.as_slice_expr()?;

    if slice.lower.is_some() || slice.step.is_some() {
        return None;
    }

    let Some(ExprUnaryOp {
        operand,
        op: UnaryOp::USub,
        ..
    }) = slice.upper.as_ref()?.as_unary_op_expr()
    else {
        return None;
    };

    let Number::Int(int) = &operand.as_number_literal_expr()?.value else {
        return None;
    };

    if *int != 1 {
        return None;
    }

    Some(value)
}

fn is_upper_case_z_string(expr: &Expr) -> bool {
    expr.as_string_literal_expr()
        .is_some_and(|string| string.value.to_str() == "Z")
}

fn is_zero_offset_timezone(value: &str) -> bool {
    matches!(
        value,
        "+00:00" | "+0000" | "+00" | "-00:00" | "-0000" | "-00"
    )
}
