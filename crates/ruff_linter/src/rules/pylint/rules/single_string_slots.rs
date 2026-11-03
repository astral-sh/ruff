use ruff_python_ast::{self as ast, ExprKind, StmtKind};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "v0.0.276", category = Category::Suspicious)]
pub(crate) struct SingleStringSlots;

impl Violation for SingleStringSlots {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class `__slots__` should be a non-string iterable".to_string()
    }
}

/// PLC0205
pub(crate) fn single_string_slots(checker: &Checker, class: &ast::StmtClassDefFields<'_>) {
    for stmt in class.body {
        match stmt.classify() {
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();
                let value = stmt_assign.value();

                for target in targets {
                    if let ExprKind::Name(name_node) = target.classify() {
                        let id = name_node.id();
                        if id.as_str() == "__slots__" {
                            if matches!(
                                value.classify(),
                                ExprKind::StringLiteral(_) | ExprKind::FString(_)
                            ) {
                                checker.report_diagnostic(SingleStringSlots, stmt.identifier());
                            }
                        }
                    }
                }
            }
            StmtKind::AnnAssign(stmt_ann_assign) if let Some(value) = stmt_ann_assign.value() => {
                let target = stmt_ann_assign.target();

                if let ExprKind::Name(name_node) = target.classify() {
                    let id = name_node.id();
                    if id.as_str() == "__slots__" {
                        if matches!(
                            value.classify(),
                            ExprKind::StringLiteral(_) | ExprKind::FString(_)
                        ) {
                            checker.report_diagnostic(SingleStringSlots, stmt.identifier());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
