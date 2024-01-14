use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the `global` keyword at the module level.
///
/// ## Why is this bad?
/// The `global` keyword is used within functions to indicate that a name
/// refers to a global variable, rather than a local variable.
///
/// At the module level, all names are global by default, so the `global`
/// keyword is redundant.
#[violation]
pub struct GlobalAtModuleLevel;

impl Violation for GlobalAtModuleLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`global` at module level is redundant")
    }
}

/// PLW0604
pub(crate) fn global_at_module_level(checker: &mut Checker, stmt: &Stmt) {
    if checker.semantic().current_scope().kind.is_module() {
        checker
            .diagnostics
            .push(Diagnostic::new(GlobalAtModuleLevel, stmt.range()));
    }
}
