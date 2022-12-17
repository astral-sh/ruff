use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// Identify all `ExprKind::Name` nodes in an AST.
struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    names: FxHashMap<&'a str, &'a Expr>,
}

impl NameFinder<'_> {
    fn new() -> Self {
        NameFinder {
            names: FxHashMap::default(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for NameFinder<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            self.names.insert(id, expr);
        }
        visitor::walk_expr(self, expr);
    }
}

/// B007
pub fn unused_loop_control_variable(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let control_names = {
        let mut finder = NameFinder::new();
        finder.visit_expr(target);
        finder.names
    };

    let used_names = {
        let mut finder = NameFinder::new();
        for stmt in body {
            finder.visit_stmt(stmt);
        }
        finder.names
    };

    for (name, expr) in control_names {
        // Ignore names that are already underscore-prefixed.
        if name.starts_with('_') {
            continue;
        }

        // Ignore any names that are actually used in the loop body.
        if used_names.contains_key(name) {
            continue;
        }

        let mut check = Check::new(
            CheckKind::UnusedLoopControlVariable(name.to_string()),
            Range::from_located(expr),
        );
        if checker.patch(check.kind.code()) {
            // Prefix the variable name with an underscore.
            check.amend(Fix::replacement(
                format!("_{name}"),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
