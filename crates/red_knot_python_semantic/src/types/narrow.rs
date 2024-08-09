use crate::semantic_index::ast_ids::HasScopedAstId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId, SymbolTable};
use crate::semantic_index::symbol_table;
use crate::types::{infer_expression_types, IntersectionTypeBuilder, Type, TypeInference};
use crate::Db;
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Return type constraint, if any, on `definition` applied by `test`.
pub(crate) fn narrowing_constraint<'db>(
    db: &'db dyn Db,
    test: Expression<'db>,
    definition: Definition<'db>,
) -> Option<Type<'db>> {
    all_narrowing_constraints(db, test)
        .get(&definition.symbol(db))
        .copied()
}

#[salsa::tracked]
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
        if let ast::Expr::Compare(expr_compare) = self.expression.node(self.db).node() {
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

    fn inference(&self) -> &TypeInference<'db> {
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
            for (op, comparator) in std::iter::zip(ops, comparators) {
                let comp_ty = self
                    .inference()
                    .expression_ty(comparator.scoped_ast_id(self.db, self.scope()));
                if matches!(op, ast::CmpOp::IsNot) {
                    let ty = IntersectionTypeBuilder::new(self.db)
                        .add_negative(comp_ty)
                        .build();
                    self.constraints.insert(symbol, ty);
                };
                // TODO other comparison types
            }
        }
    }
}
