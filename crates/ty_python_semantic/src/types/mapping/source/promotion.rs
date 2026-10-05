//! Promotion leaves and nominal carriers reuse the retained source mapping visitor.

use super::{MappingSourceEffects, MaterializationOperation, RetainedMappingSource, SourceMapping};
use salsa::execution_probe::{RunError, RunResult};

use crate::types::class::mapping::map_generic_alias_with;
use crate::types::instance::mapping::{
    NominalMappingEffects, NominalMappingFacts, NominalMappingWork, map_nominal_with,
};
use crate::types::instance::{ExplicitAnyInstanceClass, NominalInstanceClass};
use crate::types::literal::EnumLiteralType;
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::mapping::effects::{MappingOperation, SharedMappingEffects};
use crate::types::promotion::leaf::{
    PromotionLeafEffects, PromotionLeafFacts, PromotionLeafWork, promote_leaf_with,
};
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingVisitor, ClassType, FunctionType, GenericAlias, KnownClass,
    NominalInstanceType, Type, TypeContext, TypeMapping,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, R: RetainedMappingSource<'run, 'db>> SourceMapping<'_, '_, '_, 'run, 'db, R> {
    /// Admits finite promotion decisions and the actual callback/result carriers separately.
    /// Inputs are copied handles or retained references; their retirement does not traverse types.
    pub(super) async fn promotion_local<T, M: FnOnce() -> RunResult<T>>(
        &self,
        work: usize,
        extra_bytes: usize,
        make: M,
    ) -> RunResult<T> {
        local_with_fixed_transfers_at(self.endpoint, work, extra_bytes, make).await?
    }

    /// Resumes one promotion leaf with the current mapping environment and source dependencies.
    pub(super) async fn promotion_leaf(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        promote_leaf_with(ty, self.visitor.env, PromotionLeafFacts, self).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    /// Resumes a nominal carrier without replacing its retained visitor or mapping polarity.
    pub(super) async fn map_source_nominal(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.function_child(|| map_nominal_with(
            db,
            instance,
            mapping,
            tcx,
            visitor,
            self,
            NominalMappingFacts,
        ))
        .await
    }
}

impl<'run, 'db: 'run, R: RetainedMappingSource<'run, 'db>> PromotionLeafEffects<'db>
    for SourceMapping<'_, '_, '_, 'run, 'db, R>
{
    type Error = RunError;

    async fn checkpoint(&self, work: PromotionLeafWork) -> RunResult<()> {
        let units = match work {
            PromotionLeafWork::Dispatch => 4,
            PromotionLeafWork::LiteralClassification => 6,
            PromotionLeafWork::ScalarRequest
            | PromotionLeafWork::EnumRequest
            | PromotionLeafWork::FunctionRequest => 4,
            PromotionLeafWork::Result => 3,
        };
        self.promotion_local(
            units,
            size_of::<(Type<'db>, Option<KnownClass>, R::Effects<'_>)>(),
            || Ok(()),
        )
        .await
    }

    async fn scalar_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.source.effects().promotion_scalar(env, class).await
    }

    async fn enum_fallback(
        &self,
        _env: &ProgramEnvironment<'db>,
        _literal: EnumLiteralType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(MaterializationOperation::Leaf(MappingOperation::Promotion))
            .await
    }

    async fn function_callable(&self, function: FunctionType<'db>) -> RunResult<Type<'db>> {
        self.source.effects().promotion_function_callable(function).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    NominalMappingEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn checkpoint(&self, work: NominalMappingWork) -> RunResult<()> {
        let units = match work {
            NominalMappingWork::Dispatch => 16,
            NominalMappingWork::ClassDispatch => 16,
            NominalMappingWork::Reconstruct => 16,
            NominalMappingWork::Publish => 16,
        };
        self.promotion_local(
            units,
            size_of::<(
                NominalInstanceType<'db>,
                NominalInstanceClass<'db>,
                ClassType<'db>,
                Type<'db>,
                R::Effects<'_>,
            )>(),
            || Ok(()),
        )
        .await
    }

    async fn explicit_any_class(
        &self,
        _db: &'db dyn Db,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.source
            .effects()
            .promotion_explicit_any_class(class)
            .await
    }

    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        SharedMappingEffects::map_tuple(self, db, tuple, mapping, tcx, visitor).await
    }

    async fn map_generic_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<GenericAlias<'db>> {
        self.promotion_local(2, 0, || self.check_visitor(visitor))
            .await?;
        self.function_child(|| map_generic_alias_with(db, alias, mapping, tcx, visitor, self)).await
    }

    async fn intern_explicit_any(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<ExplicitAnyInstanceClass<'db>> {
        self.source
            .effects()
            .promotion_intern_explicit_any(class)
            .await
    }
}
