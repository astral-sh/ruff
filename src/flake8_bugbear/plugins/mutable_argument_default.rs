use rustpython_ast::{Arguments, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn is_mutable_func(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Name { id, .. }
            if id == "dict"
                || id == "list"
                || id == "set"
                || id == "Counter"
                || id == "OrderedDict"
                || id == "defaultdict"
                || id == "deque" =>
        {
            true
        }
        ExprKind::Attribute { value, attr, .. }
            if (attr == "Counter"
                || attr == "OrderedDict"
                || attr == "defaultdict"
                || attr == "deque") =>
        {
            matches!(&value.node, ExprKind::Name { id, .. } if id == "collections")
        }
        _ => false,
    }
}

/// B006
pub fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    for expr in arguments
        .defaults
        .iter()
        .chain(arguments.kw_defaults.iter())
    {
        match &expr.node {
            ExprKind::List { .. }
            | ExprKind::Dict { .. }
            | ExprKind::Set { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => {
                checker.add_check(Check::new(
                    CheckKind::MutableArgumentDefault,
                    Range::from_located(expr),
                ));
            }
            ExprKind::Call { func, .. } => {
                if is_mutable_func(func) {
                    checker.add_check(Check::new(
                        CheckKind::MutableArgumentDefault,
                        Range::from_located(expr),
                    ));
                }
            }
            _ => {}
        }
    }
}
