//! Dependencies of public type promotion and its final, top-level singleton reduction.

use std::convert::Infallible;
use std::future::Future;

use super::enums::is_single_member_enum;
use super::{ClassLiteral, NominalInstanceType, Type, UnionType};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) mod classification;
pub(in crate::types) mod leaf;

/// These reservations cover fixed-sized steps. Recursive mapping and union normalization
/// remain separate dependencies with their own work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicPromotionWork {
    Admission,
    SingletonDispatch,
    /// Dispatch singleton classification; its field reads and enum metadata are separate dependencies.
    SingletonClassification,
    UnionRequest,
}

pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

pub(crate) trait PublicPromotionFacts<'db>: sealed::Sealed {
    type Error;

    fn enum_singleton(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;
}

pub(crate) trait PublicPromotionEffects<'db>: sealed::Sealed {
    type Error;

    fn checkpoint(
        &self,
        work: PublicPromotionWork,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn regular(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn is_singleton(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;
}

pub(crate) trait SynchronousPublicPromotionEffects<'db>: PublicPromotionFacts<'db> {
    fn checkpoint(&self, work: PublicPromotionWork) -> Result<(), Self::Error>;

    fn regular(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    fn is_singleton(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(crate) fn inline_public_promotion_result<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(crate) struct InlinePublicPromotionEffects;

impl sealed::Sealed for InlinePublicPromotionEffects {}

impl<'db> PublicPromotionFacts<'db> for InlinePublicPromotionEffects {
    type Error = Infallible;

    fn enum_singleton(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(is_single_member_enum(db, class))
    }
}

impl<'db> SynchronousPublicPromotionEffects<'db> for InlinePublicPromotionEffects {
    fn checkpoint(&self, _work: PublicPromotionWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn regular(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.promote(db, env))
    }

    fn is_singleton(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Infallible> {
        classification::classify_singleton_sync(
            instance,
            classification::SingletonFacts,
            &classification::OrdinarySingletonEffects { db, facts: self },
        )
    }

    fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        self.checkpoint(PublicPromotionWork::UnionRequest)?;
        Ok(UnionType::from_two_elements(db, env, first, second))
    }
}
