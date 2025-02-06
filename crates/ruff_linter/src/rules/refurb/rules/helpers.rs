use std::borrow::Cow;

use ruff_python_ast::{self as ast, parenthesize::parenthesized_range};

use crate::checkers::ast::Checker;

/// A helper function that extracts the `iter` from a [`ast::StmtFor`] node and,
/// if the `iter` is an unparenthesized tuple, adds parentheses:
///
/// - `for x in z: ...`       ->  `"x"`
/// - `for (x, y) in z: ...`  ->  `"(x, y)"`
/// - `for [x, y] in z: ...`  ->  `"[x, y]"`
/// - `for x, y in z: ...`    ->  `"(x, y)"`      # <-- Parentheses added only for this example
pub(super) fn parenthesize_loop_iter_if_necessary<'a>(
    for_stmt: &'a ast::StmtFor,
    checker: &'a Checker,
) -> Cow<'a, str> {
    let locator = checker.locator();
    let iter = for_stmt.iter.as_ref();

    let original_parenthesized_range = parenthesized_range(
        iter.into(),
        for_stmt.into(),
        checker.comment_ranges(),
        checker.source(),
    );

    if let Some(range) = original_parenthesized_range {
        return Cow::Borrowed(locator.slice(range));
    }

    let iter_in_source = locator.slice(iter);

    match iter {
        ast::Expr::Tuple(tuple) if !tuple.parenthesized => {
            Cow::Owned(format!("({iter_in_source})"))
        }
        _ => Cow::Borrowed(iter_in_source),
    }
}
