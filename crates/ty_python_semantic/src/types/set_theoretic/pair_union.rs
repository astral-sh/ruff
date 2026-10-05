//! Canonical union-pair field reads and builder operations in their original order.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::{Type, TypePair, UnionBuilder};
use crate::{Db, Program, ProgramEnvironment};

pub(super) struct OrdinaryPairUnionEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousPairUnionEffects)]
    pub(in crate::types) trait PairUnionEffects<'db> {
        type Error;

        #[operation(source)]
        async fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(local)]
        async fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = PairUnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn produce_with<'db, E: PairUnionEffects<'db>>(
        pair: TypePair<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let program = effects.program(pair).await?;
        let env = effects.environment(program).await?;
        let mut builder = effects.new_union(&env).await?;
        let first = effects.first(pair).await?;
        effects.union_add(&mut builder, first).await?;
        let second = effects.second(pair).await?;
        effects.union_add(&mut builder, second).await?;
        effects.union_build(builder).await
    }
}

impl<'db> SynchronousPairUnionEffects<'db> for OrdinaryPairUnionEffects<'db> {
    type Error = Infallible;

    fn program(&self, pair: TypePair<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(pair.program(self.db))
    }

    fn environment(&self, program: Program<'db>) -> Result<ProgramEnvironment<'db>, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error> {
        Ok(UnionBuilder::new(self.db, env))
    }

    fn first(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.first(self.db))
    }

    fn second(&self, pair: TypePair<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(pair.second(self.db))
    }

    fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error> {
        builder.add_in_place(ty);
        Ok(())
    }

    fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(builder.build())
    }
}
