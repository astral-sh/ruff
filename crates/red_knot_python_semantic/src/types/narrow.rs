use crate::semantic_index::ast_ids::HasScopedAstId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId, SymbolTable};
use crate::semantic_index::symbol_table;
use crate::types::{infer_expression_types, IntersectionBuilder, Type, TypeInference};
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
    test: Expression<'db>,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    all_narrowing_constraints(db, test)
        .get(&definition.symbol(db))
        .copied()
}

#[salsa::tracked(return_ref)]
fn all_narrowing_constraints<'db>(
    db: &'db dyn Db,
    test: Expression<'db>,
) -> NarrowingConstraints<'db> {
    NarrowingConstraintsBuilder::new(db, test).finish()
}

type NarrowingConstraints<'db> = FxHashMap<ScopedSymbolId, Type<'db>>;

struct NarrowingConstraintsBuilder<'db> {
    db: &'db dyn Db,
    expression: Expression<'db>,
    constraints: NarrowingConstraints<'db>,
}

impl<'db> NarrowingConstraintsBuilder<'db> {
    fn new(db: &'db dyn Db, expression: Expression<'db>) -> Self {
        Self {
            db,
            expression,
            constraints: NarrowingConstraints::default(),
        }
    }

    fn finish(mut self) -> NarrowingConstraints<'db> {
        if let ast::Expr::Compare(expr_compare) = self.expression.node_ref(self.db).node() {
            self.add_expr_compare(expr_compare);
        }
        // TODO other test expression kinds

        self.constraints.shrink_to_fit();
        self.constraints
    }

    fn symbols(&self) -> Arc<SymbolTable> {
        symbol_table(self.db, self.scope())
    }

    fn scope(&self) -> ScopeId<'db> {
        self.expression.scope(self.db)
    }

    fn inference(&self) -> &'db TypeInference<'db> {
        infer_expression_types(self.db, self.expression)
    }

    fn add_expr_compare(&mut self, expr_compare: &ast::ExprCompare) {
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
            let inference = self.inference();
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
}
