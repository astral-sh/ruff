use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
/// ## References
/// - [Python documentation: `zip`](https://docs.python.org/3/library/functions.html#zip)
#[violation]
pub struct ZipWithoutExplicitStrict;

impl Violation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
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
            checker
                .diagnostics
                .push(Diagnostic::new(ZipWithoutExplicitStrict, call.range()));
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
                if args.len() == 2 && is_const_none(&args[1]) {
                    return true;
                }

                // Ex) `iterools.repeat(1, times=None)`
                for keyword in keywords {
                    if keyword.arg.as_ref().is_some_and(|name| name == "times") {
                        if is_const_none(&keyword.value) {
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
