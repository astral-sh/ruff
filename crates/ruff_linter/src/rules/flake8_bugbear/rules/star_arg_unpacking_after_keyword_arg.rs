use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, Arguments};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.109", category = Category::Suspicious)]
pub(crate) struct StarArgUnpackingAfterKeywordArg;

impl Violation for StarArgUnpackingAfterKeywordArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Star-arg unpacking after a keyword argument is strongly discouraged".to_string()
    }
}

/// B026
pub(crate) fn star_arg_unpacking_after_keyword_arg(checker: &Checker, arguments: Arguments<'_>) {
    let mut items = arguments.iter_source_order();
    let Some(_) = items.find(|item| matches!(item, ArgOrKeyword::Keyword(_))) else {
        return;
    };
    for item in items {
        if let ArgOrKeyword::Arg(arg) = item
            && arg.is_starred_expr()
        {
            checker.report_diagnostic(StarArgUnpackingAfterKeywordArg, arg.range());
        }
    }
}
