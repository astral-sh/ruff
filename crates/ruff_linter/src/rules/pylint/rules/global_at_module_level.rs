use ast::Stmt;
use ruff_python_ast::{self as ast};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `global` at the top level of a module.
///
/// ## Why is this bad?
/// `global` at the top level of a module is redundant.
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
    if !checker.semantic().at_top_level() {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(GlobalAtModuleLevel, stmt.range()));
}
