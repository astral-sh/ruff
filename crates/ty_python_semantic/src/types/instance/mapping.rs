//! Finite nominal mapping that retains tuple and generic-alias mapping as separate children.

use super::{
    ExplicitAnyInstanceClass, NominalInstanceClass, NominalInstanceInner, NominalInstanceType,
};
use crate::Db;
use crate::types::mapping::effects::{MappingEffects, MappingWork, SynchronousMappingEffects};
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingVisitor, ClassType, GenericAlias, Type, TypeContext, TypeMapping,
};

/// Identifies fixed local decisions and handle transfers in nominal mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum NominalMappingWork {
    Dispatch,
    ClassDispatch,
    Reconstruct,
    Publish,
}

/// Reads and constructs only the inline representation of a nominal instance.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct NominalMappingFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies stored class reads, mapping children, and `ExplicitAnyInstanceClass` interning.
    #[synchronous(SynchronousNominalMappingEffects)]
    pub(in crate::types) trait NominalMappingEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: NominalMappingWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn explicit_any_class(&self, db: &'db dyn Db, class: ExplicitAnyInstanceClass<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn map_tuple(&self, db: &'db dyn Db, tuple: TupleType<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn map_generic_alias(&self, db: &'db dyn Db, alias: GenericAlias<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<GenericAlias<'db>, Self::Error>;
        #[operation(source)]
        async fn intern_explicit_any(&self, db: &'db dyn Db, class: ClassType<'db>) -> Result<ExplicitAnyInstanceClass<'db>, Self::Error>;
    }

    #[finite_capability]
    impl NominalMappingFacts {
        fn inner<'db>(&self, instance: NominalInstanceType<'db>) -> NominalInstanceInner<'db> { instance.0 }
        fn nominal<'db>(&self, instance: NominalInstanceType<'db>) -> Type<'db> { Type::NominalInstance(instance) }
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn non_tuple<'db>(&self, class: NominalInstanceClass<'db>) -> Type<'db> {
            Type::NominalInstance(NominalInstanceType(NominalInstanceInner::NonTuple(class)))
        }
    }

    /// Maps a nominal instance while preserving its representation and explicit-`Any` inheritance.
    /// Tuples and generic aliases use the supplied visitor and mapping descriptor in their children.
    #[synchronous(map_nominal_sync)]
    #[capabilities(effects = NominalMappingEffects, facts = NominalMappingFacts)]
    #[passive_values(NominalMappingWork::Dispatch, NominalMappingWork::ClassDispatch, NominalMappingWork::Reconstruct, NominalMappingWork::Publish, NominalInstanceClass::Plain, NominalInstanceClass::InheritsFromExplicitAny, ClassType::Generic)]
    pub(in crate::types) async fn map_nominal_with<'db, E: NominalMappingEffects<'db>>(
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
        facts: NominalMappingFacts,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint(NominalMappingWork::Dispatch).await?;
        let result = match facts.inner(instance) {
            NominalInstanceInner::ExactTuple(tuple) => effects.map_tuple(db, tuple, mapping, tcx, visitor).await?,
            NominalInstanceInner::SysVersionInfo => facts.nominal(instance),
            NominalInstanceInner::Object => facts.object(),
            NominalInstanceInner::NonTuple(stored) => {
                let class = match stored {
                    NominalInstanceClass::Plain(class) => class,
                    NominalInstanceClass::InheritsFromExplicitAny(class) => effects.explicit_any_class(db, class).await?,
                };
                effects.checkpoint(NominalMappingWork::ClassDispatch).await?;
                let mapped = match class {
                    ClassType::NonGeneric(_) => class,
                    ClassType::Generic(alias) => ClassType::Generic(effects.map_generic_alias(db, alias, mapping, tcx, visitor).await?),
                };
                effects.checkpoint(NominalMappingWork::Reconstruct).await?;
                let stored = match stored {
                    NominalInstanceClass::Plain(_) => NominalInstanceClass::Plain(mapped),
                    NominalInstanceClass::InheritsFromExplicitAny(_) => NominalInstanceClass::InheritsFromExplicitAny(effects.intern_explicit_any(db, mapped).await?),
                };
                facts.non_tuple(stored)
            }
        };
        effects.checkpoint(NominalMappingWork::Publish).await?;
        Ok(result)
    }
}

/// Adapts existing mapping providers to the finite nominal algorithm for ordinary execution.
#[derive(Debug)]
pub(super) struct MappingNominalEffects<'a, E>(pub(super) &'a E);

impl<'db, E: MappingEffects<'db>> NominalMappingEffects<'db> for MappingNominalEffects<'_, E> {
    type Error = E::Error;

    async fn checkpoint(&self, _work: NominalMappingWork) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn explicit_any_class(
        &self,
        db: &'db dyn Db,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.class(db))
    }

    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0.map_tuple(db, tuple, mapping, tcx, visitor).await
    }

    async fn map_generic_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        alias
            .apply_type_mapping_with(db, mapping, tcx, visitor, self.0)
            .await
    }

    async fn intern_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<ExplicitAnyInstanceClass<'db>, Self::Error> {
        self.0.checkpoint(MappingWork::ExplicitAnyIntern).await?;
        Ok(ExplicitAnyInstanceClass::new(db, class))
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousNominalMappingEffects<'db>
    for MappingNominalEffects<'_, E>
{
    type Error = E::Error;

    fn checkpoint(&self, _work: NominalMappingWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn explicit_any_class(
        &self,
        db: &'db dyn Db,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.class(db))
    }

    fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0.map_tuple(db, tuple, mapping, tcx, visitor)
    }

    fn map_generic_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        alias.apply_type_mapping_sync(db, mapping, tcx, visitor, self.0)
    }

    fn intern_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<ExplicitAnyInstanceClass<'db>, Self::Error> {
        self.0.checkpoint(MappingWork::ExplicitAnyIntern)?;
        Ok(ExplicitAnyInstanceClass::new(db, class))
    }
}
