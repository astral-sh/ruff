use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{BindingKind, Range};
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// Return `true` if the `module` was imported using a star import (e.g., `from
/// sys import *`).
fn is_module_star_imported(checker: &Checker, module: &str) -> bool {
    checker.current_scopes().any(|scope| {
        scope.values.values().any(|index| {
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
            .values
            .values()
            .find_map(|index| match &checker.bindings[*index].kind {
                // e.g. module=sys object=exit
                // `import sys`         -> `sys.exit`
                // `import sys as sys2` -> `sys2.exit`
                BindingKind::Importation(name, full_name) if full_name == module => {
                    Some(format!("{name}.{member}"))
                }
                // e.g. module=os.path object=join
                // `from os.path import join`          -> `join`
                // `from os.path import join as join2` -> `join2`
                BindingKind::FromImportation(name, full_name)
                    if full_name == &format!("{module}.{member}") =>
                {
                    Some(name.to_string())
                }
                // e.g. module=os.path object=join
                // `from os.path import *` -> `join`
                BindingKind::StarImportation(_, name)
                    if name.as_ref().map(|name| name == module).unwrap_or_default() =>
                {
                    Some(member.to_string())
                }
                // e.g. module=os.path object=join
                // `import os.path ` -> `os.path.join`
                BindingKind::SubmoduleImportation(_, full_name) if full_name == module => {
                    Some(format!("{full_name}.{member}"))
                }
                // Non-imports.
                _ => None,
            })
    })
}

/// RUF004
pub fn use_sys_exit(checker: &mut Checker, func: &Expr) {
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
        let mut check = Check::new(
            CheckKind::UseSysExit(name.to_string()),
            Range::from_located(func),
        );
        if checker.patch(check.kind.code()) {
            if let Some(content) = get_member_import_name_alias(checker, "sys", "exit") {
                check.amend(Fix::replacement(
                    content,
                    func.location,
                    func.end_location.unwrap(),
                ));
            }
        }
        checker.add_check(check);
    }
}
