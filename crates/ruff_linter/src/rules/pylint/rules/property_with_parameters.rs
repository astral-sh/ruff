use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{identifier::Identifier, Decorator, Parameters, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for property definitions that accept function parameters.
///
/// ## Why is this bad?
/// Properties cannot be called with parameters.
///
/// If you need to pass parameters to a property, create a method with the
/// desired parameters and call that method instead.
///
/// ## Example
/// ```python
/// class Cat:
///     @property
///     def purr(self, volume):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Cat:
///     @property
///     def purr(self):
///         ...
///
///     def purr_volume(self, volume):
///         ...
/// ```
///
/// ## References
/// - [Python documentation: `property`](https://docs.python.org/3/library/functions.html#property)
#[violation]
pub struct PropertyWithParameters;

impl Violation for PropertyWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot have defined parameters for properties")
    }
}

/// PLR0206
pub(crate) fn property_with_parameters(
    checker: &mut Checker,
    stmt: &Stmt,
    decorator_list: &[Decorator],
    parameters: &Parameters,
) {
    if parameters.len() <= 1 {
        return;
    }
    let semantic = checker.semantic();
    if decorator_list
        .iter()
        .any(|decorator| semantic.match_builtin_expr(&decorator.expression, "property"))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(PropertyWithParameters, stmt.identifier()));
    }
}
