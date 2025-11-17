use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;
use unicode_normalization::UnicodeNormalization;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `getattr` that take a constant attribute value as an
/// argument (e.g., `getattr(obj, "foo")`).
///
/// ## Why is this bad?
/// `getattr` is used to access attributes dynamically. If the attribute is
/// defined as a constant, it is no safer than a typical property access. When
/// possible, prefer property access over `getattr` calls, as the former is
/// more concise and idiomatic.
///
///
/// ## Example
/// ```python
/// getattr(obj, "foo")
/// ```
///
/// Use instead:
/// ```python
/// obj.foo
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe for attribute names that are not in NFKC (Normalization Form KC)
/// normalization. Python normalizes identifiers using NFKC when using attribute access syntax
/// (e.g., `obj.attr`), but does not normalize string arguments passed to `getattr`. Rewriting
/// `getattr(obj, "ſ")` to `obj.ſ` would be interpreted as `obj.s` at runtime, changing behavior.
///
/// For example, the long s character `"ſ"` normalizes to `"s"` under NFKC, so:
/// ```python
/// # This accesses an attribute with the exact name "ſ" (if it exists)
/// value = getattr(obj, "ſ")
///
/// # But this would normalize to "s" and access a different attribute
/// obj.ſ  # This is interpreted as obj.s, not obj.ſ
/// ```
///
/// ## References
/// - [Python documentation: `getattr`](https://docs.python.org/3/library/functions.html#getattr)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.110")]
pub(crate) struct GetAttrWithConstant;

impl AlwaysFixableViolation for GetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not call `getattr` with a constant attribute value. It is not any safer than \
            normal property access."
            .to_string()
    }

    fn fix_title(&self) -> String {
        "Replace `getattr` with attribute access".to_string()
    }
}

/// B009
pub(crate) fn getattr_with_constant(checker: &Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let [obj, arg] = args else {
        return;
    };
    if obj.is_starred_expr() {
        return;
    }
    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = arg else {
        return;
    };
    if !is_identifier(value.to_str()) {
        return;
    }
    if is_mangled_private(value.to_str()) {
        return;
    }
    if !checker.semantic().match_builtin_expr(func, "getattr") {
        return;
    }

    // Mark fixes as unsafe for non-NFKC attribute names. Python normalizes identifiers using NFKC, so using
    // attribute syntax (e.g., `obj.attr`) would normalize the name and potentially change
    // program behavior.
    let attr_name = value.to_str();
    let is_unsafe = attr_name.nfkc().collect::<String>() != attr_name;

    let mut diagnostic = checker.report_diagnostic(GetAttrWithConstant, expr.range());
    let edit = Edit::range_replacement(
        pad(
            if matches!(
                obj,
                Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_)
            ) && !checker.locator().contains_line_break(obj.range())
            {
                format!("{}.{}", checker.locator().slice(obj), value)
            } else {
                // Defensively parenthesize any other expressions. For example, attribute accesses
                // on `int` literals must be parenthesized, e.g., `getattr(1, "real")` becomes
                // `(1).real`. The same is true for named expressions and others.
                format!("({}).{}", checker.locator().slice(obj), value)
            },
            expr.range(),
            checker.locator(),
        ),
        expr.range(),
    );
    let fix = if is_unsafe {
        Fix::unsafe_edit(edit)
    } else {
        Fix::safe_edit(edit)
    };
    diagnostic.set_fix(fix);
}
