use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for data classes that only set attributes in an `__init__` method.
///
/// ## Why is this bad?
/// Data classes that only set attributes in an `__init__` method can be
/// replaced with `dataclasses` for better readability and maintainability.
///
/// ## Example
/// ```python
/// class Point:
///     def __init__(self, x, y):
///         self.x = x
///         self.y = y
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass
///
/// @dataclass
/// class Point:
///     x: int
///     y: int
/// ```
///
/// ## References
/// - [Python documentation: `dataclasses`](https://docs.python.org/3/library/dataclasses.html)
#[violation]
pub struct UseDataclassesForDataClasses;

impl Violation for UseDataclassesForDataClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `dataclasses` for data classes that only set attributes in an `__init__` method")
    }
}

/// B903
pub(crate) fn use_dataclasses_for_data_classes(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::ClassDef(ast::StmtClassDef { body, .. }) = stmt else {
        return;
    };

    for stmt in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef {
            name,
            parameters,
            body,
            ..
        }) = stmt
        else {
            continue;
        };

        if name.id != "__init__" {
            continue;
        }

        let mut has_only_attribute_assignments = true;
        for stmt in body {
            if let Stmt::Assign(ast::StmtAssign { targets, .. }) = stmt {
                if targets.len() != 1 {
                    has_only_attribute_assignments = false;
                    break;
                }

                let Expr::Attribute(ast::ExprAttribute { value, .. }) = &targets[0] else {
                    has_only_attribute_assignments = false;
                    break;
                };

                if !matches!(value.as_ref(), Expr::Name(_)) {
                    has_only_attribute_assignments = false;
                    break;
                }
            } else {
                has_only_attribute_assignments = false;
                break;
            }
        }

        if has_only_attribute_assignments {
            checker.diagnostics.push(Diagnostic::new(
                UseDataclassesForDataClasses,
                stmt.range(),
            ));
        }
    }
}
