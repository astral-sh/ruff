//! Shared construction of the first MRO entry and dispatch for its lazy tail.

use crate::types::mro::field_reads::MroFieldReads;

use std::convert::Infallible;

use crate::Db;
use crate::types::class::default_class_specialization_with;
use crate::types::class::{
    DynamicClassLiteral, DynamicEnumLiteral, DynamicNamedTupleLiteral, DynamicTypedDictLiteral,
};
use crate::types::class_base::ClassBase;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::source_read::UnrestrictedSourceRead;
use crate::types::{ClassLiteral, ClassType, StaticClassLiteral};

#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug))]
pub(in crate::types) enum MroRootWork {
    GenericContext,
    DefaultSpecialization,
    TupleRuntimeSpecialization,
    GenericAlias,
}

/// The tail request retains the original class kind so each provider can use its canonical MRO.
#[derive(Clone, Copy)]
pub(in crate::types) enum MroTailRequest<'db> {
    Static(StaticClassLiteral<'db>, Option<Specialization<'db>>),
    Dynamic(DynamicClassLiteral<'db>),
    DynamicNamedTuple(DynamicNamedTupleLiteral<'db>),
    DynamicTypedDict(DynamicTypedDictLiteral<'db>),
    DynamicEnum(DynamicEnumLiteral<'db>),
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// Prepared providers read the class context after its work checkpoint.
pub(in crate::types) trait MroRootFacts<'db>: sealed::Sealed {
    type Error;
}

pub(in crate::types) trait MroRootEffects<'db>: MroRootFacts<'db> {
    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    async fn checkpoint(&self, work: MroRootWork) -> Result<(), Self::Error>;
    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;
    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
}

/// Ordinary iteration calls the shared decisions directly without constructing futures.
pub(in crate::types) trait SynchronousMroRootEffects<'db>:
    MroRootFacts<'db>
{
    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;

    fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;

    fn checkpoint(&self, work: MroRootWork) -> Result<(), Self::Error>;
    fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;
    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_mro_root]
#[inline]
pub(in crate::types) async fn apply_optional_class_specialization_with<
    'db,
    E: MroRootEffects<'db>,
>(
    fields: MroFieldReads<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<ClassType<'db>, E::Error> {
    let _ = fields;
    let Some(specialization) = specialization else {
        effects
            .checkpoint(MroRootWork::DefaultSpecialization)
            .await?;
        return effects.default_class_specialization(class).await;
    };
    effects.checkpoint(MroRootWork::GenericContext).await?;
    let Some(_) = effects.generic_context(class).await? else {
        return Ok(ClassType::NonGeneric(class.into()));
    };

    effects.checkpoint(MroRootWork::GenericAlias).await?;
    effects.generic_alias(class, specialization).await
}

#[ty_mapping_probe_macros::dual_mro_root]
#[inline]
pub(in crate::types) async fn mro_first_with<'db, E: MroRootEffects<'db>>(
    fields: MroFieldReads<'db>,
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<ClassBase<'db>, E::Error> {
    let class = match class {
        ClassLiteral::Static(literal) => {
            apply_optional_class_specialization_with(fields, literal, specialization, effects)
                .await?
        }
        ClassLiteral::Dynamic(literal) => ClassType::NonGeneric(literal.into()),
        ClassLiteral::DynamicNamedTuple(literal) => ClassType::NonGeneric(literal.into()),
        ClassLiteral::DynamicTypedDict(literal) => ClassType::NonGeneric(literal.into()),
        ClassLiteral::DynamicEnum(literal) => ClassType::NonGeneric(literal.into()),
    };
    Ok(ClassBase::Class(class))
}

#[ty_mapping_probe_macros::dual_mro_root]
#[inline]
pub(in crate::types) async fn mro_tail_request_with<'db, E: MroRootEffects<'db>>(
    fields: MroFieldReads<'db>,
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<MroTailRequest<'db>, E::Error> {
    let _ = fields;
    Ok(match class {
        ClassLiteral::Static(literal) => {
            let specialization = match specialization {
                Some(specialization) => {
                    effects
                        .checkpoint(MroRootWork::TupleRuntimeSpecialization)
                        .await?;
                    Some(effects.tuple_runtime_specialization(specialization).await?)
                }
                None => None,
            };
            MroTailRequest::Static(literal, specialization)
        }
        ClassLiteral::Dynamic(literal) => MroTailRequest::Dynamic(literal),
        ClassLiteral::DynamicNamedTuple(literal) => MroTailRequest::DynamicNamedTuple(literal),
        ClassLiteral::DynamicTypedDict(literal) => MroTailRequest::DynamicTypedDict(literal),
        ClassLiteral::DynamicEnum(literal) => MroTailRequest::DynamicEnum(literal),
    })
}

pub(in crate::types) struct InlineMroRootEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> InlineMroRootEffects<'db> {
    #[inline]
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineMroRootEffects<'_> {}

impl<'db> MroRootFacts<'db> for InlineMroRootEffects<'db> {
    type Error = Infallible;
}

impl<'db> SynchronousMroRootEffects<'db> for InlineMroRootEffects<'db> {
    fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    #[inline]
    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.generic_context(self.db))
    }

    #[inline]
    fn checkpoint(&self, _work: MroRootWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        default_class_specialization_with(self.db, class, &UnrestrictedSourceRead)
    }

    #[inline]
    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(specialization.tuple_runtime_element_specialization(self.db))
    }
}
