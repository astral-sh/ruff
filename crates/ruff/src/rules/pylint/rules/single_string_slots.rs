use rustpython_parser::ast::{self, Constant, Expr, Stmt, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
/// Any non-string iterable may be assigned to`__slots__`. To use a single
/// string attribute in `__slots__`, wrap the string in another iterable type
/// (e.g., a `tuple`).
///
/// ## Example
/// ```python
/// class Person:
///     __slots__: str = "name"
///
///     def __init__(self, name: string) -> None:
///         self.name = name
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     __slots__: tuple[str, ...] = ("name",)
///
///     def __init__(self, name: string) -> None:
///         self.name = name
/// ```
///
/// ## References
/// - [Python documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
#[violation]
pub struct SingleStringSlots;

impl Violation for SingleStringSlots {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class `__slots__` should be a non-string iterable")
    }
}

fn is_string(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(_),
            ..
        })
    )
}

/// PLC0205
pub(crate) fn single_string_slots(checker: &mut Checker, class: &StmtClassDef) {
    for stmt in &class.body {
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        if id.as_str() == "__slots__" && is_string(value) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SingleStringSlots, stmt.identifier()));
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
                    if id.as_str() == "__slots__" && is_string(value) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SingleStringSlots, stmt.identifier()));
                    }
                }
            }
            _ => {}
        }
    }
}
