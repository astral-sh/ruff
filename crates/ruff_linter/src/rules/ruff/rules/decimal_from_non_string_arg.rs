use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Decimal` calls that pass a non-string argument.
///
/// ## Why is this bad?
/// The `Decimal` class is designed to handle numbers with fixed-point precision.
/// Passing numeric literals or variables can lead to precision loss or unexpected
/// behavior. Using a string argument ensures the exact decimal value is preserved.
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
    let Some(arg) = call.arguments.args.first() else {
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
        // Numeric literals (int, float, complex, bool) → reject
        ResolvedPythonType::Atom(PythonType::Number(_)) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Other known types (bytes, list, dict, etc.) → reject
        ResolvedPythonType::Atom(_) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Union types → reject
        ResolvedPythonType::Union(_) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // TypeError → reject
        ResolvedPythonType::TypeError => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Unknown (variables, function calls) → check annotation if possible
        ResolvedPythonType::Unknown => {
            if !is_string_typed_name(checker, arg) {
                checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
            }
        }
    }
}

/// Check if an expression is a name reference that is annotated as `str`.
fn is_string_typed_name(checker: &Checker, expr: &Expr) -> bool {
    let Expr::Name(name) = expr else {
        return false;
    };

    let Some(binding_id) = checker.semantic().resolve_name(name) else {
        return false;
    };

    let binding = checker.semantic().binding(binding_id);

    // Check if the binding's source statement has a `str` annotation
    if let Some(node_id) = binding.source {
        let stmt = checker.semantic().statement(node_id);
        if let ast::Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) = stmt {
            if let Expr::Name(ann_name) = annotation.as_ref() {
                return ann_name.id == "str";
            }
        }
    }

    false
}
