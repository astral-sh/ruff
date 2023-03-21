use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::scope::BindingKind;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SysExitAlias {
    pub name: String,
}

impl Violation for SysExitAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SysExitAlias { name } = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|SysExitAlias { name }| format!("Replace `{name}` with `sys.exit()`"))
    }
}
/// Return `true` if the `module` was imported using a star import (e.g., `from
/// sys import *`).
fn is_module_star_imported(checker: &Checker, module: &str) -> bool {
    checker.ctx.scopes().any(|scope| {
        scope.binding_ids().any(|index| {
            if let BindingKind::StarImportation(_, name) = &checker.ctx.bindings[*index].kind {
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
    checker.ctx.scopes().find_map(|scope| {
        scope
            .binding_ids()
            .find_map(|index| match &checker.ctx.bindings[*index].kind {
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

/// PLR1722
pub fn sys_exit_alias(checker: &mut Checker, func: &Expr) {
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
        if !checker.ctx.is_builtin(name) {
            continue;
        }
        let mut diagnostic = Diagnostic::new(
            SysExitAlias {
                name: name.to_string(),
            },
            Range::from(func),
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
