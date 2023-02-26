use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::{
    checkers::ast::Checker,
    registry::{Diagnostic, Rule},
    Range,
};

use super::{AllWithModelForm, ExcludeWithModelForm};

/// Return `true` if a Python class appears to be a Django model based on a base class.
pub fn is_model(checker: &Checker, base: &Expr) -> bool {
    checker.resolve_call_path(base).map_or(false, |call_path| {
        call_path.as_slice() == ["django", "db", "models", "Model"]
    })
}

pub fn is_model_form(checker: &Checker, base: &Expr) -> bool {
    checker.resolve_call_path(base).map_or(false, |call_path| {
        call_path.as_slice() == ["django", "forms", "ModelForm"]
            || call_path.as_slice() == ["django", "forms", "models", "ModelForm"]
    })
}

pub fn get_model_field_name<'a>(checker: &'a Checker, expr: &'a Expr) -> Option<&'a str> {
    checker.resolve_call_path(expr).and_then(|call_path| {
        let call_path = call_path.as_slice();
        if !call_path.starts_with(&["django", "db", "models"]) {
            return None;
        }
        call_path.last().copied()
    })
}

/// DJ006, DJ007
pub fn check_model_form(
    checker: &Checker,
    bases: &[Expr],
    body: &[Stmt],
    _class_location: &Stmt,
) -> Option<Diagnostic> {
    if !bases.iter().any(|base| is_model_form(checker, base)) {
        return None;
    }
    for element in body.iter() {
        let StmtKind::ClassDef { name, body, .. } = &element.node else {
            continue;
        };
        if name != "Meta" {
            continue;
        }
        for element in body.iter() {
            let StmtKind::Assign { targets, value, .. } = &element.node else {
                continue;
            };
            for target in targets.iter() {
                let ExprKind::Name { id, .. } = &target.node else {
                    continue;
                };

                // DJ006
                if checker.settings.rules.enabled(&Rule::ExcludeWithModelForm) && id == "exclude" {
                    return Some(Diagnostic::new(
                        ExcludeWithModelForm,
                        Range::from_located(target),
                    ));
                }

                // DJ007
                if !checker.settings.rules.enabled(&Rule::AllWithModelForm) {
                    continue;
                }
                if id != "fields" {
                    continue;
                }
                let ExprKind::Constant { value, .. } = &value.node else {
                    continue;
                };
                match &value {
                    Constant::Str(s) => {
                        if s == "__all__" {
                            return Some(Diagnostic::new(
                                AllWithModelForm,
                                Range::from_located(element),
                            ));
                        }
                    }
                    Constant::Bytes(b) => {
                        if b == "__all__".as_bytes() {
                            return Some(Diagnostic::new(
                                AllWithModelForm,
                                Range::from_located(element),
                            ));
                        }
                    }
                    _ => (),
                };
            }
        }
    }
    None
}
