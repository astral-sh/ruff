use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Constant, Expr, ExprKind};

use crate::ast::helpers::{compose_call_path, match_call_path};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::flake8_bugbear::plugins::mutable_argument_default::is_mutable_func;

const IMMUTABLE_FUNCS: [&str; 7] = [
    "tuple",
    "frozenset",
    "operator.attrgetter",
    "operator.itemgetter",
    "operator.methodcaller",
    "types.MappingProxyType",
    "re.compile",
];

fn is_immutable_func(
    expr: &Expr,
    extend_immutable_calls: &[&str],
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
) -> bool {
    compose_call_path(expr)
        .map(|call_path| {
            IMMUTABLE_FUNCS
                .iter()
                .chain(extend_immutable_calls)
                .any(|target| match_call_path(&call_path, target, from_imports))
        })
        .unwrap_or(false)
}

struct ArgumentDefaultVisitor<'a> {
    checks: Vec<(CheckKind, Range)>,
    extend_immutable_calls: &'a [&'a str],
    from_imports: &'a FnvHashMap<&'a str, FnvHashSet<&'a str>>,
}

impl<'a, 'b> Visitor<'b> for ArgumentDefaultVisitor<'b>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Call { func, args, .. } => {
                if !is_mutable_func(func, self.from_imports)
                    && !is_immutable_func(func, self.extend_immutable_calls, self.from_imports)
                    && !is_nan_or_infinity(func, args)
                {
                    self.checks.push((
                        CheckKind::FunctionCallArgumentDefault(compose_call_path(expr)),
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
    let extend_immutable_cells: Vec<&str> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|s| s.as_str())
        .collect();
    let mut visitor = ArgumentDefaultVisitor {
        checks: vec![],
        extend_immutable_calls: &extend_immutable_cells,
        from_imports: &checker.from_imports,
    };
    for expr in arguments
        .defaults
        .iter()
        .chain(arguments.kw_defaults.iter())
    {
        visitor.visit_expr(expr);
    }
    for (check, range) in visitor.checks {
        checker.add_check(Check::new(check, range));
    }
}
