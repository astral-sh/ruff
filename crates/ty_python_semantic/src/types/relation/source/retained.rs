//! Queued comparisons borrow a checker whose original owners outlive the execution run.

use std::future::Future;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::{BorrowedClassPairs, BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::Db;
use crate::types::constraints::ConstraintSet;
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::stable_storage::StableStorage;
use crate::types::relation::{DisjointnessChecker, TypeRelation, TypeRelationChecker};
use crate::types::{ClassType, Type};

#[cfg(test)]
pub(in crate::types) mod observations;

/// Owns the access needed to recreate the existing borrowed effects inside a queued task.
/// Cloning a factory copies handles; it must not allocate or invoke database operations.
pub(in crate::types) trait RetainedRelationSource<'run, 'db: 'run>:
    Clone + 'run
{
    type Effects<'call>: RelationSourceEffects<'run, 'db>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_>;
}

pub(super) trait PairChildren<'run, 'db: 'run, 'c> {
    async fn pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>;

    async fn disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        checker: &DisjointnessChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>;

    async fn derived_disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>;

    async fn derived_subtyping_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        checker: &DisjointnessChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>;
}

pub(super) struct UnavailablePairs;

impl<'run, 'db: 'run, 'c> PairChildren<'run, 'db, 'c> for UnavailablePairs {
    async fn pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &TypeRelationChecker<'_, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }

    async fn disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &DisjointnessChecker<'_, 'c, 'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }

    async fn derived_disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let endpoint = effects.endpoint();
        let checker = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<DisjointnessChecker<'_, 'c, 'db>>() * 2)?;
                Ok(checker.as_disjointness_checker())
            })
            .await;
        BorrowedPairs {
            children: self,
            db,
            endpoint,
            effects,
            constraints: checker.constraints,
        }
        .disjoint_pair(&checker, left, right)
        .await
    }

    async fn derived_subtyping_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &DisjointnessChecker<'_, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }
}

pub(in crate::types) struct CheckerStorage<'a, 'c, 'db> {
    storage: StableStorage<TypeRelationChecker<'a, 'c, 'db>>,
    disjointness: StableStorage<DisjointnessChecker<'a, 'c, 'db>>,
}

impl<'a, 'c, 'db> CheckerStorage<'a, 'c, 'db> {
    pub(in crate::types) fn new() -> Self {
        Self {
            storage: StableStorage::new(),
            disjointness: StableStorage::new(),
        }
    }

    #[cfg(test)]
    pub(in crate::types) fn retained_payload(&self) -> Option<(usize, usize)> {
        let (relations, relation_bytes) = self.storage.retained_payload()?;
        let (disjointness, disjointness_bytes) = self.disjointness.retained_payload()?;
        Some((
            relations.checked_add(disjointness)?,
            relation_bytes.checked_add(disjointness_bytes)?,
        ))
    }

    /// Retains a checker and admits its factory, result transfers, and eventual destruction.
    /// The factory borrows already retained owners and does not allocate additional payloads.
    pub(in crate::types) async fn allocate_with_fixed_transfers<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        make: impl FnOnce() -> TypeRelationChecker<'a, 'c, 'db> + 'call,
    ) -> RunResult<RetainedChecker<'pool, 'a, 'c, 'db>>
    where
        'a: 'pool,
        'c: 'pool,
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        local_with_fixed_transfers_at(endpoint, 2, 0, || {
            self.allocate_checker(endpoint, make)
        }).await?
    }

    /// Allocates the checker from existing owner references; callers admit their local carriers.
    fn allocate_checker<'pool>(
        &'pool self,
        endpoint: &TaskEndpoint<'_, 'db>,
        make: impl FnOnce() -> TypeRelationChecker<'a, 'c, 'db>,
    ) -> RunResult<RetainedChecker<'pool, 'a, 'c, 'db>>
    where
        'a: 'pool,
        'c: 'pool,
    {
        let checker = self.storage.allocate_admitted(endpoint, 2, make)?;
        Ok(RetainedChecker { checker, storage: self })
    }

    /// The constructor uses already retained owners and performs no heap allocation or callout.
    /// Mutable visitor and diagnostic descendants still require their own admission.
    pub(in crate::types) fn allocate<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        make: impl FnOnce() -> TypeRelationChecker<'a, 'c, 'db> + 'call,
    ) -> impl Future<Output = RetainedChecker<'pool, 'a, 'c, 'db>> + 'call
    where
        'a: 'pool,
        'c: 'pool,
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(|| {
            let requested_bytes = size_of::<RetainedChecker<'pool, 'a, 'c, 'db>>()
                .checked_mul(2)
                .ok_or(RunError::Contract("retained checker quotation overflow"))?;
            endpoint.admit_work(2)?;
            endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
            self.allocate_checker(endpoint, make)
        })
    }

    /// The constructor shares the original owners; mutable descendants require admission.
    pub(in crate::types) fn allocate_disjointness<'pool, 'call, 'run>(
        &'pool self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        make: impl FnOnce() -> DisjointnessChecker<'a, 'c, 'db> + 'call,
    ) -> impl Future<Output = RetainedDisjointnessChecker<'pool, 'a, 'c, 'db>> + 'call
    where
        'a: 'pool,
        'c: 'pool,
        'db: 'run,
        'pool: 'call,
        'run: 'call,
    {
        endpoint.local_call(|| {
            let work = size_of::<DisjointnessChecker<'a, 'c, 'db>>()
                .checked_mul(2)
                .ok_or(RunError::Contract("retained checker quotation overflow"))?;
            let checker = self.disjointness.allocate_admitted(endpoint, work, make)?;
            Ok(RetainedDisjointnessChecker {
                checker,
                storage: self,
            })
        })
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct RetainedChecker<'pool, 'a, 'c, 'db> {
    checker: &'pool TypeRelationChecker<'a, 'c, 'db>,
    storage: &'pool CheckerStorage<'a, 'c, 'db>,
}

impl<'pool, 'a, 'c, 'db> RetainedChecker<'pool, 'a, 'c, 'db> {
    fn checked(
        self,
        supplied: &TypeRelationChecker<'_, 'c, 'db>,
    ) -> RunResult<&'pool TypeRelationChecker<'a, 'c, 'db>> {
        if std::ptr::addr_eq(
            std::ptr::from_ref(self.checker),
            std::ptr::from_ref(supplied),
        ) {
            // Return the independently retained reference, never extend the supplied borrow.
            Ok(self.checker)
        } else {
            Err(RunError::Contract("relation checker is not retained"))
        }
    }

    pub(in crate::types) async fn pair<'run, R>(
        self,
        db: &'db dyn Db,
        access: R,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        'pool: 'run,
        'a: 'run,
        'c: 'run,
        'db: 'run,
        R: RetainedRelationSource<'run, 'db>,
    {
        let children = QueuedPairs {
            checker: RetainedComparison::Relation(self),
            access,
        };
        let effects = children.access.effects();
        children
            .pair(db, &effects, self.checker, source, target)
            .await
    }

    /// Queues a type comparison after admitting its retained access and borrowed effect carriers.
    pub(in crate::types) async fn pair_with_fixed_transfers<'run, R>(
        self,
        db: &'db dyn Db,
        endpoint: &TaskEndpoint<'run, 'db>,
        access: R,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        'pool: 'run,
        'a: 'run,
        'c: 'run,
        'db: 'run,
        R: RetainedRelationSource<'run, 'db>,
    {
        let children = local_with_fixed_transfers_at(endpoint, 2, 0, || QueuedPairs {
            checker: RetainedComparison::Relation(self),
            access,
        }).await?;
        let effects = local_with_fixed_transfers_at(endpoint, 1, 0, || {
            children.access.effects()
        }).await?;
        children.pair(db, &effects, self.checker, source, target).await
    }

    /// Queues a direct class comparison with this checker's original constraints and visitors.
    /// Admits its carrier and borrowed effects before constructing them.
    pub(in crate::types) async fn class_pair<'run, R>(
        self,
        db: &'db dyn Db,
        endpoint: &TaskEndpoint<'run, 'db>,
        access: R,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        'pool: 'run,
        'a: 'run,
        'c: 'run,
        'db: 'run,
        R: RetainedRelationSource<'run, 'db>,
    {
        endpoint
            .local_call(|| {
                let requested_bytes = size_of::<QueuedPairs<'pool, 'a, 'c, 'db, R>>()
                    .checked_add(size_of::<R::Effects<'_>>())
                    .ok_or(RunError::Contract("direct class entry quotation overflow"))?;
                endpoint.admit_work(3)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })
            })
            .await;
        let children = QueuedPairs {
            checker: RetainedComparison::Relation(self),
            access,
        };
        let effects = children.access.effects();
        let retained = effects
            .endpoint()
            .local_call(|| {
                effects.endpoint().admit_work(1)?;
                let checker = children.checker.relation(self.checker)?;
                QueuedComparison::admit_transfer(effects.endpoint())?;
                Ok(checker)
            })
            .await;
        children
            .enqueue(
                db,
                &effects,
                QueuedComparison::Relation {
                    checker: retained,
                    operands: RelationOperands::Classes { source, target },
                },
            )
            .await
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct RetainedDisjointnessChecker<'pool, 'a, 'c, 'db> {
    checker: &'pool DisjointnessChecker<'a, 'c, 'db>,
    storage: &'pool CheckerStorage<'a, 'c, 'db>,
}

impl<'pool, 'a, 'c, 'db> RetainedDisjointnessChecker<'pool, 'a, 'c, 'db> {
    fn checked(
        self,
        supplied: &DisjointnessChecker<'_, 'c, 'db>,
    ) -> RunResult<&'pool DisjointnessChecker<'a, 'c, 'db>> {
        if std::ptr::addr_eq(
            std::ptr::from_ref(self.checker),
            std::ptr::from_ref(supplied),
        ) {
            // Return the independently retained reference, never extend the supplied borrow.
            Ok(self.checker)
        } else {
            Err(RunError::Contract("disjointness checker is not retained"))
        }
    }

    pub(in crate::types) async fn disjoint_pair<'run, R>(
        self,
        db: &'db dyn Db,
        access: R,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        'pool: 'run,
        'a: 'run,
        'c: 'run,
        'db: 'run,
        R: RetainedRelationSource<'run, 'db>,
    {
        let children = QueuedPairs {
            checker: RetainedComparison::Disjointness(self),
            access,
        };
        let effects = children.access.effects();
        children
            .disjoint_pair(db, &effects, self.checker, left, right)
            .await
    }
}

#[derive(Clone, Copy)]
enum RetainedComparison<'pool, 'a, 'c, 'db> {
    Relation(RetainedChecker<'pool, 'a, 'c, 'db>),
    Disjointness(RetainedDisjointnessChecker<'pool, 'a, 'c, 'db>),
}

impl<'pool, 'a, 'c, 'db> RetainedComparison<'pool, 'a, 'c, 'db> {
    fn relation(
        self,
        supplied: &TypeRelationChecker<'_, 'c, 'db>,
    ) -> RunResult<RetainedChecker<'pool, 'a, 'c, 'db>> {
        let Self::Relation(retained) = self else {
            return Err(RunError::Contract("relation checker is not retained"));
        };
        retained.checked(supplied)?;
        Ok(retained)
    }

    fn disjointness(
        self,
        supplied: &DisjointnessChecker<'_, 'c, 'db>,
    ) -> RunResult<RetainedDisjointnessChecker<'pool, 'a, 'c, 'db>> {
        let Self::Disjointness(retained) = self else {
            return Err(RunError::Contract("disjointness checker is not retained"));
        };
        retained.checked(supplied)?;
        Ok(retained)
    }
}

/// Keeps the direct class entry distinct from the general type-pair entry.
/// Like `ClassType::has_relation_to`, it enters `check_class_pair` directly.
#[derive(Clone, Copy, Debug)]
enum RelationOperands<'db> {
    Types {
        source: Type<'db>,
        target: Type<'db>,
    },
    Classes {
        source: ClassType<'db>,
        target: ClassType<'db>,
    },
}

enum QueuedComparison<'pool, 'a, 'c, 'db> {
    Relation {
        checker: RetainedChecker<'pool, 'a, 'c, 'db>,
        operands: RelationOperands<'db>,
    },
    Disjointness {
        checker: RetainedDisjointnessChecker<'pool, 'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    },
}

impl<'pool, 'a, 'c, 'db> QueuedComparison<'pool, 'a, 'c, 'db> {
    /// Admits construction and source-level transfers of this operand/checker carrier.
    /// Six transfers follow construction: into `enqueue`, the `child_call` factory, its request
    /// future, the `demand` factory, its queued future, and the operation future.
    /// The nested `RelationOperands` is included; runtime task storage is admitted separately.
    fn admit_transfer(endpoint: &TaskEndpoint<'_, 'db>) -> RunResult<()> {
        let requested_bytes = size_of::<Self>()
            .checked_mul(7)
            .ok_or(RunError::Contract("queued comparison transfer quotation overflow"))?;
        endpoint.admit_work(7)?;
        endpoint.admit(ExecutionWork::Resource { requested_bytes })
    }

    const fn checker(&self) -> RetainedComparison<'pool, 'a, 'c, 'db> {
        match self {
            Self::Relation { checker, operands: _ } => RetainedComparison::Relation(*checker),
            Self::Disjointness {
                checker,
                left: _,
                right: _,
            } => RetainedComparison::Disjointness(*checker),
        }
    }
}

struct QueuedPairs<'pool, 'a, 'c, 'db, R> {
    checker: RetainedComparison<'pool, 'a, 'c, 'db>,
    access: R,
}

impl<'run, 'pool: 'run, 'a: 'run, 'c: 'run, 'db: 'run, R: RetainedRelationSource<'run, 'db>>
    PairChildren<'run, 'db, 'c> for QueuedPairs<'pool, 'a, 'c, 'db, R>
{
    async fn pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        supplied: &TypeRelationChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let checker = local_with_fixed_transfers_at(effects.endpoint(), 1, 0, || {
            let checker = self.checker.relation(supplied)?;
            QueuedComparison::admit_transfer(effects.endpoint())?;
            Ok(checker)
        }).await??;
        self.enqueue(
            db,
            effects,
            QueuedComparison::Relation {
                checker,
                operands: RelationOperands::Types { source, target },
            },
        )
        .await
    }

    async fn disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        supplied: &DisjointnessChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let checker = effects
            .endpoint()
            .local_call(|| {
                effects.endpoint().admit_work(1)?;
                let checker = self.checker.disjointness(supplied)?;
                QueuedComparison::admit_transfer(effects.endpoint())?;
                Ok(checker)
            })
            .await;
        self.enqueue(
            db,
            effects,
            QueuedComparison::Disjointness { checker, left, right },
        )
        .await
    }

    async fn derived_disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        supplied: &TypeRelationChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let endpoint = effects.endpoint();
        let retained = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let checker = self.checker.relation(supplied)?;
                QueuedComparison::admit_transfer(endpoint)?;
                Ok(checker)
            })
            .await;
        let checker = retained
            .storage
            .allocate_disjointness(endpoint, || retained.checker.as_disjointness_checker())
            .await;
        self.enqueue(
            db,
            effects,
            QueuedComparison::Disjointness { checker, left, right },
        )
        .await
    }

    async fn derived_subtyping_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        supplied: &DisjointnessChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let endpoint = effects.endpoint();
        let retained = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let checker = self.checker.disjointness(supplied)?;
                QueuedComparison::admit_transfer(endpoint)?;
                Ok(checker)
            })
            .await;
        let checker = retained
            .storage
            .allocate(endpoint, || {
                retained
                    .checker
                    .as_relation_checker(TypeRelation::Subtyping)
            })
            .await;
        self.enqueue(
            db,
            effects,
            QueuedComparison::Relation {
                checker,
                operands: RelationOperands::Types { source, target },
            },
        )
        .await
    }
}

impl<'pool, 'a, 'c, 'db, R> QueuedPairs<'pool, 'a, 'c, 'db, R> {
    async fn enqueue<'run, E: RelationSourceEffects<'run, 'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
        comparison: QueuedComparison<'pool, 'a, 'c, 'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        'pool: 'run,
        'a: 'run,
        'c: 'run,
        'db: 'run,
        R: RetainedRelationSource<'run, 'db>,
    {
        let endpoint = effects.endpoint();
        let (children, child_endpoint, has_observations) =
            local_with_fixed_transfers_at(endpoint, 4, 0, || {
                if let QueuedComparison::Relation {
                    operands: RelationOperands::Classes { .. },
                    ..
                } = &comparison
                {
                    let requested_bytes = size_of::<R::Effects<'_>>()
                        .checked_add(size_of::<
                            BorrowedPairs<'_, 'run, 'db, 'c, R::Effects<'_>, Self>,
                        >())
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<
                                BorrowedClassPairs<'_, '_, 'run, 'db, 'c, R::Effects<'_>, Self>,
                            >())
                        })
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<ClassType<'db>>().checked_mul(4)?)
                        })
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<ConstraintSet<'db, 'c>>().checked_mul(2)?)
                        })
                        .ok_or(RunError::Contract("queued class comparison quotation overflow"))?;
                    endpoint.admit_work(8)?;
                    endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                }
                let checker = comparison.checker();
                let has_observations = match checker {
                    RetainedComparison::Relation(checker) => checker.checker.observations.is_some(),
                    RetainedComparison::Disjointness(checker) => checker.checker.observations.is_some(),
                };
                Ok((
                    QueuedPairs {
                        checker,
                        access: self.access.clone(),
                    },
                    endpoint.clone(),
                    has_observations,
                ))
            })
            .await??;
        if has_observations {
            return effects
                .unavailable(RelationSourceOperation::CheckerMode)
                .await;
        }
        Ok(endpoint
            .child_call(|| async move {
                endpoint
                    .demand(move || async move {
                        let operation = async move {
                            let effects = local_with_fixed_transfers_at(&child_endpoint, 1, 0, || {
                                children.access.effects()
                            }).await?;
                            let pairs = local_with_fixed_transfers_at(&child_endpoint, 3, 0, || BorrowedPairs {
                                db,
                                endpoint: &child_endpoint,
                                effects: &effects,
                                constraints: match children.checker {
                                    RetainedComparison::Relation(checker) => checker.checker.constraints,
                                    RetainedComparison::Disjointness(checker) => checker.checker.constraints,
                                },
                                children: &children,
                            }).await?;
                            match comparison {
                                QueuedComparison::Relation {
                                    checker,
                                    operands: RelationOperands::Types { source, target },
                                } => {
                                    #[cfg(test)]
                                    super::resources::observations::observe_assignability_pair(checker.checker);
                                    pairs.pair(db, checker.checker, source, target).await
                                }
                                QueuedComparison::Relation {
                                    checker,
                                    operands: RelationOperands::Classes { source, target },
                                } => {
                                    #[cfg(test)]
                                    super::resources::observations::observe_class_condition_pair(checker.checker);
                                    pairs.check_class_pair(checker.checker, source, target).await
                                }
                                QueuedComparison::Disjointness { checker, left, right } => {
                                    pairs.disjoint_pair(checker.checker, left, right).await
                                }
                            }
                        };
                        #[cfg(test)]
                        return observations::observe(db, operation).await;
                        #[cfg(not(test))]
                        operation.await
                    })?
                    .await
            })
            .await)
    }
}
