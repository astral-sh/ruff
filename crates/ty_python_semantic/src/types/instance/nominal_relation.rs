//! Nominal instance comparison selects the existing tuple or class relation.

use std::convert::Infallible;

use super::{NominalInstanceInner, NominalInstanceType};
use crate::Db;
use crate::types::ClassType;
use crate::types::constraints::ConstraintSet;
use crate::types::relation::TypeRelationChecker;
use crate::types::tuple::TupleType;

pub(in crate::types) struct NominalPairFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousNominalPairEffects)]
    pub(in crate::types) trait NominalPairEffects<'c, 'db: 'c> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn always(&self) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn tuple_pair(&self, source: TupleType<'db>, target: TupleType<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn class(&self, instance: NominalInstanceType<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn class_pair(&self, source: ClassType<'db>, target: ClassType<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[finite_capability]
    impl NominalPairFacts {
        fn inner<'db>(&self, instance: NominalInstanceType<'db>) -> NominalInstanceInner<'db> {
            instance.0
        }
    }

    #[synchronous(check_nominal_pair_sync)]
    #[capabilities(effects = NominalPairEffects, facts = NominalPairFacts)]
    #[passive_values()]
    pub(in crate::types) async fn check_nominal_pair_with<'c, 'db: 'c, E: NominalPairEffects<'c, 'db>>(
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
        facts: NominalPairFacts,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        effects.checkpoint().await?;
        match (facts.inner(source), facts.inner(target)) {
            (_, NominalInstanceInner::Object) => effects.always().await,
            (
                NominalInstanceInner::ExactTuple(source_tuple),
                NominalInstanceInner::ExactTuple(target_tuple),
            ) => effects.tuple_pair(source_tuple, target_tuple).await,
            _ => {
                let source = effects.class(source).await?;
                let target = effects.class(target).await?;
                effects.class_pair(source, target).await
            }
        }
    }
}

pub(super) struct InlineNominalPairs<'check, 'a, 'c, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'c, 'db: 'c> SynchronousNominalPairEffects<'c, 'db> for InlineNominalPairs<'_, '_, 'c, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn always(&self) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.always())
    }

    fn tuple_pair(
        &self,
        source: TupleType<'db>,
        target: TupleType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_tuple_type_pair(self.db, source, target))
    }

    fn class(&self, instance: NominalInstanceType<'db>) -> Result<ClassType<'db>, Infallible> {
        Ok(instance.class(self.db, self.checker.env))
    }

    fn class_pair(
        &self,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_class_pair(self.db, source, target))
    }
}
