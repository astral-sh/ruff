use ruff_python_ast as ast;

/// Checks if `expr` is a string literal that represents NaN.
/// E.g., `"NaN"`, `"-nAn"`, `"+nan"`, or even `" -NaN \n \t"`
/// Returns `None` if it's not. Else `Some("nan")`, `Some("-nan")`, or `Some("+nan")`.
pub(crate) fn as_nan_float_string_literal(expr: &ast::Expr) -> Option<&'static str> {
    find_any_ignore_ascii_case(expr, &["nan", "+nan", "-nan"])
}

/// Returns `true` if `expr` is a string literal that represents a non-finite float.
/// E.g., `"NaN"`, "-inf", `"Infinity"`, or even `" +Inf \n \t"`.
/// Return `None` if it's not. Else the lowercased, trimmed string literal,
/// e.g., `Some("nan")`, `Some("-inf")`, or `Some("+infinity")`.
pub(crate) fn as_non_finite_float_string_literal(expr: &ast::Expr) -> Option<&'static str> {
    find_any_ignore_ascii_case(
        expr,
        &[
            "nan",
            "+nan",
            "-nan",
            "inf",
            "+inf",
            "-inf",
            "infinity",
            "+infinity",
            "-infinity",
        ],
    )
}

fn find_any_ignore_ascii_case(expr: &ast::Expr, patterns: &[&'static str]) -> Option<&'static str> {
    let value = &expr.as_string_literal_expr()?.value;

    let value = value.to_str().trim();
    patterns
        .iter()
        .find(|other| value.eq_ignore_ascii_case(other))
        .copied()
}
