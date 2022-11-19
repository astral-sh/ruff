use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, Range};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn has_builtin_exit_in_scope(checker: &Checker) -> bool {
    matches!(
        checker.current_scopes().find_map(|s| s.values.get("exit")),
        Some(Binding {
            kind: BindingKind::Builtin,
            ..
        })
    )
}

fn get_exit_name(checker: &Checker) -> Option<String> {
    checker.current_scopes().find_map(|s| {
        s.values.values().find_map(|v| match &v.kind {
            BindingKind::Importation(name, full_name, _) if full_name == &"sys".to_string() => {
                Some(name.to_owned() + ".exit")
            }
            BindingKind::FromImportation(name, full_name, _)
                if full_name == &"sys.exit".to_string() =>
            {
                Some(name.to_owned())
            }
            _ => None,
        })
    })
}

pub fn convert_sys_to_sys_exit(checker: &mut Checker, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exit" {
            if has_builtin_exit_in_scope(checker) {
                let mut check =
                    Check::new(CheckKind::ConvertExitToSysExit, Range::from_located(func));
                if checker.patch(check.kind.code()) {
                    if let Some(content) = get_exit_name(checker) {
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
