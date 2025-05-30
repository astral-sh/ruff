use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::Violation;
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
#[derive(ViolationMetadata)]
pub(crate) struct GlobalAtModuleLevel;

impl Violation for GlobalAtModuleLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`global` at module level is redundant".to_string()
    }
}

/// PLW0604
pub(crate) fn global_at_module_level(checker: &Checker, stmt: &Stmt) {
    if checker.semantic().current_scope().kind.is_module() {
        checker.report_diagnostic(GlobalAtModuleLevel, stmt.range());
    }
}
