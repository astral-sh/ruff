//! TypeVar subclass checks borrow the caller's `BorrowedPairs` and `TypeRelationChecker`.
//!
//! `BorrowedPairs` keeps recursive comparisons on the existing `PairChildren` path, using the
//! caller's constraint storage and relation visit state. This adapter supports the initial
//! TypeVar inspection; semantic dependencies without controlled implementations explicitly
//! refuse before performing their operations.

use salsa::execution_probe::{RunError, RunResult};

use super::{BorrowedPairs, PairChildren, RelationSourceEffects, RelationSourceOperation};
use crate::types::constraints::ConstraintSet;
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::typevar_subclass::TypeVarSubclassEffects;
use crate::types::{BoundTypeVarInstance, InstanceProjection, SubclassOfType, Type};

pub(super) struct BorrowedTypeVarSubclass<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pub(super) pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    pub(super) checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'run, 'db: 'run + 'c, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    TypeVarSubclassEffects<'c, 'db> for BorrowedTypeVarSubclass<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;

    async fn source_typevar(
        &self,
        source: SubclassOfType<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                self.pairs.endpoint.check_completion()?;
                Ok(source.into_type_var())
            })
            .await)
    }

    async fn exact_upper_bound(
        &self,
        _source: SubclassOfType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::TypeVarSubclassUpperBound)
            .await
    }

    async fn is_metaclass_instance(&self, _target: Type<'db>) -> RunResult<bool> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::TypeVarSubclassMetaclassTarget)
            .await
    }

    async fn metaclass_instance(&self, source: SubclassOfType<'db>) -> RunResult<Type<'db>> {
        self.pairs
            .subclass_metaclass_instance(self.checker, source)
            .await
    }

    async fn instance_projection(
        &self,
        _target: Type<'db>,
    ) -> RunResult<Option<InstanceProjection<Type<'db>>>> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::TypeVarSubclassInstanceProjection)
            .await
    }

    async fn transposed_typevar(
        &self,
        _source: SubclassOfType<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::TypeVarSubclassTranspose)
            .await
    }

    async fn compare(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .check_type_pair(self.checker, source, target)
            .await
    }
}
