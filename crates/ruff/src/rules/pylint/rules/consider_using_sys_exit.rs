use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::helpers;
use crate::ast::types::{BindingKind, Range};
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{AutofixKind, Availability, Violation};

define_violation!(
    pub struct ConsiderUsingSysExit {
        pub name: String,
    }
);
impl Violation for ConsiderUsingSysExit {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingSysExit { name } = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|ConsiderUsingSysExit { name }| format!("Replace `{name}` with `sys.exit()`"))
    }
}
/// Return `true` if the `module` was imported using a star import (e.g., `from
/// sys import *`).
fn is_module_star_imported(checker: &Checker, module: &str) -> bool {
    checker.current_scopes().any(|scope| {
        scope.bindings.values().any(|index| {
            if let BindingKind::StarImportation(_, name) = &checker.bindings[*index].kind {
                name.as_ref().map(|name| name == module).unwrap_or_default()
            } else {
                false
            }
        })
    })
}

/// RUF004
pub fn consider_using_sys_exit(checker: &mut Checker, func: &Expr) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    for name in ["exit", "quit"] {
        if id != name {
            continue;
        }
        if name == "exit" && is_module_star_imported(checker, "sys") {
            continue;
        }
        if !checker.is_builtin(name) {
            continue;
        }
        let mut diagnostic = Diagnostic::new(
            ConsiderUsingSysExit {
                name: name.to_string(),
            },
            Range::from_located(func),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(content) = helpers::get_member_import_name_alias(checker, "sys", "exit") {
                diagnostic.amend(Fix::replacement(
                    content,
                    func.location,
                    func.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
