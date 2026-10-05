use salsa::execution_probe::{RunError, RunResult};

use super::tuple::BorrowedTuplePairs;
use super::{BorrowedPairs, PairChildren, PairEffects, RelationSourceEffects};
use crate::types::constraints::ConstraintSet;
use crate::types::instance::nominal_relation::NominalPairEffects;
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::relation::TypeRelationChecker;
use crate::types::tuple::TupleType;
use crate::types::tuple::relation::{TupleRelationFacts, check_tuple_pair_with};
use crate::types::{ClassType, NominalInstanceType};

pub(super) struct BorrowedNominalPairs<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pub(super) pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    pub(super) checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'run, 'db: 'run + 'c, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    NominalPairEffects<'c, 'db> for BorrowedNominalPairs<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(3)?;
                self.pairs.endpoint.check_completion()
            })
            .await)
    }

    async fn always(&self) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.check_completion()?;
                Ok(self.checker.always())
            })
            .await)
    }

    async fn tuple_pair(
        &self,
        source: TupleType<'db>,
        target: TupleType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let effects = local_with_fixed_transfers_at(self.pairs.endpoint, 1, 0, || {
            BorrowedTuplePairs {
                pairs: self.pairs,
                checker: self.checker,
            }
        })
        .await?;
        check_tuple_pair_with(
            source,
            target,
            &effects,
            TupleRelationFacts,
        )
        .await
    }

    async fn class(&self, instance: NominalInstanceType<'db>) -> RunResult<ClassType<'db>> {
        self.pairs
            .effects
            .nominal_class(self.checker.env, instance)
            .await
    }

    async fn class_pair(
        &self,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::check_class_pair(self.pairs, self.checker, source, target).await
    }
}
