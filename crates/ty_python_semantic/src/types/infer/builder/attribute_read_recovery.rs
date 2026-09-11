use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use rustc_hash::FxHashSet;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::{NodeWithScopeKind, ScopeId};

use crate::Db;

/// Find simple reads following an assignment to the same attribute in a straight-line block.
/// These reads can reuse an existing assignment's recovery type without repeating the diagnostic
/// for a missing attribute. The caller must still check that the assignment is definitely bound.
///
/// This query only controls diagnostics; it does not establish member presence. Calls, other
/// writes, complex expressions, and control-flow boundaries end recovery. Computing eligibility
/// from syntax keeps it independent of the order in which inference queries run.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn eligible_reads(db: &dyn Db, scope: ScopeId<'_>) -> FxHashSet<ExpressionNodeKey> {
    let module = parsed_module(db, scope.python_file(db)).load(db);
    let body = match scope.node(db) {
        NodeWithScopeKind::Module => &module.syntax().body,
        NodeWithScopeKind::Function(function) => &function.node(&module).body,
        NodeWithScopeKind::Class(class) => &class.node(&module).body,
        _ => return FxHashSet::default(),
    };
    let mut visitor = AttributeReadRecovery::default();
    visitor.visit_body(body);
    visitor.reads.shrink_to_fit();
    visitor.reads
}

#[derive(Default)]
struct AttributeReadRecovery {
    assignment: Option<PlaceExpr>,
    reads: FxHashSet<ExpressionNodeKey>,
}

impl<'ast> Visitor<'ast> for AttributeReadRecovery {
    fn visit_body(&mut self, body: &'ast [ast::Stmt]) {
        self.assignment = None;
        for statement in body {
            self.visit_stmt(statement);
        }
        self.assignment = None;
    }

    fn visit_stmt(&mut self, statement: &'ast ast::Stmt) {
        match statement {
            ast::Stmt::Assign(assignment) => {
                self.visit_expr(&assignment.value);
                self.assignment = match assignment.targets.as_slice() {
                    [target @ ast::Expr::Attribute(_)] => PlaceExpr::try_from_expr(target),
                    _ => None,
                };
            }
            ast::Stmt::Expr(statement) => self.visit_expr(&statement.value),
            ast::Stmt::Return(statement) => {
                if let Some(value) = &statement.value {
                    self.visit_expr(value);
                }
                self.assignment = None;
            }
            ast::Stmt::ClassDef(_) | ast::Stmt::FunctionDef(_) => self.assignment = None,
            _ => {
                self.assignment = None;
                walk_stmt(self, statement);
                self.assignment = None;
            }
        }
    }

    fn visit_expr(&mut self, expression: &'ast ast::Expr) {
        match expression {
            ast::Expr::Attribute(attribute) => {
                self.visit_expr(&attribute.value);
                if attribute.ctx.is_load()
                    && let Some(assignment) = &self.assignment
                    && PlaceExpr::try_from_expr(expression).as_ref() == Some(assignment)
                {
                    self.reads.insert(expression.into());
                }
            }
            ast::Expr::Call(_) => {
                // Arguments are evaluated before the call can invalidate the assignment.
                walk_expr(self, expression);
                self.assignment = None;
            }
            ast::Expr::Name(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::EllipsisLiteral(_) => {}
            _ => self.assignment = None,
        }
    }
}
