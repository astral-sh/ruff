//! Numeric-tower unions with canonical known-instance and union-construction dependencies.

use std::convert::Infallible;

use super::{KnownUnion, UnionType};
use crate::types::{KnownClass, Type};
use crate::{Db, ProgramEnvironment};

/// Identifies the bounded recipe dispatch and final handle transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum NumericUnionWork {
    Dispatch,
    Publish,
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies the known instances and the original two- or three-element union builder.
    #[synchronous(SynchronousNumericUnionEffects)]
    pub(in crate::types) trait NumericUnionEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: NumericUnionWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn known_instance(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_two(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_three(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>, third: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    /// Builds `int | float` or `int | float | complex`, requesting instances in that order.
    /// The two recipes preserve their distinct union builders and delegate all normalization.
    #[synchronous(numeric_union_sync)]
    #[capabilities(effects = NumericUnionEffects)]
    #[passive_values(NumericUnionWork::Dispatch, NumericUnionWork::Publish, KnownClass::Int, KnownClass::Float, KnownClass::Complex)]
    pub(in crate::types) async fn numeric_union_with<'db, E: NumericUnionEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: KnownUnion,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint(NumericUnionWork::Dispatch).await?;
        let integer = effects.known_instance(db, env, KnownClass::Int).await?;
        let float = effects.known_instance(db, env, KnownClass::Float).await?;
        let result = match union {
            KnownUnion::Float => effects.union_two(db, env, integer, float).await?,
            KnownUnion::Complex => {
                let complex = effects.known_instance(db, env, KnownClass::Complex).await?;
                effects.union_three(db, env, integer, float, complex).await?
            }
        };
        effects.checkpoint(NumericUnionWork::Publish).await?;
        Ok(result)
    }
}

/// Uses ordinary canonical queries and the original union builders synchronously.
#[derive(Clone, Copy, Debug)]
pub(super) struct InlineNumericUnionEffects;

impl<'db> SynchronousNumericUnionEffects<'db> for InlineNumericUnionEffects {
    type Error = Infallible;

    fn checkpoint(&self, _work: NumericUnionWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn known_instance(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(db, env))
    }

    fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(db, env, first, second))
    }

    fn union_three(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
        third: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_elements(db, env, [first, second, third]))
    }
}
