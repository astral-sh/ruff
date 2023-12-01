use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `global` variables that are not assigned a value in the current
/// scope.
///
/// ## Why is this bad?
/// The `global` keyword allows an inner scope to modify a variable declared
/// in the outer scope. If the variable is not modified within the inner scope,
/// there is no need to use `global`.
///
/// ## Example
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     global DEBUG
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// Use instead:
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
#[violation]
pub struct GlobalVariableNotAssigned {
    pub name: String,
}

impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}
