use rustpython_ast::{Arguments, ExprKind};

use crate::ast::types::{CheckLocator, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

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
                    checker.locate_check(Range::from_located(expr)),
                ));
            }
            ExprKind::Call { func, .. } => match &func.node {
                ExprKind::Name { id, .. }
                    if id == "dict"
                        || id == "list"
                        || id == "set"
                        || id == "Counter"
                        || id == "OrderedDict"
                        || id == "defaultdict"
                        || id == "deque" =>
                {
                    checker.add_check(Check::new(
                        CheckKind::MutableArgumentDefault,
                        checker.locate_check(Range::from_located(expr)),
                    ));
                }
                ExprKind::Attribute { value, attr, .. }
                    if (attr == "Counter"
                        || attr == "OrderedDict"
                        || attr == "defaultdict"
                        || attr == "deque") =>
                {
                    match &value.node {
                        ExprKind::Name { id, .. } if id == "collections" => {
                            checker.add_check(Check::new(
                                CheckKind::MutableArgumentDefault,
                                checker.locate_check(Range::from_located(expr)),
                            ));
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
