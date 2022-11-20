use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

#[derive(Default)]
struct NameFinder<'a> {
    names: FxHashMap<&'a str, &'a Expr>,
}

impl<'a, 'b> Visitor<'b> for NameFinder<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Name { id, .. } => {
                self.names.insert(id, expr);
            }
            ExprKind::ListComp { generators, .. }
            | ExprKind::DictComp { generators, .. }
            | ExprKind::SetComp { generators, .. }
            | ExprKind::GeneratorExp { generators, .. } => {
                for comp in generators {
                    self.visit_expr(&comp.iter);
                }
            }
            ExprKind::Lambda { args, body } => {
                visitor::walk_expr(self, body);
                for arg in args.args.iter() {
                    self.names.remove(arg.node.arg.as_str());
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// B020
pub fn loop_variable_overrides_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    let target_names = {
        let mut target_finder = NameFinder::default();
        target_finder.visit_expr(target);
        target_finder.names
    };
    let iter_names = {
        let mut iter_finder = NameFinder::default();
        iter_finder.visit_expr(iter);
        iter_finder.names
    };

    for (name, expr) in target_names {
        if iter_names.contains_key(name) {
            checker.add_check(Check::new(
                CheckKind::LoopVariableOverridesIterator(name.to_string()),
                Range::from_located(expr),
            ));
        }
    }
}
