use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Decimal` calls that pass a non-string argument.
///
/// ## Why is this bad?
/// Constructing `Decimal` from a string literal ensures the exact decimal value
/// is preserved and makes the intended value explicit in code. Passing integer
/// literals or integer-typed variables bypasses this explicitness — while integers
/// don't lose precision, enforcing string construction provides consistency and
/// prevents accidental use of float variables.
///
/// This rule does **not** fire on float literals (use `RUF032` for that), tuple
/// constructor forms, or expressions whose type cannot be determined.
///
/// ## Example
///
/// ```python
/// from decimal import Decimal
///
/// num = Decimal(1)
/// x: int = 42
/// num = Decimal(x)
/// ```
///
/// Use instead:
/// ```python
/// from decimal import Decimal
///
/// num = Decimal("1")
/// x: str = "42"
/// num = Decimal(x)
/// ```
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.15")]
pub(crate) struct DecimalFromNonStringArg;

impl Violation for DecimalFromNonStringArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`Decimal()` called with a non-string argument".to_string()
    }
}

/// RUF076
pub(crate) fn decimal_from_non_string_arg(checker: &Checker, call: &ast::ExprCall) {
    // Extract the effective `value` argument: positional first, then keyword `value=`.
    let arg = call
        .arguments
        .args
        .first()
        .or_else(|| {
            call.arguments
                .find_keyword("value")
                .map(|kw| &kw.value)
        });

    let Some(arg) = arg else {
        return;
    };

    // Verify call target is decimal.Decimal
    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["decimal", "Decimal"])
        })
    {
        return;
    }

    let resolved_type = ResolvedPythonType::from(arg);

    match resolved_type {
        // String literals, f-strings, string concatenations → allowed
        ResolvedPythonType::Atom(PythonType::String) => {}

        // Tuple literals → allowed (valid Decimal sign/digits/exponent form)
        ResolvedPythonType::Atom(PythonType::Tuple) => {}

        // Float literals → defer to RUF032 which provides a fix
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Float)) => {}

        // Integer/complex/bool literals → reject
        ResolvedPythonType::Atom(PythonType::Number(_)) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }

        // Bytes literals → reject
        ResolvedPythonType::Atom(PythonType::Bytes) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }

        // Other known atom types (list, dict, set, etc.) → reject
        ResolvedPythonType::Atom(_) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }

        // Unknown (variables, function calls, attributes) → check binding annotation
        ResolvedPythonType::Unknown => {
            if is_known_non_string_binding(checker, arg) {
                checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
            }
        }

        // Union, TypeError → do not report (too uncertain)
        ResolvedPythonType::Union(_) | ResolvedPythonType::TypeError => {}
    }
}

/// Returns `true` if the expression is a name bound to a known non-string type
/// (e.g., `int`, `float`). Uses Ruff's semantic type-checking infrastructure
/// which handles annotated assignments, function parameters, and common aliases.
fn is_known_non_string_binding(checker: &Checker, expr: &Expr) -> bool {
    let Expr::Name(name) = expr else {
        return false;
    };

    let Some(binding_id) = checker.semantic().resolve_name(name) else {
        return false;
    };

    let binding = checker.semantic().binding(binding_id);
    let semantic = checker.semantic();

    // If it's annotated as str, it's fine
    if typing::is_string(binding, semantic) {
        return false;
    }

    // If it's annotated as int or float, it's definitely bad
    typing::is_int(binding, semantic) || typing::is_float(binding, semantic)
}
