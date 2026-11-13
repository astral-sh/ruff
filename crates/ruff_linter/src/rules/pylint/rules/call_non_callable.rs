use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for calls to objects whose type is known and does not implement
/// `__call__`, such as literals for lists, strings, tuples, and dictionaries.
///
/// ## Why is this bad?
/// Only objects whose type implements `__call__` can be called. Calling
/// an object of a type that doesn't implement `__call__` will result in a
/// `TypeError` at runtime, and a `SyntaxWarning`
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
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Correctness)]
pub(crate) struct CallNonCallable {
    python_type: String,
}

impl Violation for CallNonCallable {
    // No fix: CPython suggests a missing comma, but we can't know the author's
    // intent (e.g. `"foo"()` vs `foo()`).
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

    if let Some(python_type) = non_callable_type(func) {
        checker.report_diagnostic(
            CallNonCallable {
                python_type: python_type.to_string(),
            },
            func.range(),
        );
    }
}

fn non_callable_type(expr: &Expr) -> Option<&'static str> {
    let python_type = match expr {
        Expr::StringLiteral(_) => "str",
        Expr::BytesLiteral(_) => "bytes",
        Expr::BooleanLiteral(_) => "bool",
        Expr::NumberLiteral(number) => match number.value {
            Number::Int(_) => "int",
            Number::Float(_) => "float",
            Number::Complex { .. } => "complex",
        },
        Expr::NoneLiteral(_) => "NoneType",
        Expr::EllipsisLiteral(_) => "ellipsis",
        Expr::Tuple(_) => "tuple",
        Expr::List(_) | Expr::ListComp(_) => "list",
        Expr::Dict(_) | Expr::DictComp(_) => "dict",
        Expr::Set(_) | Expr::SetComp(_) => "set",
        Expr::Generator(_) => "generator",
        Expr::FString(_) => "str",
        Expr::TString(_) => "Template",
        _ => return None,
    };
    Some(python_type)
}
