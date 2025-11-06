use ruff_macros::{ViolationMetadata, derive_message_formats};

use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::rules::flake8_bugbear::helpers::any_infinite_iterables;
use crate::{AlwaysFixableViolation, Applicability, Fix};

/// ## What it does
/// Checks for `zip` calls without an explicit `strict` parameter when called with two or more iterables, or any starred argument.
///
/// ## Why is this bad?
/// By default, if the iterables passed to `zip` are of different lengths, the
/// resulting iterator will be silently truncated to the length of the shortest
/// iterable. This can lead to subtle bugs.
///
/// Pass `strict=True` to raise a `ValueError` if the iterables are of
/// non-uniform length. Alternatively, if the iterables are deliberately of
/// different lengths, pass `strict=False` to make the intention explicit.
///
/// ## Example
/// ```python
/// zip(a, b)
/// ```
///
/// Use instead:
/// ```python
/// zip(a, b, strict=True)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe. While adding `strict=False` preserves
/// the runtime behavior, it can obscure situations where the iterables are of
/// unequal length. Ruff prefers to alert users so they can choose the intended
/// behavior themselves.
///
/// ## References
/// - [Python documentation: `zip`](https://docs.python.org/3/library/functions.html#zip)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.167")]
pub(crate) struct ZipWithoutExplicitStrict;

impl AlwaysFixableViolation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`zip()` without an explicit `strict=` parameter".to_string()
    }

    fn fix_title(&self) -> String {
        "Add explicit value for parameter `strict=`".to_string()
    }
}

/// B905
pub(crate) fn zip_without_explicit_strict(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    if semantic.match_builtin_expr(&call.func, "zip")
        && call.arguments.find_keyword("strict").is_none()
        && (
            // at least 2 iterables
            call.arguments.args.len() >= 2
            // or a starred argument
            || call.arguments.args.iter().any(ast::Expr::is_starred_expr)
        )
        && !any_infinite_iterables(call.arguments.args.iter(), semantic)
    {
        checker
            .report_diagnostic(ZipWithoutExplicitStrict, call.range())
            .set_fix(Fix::applicable_edit(
                add_argument(
                    "strict=False",
                    &call.arguments,
                    checker.comment_ranges(),
                    checker.locator().contents(),
                ),
                Applicability::Unsafe,
            ));
    }
}
