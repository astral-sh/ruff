//! Conditions that cannot change during a loop, even though their initial truthiness is unknown.

use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ty_python_core::definition::DefinitionKind;

use crate::types::{Type, diagnostic::INVARIANT_WHILE_CONDITION, infer::TypeInferenceBuilder};

impl TypeInferenceBuilder<'_, '_> {
    pub(super) fn check_invariant_while_condition(&self, statement: &ast::StmtWhile) {
        if !self.context.is_lint_enabled(&INVARIANT_WHILE_CONDITION)
            || !self
                .index
                .scope(self.scope().file_scope_id(self.db()))
                .kind()
                .is_function_like()
            || !self.condition_truthiness(&statement.test).is_ambiguous()
            || !self.is_invariant_loop_condition(&statement.test, statement)
        {
            return;
        }

        self.report_invariant_while_condition(statement);
    }

    fn is_invariant_loop_condition(
        &self,
        expression: &ast::Expr,
        statement: &ast::StmtWhile,
    ) -> bool {
        match expression {
            ast::Expr::Name(name) => {
                self.is_invariant_loop_name(name, statement)
                    && self.has_invariant_loop_value(self.expression_type(expression))
            }
            ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_) => true,
            ast::Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => {
                self.is_invariant_loop_condition(&unary.operand, statement)
            }
            ast::Expr::BoolOp(boolean) => boolean
                .values
                .iter()
                .all(|value| self.is_invariant_loop_condition(value, statement)),
            ast::Expr::Compare(compare) => {
                compare.iter().all(|(left, op, right)| match op {
                    // Identity does not invoke methods on either operand. Even mutable
                    // objects keep the same identity when their local binding is unchanged.
                    ast::CmpOp::Is | ast::CmpOp::IsNot => {
                        self.is_invariant_loop_reference(left, statement)
                            && self.is_invariant_loop_reference(right, statement)
                    }
                    ast::CmpOp::Eq
                    | ast::CmpOp::NotEq
                    | ast::CmpOp::Lt
                    | ast::CmpOp::LtE
                    | ast::CmpOp::Gt
                    | ast::CmpOp::GtE => {
                        self.is_invariant_loop_condition(left, statement)
                            && self.is_invariant_loop_condition(right, statement)
                    }
                    _ => false,
                })
            }
            _ => false,
        }
    }

    fn is_invariant_loop_reference(
        &self,
        expression: &ast::Expr,
        statement: &ast::StmtWhile,
    ) -> bool {
        match expression {
            ast::Expr::Name(name) => self.is_invariant_loop_name(name, statement),
            ast::Expr::NoneLiteral(_) | ast::Expr::BooleanLiteral(_) => true,
            _ => false,
        }
    }

    fn is_invariant_loop_name(&self, name: &ast::ExprName, statement: &ast::StmtWhile) -> bool {
        let db = self.db();
        let scope = self.scope().file_scope_id(db);
        let places = self.index.place_table(scope);
        let Some(symbol) = places.symbol_id(&name.id) else {
            return false;
        };
        if !places.symbol(symbol).is_local() {
            return false;
        }

        // Inspect all bindings, including nested functions defined inside the loop or after
        // its condition. Their nonlocal writes need not be visible at the condition's use.
        self.index
            .use_def_map(scope)
            .reachable_bindings(symbol.into())
            .filter_map(|binding| binding.binding.definition())
            .all(|definition| {
                let kind = definition.kind(db);
                match kind {
                    DefinitionKind::NestedBindings(nested) => nested
                        .visible_binding_sources(self.index, scope)
                        .next()
                        .is_none(),
                    // A loop header also covers deletion and other invalidations that do not
                    // create ordinary definitions. An enclosing loop's header is harmless.
                    _ => !statement
                        .range()
                        .contains_range(kind.target_range(self.module())),
                }
            })
    }

    fn has_invariant_loop_value(&self, ty: Type<'_>) -> bool {
        let db = self.db();
        let ty = ty.expand_top_level_aliases(db, self.program_environment());
        match ty {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| self.has_invariant_loop_value(*ty)),
            Type::LiteralValue(literal) => {
                literal.is_bool() || literal.is_int() || literal.is_string() || literal.is_bytes()
            }
            // Unlike int or str, bool cannot be subclassed to give it stateful truthiness
            // or comparison methods. Do not assume an arbitrary nominal type is immutable.
            _ => ty.is_bool(db) || ty.is_none(db),
        }
    }
}
