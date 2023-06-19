use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for star-arg unpacking after a keyword argument.
///
/// ## Why is this bad?
/// Star-arg unpacking works only when the keyword parameter is declared after
/// all parameters supplied by the star-arg unpacking. This behavior is
/// confusing and unlikely to be intended, and is legal only for backwards
/// compatibility.
///
/// ## Example
/// ```python
/// def foo(x, y, z):
///     return x, y, z
///
///
/// foo(1, 2, 3)  # (1, 2, 3)
/// foo(1, *[2, 3])  # (1, 2, 3)
/// # foo(x=1, *[2, 3])  # TypeError
/// # foo(y=2, *[1, 3])  # TypeError
/// foo(z=3, *[1, 2])  # (1, 2, 3) # This is OK, but confusing!
/// ```
///
/// Use instead:
/// ```python
/// def foo(x, y, z):
///     return x, y, z
///
///
/// foo(1, 2, 3)  # (1, 2, 3)
/// foo(x=1, y=2, z=3)  # (1, 2, 3)
/// foo(*[1, 2, 3])  # (1, 2, 3)
/// foo(*[1, 2], 3)  # (1, 2, 3)
/// ```
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [Disallow iterable argument unpacking after a keyword argument?](https://github.com/python/cpython/issues/82741)
#[violation]
pub struct StarArgUnpackingAfterKeywordArg;

impl Violation for StarArgUnpackingAfterKeywordArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Star-arg unpacking after a keyword argument is strongly discouraged")
    }
}

/// B026
pub(crate) fn star_arg_unpacking_after_keyword_arg(
    checker: &mut Checker,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(keyword) = keywords.first() else {
        return;
    };
    for arg in args {
        let Expr::Starred (_) = arg else {
            continue;
        };
        if arg.start() <= keyword.start() {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            StarArgUnpackingAfterKeywordArg,
            arg.range(),
        ));
    }
}
