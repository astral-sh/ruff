use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of names that are declared as `global` prior to the
/// relevant `global` declaration.
///
/// ## Why is this bad?
/// The `global` declaration applies to the entire scope. Using a name that's
/// declared as `global` in a given scope prior to the relevant `global`
/// declaration is a syntax error.
///
/// ## Example
/// ```python
/// counter = 1
///
///
/// def increment():
///     print(f"Adding 1 to {counter}")
///     global counter
///     counter += 1
/// ```
///
/// Use instead:
/// ```python
/// counter = 1
///
///
/// def increment():
///     global counter
///     print(f"Adding 1 to {counter}")
///     counter += 1
/// ```
///
/// ## References
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
#[violation]
pub struct LoadBeforeGlobalDeclaration {
    name: String,
    row: SourceRow,
}

impl Violation for LoadBeforeGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoadBeforeGlobalDeclaration { name, row } = self;
        format!("Name `{name}` is used prior to global declaration on {row}")
    }
}

/// PLE0118
pub(crate) fn load_before_global_declaration(checker: &mut Checker, name: &str, expr: &Expr) {
    if let Some(stmt) = checker.semantic().global(name) {
        if expr.start() < stmt.start() {
            checker.diagnostics.push(Diagnostic::new(
                LoadBeforeGlobalDeclaration {
                    name: name.to_string(),
                    row: checker.compute_source_row(stmt.start()),
                },
                expr.range(),
            ));
        }
    }
}
