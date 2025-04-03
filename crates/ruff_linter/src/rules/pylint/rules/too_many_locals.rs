use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::{Scope, ScopeKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions that include too many local variables.
///
/// By default, this rule allows up to fifteen locals, as configured by the
/// [`lint.pylint.max-locals`] option.
///
/// ## Why is this bad?
/// Functions with many local variables are harder to understand and maintain.
///
/// Consider refactoring functions with many local variables into smaller
/// functions with fewer assignments.
///
/// ## Options
/// - `lint.pylint.max-locals`
#[derive(ViolationMetadata)]
pub(crate) struct TooManyLocals {
    current_amount: usize,
    max_amount: usize,
}

impl Violation for TooManyLocals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyLocals {
            current_amount,
            max_amount,
        } = self;
        format!("Too many local variables ({current_amount}/{max_amount})")
    }
}

/// PLR0914
pub(crate) fn too_many_locals(checker: &Checker, scope: &Scope) {
    let num_locals = scope
        .binding_ids()
        .filter(|id| {
            let binding = checker.semantic().binding(*id);
            binding.kind.is_assignment()
        })
        .count();
    if num_locals > checker.settings.pylint.max_locals {
        if let ScopeKind::Function(func) = scope.kind {
            checker.report_diagnostic(Diagnostic::new(
                TooManyLocals {
                    current_amount: num_locals,
                    max_amount: checker.settings.pylint.max_locals,
                },
                func.identifier(),
            ));
        }
    }
}
