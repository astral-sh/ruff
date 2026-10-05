//! Contextual expression inference shares value dispatch and expression storage with its owner.

use std::convert::Infallible;

use super::*;
use crate::types::infer::builder::source_expression::{
    ApplyTypeContextEffects, ContextualExpressionEffects, OrdinaryApplyTypeContextEffects,
    SourceExpressionEffects, SynchronousApplyTypeContextEffects,
};
use crate::types::infer::builder::type_form::{
    OrdinaryTypeFormEffects, SynchronousTypeFormEffects, TypeFormEffects, contextual_type_form_with,
};
use crate::types::literal::LiteralValueType;
use crate::types::type_alias::AliasResolutionStep;

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn resolve_context_alias(
        &self,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        match self.local(1, 0, || ty.alias_resolution_step()).await? {
            AliasResolutionStep::Resolved(ty) => Ok(ty),
            AliasResolutionStep::Alias(_) | AliasResolutionStep::UnboundRecursiveVariable => {
                self.unavailable(SourceOperation::TypeAliasResolution).await
            }
            AliasResolutionStep::Recursive(_) => {
                self.unavailable(SourceOperation::RecursiveTypeUnfold).await
            }
        }
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ContextualExpressionEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn contextual(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        contextual_type_form_with(builder, expression, target, self).await
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        SourceExpressionEffects::store_expression(self, builder, expression, ty).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> TypeFormEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn resolve_alias(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.resolve_context_alias(ty).await
    }

    async fn union_elements(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        union: UnionType<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn next_element(
        &self,
        elements: &[Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(2, 0, || {
            infallible(SynchronousTypeFormEffects::next_element(
                &OrdinaryTypeFormEffects,
                elements,
                cursor,
            ))
        })
        .await
    }

    async fn non_type_form_fallback(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _target: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ContextualTypeFormFallback)
            .await
    }

    async fn positive_interpretation(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _target: Type<'db>,
        _fallback: Option<Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ContextualTypeFormPositive)
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ApplyTypeContextEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn resolve_alias(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.resolve_context_alias(ty).await
    }

    async fn filter_literal_union(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _union: UnionType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ContextualLiteralFilter)
            .await
    }

    async fn is_assignable(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ContextualLiteralAssignability)
            .await
    }

    async fn unpromotable_literal(&self, literal: LiteralValueType<'db>) -> RunResult<Type<'db>> {
        self.local(1, 0, || {
            infallible(SynchronousApplyTypeContextEffects::unpromotable_literal(
                &OrdinaryApplyTypeContextEffects,
                literal,
            ))
        })
        .await
    }

    async fn collection_initializer(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Option<Definition<'db>>> {
        // FrozenMap uses binary search over fixed-size node keys. The address space bounds the
        // number of comparisons independently of the number of entries in this source file.
        self.local(usize::BITS as usize * 4 + 8, 0, || {
            infallible(SynchronousApplyTypeContextEffects::collection_initializer(
                &OrdinaryApplyTypeContextEffects,
                builder,
                expression,
            ))
        })
        .await
    }

    async fn insert_collection_constraint(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
        _target: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::CollectionConstraintStorage)
            .await
    }
}
