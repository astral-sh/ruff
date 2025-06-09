use std::borrow::Cow;

use ruff_python_ast::{self as ast, parenthesize::parenthesized_range};

use crate::checkers::ast::Checker;

/// A helper function that extracts the `iter` from a [`ast::StmtFor`] node and
/// adds parentheses if needed.
///
/// These cases are okay and will not be modified:
///
/// - `for x in z: ...`       ->  `"z"`
/// - `for x in (y, z): ...`  ->  `"(y, z)"`
/// - `for x in [y, z]: ...`  ->  `"[y, z]"`
///
/// While these cases require parentheses:
///
/// - `for x in y, z: ...`                   ->  `"(y, z)"`
/// - `for x in lambda: 0: ...`              ->  `"(lambda: 0)"`
/// - `for x in (1,) if True else (2,): ...` ->  `"((1,) if True else (2,))"`
pub(super) fn parenthesize_loop_iter_if_necessary<'a>(
    for_stmt: &'a ast::StmtFor,
    checker: &'a Checker,
    location: IterLocation,
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
        ast::Expr::Lambda(_) | ast::Expr::If(_)
            if matches!(location, IterLocation::Comprehension) =>
        {
            Cow::Owned(format!("({iter_in_source})"))
        }
        _ => Cow::Borrowed(iter_in_source),
    }
}

#[derive(Copy, Clone)]
pub(super) enum IterLocation {
    Call,
    Comprehension,
}
