use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{parenthesize::parenthesized_range, Expr, ExprCall};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for `itertools.starmap` calls where the second argument is a `zip` call.
///
/// ## Why is this bad?
/// `zip`-ping iterables only to unpack them later from within `starmap` is unnecessary.
/// For such cases, `map()` should be used instead.
///
/// ## Example
///
/// ```python
/// from itertools import starmap
///
///
/// starmap(func, zip(a, b))
/// starmap(func, zip(a, b, strict=True))
/// ```
///
/// Use instead:
///
/// ```python
/// map(func, a, b)
/// map(func, a, b, strict=True)  # 3.14+
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct StarmapZip;

impl AlwaysFixableViolation for StarmapZip {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`itertools.starmap` called on `zip` iterable".to_string()
    }

    fn fix_title(&self) -> String {
        "Use `map` instead".to_string()
    }
}

/// RUF058
pub(crate) fn starmap_zip(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !call.arguments.keywords.is_empty() {
        return;
    }

    let [_map_func, Expr::Call(iterable_call)] = &*call.arguments.args else {
        return;
    };

    if !iterable_call.arguments.keywords.is_empty() {
        // TODO: Pass `strict=` to `map` too when 3.14 is supported.
        return;
    }

    if !semantic
        .resolve_qualified_name(&call.func)
        .is_some_and(|it| matches!(it.segments(), ["itertools", "starmap"]))
    {
        return;
    }

    if !semantic.match_builtin_expr(&iterable_call.func, "zip") {
        return;
    }

    let fix = replace_with_map(call, iterable_call, checker);
    let diagnostic = Diagnostic::new(StarmapZip, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn replace_with_map(starmap: &ExprCall, zip: &ExprCall, checker: &Checker) -> Fix {
    let change_func_to_map = Edit::range_replacement("map".to_string(), starmap.func.range());

    let mut remove_zip = vec![];

    let full_zip_range = parenthesized_range(
        zip.into(),
        starmap.into(),
        checker.comment_ranges(),
        checker.source(),
    )
    .unwrap_or(zip.range());

    // Delete any parentheses around the `zip` call to prevent that the arguemnt turns into a tuple.
    remove_zip.push(Edit::range_deletion(TextRange::new(
        full_zip_range.start(),
        zip.start(),
    )));

    let full_zip_func_range = parenthesized_range(
        (&zip.func).into(),
        zip.into(),
        checker.comment_ranges(),
        checker.source(),
    )
    .unwrap_or(zip.func.range());

    // Delete the `zip` callee
    remove_zip.push(Edit::range_deletion(full_zip_func_range));

    // Delete the `(` from the `zip(...)` call
    remove_zip.push(Edit::range_deletion(zip.arguments.l_paren_range()));

    // `zip` can be called without arguments but `map` can't.
    if zip.arguments.is_empty() {
        remove_zip.push(Edit::insertion("[]".to_string(), zip.arguments.start()));
    }

    let after_zip = checker.tokens().after(full_zip_range.end());

    // Remove any trailing commas after the `zip` call to avoid multiple trailing commas
    // if the iterable has a trailing comma.
    if let Some(trailing_comma) = after_zip.iter().find(|token| !token.kind().is_trivia()) {
        if trailing_comma.kind() == TokenKind::Comma {
            remove_zip.push(Edit::range_deletion(trailing_comma.range()));
        }
    }

    // Delete the `)` from the `zip(...)` call
    remove_zip.push(Edit::range_deletion(zip.arguments.r_paren_range()));

    // Delete any trailing parentheses wrapping the `zip` call.
    remove_zip.push(Edit::range_deletion(TextRange::new(
        zip.end(),
        full_zip_range.end(),
    )));

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.intersects(starmap.func.range())
        || comment_ranges.intersects(full_zip_range)
    {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edits(change_func_to_map, remove_zip, applicability)
}
