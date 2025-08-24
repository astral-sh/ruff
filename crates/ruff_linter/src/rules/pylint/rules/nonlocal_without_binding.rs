use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `nonlocal` names without bindings.
///
/// ## Why is this bad?
/// `nonlocal` names must be bound to a name in an outer scope.
/// Violating this rule leads to a `SyntaxError` at runtime.
///
/// ## Example
/// ```python
/// def foo():
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar = 1
///
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// ## References
/// - [Python documentation: The `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#nonlocal)
/// - [PEP 3104 – Access to Names in Outer Scopes](https://peps.python.org/pep-3104/)
#[derive(ViolationMetadata)]
pub(crate) struct NonlocalWithoutBinding {
    pub(crate) name: String,
}

impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}

/// PLE0117
pub(crate) fn nonlocal_without_binding(checker: &Checker, nonlocal: &ast::StmtNonlocal) {
    if !checker.semantic().scope_id.is_global() {
        for name in &nonlocal.names {
            if checker.semantic().nonlocal(name).is_none() {
                checker.report_diagnostic(
                    NonlocalWithoutBinding {
                        name: name.to_string(),
                    },
                    name.range(),
                );
            }
        }
    }
}
