//! Shared base iterator starts and canonical MRO collection dependencies.

use crate::types::mro::field_reads::MroFieldReads;

use std::collections::VecDeque;
use std::convert::Infallible;

use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral};
use crate::{Db, ProgramEnvironment};

use super::collection::base::{collect_start_sync, collect_start_with_root_sync};
use super::root::InlineMroRootEffects;
use super::{Mro, MroIterator};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ClassMroStart<'db> {
    pub(in crate::types) class: ClassLiteral<'db>,
    pub(in crate::types) specialization: Option<Specialization<'db>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum BaseMroStart<'db> {
    /// An MRO of length 2 that consists of the first element and then `object`.
    Length2([ClassBase<'db>; 2]),
    /// An MRO of length 3 that consists of two elements and then `object`.
    Length3([ClassBase<'db>; 3]),
    /// The MRO of an arbitrary class. It may have any length.
    Class(ClassMroStart<'db>),
}

impl<'db> BaseMroStart<'db> {
    #[inline]
    pub(in crate::types) fn into_iter(self, db: &'db dyn Db) -> ClassBaseMroIterator<'db> {
        match self {
            Self::Length2(elements) => ClassBaseMroIterator::Length2(elements.into_iter()),
            Self::Length3(elements) => ClassBaseMroIterator::Length3(elements.into_iter()),
            Self::Class(start) => ClassBaseMroIterator::FromClass(MroIterator::new(
                db,
                start.class,
                start.specialization,
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum BaseMroWork {
    ClassDispatch,
    BaseDispatch,
    ObjectBase,
    CompositionRequest,
    CollectionRequest,
    SingleCollectionRequest,
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait BaseMroFacts<'db>: sealed::Sealed {
    type Error;
}

pub(in crate::types) trait BaseMroEffects<'db>: BaseMroFacts<'db> {
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error>;

    async fn checkpoint(&self, work: BaseMroWork) -> Result<(), Self::Error>;
    async fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    async fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error>;
    async fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousBaseMroEffects<'db>:
    BaseMroFacts<'db>
{
    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;
    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Self::Error>;

    fn checkpoint(&self, work: BaseMroWork) -> Result<(), Self::Error>;
    fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error>;
    fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_base_mro]
#[inline]
pub(in crate::types) async fn class_mro_start_with<'db, E: BaseMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    class: ClassType<'db>,
    additional: Option<Specialization<'db>>,
    effects: &E,
) -> Result<ClassMroStart<'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(BaseMroWork::ClassDispatch).await?;
    let start = match class {
        ClassType::NonGeneric(class) => ClassMroStart {
            class,
            specialization: None,
        },
        ClassType::Generic(generic) => {
            let origin = effects.alias_origin(generic).await?;
            let specialization = effects.alias_specialization(generic).await?;
            let specialization = if let Some(additional) = additional {
                effects.checkpoint(BaseMroWork::CompositionRequest).await?;
                effects
                    .compose_specialization(specialization, additional)
                    .await?
            } else {
                specialization
            };
            ClassMroStart {
                class: ClassLiteral::Static(origin),
                specialization: Some(specialization),
            }
        }
    };
    effects.checkpoint(BaseMroWork::Publish).await?;
    Ok(start)
}

#[ty_mapping_probe_macros::dual_base_mro]
#[inline]
pub(in crate::types) async fn base_mro_start_with<'db, E: BaseMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    env: &ProgramEnvironment<'db>,
    base: ClassBase<'db>,
    additional: Option<Specialization<'db>>,
    effects: &E,
) -> Result<BaseMroStart<'db>, E::Error> {
    effects.checkpoint(BaseMroWork::BaseDispatch).await?;
    let start = match base {
        ClassBase::Protocol => {
            effects.checkpoint(BaseMroWork::ObjectBase).await?;
            BaseMroStart::Length3([base, ClassBase::Generic, effects.object_base(env).await?])
        }
        ClassBase::Any
        | ClassBase::Dynamic(_)
        | ClassBase::Divergent(_)
        | ClassBase::Generic
        | ClassBase::TypedDict(_) => {
            effects.checkpoint(BaseMroWork::ObjectBase).await?;
            BaseMroStart::Length2([base, effects.object_base(env).await?])
        }
        ClassBase::Class(class) => {
            BaseMroStart::Class(class_mro_start_with(fields, class, additional, effects).await?)
        }
    };
    effects.checkpoint(BaseMroWork::Publish).await?;
    Ok(start)
}

#[ty_mapping_probe_macros::dual_base_mro]
#[inline]
pub(in crate::types) async fn collect_base_mro_with<'db, E: BaseMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    env: &ProgramEnvironment<'db>,
    base: ClassBase<'db>,
    additional: Option<Specialization<'db>>,
    effects: &E,
) -> Result<VecDeque<ClassBase<'db>>, E::Error> {
    let start = base_mro_start_with(fields, env, base, additional, effects).await?;
    effects.checkpoint(BaseMroWork::CollectionRequest).await?;
    let collected = effects.collect_start(start).await?;
    effects.checkpoint(BaseMroWork::Publish).await?;
    Ok(collected)
}

#[ty_mapping_probe_macros::dual_base_mro]
#[inline]
pub(in crate::types) async fn collect_single_base_mro_with<'db, E: BaseMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    env: &ProgramEnvironment<'db>,
    root: ClassType<'db>,
    base: ClassBase<'db>,
    additional: Option<Specialization<'db>>,
    effects: &E,
) -> Result<Mro<'db>, E::Error> {
    let start = base_mro_start_with(fields, env, base, additional, effects).await?;
    effects
        .checkpoint(BaseMroWork::SingleCollectionRequest)
        .await?;
    let collected = effects.collect_start_with_root(root, start).await?;
    effects.checkpoint(BaseMroWork::Publish).await?;
    Ok(collected)
}

pub(in crate::types) struct InlineBaseMroEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineBaseMroEffects<'db> {
    #[inline]
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineBaseMroEffects<'_> {}

impl<'db> BaseMroFacts<'db> for InlineBaseMroEffects<'db> {
    type Error = Infallible;
}

impl<'db> SynchronousBaseMroEffects<'db> for InlineBaseMroEffects<'db> {
    #[inline]
    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Infallible> {
        Ok(MroFieldReads::new(self.db).alias_origin(alias))
    }

    #[inline]
    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(MroFieldReads::new(self.db).alias_specialization(alias))
    }

    #[inline]
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Infallible> {
        Ok(ClassBase::object(self.db, env))
    }

    #[inline]
    fn checkpoint(&self, _work: BaseMroWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(base.apply_optional_specialization(self.db, Some(additional)))
    }

    #[inline]
    fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Infallible> {
        collect_start_sync(self.db, start, &InlineMroRootEffects::new(self.db))
    }

    #[inline]
    fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Infallible> {
        collect_start_with_root_sync(self.db, root, start, &InlineMroRootEffects::new(self.db))
    }
}

/// An iterator over the MRO of a class base.
#[derive(Clone)]
pub(in crate::types) enum ClassBaseMroIterator<'db> {
    Length2(core::array::IntoIter<ClassBase<'db>, 2>),
    Length3(core::array::IntoIter<ClassBase<'db>, 3>),
    FromClass(MroIterator<'db>),
}

impl<'db> Iterator for ClassBaseMroIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Length2(iter) => iter.next(),
            Self::Length3(iter) => iter.next(),
            Self::FromClass(iter) => iter.next(),
        }
    }
}

impl std::iter::FusedIterator for ClassBaseMroIterator<'_> {}
