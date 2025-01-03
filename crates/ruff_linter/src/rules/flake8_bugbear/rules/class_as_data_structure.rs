use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::visibility::{self, Visibility::Public};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes that only have a public `__init__` method,
/// without base classes and 0 decorators.
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
pub(crate) fn class_as_data_structure(checker: &mut Checker, class_def: &ast::StmtClassDef) {
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
                {
                    has_dunder_init = true;
                }
                if matches!(visibility::method_visibility(func_def), Public) {
                    public_methods += 1;
                }
            }
            ast::Stmt::ClassDef(_) => {
                // allow classes with nested classes, often used for config
                return;
            }
            _ => {}
        }
    }

    if has_dunder_init && public_methods == 1 {
        checker
            .diagnostics
            .push(Diagnostic::new(ClassAsDataStructure, class_def.range()));
    }
}
