use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

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
/// Use the `strict` parameter to raise a `ValueError` if the iterables are of
/// non-uniform length.
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
/// `**kwargs`, as adding a `check` keyword argument to such a call may lead
/// to a duplicate keyword argument error.
///
/// ## References
/// - [Python documentation: `zip`](https://docs.python.org/3/library/functions.html#zip)
#[violation]
pub struct ZipWithoutExplicitStrict;

impl AlwaysFixableViolation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
    }

    fn fix_title(&self) -> String {
        "Add explicit `strict=False`".to_string()
    }
}

/// B905
pub(crate) fn zip_without_explicit_strict(checker: &mut Checker, call: &ast::ExprCall) {
    if let Expr::Name(ast::ExprName { id, .. }) = call.func.as_ref() {
        if id == "zip"
            && checker.semantic().is_builtin("zip")
            && call.arguments.find_keyword("strict").is_none()
            && !call
                .arguments
                .args
                .iter()
                .any(|arg| is_infinite_iterator(arg, checker.semantic()))
        {
            let mut diagnostic = Diagnostic::new(ZipWithoutExplicitStrict, call.range());
            diagnostic.set_fix(Fix::applicable_edit(
                add_argument(
                    "strict=False",
                    &call.arguments,
                    checker.indexer().comment_ranges(),
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
            ));
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Return `true` if the [`Expr`] appears to be an infinite iterator (e.g., a call to
/// `itertools.cycle` or similar).
fn is_infinite_iterator(arg: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        ..
    }) = &arg
    else {
        return false;
    };

    semantic.resolve_call_path(func).is_some_and(|call_path| {
        match call_path.as_slice() {
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
