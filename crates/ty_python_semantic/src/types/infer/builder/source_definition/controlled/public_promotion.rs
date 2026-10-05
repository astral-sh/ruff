//! Public type promotion uses the retained mapping visitor and canonical source children.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::ClassCheckOperation;
use crate::types::instance::{
    ExplicitAnyInstanceClass, NominalClassFacts, NominalKnownClassEffects, nominal_known_class_with,
};
use crate::types::mapping::OwnedTypeMapping;
#[cfg(test)]
use crate::types::mapping::source::public_promotion_observations as observations;
use crate::types::promotion::classification::{
    SingletonClassificationWork, SingletonEffects, SingletonFacts, classify_singleton_with,
};
use crate::types::promotion::{PublicPromotionEffects, PublicPromotionWork, sealed};
use crate::types::set_theoretic::numeric::{
    NumericUnionEffects, NumericUnionWork, numeric_union_with,
};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, KnownClass, KnownUnion, NominalInstanceType,
    PromotionMode, StaticClassLiteral, Type, UnionBuilder,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Promotes a public member through regular mapping, then widens a top-level singleton.
    /// The final admitted transfer completes before the caller may publish its member result.
    pub(super) async fn promote_public_type_source(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let result = self
            .class_object_child(|| ty.promote_public_with(self.db(), env, self))
            .await?;
        #[cfg(test)]
        observations::stage(self.db(), observations::Stage::FinalTransfer);
        self.class_object_local(3, 0, || {
            #[cfg(test)]
            observations::stage(self.db(), observations::Stage::Transferred);
            result
        })
        .await
    }

    /// Maps a type with the specified regular-promotion polarity using the retained source visitor.
    pub(in crate::types::infer) async fn promote_regular(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        mode: PromotionMode,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        let mapping = self
            .class_object_local(3, 0, || OwnedTypeMapping::PromoteRegular(mode))
            .await?;
        self.apply_mapping(ty, program, mapping).await
    }

    /// Resolves a scalar literal's canonical fallback instance in the retained program.
    pub(super) async fn promotion_scalar(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        #[cfg(test)]
        observations::stage(self.db(), observations::Stage::ScalarRequest(class));
        self.class_object_child(|| self.access.known_class_instance(program, class))
            .await
    }

    /// Reads a nominal instance's known-class tag through the shared finite representation dispatch.
    pub(super) async fn promotion_nominal_known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.class_object_local(
            12,
            size_of::<(NominalInstanceType<'db>, ClassType<'db>)>(),
            || (),
        )
        .await?;
        self.class_object_child(|| nominal_known_class_with(instance, NominalClassFacts, self))
            .await
    }

    /// Builds the numeric-tower union using canonical instances and the shared finite recipe.
    pub(super) async fn promotion_numeric_union(
        &self,
        env: &ProgramEnvironment<'db>,
        union: KnownUnion,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.class_object_child(|| numeric_union_with(self.db(), env, union, self))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PublicPromotionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: PublicPromotionWork) -> RunResult<()> {
        let units = match work {
            PublicPromotionWork::Admission | PublicPromotionWork::UnionRequest => 4,
            PublicPromotionWork::SingletonDispatch => 3,
            PublicPromotionWork::SingletonClassification => 3,
        };
        self.class_object_local(
            units,
            size_of::<(Type<'db>, Option<NominalInstanceType<'db>>, bool)>(),
            || (),
        )
        .await
    }

    async fn regular(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.class_object_child(|| self.promote_regular(ty, env, PromotionMode::On))
            .await
    }

    async fn is_singleton(
        &self,
        _db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.class_object_child(|| classify_singleton_with(instance, SingletonFacts, self))
            .await
    }

    async fn union_two(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        PublicPromotionEffects::checkpoint(self, PublicPromotionWork::UnionRequest).await?;
        self.environment_program(env).await?;
        #[cfg(test)]
        observations::stage(self.db(), observations::Stage::SingletonUnion);
        self.class_object_child(|| self.access.union_from_two_elements(first, second))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SingletonEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: SingletonClassificationWork) -> RunResult<()> {
        let units = match work {
            SingletonClassificationWork::Dispatch => 5,
            SingletonClassificationWork::ClassDispatch => 5,
            SingletonClassificationWork::LiteralDispatch => 6,
            SingletonClassificationWork::KnownDecision => 4,
            SingletonClassificationWork::EnumRequest => 3,
            SingletonClassificationWork::Result => 3,
        };
        self.class_object_local(
            units,
            size_of::<(
                NominalInstanceType<'db>,
                ClassType<'db>,
                ClassLiteral<'db>,
                Option<KnownClass>,
                bool,
            )>(),
            || (),
        )
        .await
    }

    async fn explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.class_object_child(|| NominalKnownClassEffects::explicit_any_class(self, class))
            .await
    }

    async fn generic_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        self.class_object_child(|| NominalKnownClassEffects::generic_origin(self, alias))
            .await
    }

    async fn static_known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.class_object_child(|| NominalKnownClassEffects::static_known(self, class))
            .await
    }

    async fn enum_singleton(&self, class: ClassLiteral<'db>) -> RunResult<bool> {
        let metadata = self
            .class_object_child(|| self.access.enum_class_metadata(class))
            .await?;
        let absent = self.class_object_local(3, 0, || metadata.is_none()).await?;
        if absent {
            Ok(false)
        } else {
            self.unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::EnumMetadata,
            ))
            .await
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NumericUnionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: NumericUnionWork) -> RunResult<()> {
        let units = match work {
            NumericUnionWork::Dispatch => 5,
            NumericUnionWork::Publish => 3,
        };
        self.class_object_local(units, size_of::<([Type<'db>; 3], KnownUnion)>(), || ())
            .await
    }

    async fn known_instance(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.class_object_child(|| self.access.known_class_instance(program, class))
            .await
    }

    async fn union_two(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.class_object_child(|| self.access.union_from_two_elements(first, second))
            .await
    }

    async fn union_three(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
        third: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let mut builder = self
            .class_object_local(14, size_of::<[Type<'db>; 3]>(), || {
                UnionBuilder::new(self.db(), env)
            })
            .await?;
        self.class_object_child(|| PairUnionEffects::union_add(self, &mut builder, first))
            .await?;
        self.class_object_child(|| PairUnionEffects::union_add(self, &mut builder, second))
            .await?;
        self.class_object_child(|| PairUnionEffects::union_add(self, &mut builder, third))
            .await?;
        self.class_object_child(|| PairUnionEffects::union_build(self, builder))
            .await
    }
}
