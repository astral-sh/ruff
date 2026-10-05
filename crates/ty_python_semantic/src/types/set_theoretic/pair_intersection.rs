//! Canonical intersection-pair field reads and builder operations in their original order.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::{IntersectionBuilder, Type, TypePair};
use crate::{Db, Program, ProgramEnvironment};

pub(super) struct OrdinaryPairIntersectionEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousPairIntersectionEffects)]
    pub(in crate::types) trait PairIntersectionEffects<'db> {
        type Error;

        #[operation(source)]
        async fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(local)]
        async fn new_intersection(&self, env: &ProgramEnvironment<'db>) -> Result<IntersectionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn intersection_add(&self, builder: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn intersection_build(&self, builder: IntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = PairIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn produce_with<'db, E: PairIntersectionEffects<'db>>(
        pair: TypePair<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let program = effects.program(pair).await?;
        let env = effects.environment(program).await?;
        let mut builder = effects.new_intersection(&env).await?;
        let first = effects.first(pair).await?;
        let second = effects.second(pair).await?;
        effects.intersection_add(&mut builder, first).await?;
        effects.intersection_add(&mut builder, second).await?;
        effects.intersection_build(builder).await
    }
}

impl<'db> SynchronousPairIntersectionEffects<'db> for OrdinaryPairIntersectionEffects<'db> {
    type Error = Infallible;

    fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(pair.program(self.db))
    }

    fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<IntersectionBuilder<'db>, Self::Error> {
        Ok(IntersectionBuilder::new(self.db, env))
    }

    fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.first(self.db))
    }

    fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.second(self.db))
    }

    fn intersection_add(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.add_positive_in_place(ty);
        Ok(())
    }

    fn intersection_build(
        &self,
        builder: IntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.build())
    }
}
