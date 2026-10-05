//! Expression and lexical-load semantics shared by immediate and suspended source inference.
//!
//! The owning definition transaction retains expression storage and diagnostics until completion.
//! Effects expose the source and semantic reads that can suspend; unavailable work never becomes
//! an undefined binding or an inferred `Unknown`.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_python_ast::{self as ast, ExprContext};
use ruff_text_size::Ranged;
use salsa::execution_probe::FieldRequest;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExpr, PlaceExprRef, ScopedPlaceId};
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::TypeInferenceBuilder;
use super::chained_comparison::ChainInput;
use crate::place::{
    ConsideredDefinitions, Definedness, LookupError, LookupResult, Place, PlaceAndQualifiers,
    RequiresExplicitReExport, TypeOrigin, place_by_id, place_from_bindings_with_reachability_cache,
};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceExprPrefixLoad, PlaceExprPrefixLoads, PlaceLoadFailure, PlaceLoadMode,
    PlaceLoadResolution, PlaceLoadResolutionStep, PlaceLoadSource, PlaceLoadSourceKind,
    resolve_place_load,
};
use crate::types::class::ClassLiteral;
use crate::types::diagnostic::{self, report_possibly_unresolved_reference};
use crate::types::function::FunctionType;
use crate::types::infer::builder::implicit_place::{
    ImplicitPlaceFacts, InlineImplicitPlaceEffects, implicit_place_sync,
};
use crate::types::known_instance::DeprecatedInstance;
use crate::types::literal::LiteralValueType;
use crate::types::{Type, TypeAndQualifiers, TypeContext, UnionType, todo_type};

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

/// Legacy operations whose semantic prerequisites are not yet supplied by the queued producer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum SourceExpressionOperation {
    ExpressionCache,
    ExpressionKind,
    Call,
    ContextualTypeForm,
    ContextualClassSpecialization,
    ContextualExpressionFinish,
    StringLiteralExpectedType,
    StringTypeAlias,
    Narrowing,
    RevealTypeFallback,
    UnresolvedReference,
    PossiblyUnresolvedReference,
    DeprecationDiagnostic,
    CallableDeprecation,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum SourceExpressionWork {
    Expression,
    StoreExpression { entries: usize, capacity: usize },
    BoundMethod,
    NameBytes(usize),
    PlaceSource,
    NarrowingConstraints(usize),
}

/// Required dependencies of shared expression dispatch and lexical place loads.
///
/// The legacy continuation is deliberately mandatory: a queued provider rejects an unsupported
/// operation without running its ordinary synchronous body.
pub(in crate::types::infer) trait SourceExpressionEffects<'db>:
    sealed::Sealed
{
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn checkpoint(&self, work: SourceExpressionWork) -> Result<(), Self::Error>;

    async fn store_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;

    async fn attribute_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        attribute: &ast::ExprAttribute,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || {
            builder.infer_attribute_expression(attribute)
        })
        .await
    }

    async fn number_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        literal: &ast::ExprNumberLiteral,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || {
            builder.infer_number_literal_expression(literal)
        })
        .await
    }

    /// Infer the type of an ellipsis literal, including in homogeneous tuple annotations.
    async fn ellipsis_literal(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        literal: &ast::ExprEllipsisLiteral,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || {
            builder.infer_ellipsis_literal_expression(literal)
        })
        .await
    }

    async fn string_literal(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        literal: &ast::ExprStringLiteral,
        context: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || {
            builder.infer_string_literal_expression(literal, context)
        })
        .await
    }

    async fn tuple_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        tuple: &ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || {
            builder.infer_tuple_expression(tuple, context)
        })
        .await
    }

    async fn chained_comparison(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        input: ChainInput<'db, '_>,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ExpressionKind, || match input {
            ChainInput::Boolean {
                expression,
                context,
            } => builder.infer_boolean_expression(expression, context),
            ChainInput::Comparison(expression) => builder.infer_compare_expression(expression),
        })
        .await
    }

    async fn contextual_type_form(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.legacy_operation(SourceExpressionOperation::ContextualTypeForm, || {
            builder.infer_type_form_contextual_expression(expression, target)
        })
        .await
    }

    async fn apply_type_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.legacy_operation(
            SourceExpressionOperation::ContextualExpressionFinish,
            || builder.apply_type_context(expression, ty, tcx),
        )
        .await
    }

    async fn specialize_class_context(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        target: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        specialize_class_context_with(
            builder,
            ty,
            target,
            &SourceClassContextEffects { effects: self },
        )
        .await
    }

    async fn legacy_operation<T>(
        &self,
        operation: SourceExpressionOperation,
        body: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn prepare_place_resolution<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExpr,
        mode: PlaceLoadMode<'expr>,
    ) -> Result<PlaceLoadResolution<'db, 'expr>, Self::Error>;

    /// Admits the prepared scope/use-def walk before the iterator advances.
    async fn next_place_resolution(
        &self,
        resolution: &mut PlaceLoadResolution<'db, '_>,
    ) -> Result<Option<PlaceLoadResolutionStep<'db>>, Self::Error>;

    async fn bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Result<Place<'db>, Self::Error>;

    async fn owning_scope_symbol(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: ScopeId<'db>,
        id: ScopedPlaceId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn implicit_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        implicit: ImplicitPlaceLoad<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn narrow_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExprRef<'_>,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> Result<Type<'db>, Self::Error> {
        self.checkpoint(SourceExpressionWork::NarrowingConstraints(
            constraints.len(),
        ))
        .await?;
        self.legacy_operation(SourceExpressionOperation::Narrowing, || {
            builder.narrow_place_with_applicable_constraints(place, ty, constraints)
        })
        .await
    }

    /// Includes any public-type promotion required before observing a binding's value.
    async fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error>;

    /// Includes fallback public-type promotion and relation-bearing union construction.
    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        primary: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn class_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<DeprecatedInstance<'db>>, Self::Error>;

    async fn function_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        function: FunctionType<'db>,
    ) -> Result<Option<DeprecatedInstance<'db>>, Self::Error>;
}

pub(in crate::types::infer) struct LegacySourceExpressionEffects;

impl sealed::Sealed for LegacySourceExpressionEffects {}

impl<'db> SourceExpressionEffects<'db> for LegacySourceExpressionEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn specialize_class_context(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        target: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.specialize_generic_class_from_context(ty, target))
    }

    async fn store_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.store_expression_type(expression, ty);
        Ok(())
    }

    fn checkpoint(
        &self,
        _work: SourceExpressionWork,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn legacy_operation<T>(
        &self,
        _operation: SourceExpressionOperation,
        body: impl FnOnce() -> T,
    ) -> impl Future<Output = Result<T, Self::Error>> {
        ready(Ok(body()))
    }

    fn prepare_place_resolution<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExpr,
        mode: PlaceLoadMode<'expr>,
    ) -> impl Future<Output = Result<PlaceLoadResolution<'db, 'expr>, Self::Error>> {
        ready(Ok(resolve_place_load(
            builder.db(),
            builder.index,
            builder.scope(),
            place,
            mode,
        )))
    }

    fn next_place_resolution(
        &self,
        resolution: &mut PlaceLoadResolution<'db, '_>,
    ) -> impl Future<Output = Result<Option<PlaceLoadResolutionStep<'db>>, Self::Error>> {
        ready(Ok(resolution.next()))
    }

    fn bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(Ok(place_from_bindings_with_reachability_cache(
            builder.db(),
            builder.program_environment(),
            bindings,
            builder.reachability_cache(),
        )
        .place))
    }

    fn owning_scope_symbol(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: ScopeId<'db>,
        id: ScopedPlaceId,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Ok(place_by_id(
            builder.db(),
            scope,
            id,
            RequiresExplicitReExport::No,
            ConsideredDefinitions::AllReachable,
        )))
    }

    fn implicit_place(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        implicit: ImplicitPlaceLoad<'db>,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(implicit_place_sync(
            builder.scope(),
            implicit,
            &InlineImplicitPlaceEffects {
                db: builder.db(),
                env: builder.program_environment(),
            },
            ImplicitPlaceFacts,
        ))
    }

    fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceAndQualifiers<'db>,
    ) -> impl Future<Output = Result<LookupResult<'db>, Self::Error>> {
        ready(Ok(place.into_lookup_result(
            builder.db(),
            builder.program_environment(),
        )))
    }

    fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        primary: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Ok(primary
            .or_fall_back_to(builder.db(), builder.program_environment(), fallback)
            .into()))
    }

    fn class_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<Option<DeprecatedInstance<'db>>, Self::Error>> {
        ready(Ok(class.deprecated(builder.db())))
    }

    fn function_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        function: FunctionType<'db>,
    ) -> impl Future<Output = Result<Option<DeprecatedInstance<'db>>, Self::Error>> {
        ready(Ok(function.implementation_deprecated(builder.db())))
    }
}

pub(super) enum ContextualExpressionResult<'db> {
    Complete(Type<'db>),
    Value,
}

pub(super) struct OrdinaryContextualExpressionEffects;

pub(super) struct OrdinaryApplyTypeContextEffects;

pub(super) struct OrdinaryClassContextEffects;

shared_semantic_family! {
    #[synchronous(SynchronousClassContextEffects)]
    pub(super) trait ClassContextEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn specialize_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: ClassLiteral<'db>, target: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(specialize_class_context_sync)]
    #[capabilities(effects = ClassContextEffects)]
    #[passive_values()]
    pub(super) async fn specialize_class_context_with<'db, 'ast, E: ClassContextEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if let Type::ClassLiteral(class) = ty {
            effects.specialize_class(builder, class, target).await
        } else {
            Ok(ty)
        }
    }
}

impl<'db, 'ast> SynchronousClassContextEffects<'db, 'ast> for OrdinaryClassContextEffects {
    type Error = Infallible;

    fn specialize_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
        target: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.specialize_class_literal_from_context(class, target))
    }
}

struct SourceClassContextEffects<'effects, E: ?Sized> {
    effects: &'effects E,
}

impl<'db, 'ast, E: SourceExpressionEffects<'db> + ?Sized> ClassContextEffects<'db, 'ast>
    for SourceClassContextEffects<'_, E>
{
    type Error = E::Error;

    async fn specialize_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
        target: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects
            .legacy_operation(
                SourceExpressionOperation::ContextualClassSpecialization,
                || builder.specialize_class_literal_from_context(class, target),
            )
            .await
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousApplyTypeContextEffects)]
    pub(super) trait ApplyTypeContextEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn resolve_alias(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn filter_literal_union(&self, builder: &TypeInferenceBuilder<'db, 'ast>, union: UnionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn is_assignable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn unpromotable_literal(&self, literal: LiteralValueType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn collection_initializer(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(local)]
        async fn insert_collection_constraint(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, target: Type<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(apply_type_context_sync)]
    #[capabilities(effects = ApplyTypeContextEffects)]
    #[passive_values()]
    pub(super) async fn apply_type_context_with<'db, 'ast, E: ApplyTypeContextEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        #[passive_state]
        let mut ty = ty;
        // Explicit literal annotations keep their values from being promoted in later inference.
        if let Type::LiteralValue(literal) = ty
            && let Some(target) = tcx.annotation
        {
            let target = effects.resolve_alias(builder, target).await?;
            let filtered = match effects.resolve_alias(builder, target).await? {
                Type::Union(union) => effects.filter_literal_union(builder, union).await?,
                _ => target,
            };
            if let literal_tcx @ (Type::Union(_) | Type::LiteralValue(_)) = filtered
                && effects.is_assignable(builder, ty, literal_tcx).await?
            {
                ty = effects.unpromotable_literal(literal).await?;
            }
        }

        if let Some(target) = tcx.annotation
            && let Some(definition) = effects.collection_initializer(builder, expression).await?
        {
            effects.insert_collection_constraint(builder, definition, target).await?;
        }
        Ok(ty)
    }
}

impl<'db, 'ast> SynchronousApplyTypeContextEffects<'db, 'ast> for OrdinaryApplyTypeContextEffects {
    type Error = Infallible;

    fn resolve_alias(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(builder.db()))
    }

    fn filter_literal_union(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        union: UnionType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(
            union.filter_expanding_aliases(builder.db(), builder.program_environment(), |ty| {
                ty.as_literal_value().is_some()
            }),
        )
    }

    fn is_assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(ty.is_assignable_to(builder.db(), builder.program_environment(), target))
    }

    fn unpromotable_literal(
        &self,
        literal: LiteralValueType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::LiteralValue(literal.to_unpromotable()))
    }

    fn collection_initializer(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(builder.index.unannotated_collection_initializer(expression))
    }

    fn insert_collection_constraint(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        target: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .collection_use_constraints
            .entry(definition)
            .or_default()
            .insert(target);
        Ok(())
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousContextualExpressionEffects)]
    pub(super) trait ContextualExpressionEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn contextual(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, target: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(contextual_expression_sync)]
    #[capabilities(effects = ContextualExpressionEffects)]
    #[passive_values(ContextualExpressionResult::Complete, ContextualExpressionResult::Value)]
    pub(super) async fn contextual_expression_with<'db, 'ast, E: ContextualExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, tcx: TypeContext<'db>, effects: &E,
    ) -> Result<ContextualExpressionResult<'db>, E::Error> {
        if let Some(target) = tcx.annotation {
            if let Some(ty) = effects.contextual(builder, expression, target).await? {
                effects.store(builder, expression, ty).await?;
                return Ok(ContextualExpressionResult::Complete(ty));
            }
        }
        Ok(ContextualExpressionResult::Value)
    }
}

impl<'db, 'ast> SynchronousContextualExpressionEffects<'db, 'ast>
    for OrdinaryContextualExpressionEffects
{
    type Error = Infallible;

    fn contextual(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(builder.infer_type_form_contextual_expression(expression, target))
    }

    fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.store_expression_type(expression, ty);
        Ok(())
    }
}

struct SourceContextualExpressionEffects<'effects, E> {
    effects: &'effects E,
}

impl<'db, 'ast, E: SourceExpressionEffects<'db>> ContextualExpressionEffects<'db, 'ast>
    for SourceContextualExpressionEffects<'_, E>
{
    type Error = E::Error;

    async fn contextual(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        target: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.effects
            .contextual_type_form(builder, expression, target)
            .await
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.effects.store_expression(builder, expression, ty).await
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) async fn infer_expression_with<E: SourceExpressionEffects<'db>>(
        &mut self,
        effects: &E,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint(SourceExpressionWork::Expression).await?;
        if self.expression_cache.is_none() {
            return self
                .infer_expression_uncached_with(effects, expression, tcx)
                .await;
        }

        // The queued provider still requires the cache merge and finalization owners before
        // entering the local continuation; the ordinary provider runs those original owners.
        effects
            .legacy_operation(SourceExpressionOperation::ExpressionCache, || {
                super::local::expression(
                    self,
                    expression,
                    tcx,
                    super::local::ExpressionMode::Cached,
                )
            })
            .await
    }

    pub(in crate::types::infer) async fn infer_expression_uncached_with<
        E: SourceExpressionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, E::Error> {
        match contextual_expression_with(
            self,
            expression,
            tcx,
            &SourceContextualExpressionEffects { effects },
        )
        .await?
        {
            ContextualExpressionResult::Complete(ty) => Ok(ty),
            ContextualExpressionResult::Value => {
                self.infer_value_expression_with(effects, expression, tcx)
                    .await
            }
        }
    }

    pub(in crate::types::infer) async fn infer_value_expression_with<
        E: SourceExpressionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, E::Error> {
        let db = self.db();
        let ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        Type::none(db, self.program_environment())
                    })
                    .await?
            }
            ast::Expr::NumberLiteral(literal) => effects.number_literal(self, literal).await?,
            ast::Expr::BooleanLiteral(literal) => {
                effects.checkpoint(SourceExpressionWork::Expression).await?;
                self.infer_boolean_literal_expression(literal)
            }
            ast::Expr::StringLiteral(literal) => effects.string_literal(self, literal, tcx).await?,
            ast::Expr::BytesLiteral(bytes_literal) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_bytes_literal_expression(bytes_literal)
                    })
                    .await?
            }
            ast::Expr::FString(fstring) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_fstring_expression(fstring)
                    })
                    .await?
            }
            ast::Expr::TString(tstring) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_tstring_expression(tstring)
                    })
                    .await?
            }
            ast::Expr::EllipsisLiteral(literal) => {
                effects.ellipsis_literal(self, literal).await?
            }
            ast::Expr::Tuple(tuple) => effects.tuple_expression(self, tuple, tcx).await?,
            ast::Expr::List(list) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_list_expression(list, tcx)
                    })
                    .await?
            }
            ast::Expr::Set(set) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_set_expression(set, tcx)
                    })
                    .await?
            }
            ast::Expr::Dict(dict) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_dict_expression(dict, tcx)
                    })
                    .await?
            }
            ast::Expr::Generator(generator) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_generator_expression(generator, tcx)
                    })
                    .await?
            }
            ast::Expr::ListComp(listcomp) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_list_comprehension_expression(listcomp, tcx)
                    })
                    .await?
            }
            ast::Expr::DictComp(dictcomp) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_dict_comprehension_expression(dictcomp, tcx)
                    })
                    .await?
            }
            ast::Expr::SetComp(setcomp) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_set_comprehension_expression(setcomp, tcx)
                    })
                    .await?
            }
            ast::Expr::Name(name) => {
                let ty = self.infer_name_expression_with(effects, name).await?;
                if let Some(target) = tcx.annotation {
                    effects.specialize_class_context(self, ty, target).await?
                } else {
                    ty
                }
            }
            ast::Expr::Attribute(attribute) => {
                let ty = effects.attribute_expression(self, attribute).await?;
                if let Some(target) = tcx.annotation {
                    effects.specialize_class_context(self, ty, target).await?
                } else {
                    ty
                }
            }
            ast::Expr::UnaryOp(unary_op) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_unary_expression(unary_op)
                    })
                    .await?
            }
            ast::Expr::BinOp(binary) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_binary_expression(binary, tcx)
                    })
                    .await?
            }
            ast::Expr::BoolOp(bool_op) => {
                effects
                    .chained_comparison(
                        self,
                        ChainInput::Boolean {
                            expression: bool_op,
                            context: tcx,
                        },
                    )
                    .await?
            }
            ast::Expr::Compare(compare) => {
                effects
                    .chained_comparison(self, ChainInput::Comparison(compare))
                    .await?
            }
            ast::Expr::Subscript(subscript) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_subscript_expression(subscript)
                    })
                    .await?
            }
            ast::Expr::Slice(slice) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_slice_expression(slice)
                    })
                    .await?
            }
            ast::Expr::If(if_expression) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_if_expression(if_expression, tcx)
                    })
                    .await?
            }
            ast::Expr::Lambda(lambda_expression) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_lambda_expression(lambda_expression, tcx)
                    })
                    .await?
            }
            ast::Expr::Call(call_expression) => {
                effects
                    .legacy_operation(SourceExpressionOperation::Call, || {
                        self.infer_call_expression(call_expression, tcx)
                    })
                    .await?
            }
            ast::Expr::Starred(starred) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_starred_expression(starred, tcx)
                    })
                    .await?
            }
            ast::Expr::Yield(yield_expression) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_yield_expression(yield_expression)
                    })
                    .await?
            }
            ast::Expr::YieldFrom(yield_from) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_yield_from_expression(yield_from)
                    })
                    .await?
            }
            ast::Expr::Await(await_expression) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_await_expression(await_expression, tcx)
                    })
                    .await?
            }
            ast::Expr::Named(named) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        self.infer_named_expression(named)
                    })
                    .await?
            }
            ast::Expr::IpyEscapeCommand(_) => {
                effects
                    .legacy_operation(SourceExpressionOperation::ExpressionKind, || {
                        todo_type!("Ipy escape command support")
                    })
                    .await?
            }
        };
        self.finish_expression_type_with(effects, expression, ty, tcx)
            .await
    }

    pub(in crate::types::infer) async fn finish_expression_type_with<
        E: SourceExpressionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, E::Error> {
        let ty = if tcx.annotation.is_some() {
            effects
                .apply_type_context(self, expression, ty, tcx)
                .await?
        } else {
            ty
        };
        effects.store_expression(self, expression, ty).await?;
        Ok(ty)
    }

    pub(in crate::types::infer) async fn infer_name_load_with_definition_with<
        E: SourceExpressionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        name_node: &ast::ExprName,
    ) -> Result<(Type<'db>, Option<Definition<'db>>), E::Error> {
        effects
            .checkpoint(SourceExpressionWork::NameBytes(name_node.id.len()))
            .await?;
        let expr = PlaceExpr::from_expr_name(name_node);
        let (resolved, _) = self
            .infer_place_load_with(effects, expr, ast::ExprRef::Name(name_node))
            .await?;
        let definition = match resolved.place {
            Place::Defined(place) => place.provenance.definition(),
            Place::Undefined => None,
        };
        let ty = match effects.place_lookup(self, resolved).await? {
            Ok(ty) => ty,
            Err(LookupError::Undefined(qualifiers)) => {
                effects
                    .legacy_operation(SourceExpressionOperation::UnresolvedReference, || {
                        self.report_unresolved_reference(name_node);
                    })
                    .await?;
                TypeAndQualifiers::new(Type::unknown(), TypeOrigin::Inferred, qualifiers)
            }
            Err(LookupError::PossiblyUndefined(type_when_bound)) => {
                effects
                    .legacy_operation(
                        SourceExpressionOperation::PossiblyUnresolvedReference,
                        || {
                            report_possibly_unresolved_reference(&self.context, name_node);
                        },
                    )
                    .await?;
                type_when_bound
            }
        };
        Ok((ty.inner_type(), definition))
    }

    pub(in crate::types::infer) async fn infer_place_load_with<E: SourceExpressionEffects<'db>>(
        &self,
        effects: &E,
        place_expr: PlaceExpr,
        expr_ref: ast::ExprRef<'_>,
    ) -> Result<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>), E::Error> {
        let mode = if self.is_deferred() && self.in_string_annotation() {
            PlaceLoadMode::StringAnnotation
        } else if self.is_deferred() {
            PlaceLoadMode::Deferred
        } else {
            PlaceLoadMode::AtExpression(expr_ref)
        };
        let mut resolution = effects
            .prepare_place_resolution(self, place_expr, mode)
            .await?;
        let mut place = PlaceAndQualifiers::from(Place::Undefined);
        let mut failure = None;
        let mut checked_deprecated = false;

        while let Some(step) = effects.next_place_resolution(&mut resolution).await? {
            match step {
                PlaceLoadResolutionStep::Source(source) => {
                    effects
                        .checkpoint(SourceExpressionWork::PlaceSource)
                        .await?;
                    if !checked_deprecated && source.is_post_lexical() {
                        // Deprecation diagnostics apply to the result of lexical name resolution,
                        // before it is combined with implicit module globals or builtins. Hence, we
                        // check for deprecation here when the first post-lexical source is yielded.
                        // If resolution stops before this, then the check after the resolution loop
                        // handles the final lexical result instead.
                        if let Some(ty) = place.place.ignore_possibly_undefined() {
                            self.check_deprecated_with(effects, expr_ref, ty).await?;
                        }
                        checked_deprecated = true;
                    }
                    place = match effects.place_lookup(self, place).await? {
                        Ok(ty) => PlaceAndQualifiers::from(Ok(ty)),
                        Err(primary) => {
                            let narrowing_constraints =
                                resolution.narrowing_constraints_for(&source);
                            let fallback = self
                                .infer_place_load_source_with(
                                    effects,
                                    resolution.place_expr(),
                                    source,
                                    narrowing_constraints,
                                )
                                .await?;
                            effects.merge_fallback(self, primary, fallback).await?
                        }
                    };
                    if place.place.is_definitely_bound() {
                        break;
                    }
                }
                PlaceLoadResolutionStep::MemberResolutionCondition(prefix_loads) => {
                    if self
                        .has_bound_place_expr_prefix_with(effects, &prefix_loads)
                        .await?
                    {
                        failure = Some(PlaceLoadFailure::NotFound);
                        break;
                    }
                }
                PlaceLoadResolutionStep::Exhausted(exhaustion_failure) => {
                    failure = Some(exhaustion_failure);
                    break;
                }
            }
        }

        if !checked_deprecated && let Some(ty) = place.place.ignore_possibly_undefined() {
            self.check_deprecated_with(effects, expr_ref, ty).await?;
        }
        if failure == Some(PlaceLoadFailure::NotFound) {
            place = match effects.place_lookup(self, place).await? {
                Ok(ty) => PlaceAndQualifiers::from(Ok(ty)),
                Err(primary) => {
                    let fallback = if let Some(name) = expr_ref
                        .as_name_expr()
                        .filter(|name| name.id == "reveal_type")
                    {
                        effects
                            .legacy_operation(SourceExpressionOperation::RevealTypeFallback, || {
                                self.infer_unimported_reveal_type_fallback(name)
                            })
                            .await?
                    } else {
                        Place::Undefined.into()
                    };
                    effects.merge_fallback(self, primary, fallback).await?
                }
            };
        }
        Ok((place, resolution.into_constraints()))
    }

    /// Returns whether any tracked place-expression prefix has a definite or possible binding in
    /// this scope.
    async fn has_bound_place_expr_prefix_with<E: SourceExpressionEffects<'db>>(
        &self,
        effects: &E,
        prefix_loads: &PlaceExprPrefixLoads<'db>,
    ) -> Result<bool, E::Error> {
        let file_scope_id = effects
            .field(prefix_loads.scope().read_fields(self.db()).file_scope_id())
            .await?;
        let use_def = self.index.use_def_map(file_scope_id);
        let mut prefixes = prefix_loads.iter();

        loop {
            effects
                .checkpoint(SourceExpressionWork::PlaceSource)
                .await?;
            let Some(prefix) = prefixes.next() else {
                return Ok(false);
            };
            let bindings = match prefix {
                PlaceExprPrefixLoad::AtUse(use_id) => use_def.bindings_at_use(use_id),
                PlaceExprPrefixLoad::AllReachable(place_id) => use_def.reachable_bindings(place_id),
                PlaceExprPrefixLoad::DefinitelyBound => return Ok(true),
            };
            if !effects.bindings(self, bindings).await?.is_undefined() {
                return Ok(true);
            }
        }
    }

    pub(in crate::types::infer) async fn infer_place_load_source_with<
        E: SourceExpressionEffects<'db>,
    >(
        &self,
        effects: &E,
        place_expr: PlaceExprRef<'_>,
        source: PlaceLoadSource<'db>,
        narrowing_constraints: &[(FileScopeId, ConstraintKey)],
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        let is_class_body_global_fallback = source.is_class_body_global_fallback();
        let place = match source.kind {
            PlaceLoadSourceKind::Bindings(bindings) => {
                let mut place = effects.bindings(self, bindings).await?;
                // Compatibility policy: ty historically treats a possibly-bound module snapshot
                // reached through a class-body global fallback as definitely bound. At runtime,
                // an unbound snapshot would continue to builtins or produce a name error.
                if is_class_body_global_fallback && let Place::Defined(defined) = place {
                    place = Place::Defined(defined.with_definedness(Definedness::AlwaysDefined));
                }
                place.into()
            }
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                effects.owning_scope_symbol(self, scope, id).await?
            }
            PlaceLoadSourceKind::Implicit(implicit) => {
                effects.implicit_place(self, implicit).await?
            }
        };
        let Place::Defined(defined) = place.place else {
            return Ok(place);
        };
        if narrowing_constraints.is_empty() {
            return Ok(place);
        }
        let narrowed = effects
            .narrow_place(self, place_expr, defined.ty, narrowing_constraints)
            .await?;
        Ok(place.map_type(|_| narrowed))
    }

    pub(in crate::types::infer) async fn infer_name_expression_with<
        E: SourceExpressionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        name: &ast::ExprName,
    ) -> Result<Type<'db>, E::Error> {
        Ok(match name.ctx {
            ExprContext::Load => {
                self.infer_name_load_with_definition_with(effects, name)
                    .await?
                    .0
            }
            ExprContext::Store => Type::Never,
            ExprContext::Del => {
                self.infer_name_load_with_definition_with(effects, name)
                    .await?;
                Type::Never
            }
            ExprContext::Invalid => Type::unknown(),
        })
    }

    pub(in crate::types::infer) async fn check_deprecated_with<
        E: SourceExpressionEffects<'db>,
        T: Ranged,
    >(
        &self,
        effects: &E,
        ranged: T,
        mut ty: Type<'db>,
    ) -> Result<(), E::Error> {
        while let Type::BoundMethod(bound) = ty {
            effects
                .checkpoint(SourceExpressionWork::BoundMethod)
                .await?;
            ty = effects
                .field(bound.field_requests(self.db()).func())
                .await?;
        }

        // First handle classes
        if let Type::ClassLiteral(class_literal) = ty {
            let Some(deprecated) = effects.class_deprecation(self, class_literal).await? else {
                return Ok(());
            };

            effects
                .legacy_operation(SourceExpressionOperation::DeprecationDiagnostic, || {
                    let Some(builder) = self.context.report_lint(&diagnostic::DEPRECATED, ranged)
                    else {
                        return;
                    };

                    let class_name = class_literal.name(self.db());
                    let mut diag = builder
                        .into_diagnostic(format_args!(r#"The class `{class_name}` is deprecated"#));
                    if let Some(message) = deprecated.message {
                        diag.set_primary_annotation_message(message.value(self.db()));
                    }
                    diag.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
                })
                .await?;
            return Ok(());
        }

        // Next handle functions
        let function = match ty {
            Type::FunctionLiteral(function) => function,
            Type::Callable(callable) => {
                effects
                    .legacy_operation(SourceExpressionOperation::CallableDeprecation, || {
                        self.report_deprecated_functions(ranged, callable.deprecated(self.db()));
                    })
                    .await?;
                return Ok(());
            }
            _ => return Ok(()),
        };

        // References to a function only check its implementation. Deprecated overloads are
        // checked at call sites, after resolving which signatures accept the arguments.
        let Some(deprecated) = effects.function_deprecation(self, function).await? else {
            return Ok(());
        };

        effects
            .legacy_operation(SourceExpressionOperation::DeprecationDiagnostic, || {
                let Some(builder) = self.context.report_lint(&diagnostic::DEPRECATED, ranged)
                else {
                    return;
                };

                let func_name = function.name(self.db());
                let mut diag = builder
                    .into_diagnostic(format_args!(r#"The function `{func_name}` is deprecated"#));
                if let Some(message) = deprecated.message {
                    diag.set_primary_annotation_message(message.value(self.db()));
                }
                diag.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
            })
            .await?;
        Ok(())
    }
}
