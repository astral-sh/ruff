//! Type inference for `await` expressions and diagnostics for unused awaitables.
//!
//! Shared helpers check where `await` is permitted and construct fixes that add it.

use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::Ranged;
use ty_python_core::scope::{NodeWithScopeKind, ScopeKind};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::UNUSED_AWAITABLE;
use crate::types::function::KnownFunction;
use crate::types::{KnownClass, Type, TypeContext};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_await_expression(
        &mut self,
        await_expression: &ast::ExprAwait,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let env = self.program_environment();
        let ast::ExprAwait {
            range: _,
            node_index: _,
            value,
        } = await_expression;

        let expr_type = self.infer_expression(
            value,
            tcx.map(|tcx| KnownClass::Awaitable.to_specialized_instance(db, env, &[tcx])),
        );

        expr_type.try_await(db, env).unwrap_or_else(|err| {
            err.report_diagnostic(&self.context, expr_type, value.as_ref().into());
            Type::unknown()
        })
    }

    pub(super) fn check_unused_awaitable(&self, expression: &ast::Expr) {
        let db = self.db();
        let ty = self.expression_type(expression);
        if ty.is_awaitable(db)
            && !self.is_known_function_call(expression)
            && let Some(builder) = self.context.report_lint(&UNUSED_AWAITABLE, expression)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Object of type `{}` is not awaited",
                ty.display(db, self.program_environment()),
            ));
            if let Some(fix) = self.await_expression_fix(expression) {
                diagnostic.help("Did you mean to `await` this expression?");
                diagnostic.set_fix(fix);
            }
        }
    }

    /// Returns `true` if `expr` is a call to a known diagnostic function
    /// (e.g., `reveal_type` or `assert_type`) whose return value should not
    /// trigger the `unused-awaitable` lint.
    fn is_known_function_call(&self, expr: &ast::Expr) -> bool {
        let ast::Expr::Call(call) = expr else {
            return false;
        };
        matches!(
            self.expression_type(&call.func),
            Type::FunctionLiteral(f)
                if matches!(
                    f.known(self.db()),
                    Some(KnownFunction::RevealType | KnownFunction::AssertType)
                )
        )
    }

    /// Returns `true` if adding `await` at `expression` would produce valid Python.
    ///
    /// Accounts for asynchronous functions, notebook cells, annotation restrictions, enclosing
    /// scopes, and the different scoping behavior of comprehensions and generator expressions.
    pub(super) fn can_await_here(&self, expression: &ast::Expr) -> bool {
        let Some(expression_scope) = self.index.try_expression_scope_id(expression) else {
            return false;
        };
        let annotation_parent_scope = self
            .index
            .annotation_parent_scope_id(self.module(), expression);

        let db = self.db();

        let mut in_eager_comprehension = false;

        for (scope_id, scope) in self.index.ancestor_scopes(expression_scope) {
            // The first iterable of a comprehension stays in the annotation's enclosing scope.
            // Eager comprehensions also inherit the restriction, but a generator body can allow
            // `await` before we reach the scope enclosing its annotation.
            // Conservatively reject annotations on every Python version, even though some allow
            // `await` before Python 3.14 without `from __future__ import annotations`. Avoiding
            // invalid syntax matters more than offering every possible fix in this rare context.
            if Some(scope_id) == annotation_parent_scope {
                return false;
            }

            // Before Python 3.11, awaiting in a nested list, set, or dict comprehension cannot
            // implicitly make its containing comprehension or generator expression asynchronous.
            if in_eager_comprehension
                && scope.kind() == ScopeKind::Comprehension
                && self.program_environment().python_version(db) < PythonVersion::PY311
                && !scope_id.is_async_comprehension(self.index)
            {
                return false;
            }

            match scope.node() {
                NodeWithScopeKind::Function(function) => {
                    return function.node(self.module()).is_async;
                }
                NodeWithScopeKind::Lambda(_)
                | NodeWithScopeKind::Class(_)
                | NodeWithScopeKind::ClassTypeParameters(_)
                | NodeWithScopeKind::FunctionTypeParameters(_)
                | NodeWithScopeKind::TypeAliasTypeParameters(_)
                | NodeWithScopeKind::TypeAlias(_) => {
                    return false;
                }
                NodeWithScopeKind::GeneratorExpression(_) => {
                    return true;
                }
                NodeWithScopeKind::Module => {
                    return source_text(db, self.file()).is_notebook();
                }
                NodeWithScopeKind::DictComprehension(_)
                | NodeWithScopeKind::ListComprehension(_)
                | NodeWithScopeKind::SetComprehension(_) => {
                    in_eager_comprehension = true;
                }
            }
        }

        false
    }

    /// Suggest awaiting an expression, adding parentheses if its precedence requires them.
    pub(super) fn await_expression_fix(&self, expression: &ast::Expr) -> Option<Fix> {
        if !self.can_await_here(expression) {
            return None;
        }

        Some(
            if expression.precedence() <= ast::OperatorPrecedence::Await {
                Fix::unsafe_edits(
                    Edit::insertion("await (".to_string(), expression.start()),
                    [Edit::insertion(")".to_string(), expression.end())],
                )
            } else {
                Fix::unsafe_edit(Edit::insertion("await ".to_string(), expression.start()))
            },
        )
    }
}
