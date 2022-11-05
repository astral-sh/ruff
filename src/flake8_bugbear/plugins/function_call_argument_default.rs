use rustpython_ast::{Arguments, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::flake8_bugbear::plugins::mutable_argument_default::is_mutable_func;

pub fn call_path(expr: &Expr) -> Option<String> {
    match &expr.node {
        ExprKind::Name { id, .. } => Some(id.to_string()),
        ExprKind::Attribute { value, attr, .. } => {
            call_path(value).map(|path| format!("{}.{}", path, attr))
        }
        _ => None,
    }
}

const IMMUTABLE_FUNCS: [&str; 11] = [
    "tuple",
    "frozenset",
    "operator.attrgetter",
    "operator.itemgetter",
    "operator.methodcaller",
    "attrgetter",
    "itemgetter",
    "methodcaller",
    "types.MappingProxyType",
    "MappingProxyType",
    "re.compile",
];

fn is_immutable_func(expr: &Expr) -> bool {
    call_path(expr).map_or_else(|| false, |p| IMMUTABLE_FUNCS.contains(&p.as_str()))
}

struct ArgumentDefaultVisitor {
    checks: Vec<Check>,
}

impl<'a, 'b> Visitor<'b> for ArgumentDefaultVisitor
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Call { func, args, .. } => {
                if !is_mutable_func(func)
                    && !is_immutable_func(func)
                    && !is_nan_or_infinity(func, args)
                {
                    self.checks.push(Check::new(
                        CheckKind::FunctionCallArgumentDefault,
                        Range::from_located(expr),
                    ))
                }
                visitor::walk_expr(self, expr)
            }
            ExprKind::Lambda { .. } => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn is_nan_or_infinity(expr: &Expr, args: &[Expr]) -> bool {
    if let ExprKind::Name { id, .. } = &expr.node {
        if id == "float" {
            if let Some(arg) = args.first() {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &arg.node
                {
                    let lowercased = value.to_lowercase();
                    return lowercased == "nan"
                        || lowercased == "+nan"
                        || lowercased == "-nan"
                        || lowercased == "inf"
                        || lowercased == "+inf"
                        || lowercased == "-inf"
                        || lowercased == "infinity"
                        || lowercased == "+infinity"
                        || lowercased == "-infinity";
                }
            }
        }
    }
    false
}

/// B008
pub fn function_call_argument_default(checker: &mut Checker, arguments: &Arguments) {
    let mut visitor = ArgumentDefaultVisitor { checks: vec![] };
    for expr in arguments
        .defaults
        .iter()
        .chain(arguments.kw_defaults.iter())
    {
        visitor.visit_expr(expr);
    }
    for check in visitor.checks {
        checker.add_check(check);
    }
}
