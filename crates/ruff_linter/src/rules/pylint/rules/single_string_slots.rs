use ruff_python_ast::{self as ast, Expr, Stmt, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for single strings assigned to `__slots__`.
///
/// ## Why is this bad?
/// In Python, the `__slots__` attribute allows you to explicitly define the
/// attributes (instance variables) that a class can have. By default, Python
/// uses a dictionary to store an object's attributes, which incurs some memory
/// overhead. However, when `__slots__` is defined, Python uses a more compact
/// internal structure to store the object's attributes, resulting in memory
/// savings.
///
/// Any string iterable may be assigned to `__slots__` (most commonly, a
/// `tuple` of strings). If a string is assigned to `__slots__`, it is
/// interpreted as a single attribute name, rather than an iterable of attribute
/// names. This can cause confusion, as users that iterate over the `__slots__`
/// value may expect to iterate over a sequence of attributes, but would instead
/// iterate over the characters of the string.
///
/// To use a single string attribute in `__slots__`, wrap the string in an
/// iterable container type, like a `tuple`.
///
/// ## Example
/// ```python
/// class Person:
///     __slots__: str = "name"
///
///     def __init__(self, name: str) -> None:
///         self.name = name
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     __slots__: tuple[str, ...] = ("name",)
///
///     def __init__(self, name: str) -> None:
///         self.name = name
/// ```
///
/// ## References
/// - [Python documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
#[derive(ViolationMetadata)]
pub(crate) struct SingleStringSlots;

impl Violation for SingleStringSlots {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class `__slots__` should be a non-string iterable".to_string()
    }
}

/// PLC0205
pub(crate) fn single_string_slots(checker: &Checker, class: &StmtClassDef) {
    for stmt in &class.body {
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        if id.as_str() == "__slots__" {
                            if matches!(value.as_ref(), Expr::StringLiteral(_) | Expr::FString(_)) {
                                checker.report_diagnostic(Diagnostic::new(
                                    SingleStringSlots,
                                    stmt.identifier(),
                                ));
                            }
                        }
                    }
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    if id.as_str() == "__slots__" {
                        if matches!(value.as_ref(), Expr::StringLiteral(_) | Expr::FString(_)) {
                            checker.report_diagnostic(Diagnostic::new(
                                SingleStringSlots,
                                stmt.identifier(),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
