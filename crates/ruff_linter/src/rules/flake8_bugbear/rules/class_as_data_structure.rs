use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::visibility::{self, Visibility::Public};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for classes that only have a public `__init__` method,
/// without base classes and decorators.
///
/// ## Why is this bad?
/// Classes with just an `__init__` are possibly better off
/// being a dataclass or a namedtuple, which have less boilerplate.
///
/// ## Example
/// ```python
/// class Point:
///     def __init__(self, x: float, y: float):
///         self.x = x
///         self.y = y
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass
///
///
/// @dataclass
/// class Point:
///     x: float
///     y: float
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ClassAsDataStructure;

impl Violation for ClassAsDataStructure {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class could be dataclass or namedtuple".to_string()
    }
}

/// B903
pub(crate) fn class_as_data_structure(checker: &Checker, class_def: &ast::StmtClassDef) {
    // skip stub files
    if checker.source_type.is_stub() {
        return;
    }

    // allow decorated classes
    if !class_def.decorator_list.is_empty() {
        return;
    }

    // allow classes with base classes
    if class_def.arguments.is_some() {
        return;
    }

    let mut public_methods = 0;
    let mut has_dunder_init = false;

    for stmt in &class_def.body {
        if public_methods > 1 && has_dunder_init {
            // we're good to break here
            break;
        }
        match stmt {
            ast::Stmt::FunctionDef(func_def) => {
                if !has_dunder_init
                    && func_def.name.to_string() == "__init__"
                    && func_def
                        .parameters
                        .iter()
                        // skip `self`
                        .skip(1)
                        .all(|param| param.annotation().is_some() && !param.is_variadic())
                    && (func_def.parameters.kwonlyargs.is_empty() || checker.target_version() >= PythonVersion::PY310)
                    // `__init__` should not have complicated logic in it
                    // only assignments
                    && func_def
                        .body
                        .iter()
                        .all(is_simple_assignment_to_attribute)
                {
                    has_dunder_init = true;
                }
                if matches!(visibility::method_visibility(func_def), Public) {
                    public_methods += 1;
                }
            }
            // Ignore class variables
            ast::Stmt::Assign(_) | ast::Stmt::AnnAssign(_) |
            // and expressions (e.g. string literals)
            ast::Stmt::Expr(_) => {}
            _ => {
                // Bail for anything else - e.g. nested classes
                // or conditional methods.
                return;
            }
        }
    }

    if has_dunder_init && public_methods == 1 {
        checker.report_diagnostic(Diagnostic::new(ClassAsDataStructure, class_def.range()));
    }
}

// Checks whether a statement is a, possibly augmented,
// assignment of a name to an attribute.
fn is_simple_assignment_to_attribute(stmt: &ast::Stmt) -> bool {
    match stmt {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            let [target] = targets.as_slice() else {
                return false;
            };
            target.is_attribute_expr() && value.is_name_expr()
        }
        ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
            target.is_attribute_expr() && value.as_ref().is_some_and(|val| val.is_name_expr())
        }
        _ => false,
    }
}
