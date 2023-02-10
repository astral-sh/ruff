use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::types::{Range, RefEquality};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct RewriteYieldFrom;
);
impl AlwaysAutofixableViolation for RewriteYieldFrom {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `yield` over `for` loop with `yield from`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `yield from`".to_string()
    }
}

/// Return `true` if the two expressions are equivalent, and consistent solely
/// of tuples and names.
fn is_same_expr(a: &Expr, b: &Expr) -> bool {
    match (&a.node, &b.node) {
        (ExprKind::Name { id: a, .. }, ExprKind::Name { id: b, .. }) => a == b,
        (ExprKind::Tuple { elts: a, .. }, ExprKind::Tuple { elts: b, .. }) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| is_same_expr(a, b))
        }
        _ => false,
    }
}

/// Collect all named variables in an expression consisting solely of tuples and
/// names.
fn collect_names(expr: &Expr) -> Vec<&str> {
    match &expr.node {
        ExprKind::Name { id, .. } => vec![id],
        ExprKind::Tuple { elts, .. } => elts.iter().flat_map(collect_names).collect(),
        _ => unreachable!("Expected: ExprKind::Name | ExprKind::Tuple"),
    }
}

#[derive(Debug)]
struct YieldFrom<'a> {
    stmt: &'a Stmt,
    body: &'a Stmt,
    iter: &'a Expr,
    names: Vec<&'a str>,
}

#[derive(Default)]
struct YieldFromVisitor<'a> {
    yields: Vec<YieldFrom<'a>>,
}

impl<'a> Visitor<'a> for YieldFromVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::For {
                target,
                body,
                orelse,
                iter,
                ..
            } => {
                // If there is an else statement, don't rewrite.
                if !orelse.is_empty() {
                    return;
                }
                // If there's any logic besides a yield, don't rewrite.
                if body.len() != 1 {
                    return;
                }
                // If the body is not a yield, don't rewrite.
                let body = &body[0];
                if let StmtKind::Expr { value } = &body.node {
                    if let ExprKind::Yield { value: Some(value) } = &value.node {
                        if is_same_expr(target, value) {
                            self.yields.push(YieldFrom {
                                stmt,
                                body,
                                iter,
                                names: collect_names(target),
                            });
                        }
                    }
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                // Don't recurse into anything that defines a new scope.
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::ListComp { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::GeneratorExp { .. }
            | ExprKind::Lambda { .. } => {
                // Don't recurse into anything that defines a new scope.
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct ReferenceVisitor<'a> {
    parent: Option<&'a Stmt>,
    references: FxHashMap<RefEquality<'a, Stmt>, Vec<&'a str>>,
}

impl<'a> Visitor<'a> for ReferenceVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let prev_parent = self.parent;
        self.parent = Some(stmt);
        visitor::walk_stmt(self, stmt);
        self.parent = prev_parent;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Name { id, ctx } => {
                if matches!(ctx, ExprContext::Load | ExprContext::Del) {
                    if let Some(parent) = self.parent {
                        self.references
                            .entry(RefEquality(parent))
                            .or_default()
                            .push(id);
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// UP028
pub fn rewrite_yield_from(checker: &mut Checker, stmt: &Stmt) {
    // Intentionally omit async functions.
    if let StmtKind::FunctionDef { body, .. } = &stmt.node {
        let yields = {
            let mut visitor = YieldFromVisitor::default();
            visitor.visit_body(body);
            visitor.yields
        };

        let references = {
            let mut visitor = ReferenceVisitor::default();
            visitor.visit_body(body);
            visitor.references
        };

        for item in yields {
            // If any of the bound names are used outside of the loop, don't rewrite.
            if references.iter().any(|(stmt, names)| {
                stmt != &RefEquality(item.stmt)
                    && stmt != &RefEquality(item.body)
                    && item.names.iter().any(|name| names.contains(name))
            }) {
                continue;
            }

            let mut diagnostic = Diagnostic::new(RewriteYieldFrom, Range::from_located(item.stmt));
            if checker.patch(diagnostic.kind.rule()) {
                let contents = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(item.iter));
                let contents = format!("yield from {contents}");
                diagnostic.amend(Fix::replacement(
                    contents,
                    item.stmt.location,
                    item.stmt.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
