//! Canonical redundancy preserves the identity guard and ordered pair producer.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::{TypeRelation, is_redundant_with_impl};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::typevar::TypeVarSet;
use crate::types::{Type, TypePair};
use crate::{Db, Program, ProgramEnvironment};

shared_semantic_family! {
    #[synchronous(SynchronousRedundancyComparisonEffects)]
    pub(in crate::types) trait RedundancyComparisonEffects<'db> {
        type Error;

        #[operation(local)]
        async fn same_type(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn canonical(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(compare_sync)]
    #[capabilities(effects = RedundancyComparisonEffects)]
    #[passive_values()]
    pub(in crate::types) async fn compare_with<'db, E: RedundancyComparisonEffects<'db>>(
        first: Type<'db>,
        second: Type<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if effects.same_type(first, second).await? {
            return Ok(true);
        }
        effects.canonical(first, second).await
    }

    #[synchronous(SynchronousRedundancyProducerEffects)]
    pub(in crate::types) trait RedundancyProducerEffects<'db> {
        type Error;

        #[operation(source)]
        async fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(source)]
        async fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn relate(&self, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = RedundancyProducerEffects)]
    #[passive_values()]
    pub(in crate::types) async fn produce_with<'db, E: RedundancyProducerEffects<'db>>(
        pair: TypePair<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let program = effects.program(pair).await?;
        let env = effects.environment(program).await?;
        let first = effects.first(pair).await?;
        let second = effects.second(pair).await?;
        effects.relate(&env, first, second).await
    }
}

pub(super) struct OrdinaryRedundancyComparison<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousRedundancyComparisonEffects<'db> for OrdinaryRedundancyComparison<'_, 'db> {
    type Error = Infallible;

    fn same_type(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        Ok(first == second)
    }

    fn canonical(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        let program = self.env.program(self.db);
        Ok(is_redundant_with_impl(
            self.db,
            TypePair::new(self.db, program, first, second),
        ))
    }
}

pub(super) struct OrdinaryRedundancyProducer<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousRedundancyProducerEffects<'db> for OrdinaryRedundancyProducer<'db> {
    type Error = Infallible;

    fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(pair.program(self.db))
    }

    fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.first(self.db))
    }

    fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.second(self.db))
    }

    fn relate(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first
            .has_relation_to(
                self.db,
                env,
                second,
                &ConstraintSetBuilder::new(),
                TypeVarSet::None,
                TypeRelation::Redundancy { pure: false },
            )
            .is_always_satisfied(self.db, env))
    }
}
