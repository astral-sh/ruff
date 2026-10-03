use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::analyze::type_inference::ResolvedPythonType;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for calls to objects whose type is known and does not implement
/// `__call__`, such as literals for lists, strings, tuples, and dictionaries.
///
/// ## Why is this bad?
/// Only objects whose type implements `__call__` can be called. Calling
/// an object of a type that doesn't implement `__call__` will result in a
/// `TypeError` at runtime, and, for many literal types, a `SyntaxWarning`
/// when Python compiles the constant expression.
///
/// This often happens unintentionally, for example, due to a missing
/// comma between two elements of a collection literal.
///
/// ## Example
/// ```python
/// # A list is not callable
/// a = [1, 2, 3]()
///
/// # Missing comma
/// b = [
///     (1, 2)
///     (3, 4)
/// ]
/// ```
///
/// Use instead:
/// ```python
/// a = [1, 2, 3]
///
/// b = [
///     (1, 2),
///     (3, 4),
/// ]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct CallNonCallable {
    python_type: String,
}

impl Violation for CallNonCallable {
    // No reliable fix in general, since we can't know the author's intent.
    // In theory, an unsafe fix could be offered to insert a comma, e.g. for
    // - collection literals: `[2 (3, 4)]` -> `[2, (3, 4)]`
    // - function args: `foo(1(2, 3))` -> `foo(1, (2, 3))`
    // or other cases, but to not complicate things, we'll wait for feedback
    // from users.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let CallNonCallable { python_type } = self;
        format!("`{python_type}` object is not callable.")
    }
}

/// PLE1102
pub(crate) fn call_non_callable(checker: &Checker, call: &ExprCall) {
    let func = &*call.func;

    let resolved_type = ResolvedPythonType::from(func);
    match resolved_type {
        // A `Union` is just a union `PythonType` non-callable atoms.
        ResolvedPythonType::Atom(_) | ResolvedPythonType::Union(_) => {
            checker.report_diagnostic(
                CallNonCallable {
                    python_type: resolved_type.to_string(),
                },
                func.range(),
            );
        }
        _ => {
            // TODO: Move to PythonType?
            if let Expr::TString(t_string) = func {
                checker.report_diagnostic(
                    CallNonCallable {
                        python_type: "Template".to_string(),
                    },
                    t_string.range(),
                );
            }
        }
    }
}
