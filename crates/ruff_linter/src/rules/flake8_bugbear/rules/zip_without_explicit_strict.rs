use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;

/// ## What it does
/// Checks for `zip` calls without an explicit `strict` parameter.
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
/// This rule's fix is marked as unsafe for `zip` calls that contain
/// `**kwargs`, as adding a `strict` keyword argument to such a call may lead
/// to a duplicate keyword argument error.
///
/// ## References
/// - [Python documentation: `zip`](https://docs.python.org/3/library/functions.html#zip)
#[derive(ViolationMetadata)]
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
        && !call
            .arguments
            .args
            .iter()
            .any(|arg| is_infinite_iterable(arg, semantic))
    {
        checker.report_diagnostic(
            Diagnostic::new(ZipWithoutExplicitStrict, call.range()).with_fix(Fix::applicable_edit(
                add_argument(
                    "strict=False",
                    &call.arguments,
                    checker.comment_ranges(),
                    checker.locator().contents(),
                ),
                // If the function call contains `**kwargs`, mark the fix as unsafe.
                if call
                    .arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.is_none())
                {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                },
            )),
        );
    }
}

/// Return `true` if the [`Expr`] appears to be an infinite iterator (e.g., a call to
/// `itertools.cycle` or similar).
pub(crate) fn is_infinite_iterable(arg: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        ..
    }) = &arg
    else {
        return false;
    };

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            match qualified_name.segments() {
                ["itertools", "cycle" | "count"] => true,
                ["itertools", "repeat"] => {
                    // Ex) `itertools.repeat(1)`
                    if keywords.is_empty() && args.len() == 1 {
                        return true;
                    }

                    // Ex) `itertools.repeat(1, None)`
                    if args.len() == 2 && args[1].is_none_literal_expr() {
                        return true;
                    }

                    // Ex) `iterools.repeat(1, times=None)`
                    for keyword in keywords {
                        if keyword.arg.as_ref().is_some_and(|name| name == "times") {
                            if keyword.value.is_none_literal_expr() {
                                return true;
                            }
                        }
                    }

                    false
                }
                _ => false,
            }
        })
}
