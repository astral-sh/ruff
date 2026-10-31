use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::fields;
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::visibility::{self, Visibility::Public};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
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
#[violation_metadata(preview_since = "0.9.0", category = Category::Pedantic)]
pub(crate) struct ClassAsDataStructure;

impl Violation for ClassAsDataStructure {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class could be dataclass or namedtuple".to_string()
    }
}

/// B903
pub(crate) fn class_as_data_structure(checker: &Checker, class_def: &fields::StmtClassDef<'_>) {
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

    for stmt in class_def.body {
        if public_methods > 1 && has_dunder_init {
            // we're good to break here
            break;
        }
        match stmt.classify() {
            flat::StmtKind::FunctionDef(func_def) => {
                if !has_dunder_init
                    && func_def.name() == "__init__"
                    && func_def
                        .parameters()
                        .iter()
                        // skip `self`
                        .skip(1)
                        .all(|param| param.annotation().is_some() && !param.is_variadic())
                    && (func_def.parameters().kwonlyargs().is_empty()
                        || checker.target_version() >= PythonVersion::PY310)
                    && (
                        // `__init__` should not have complicated logic in it
                        // only assignments
                        func_def
                            .body()
                            .iter()
                            .all(is_simple_assignment_to_attribute)
                    )
                {
                    has_dunder_init = true;
                }
                if matches!(visibility::method_visibility(func_def), Public) {
                    public_methods += 1;
                }
            }
            // Ignore class variables
            flat::StmtKind::Assign(_) | flat::StmtKind::AnnAssign(_) => {}
            // Ignore expressions (e.g. string literals)
            flat::StmtKind::Expr(_) => {}
            _ => {
                // Bail for anything else - e.g. nested classes
                // or conditional methods.
                return;
            }
        }
    }

    if has_dunder_init && public_methods == 1 {
        checker.report_diagnostic(ClassAsDataStructure, class_def.range);
    }
}

// Checks whether a statement is a, possibly augmented,
// assignment of a name to an attribute.
fn is_simple_assignment_to_attribute(stmt: flat::View<'_, ast::Stmt>) -> bool {
    match stmt.classify() {
        flat::StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            let value = stmt_assign.value();

            let Some([target]) = targets.as_array() else {
                return false;
            };
            target.is_attribute_expr() && value.is_name_expr()
        }
        flat::StmtKind::AnnAssign(stmt_ann_assign) => {
            let target = stmt_ann_assign.target();
            let value = stmt_ann_assign.value();

            target.is_attribute_expr() && value.is_some_and(flat::View::is_name_expr)
        }
        _ => false,
    }
}
