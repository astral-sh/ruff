use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, Range, Scope};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn get_sys_import(scope: &Scope) -> Option<String> {
    let sys_binding_kind =
        scope.values.values().map(|v| &v.kind).find(
            |b| matches!(b, BindingKind::Importation(_, sys, _) if sys == &"sys".to_string()),
        );
    if let Some(BindingKind::Importation(name, ..)) = sys_binding_kind {
        return Some(name.clone());
    }
    None
}

pub fn convert_sys_to_sys_exit(checker: &mut Checker, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exit" {
            let scope = checker.current_scope();
            if let Some(Binding {
                kind: BindingKind::Builtin,
                ..
            }) = scope.values.get("exit")
            {
                let mut check =
                    Check::new(CheckKind::ConvertExitToSysExit, Range::from_located(func));
                if checker.patch(check.kind.code()) {
                    if let Some(mut content) = get_sys_import(scope) {
                        content.push_str(".exit");
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
