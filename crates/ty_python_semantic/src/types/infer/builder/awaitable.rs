//! Type inference for `await` expressions and diagnostics for unused awaitables.
//!
//! Shared helpers check where `await` is permitted and construct fixes that add it.

use std::convert::Infallible;

use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::Ranged;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::scope::{NodeWithScopeKind, ScopeKind};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::UNUSED_AWAITABLE;
use crate::types::function::KnownFunction;
use crate::types::{
    IntersectionType, KnownClass, NominalInstanceType, Type, TypeContext, UnionType,
};

struct OrdinaryAwaitableEffects;

shared_semantic_family! {
    #[synchronous(SynchronousAwaitableEffects)]
    pub(super) trait AwaitableEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn expression_type(&self, builder: &TypeInferenceBuilder<'db, '_>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn is_awaitable(&self, builder: &TypeInferenceBuilder<'db, '_>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn nominal_awaitable(&self, builder: &TypeInferenceBuilder<'db, '_>, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn union_awaitable(&self, builder: &TypeInferenceBuilder<'db, '_>, union: UnionType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn intersection_awaitable(&self, builder: &TypeInferenceBuilder<'db, '_>, intersection: IntersectionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_known_function_call(&self, builder: &TypeInferenceBuilder<'db, '_>, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn report_unused_awaitable(&self, builder: &TypeInferenceBuilder<'db, '_>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    /// Classify types whose values must be awaited before being discarded.
    ///
    /// Only `types.CoroutineType` instances are directly awaitable here. Unions require every
    /// element to be awaitable; intersections require at least one positive element to be awaitable.
    #[synchronous(is_awaitable_sync)]
    #[capabilities(effects = AwaitableEffects)]
    #[passive_values()]
    pub(super) async fn is_awaitable_with<'db, E: AwaitableEffects<'db>>(
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::NominalInstance(instance) => effects.nominal_awaitable(builder, instance).await,
            Type::Union(union) => effects.union_awaitable(builder, union).await,
            Type::Intersection(intersection) => effects.intersection_awaitable(builder, intersection).await,
            _ => Ok(false),
        }
    }

    #[synchronous(check_unused_awaitable_sync)]
    #[capabilities(effects = AwaitableEffects)]
    #[passive_values()]
    pub(super) async fn check_unused_awaitable_with<'db, E: AwaitableEffects<'db>>(
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        effects: &E,
    ) -> Result<(), E::Error> {
        let ty = effects.expression_type(builder, expression).await?;
        if effects.is_awaitable(builder, ty).await?
            && !effects.is_known_function_call(builder, expression).await?
        {
            effects.report_unused_awaitable(builder, expression, ty).await?;
        }
        Ok(())
    }
}

impl<'db> SynchronousAwaitableEffects<'db> for OrdinaryAwaitableEffects {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.expression_type(expression))
    }

    fn is_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        is_awaitable_sync(builder, ty, self)
    }

    fn nominal_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(matches!(
            instance.known_class(builder.db()),
            Some(KnownClass::CoroutineType)
        ))
    }

    fn union_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        union: UnionType<'db>,
    ) -> Result<bool, Self::Error> {
        let elements = union.elements(builder.db());
        // Guard against empty unions (`Never`), since `all()` on an empty
        // iterator returns `true`.
        Ok(!elements.is_empty()
            && elements.iter().all(|ty| {
                let Ok(awaitable) = is_awaitable_sync(builder, *ty, self);
                awaitable
            }))
    }

    fn intersection_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        intersection: IntersectionType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(intersection.positive(builder.db()).iter().any(|ty| {
            let Ok(awaitable) = is_awaitable_sync(builder, *ty, self);
            awaitable
        }))
    }

    fn is_known_function_call(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<bool, Self::Error> {
        Ok(builder.is_known_function_call(expression))
    }

    fn report_unused_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        if let Some(diagnostic) = builder.context.report_lint(&UNUSED_AWAITABLE, expression) {
            let mut diagnostic = diagnostic.into_diagnostic(format_args!(
                "Object of type `{}` is not awaited",
                ty.display(builder.db(), builder.program_environment()),
            ));
            if let Some(fix) = builder.await_expression_fix(expression) {
                diagnostic.help("Did you mean to `await` this expression?");
                diagnostic.set_fix(fix);
            }
        }
        Ok(())
    }
}

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
        let Ok(()) = check_unused_awaitable_sync(self, expression, &OrdinaryAwaitableEffects);
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
