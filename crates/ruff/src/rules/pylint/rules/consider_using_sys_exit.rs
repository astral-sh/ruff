use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::{BindingKind, Range};
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

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

/// Return the appropriate `sys.exit` reference based on the current set of
/// imports, or `None` is `sys.exit` hasn't been imported.
fn get_member_import_name_alias(checker: &Checker, module: &str, member: &str) -> Option<String> {
    checker.current_scopes().find_map(|scope| {
        scope
            .bindings
            .values()
            .find_map(|index| match &checker.bindings[*index].kind {
                // e.g. module=sys object=exit
                // `import sys`         -> `sys.exit`
                // `import sys as sys2` -> `sys2.exit`
                BindingKind::Importation(name, full_name) => {
                    if full_name == &module {
                        Some(format!("{name}.{member}"))
                    } else {
                        None
                    }
                }
                // e.g. module=os.path object=join
                // `from os.path import join`          -> `join`
                // `from os.path import join as join2` -> `join2`
                BindingKind::FromImportation(name, full_name) => {
                    let mut parts = full_name.split('.');
                    if parts.next() == Some(module)
                        && parts.next() == Some(member)
                        && parts.next().is_none()
                    {
                        Some((*name).to_string())
                    } else {
                        None
                    }
                }
                // e.g. module=os.path object=join
                // `from os.path import *` -> `join`
                BindingKind::StarImportation(_, name) => {
                    if name.as_ref().map(|name| name == module).unwrap_or_default() {
                        Some(member.to_string())
                    } else {
                        None
                    }
                }
                // e.g. module=os.path object=join
                // `import os.path ` -> `os.path.join`
                BindingKind::SubmoduleImportation(_, full_name) => {
                    if full_name == &module {
                        Some(format!("{full_name}.{member}"))
                    } else {
                        None
                    }
                }
                // Non-imports.
                _ => None,
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
            if let Some(content) = get_member_import_name_alias(checker, "sys", "exit") {
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
