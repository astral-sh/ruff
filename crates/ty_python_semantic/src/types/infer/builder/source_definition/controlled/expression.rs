//! Place loads and their unpublished resolver/cache storage.

use std::ops::ControlFlow;

use ruff_python_ast as ast;
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExpr, PlaceExprRef, ScopedPlaceId};
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::source_effects::SourcePlaceEffects;
use crate::place::{
    ConsideredDefinitions, LookupError, LookupResult, Place, PlaceAndQualifiers,
    RequiresExplicitReExport, place_from_bindings_with,
};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadMode, PlaceLoadResolution, PlaceLoadResolutionStep,
    place_load_construction_work, resolve_place_load_with_scope_fields,
};
use crate::types::{KnownClass, Type};
use crate::types::class::ClassLiteral;
use crate::types::function::FunctionType;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::applicable_constraints::{
    ApplicableConstraintsFacts, narrow_place_with_applicable_constraints_with,
};
use crate::types::infer::builder::attribute::{AttributeFacts, infer_attribute_expression_with};
use crate::types::infer::builder::chained_comparison::{ChainFacts, ChainInput, infer_chain_with};
use crate::types::infer::builder::number_literal::{NumberLiteralFacts, infer_number_literal_with};
use crate::types::infer::builder::source_binding::SourceBindingEffects;
use crate::types::infer::builder::source_expression::{
    SourceExpressionEffects, SourceExpressionOperation, SourceExpressionWork, sealed,
};
use crate::types::infer::builder::string_literal::{StringLiteralFacts, infer_string_literal_with};
use crate::types::infer::builder::tuple_expression::{
    TupleExpressionFacts, infer_tuple_expression_with,
};
use crate::types::known_instance::DeprecatedInstance;
use ty_python_core::ExpressionNodeKey;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceExpressionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn narrow_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExprRef<'_>,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> RunResult<Type<'db>> {
        narrow_place_with_applicable_constraints_with(
            builder,
            place,
            ty,
            constraints,
            ApplicableConstraintsFacts,
            self,
        )
        .await
    }

    async fn number_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        literal: &ast::ExprNumberLiteral,
    ) -> RunResult<Type<'db>> {
        infer_number_literal_with(builder, literal, NumberLiteralFacts, self).await
    }

    async fn ellipsis_literal(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(builder.program_environment()).await?;
        self.access
            .known_class_instance(program, KnownClass::EllipsisType)
            .await
    }

    async fn string_literal(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        literal: &ast::ExprStringLiteral,
        context: crate::types::TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        infer_string_literal_with(builder, literal, context, StringLiteralFacts, self).await
    }

    async fn tuple_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        tuple: &ast::ExprTuple,
        context: crate::types::TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        infer_tuple_expression_with(builder, tuple, context, TupleExpressionFacts, self).await
    }

    async fn attribute_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        attribute: &ast::ExprAttribute,
    ) -> RunResult<Type<'db>> {
        infer_attribute_expression_with(builder, attribute, AttributeFacts, self).await
    }

    async fn chained_comparison(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        input: ChainInput<'db, '_>,
    ) -> RunResult<Type<'db>> {
        infer_chain_with(builder, input, ChainFacts, self).await
    }

    async fn contextual_type_form(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        crate::types::infer::builder::type_form::contextual_type_form_with(
            builder, expression, target, self,
        )
        .await
    }

    async fn apply_type_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: crate::types::TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        crate::types::infer::builder::source_expression::apply_type_context_with(
            builder, expression, ty, tcx, self,
        )
        .await
    }

    async fn store_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let entries = builder.expressions.len();
        let capacity = builder.expressions.capacity();
        let work = Self::checked(capacity.checked_mul(4).and_then(|n| n.checked_add(16)))?;
        let bytes = if entries == capacity {
            Self::checked(
                entries
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(4))
                    .and_then(|n| n.checked_mul(size_of::<(ExpressionNodeKey, Type<'db>)>() + 1))
                    .and_then(|n| n.checked_add(64)),
            )?
        } else {
            0
        };
        let bytes = Self::checked(bytes.checked_add(size_of::<(ExpressionNodeKey, Type<'db>)>()))?;
        self.local(work, bytes, || {
            builder.expressions.reserve(1);
            builder.store_expression_type(expression, ty);
            #[cfg(test)]
            super::observations::observe(builder.db(), super::observations::Event::Stored);
            #[cfg(test)]
            if expression.is_call_expr() {
                super::observations::observe(builder.db(), super::observations::Event::CallStored);
            }
        })
        .await
    }

    async fn checkpoint(&self, work: SourceExpressionWork) -> RunResult<()> {
        let units = match work {
            SourceExpressionWork::Expression
            | SourceExpressionWork::BoundMethod
            | SourceExpressionWork::PlaceSource => 1,
            SourceExpressionWork::NameBytes(bytes) => Self::checked(bytes.checked_add(3))?,
            SourceExpressionWork::StoreExpression { .. } => {
                return self.unavailable(SourceOperation::ExpressionStorage).await;
            }
            SourceExpressionWork::NarrowingConstraints(_) => {
                return self.unavailable(SourceOperation::Narrowing).await;
            }
        };
        self.work(units).await
    }

    async fn legacy_operation<T>(
        &self,
        operation: SourceExpressionOperation,
        _body: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.unavailable(SourceOperation::Expression(operation))
            .await
    }

    async fn prepare_place_resolution<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExpr,
        mode: PlaceLoadMode<'expr>,
    ) -> RunResult<PlaceLoadResolution<'db, 'expr>> {
        let db = builder.db();
        let scope = builder.scope();
        let file = self.local(1, 0, || builder.program_file()).await?;
        self.check_file_program(file).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let units = Self::checked(place_load_construction_work(
            builder.index,
            file_scope,
            &place,
        ))?;
        self.local(units, 0, || {
            resolve_place_load_with_scope_fields(
                builder.index,
                scope,
                file,
                file_scope,
                place,
                mode,
            )
        })
        .await
    }

    async fn next_place_resolution(
        &self,
        resolution: &mut PlaceLoadResolution<'db, '_>,
    ) -> RunResult<Option<PlaceLoadResolutionStep<'db>>> {
        loop {
            let mut admission = resolution
                .prepare_next()
                .ok_or(RunError::Contract("place resolver quotation overflow"))?;
            while admission.needs_measurement() {
                let work = admission.measurement_work();
                self.local(work, 0, || admission.measure_next())
                    .await?
                    .ok_or(RunError::Contract("place resolver measurement overflow"))?;
            }
            let (work, bytes) = admission.cost().ok_or(RunError::Contract(
                "place resolver storage quotation overflow",
            ))?;
            let progress = self
                .local(work, bytes, || {
                    let progress = admission.reserve().advance();
                    #[cfg(test)]
                    if let ControlFlow::Break(step) = &progress {
                        super::observations::observe(
                            self.db(),
                            super::observations::Event::PlaceResolutionStep,
                        );
                        if matches!(
                            step,
                            Some(PlaceLoadResolutionStep::MemberResolutionCondition(_))
                        ) {
                            super::observations::observe(
                                self.db(),
                                super::observations::Event::PlacePrefixesReady,
                            );
                        }
                    }
                    progress
                })
                .await?;
            if let ControlFlow::Break(step) = progress {
                return Ok(step);
            }
        }
    }

    async fn bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> RunResult<Place<'db>> {
        let units = Self::checked(
            bindings
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(4)),
        )?;
        self.work(units).await?;
        let cache = SourceBindingEffects::reachability_cache(self, builder).await?;
        Ok(place_from_bindings_with(
            builder.program_environment(),
            self,
            bindings,
            RequiresExplicitReExport::No,
            Some(cache),
        )
        .await?
        .place)
    }

    async fn owning_scope_symbol(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: ScopeId<'db>,
        id: ScopedPlaceId,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        SourcePlaceEffects::place_by_id(
            self,
            builder.db(),
            scope,
            id,
            RequiresExplicitReExport::No,
            ConsideredDefinitions::AllReachable,
        )
        .await
    }

    async fn implicit_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        implicit: ImplicitPlaceLoad<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.infer_implicit_place(builder, implicit).await
    }

    async fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        self.work(1).await?;
        place
            .into_lookup_result_with(builder.db(), builder.program_environment(), self)
            .await
    }

    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        primary: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.work(1).await?;
        Ok(primary
            .or_fall_back_to_with(builder.db(), builder.program_environment(), self, fallback)
            .await?
            .into())
    }

    async fn class_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<DeprecatedInstance<'db>>> {
        let db = builder.db();
        let file = self.class_file(class).await?;
        self.check_file_program(file).await?;
        let Some(class) = class.as_static() else {
            return Ok(None);
        };
        self.field(class.field_requests(db).deprecated()).await
    }

    async fn function_deprecation(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        function: FunctionType<'db>,
    ) -> RunResult<Option<DeprecatedInstance<'db>>> {
        self.work(2).await?;
        function
            .implementation_deprecated_with(self.db(), self)
            .await
    }
}
