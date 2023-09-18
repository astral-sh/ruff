use ruff_python_ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function calls that use star-argument unpacking after providing a
/// keyword argument
///
/// ## Why is this bad?
/// In Python, you can use star-argument unpacking to pass a list or tuple of
/// arguments to a function.
///
/// Providing a star-argument after a keyword argument can lead to confusing
/// behavior, and is only supported for backwards compatibility.
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
/// foo(z=3, *[1, 2])  # (1, 2, 3)  # No error, but confusing!
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
        let Expr::Starred(_) = arg else {
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
