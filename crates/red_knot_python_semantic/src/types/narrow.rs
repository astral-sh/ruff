use crate::semantic_index::ast_ids::HasScopedAstId;
use crate::semantic_index::constraint::{Constraint, PatternConstraint};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId, SymbolTable};
use crate::semantic_index::symbol_table;
use crate::types::{infer_expression_types, IntersectionBuilder, Type};
use crate::Db;
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Return the type constraint that `test` (if true) would place on `definition`, if any.
///
/// For example, if we have this code:
///
/// ```python
/// y = 1 if flag else None
/// x = 1 if flag else None
/// if x is not None:
///     ...
/// ```
///
/// The `test` expression `x is not None` places the constraint "not None" on the definition of
/// `x`, so in that case we'd return `Some(Type::Intersection(negative=[Type::None]))`.
///
/// But if we called this with the same `test` expression, but the `definition` of `y`, no
/// constraint is applied to that definition, so we'd just return `None`.
pub(crate) fn narrowing_constraint<'db>(
    db: &'db dyn Db,
    constraint: Constraint<'db>,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    match constraint {
        Constraint::Expression(expression) => {
            all_narrowing_constraints_for_expression(db, expression)
                .get(&definition.symbol(db))
                .copied()
        }
        Constraint::Pattern(pattern) => all_narrowing_constraints_for_pattern(db, pattern)
            .get(&definition.symbol(db))
            .copied(),
    }
}

#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternConstraint<'db>,
) -> NarrowingConstraints<'db> {
    NarrowingConstraintsBuilder::new(db, Constraint::Pattern(pattern)).finish()
}

#[salsa::tracked(return_ref)]
fn all_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> NarrowingConstraints<'db> {
    NarrowingConstraintsBuilder::new(db, Constraint::Expression(expression)).finish()
}

type NarrowingConstraints<'db> = FxHashMap<ScopedSymbolId, Type<'db>>;

struct NarrowingConstraintsBuilder<'db> {
    db: &'db dyn Db,
    constraint: Constraint<'db>,
    constraints: NarrowingConstraints<'db>,
}

impl<'db> NarrowingConstraintsBuilder<'db> {
    fn new(db: &'db dyn Db, constraint: Constraint<'db>) -> Self {
        Self {
            db,
            constraint,
            constraints: NarrowingConstraints::default(),
        }
    }

    fn finish(mut self) -> NarrowingConstraints<'db> {
        match self.constraint {
            Constraint::Expression(expression) => self.evaluate_expression_constraint(expression),
            Constraint::Pattern(pattern) => self.evaluate_pattern_constraint(pattern),
        }

        self.constraints.shrink_to_fit();
        self.constraints
    }

    fn evaluate_expression_constraint(&mut self, expression: Expression<'db>) {
        if let ast::Expr::Compare(expr_compare) = expression.node_ref(self.db).node() {
            self.add_expr_compare(expr_compare, expression);
        }
        // TODO other test expression kinds
    }

    fn evaluate_pattern_constraint(&mut self, pattern: PatternConstraint<'db>) {
        let subject = pattern.subject(self.db);

        match pattern.pattern(self.db).node() {
            ast::Pattern::MatchValue(_) => {
                // TODO
            }
            ast::Pattern::MatchSingleton(singleton_pattern) => {
                self.add_match_pattern_singleton(subject, singleton_pattern);
            }
            ast::Pattern::MatchSequence(_) => {
                // TODO
            }
            ast::Pattern::MatchMapping(_) => {
                // TODO
            }
            ast::Pattern::MatchClass(_) => {
                // TODO
            }
            ast::Pattern::MatchStar(_) => {
                // TODO
            }
            ast::Pattern::MatchAs(_) => {
                // TODO
            }
            ast::Pattern::MatchOr(_) => {
                // TODO
            }
        }
    }

    fn symbols(&self) -> Arc<SymbolTable> {
        symbol_table(self.db, self.scope())
    }

    fn scope(&self) -> ScopeId<'db> {
        match self.constraint {
            Constraint::Expression(expression) => expression.scope(self.db),
            Constraint::Pattern(pattern) => pattern.scope(self.db),
        }
    }

    fn add_expr_compare(&mut self, expr_compare: &ast::ExprCompare, expression: Expression<'db>) {
        let ast::ExprCompare {
            range: _,
            left,
            ops,
            comparators,
        } = expr_compare;

        if let ast::Expr::Name(ast::ExprName {
            range: _,
            id,
            ctx: _,
        }) = left.as_ref()
        {
            // SAFETY: we should always have a symbol for every Name node.
            let symbol = self.symbols().symbol_id_by_name(id).unwrap();
            let scope = self.scope();
            let inference = infer_expression_types(self.db, expression);
            for (op, comparator) in std::iter::zip(&**ops, &**comparators) {
                let comp_ty = inference.expression_ty(comparator.scoped_ast_id(self.db, scope));
                if matches!(op, ast::CmpOp::IsNot) {
                    let ty = IntersectionBuilder::new(self.db)
                        .add_negative(comp_ty)
                        .build();
                    self.constraints.insert(symbol, ty);
                };
                // TODO other comparison types
            }
        }
    }

    fn add_match_pattern_singleton(
        &mut self,
        subject: &ast::Expr,
        pattern: &ast::PatternMatchSingleton,
    ) {
        if let Some(ast::ExprName { id, .. }) = subject.as_name_expr() {
            // SAFETY: we should always have a symbol for every Name node.
            let symbol = self.symbols().symbol_id_by_name(id).unwrap();

            let ty = match pattern.value {
                ast::Singleton::None => Type::None,
                ast::Singleton::True => Type::BooleanLiteral(true),
                ast::Singleton::False => Type::BooleanLiteral(false),
            };
            self.constraints.insert(symbol, ty);
        }
    }
}
