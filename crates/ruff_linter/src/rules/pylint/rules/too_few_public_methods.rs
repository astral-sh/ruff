use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::visibility::{self, Visibility::Public};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes with too few public methods
///
/// By default, this rule allows down to 2 public methods, as configured by
/// the [`pylint.min-public-methods`] option.
///
/// ## Why is this bad?
/// Classes with too few public methods are possibly better off
/// being a dataclass or a namedtuple, which are more lightweight.
///
/// ## Example
/// Assuming that `pylint.min-public-settings` is set to 2:
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
///
/// ## Options
/// - `pylint.min-public-methods`
#[violation]
pub struct TooFewPublicMethods {
    methods: usize,
    min_methods: usize,
}

impl Violation for TooFewPublicMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooFewPublicMethods {
            methods,
            min_methods,
        } = self;
        format!("Too few public methods ({methods} < {min_methods})")
    }
}

/// R0903
pub(crate) fn too_few_public_methods(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
    min_methods: usize,
) {
    let methods = class_def
        .body
        .iter()
        .filter(|stmt| {
            stmt.as_function_def_stmt()
                .is_some_and(|node| matches!(visibility::method_visibility(node), Public))
        })
        .count();

    if methods < min_methods {
        checker.diagnostics.push(Diagnostic::new(
            TooFewPublicMethods {
                methods,
                min_methods,
            },
            class_def.range(),
        ));
    }
}
