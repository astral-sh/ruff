use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, Range};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn is_module_star_imported(checker: &Checker, module: String) -> bool {
    checker.current_scopes().any(|s| {
        s.values.values().any(|v| {
            matches!(&v.kind,BindingKind::StarImportation(_, name)
                    if name.is_some() && name.as_ref().unwrap() == &module
            )
        })
    })
}

fn has_builtin_exit_in_scope(checker: &Checker) -> bool {
    !is_module_star_imported(checker, "sys".to_string())
        && matches!(
            checker.current_scopes().find_map(|s| s.values.get("exit")),
            Some(Binding {
                kind: BindingKind::Builtin,
                ..
            })
        )
}

fn get_object_import_name_alias(
    checker: &Checker,
    module: String,
    object: String,
) -> Option<String> {
    checker.current_scopes().find_map(|s| {
        s.values.values().find_map(|v| match &v.kind {
            // e.g. module=sys object=exit
            // `import sys`         -> `sys.exit`
            // `import sys as sys2` -> `sys2.exit`
            BindingKind::Importation(name, full_name, _) if full_name == &module => {
                Some(format!("{}.{}", name, object))
            }
            // e.g. module=os.path object=join
            // `from os.path import join`          -> `join`
            // `from os.path import join as join2` -> `join2`
            BindingKind::FromImportation(name, full_name, _)
                if full_name == &format!("{}.{}", module, object) =>
            {
                Some(name.to_owned())
            }
            // e.g. module=os.path object=join
            // `from os.path import *` -> `join`
            BindingKind::StarImportation(_, name)
                if name.is_some() && name.as_ref().unwrap() == &module =>
            {
                Some(object.clone())
            }
            // e.g. module=os.path object=join
            // `import os.path ` -> `os.path.join`
            BindingKind::SubmoduleImportation(_, full_name, _) if full_name == &module => {
                Some(format!("{}.{}", full_name.to_owned(), object))
            }
            // Rest is not an import
            _ => None,
        })
    })
}

pub fn convert_exit_to_sys_exit(checker: &mut Checker, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exit" {
            if has_builtin_exit_in_scope(checker) {
                let mut check =
                    Check::new(CheckKind::ConvertExitToSysExit, Range::from_located(func));
                if checker.patch(check.kind.code()) {
                    if let Some(content) =
                        get_object_import_name_alias(checker, "sys".to_string(), "exit".to_string())
                    {
                        check.amend(Fix::replacement(
                            content,
                            func.location,
                            func.end_location.unwrap(),
                        ))
                    }
                }
                checker.add_check(check);
            }
        }
    }
}
