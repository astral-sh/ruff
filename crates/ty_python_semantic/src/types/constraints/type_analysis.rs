//! Bound classification and caching of completed bound-depth results.

use super::variables::Constraint;
use super::{ConstraintId, ConstraintSetStorage, max_constructor_and_typevar_depth};
use crate::types::visitor::{OrdinaryTypeWalk, TypeSearchMode, Unrestricted, search_type_sync};
use crate::types::{BoundTypeVarInstance, DynamicType, MaterializationKind, Type};
use crate::{Db, ProgramEnvironment};
use std::convert::Infallible;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoundSearch {
    TypeVar,
    UnspecializedTypeVar,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstraintTypeWork {
    Classification,
    ConstraintDepth,
    DepthCacheLookup,
    DepthCachePublish,
}
pub(super) trait ConstraintTypeEffects<'db> {
    type Error;
    async fn checkpoint(&mut self, work: ConstraintTypeWork) -> Result<(), Self::Error>;
    async fn search_bound(
        &mut self,
        bound: Type<'db>,
        search: BoundSearch,
    ) -> Result<bool, Self::Error>;
    async fn materialize_bound(
        &mut self,
        bound: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error>;
    async fn type_depth(&mut self, bound: Type<'db>) -> Result<(u16, u16), Self::Error>;
}
pub(super) trait ConstraintDepthCacheEffects<'db>: ConstraintTypeEffects<'db> {
    async fn depth_cache_get(
        &mut self,
        id: ConstraintId,
    ) -> Result<Option<(u16, u16)>, Self::Error>;
    async fn depth_constraint(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Self::Error>;
    async fn depth_cache_publish(
        &mut self,
        id: ConstraintId,
        depth: (u16, u16),
    ) -> Result<(), Self::Error>;
}
pub(super) trait SyncConstraintTypeEffects<'db> {
    type Error;
    fn checkpoint(&mut self, work: ConstraintTypeWork) -> Result<(), Self::Error>;
    fn search_bound(&mut self, bound: Type<'db>, search: BoundSearch) -> Result<bool, Self::Error>;
    fn materialize_bound(
        &mut self,
        bound: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error>;
    fn type_depth(&mut self, bound: Type<'db>) -> Result<(u16, u16), Self::Error>;
}
pub(super) trait SyncConstraintDepthCacheEffects<'db>:
    SyncConstraintTypeEffects<'db>
{
    fn depth_cache_get(&mut self, id: ConstraintId) -> Result<Option<(u16, u16)>, Self::Error>;
    fn depth_constraint(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Self::Error>;
    fn depth_cache_publish(
        &mut self,
        id: ConstraintId,
        depth: (u16, u16),
    ) -> Result<(), Self::Error>;
}
#[ty_mapping_probe_macros::dual_constraint_type]
pub(super) async fn constraint_as_concrete_with<'db, E: ConstraintTypeEffects<'db>>(
    constraint: Constraint<'db>,
    effects: &mut E,
) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
    effects
        .checkpoint(ConstraintTypeWork::Classification)
        .await?;
    let (typevar, bound) = match constraint {
        Constraint::ConcreteLower(this) => (this.typevar, this.bound),
        Constraint::ConcreteUpper(this) => (this.typevar, this.bound),
        Constraint::ConcreteEquivalence(this) => (this.typevar, this.bound),
        Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => return Ok(None),
    };
    if effects.search_bound(bound, BoundSearch::TypeVar).await? {
        return Ok(None);
    }
    if effects
        .search_bound(bound, BoundSearch::UnspecializedTypeVar)
        .await?
    {
        return Ok(None);
    }
    let bottom = effects
        .materialize_bound(bound, MaterializationKind::Bottom)
        .await?;
    let top = effects
        .materialize_bound(bound, MaterializationKind::Top)
        .await?;
    Ok((bottom == top).then_some(typevar))
}
#[ty_mapping_probe_macros::dual_constraint_type]
pub(super) async fn constraint_bound_depth_with<'db, E: ConstraintTypeEffects<'db>>(
    constraint: Constraint<'db>,
    effects: &mut E,
) -> Result<(u16, u16), E::Error> {
    effects
        .checkpoint(ConstraintTypeWork::ConstraintDepth)
        .await?;
    let bound = match constraint {
        Constraint::ConcreteLower(this) => this.bound,
        Constraint::ConcreteUpper(this) => this.bound,
        Constraint::ConcreteEquivalence(this) => this.bound,
        Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => return Ok((0, 0)),
    };
    effects.type_depth(bound).await
}
#[ty_mapping_probe_macros::dual_constraint_type]
pub(super) async fn cached_constraint_bound_depth_with<'db, E: ConstraintDepthCacheEffects<'db>>(
    id: ConstraintId,
    effects: &mut E,
) -> Result<(u16, u16), E::Error> {
    effects
        .checkpoint(ConstraintTypeWork::DepthCacheLookup)
        .await?;
    if let Some(depth) = effects.depth_cache_get(id).await? {
        return Ok(depth);
    }
    let constraint = effects.depth_constraint(id).await?;
    let depth = constraint_bound_depth_with(constraint, effects).await?;
    effects
        .checkpoint(ConstraintTypeWork::DepthCachePublish)
        .await?;
    effects.depth_cache_publish(id, depth).await?;
    Ok(depth)
}

pub(super) struct OrdinaryConstraintTypes<'env, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
}
impl<'db> SyncConstraintTypeEffects<'db> for OrdinaryConstraintTypes<'_, 'db> {
    type Error = Infallible;
    fn checkpoint(&mut self, _: ConstraintTypeWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn search_bound(&mut self, bound: Type<'db>, search: BoundSearch) -> Result<bool, Infallible> {
        let mode = match search {
            BoundSearch::TypeVar => TypeSearchMode::SkipLazyAttributes,
            BoundSearch::UnspecializedTypeVar => TypeSearchMode::IncludeAliasArguments,
        };
        let query = |ty| match search {
            BoundSearch::TypeVar => matches!(ty, Type::TypeVar(_)),
            BoundSearch::UnspecializedTypeVar => {
                matches!(ty, Type::Dynamic(DynamicType::UnspecializedTypeVar))
            }
        };
        search_type_sync(
            bound,
            mode,
            crate::types::visitor::TypeWalkFacts,
            &mut OrdinaryTypeWalk {
                db: self.db,
                env: self.env,
                query,
                control: &mut Unrestricted,
            },
        )
    }
    fn materialize_bound(
        &mut self,
        bound: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Infallible> {
        Ok(match kind {
            MaterializationKind::Bottom => bound.bottom_materialization(self.db, self.env),
            MaterializationKind::Top => bound.top_materialization(self.db, self.env),
        })
    }
    fn type_depth(&mut self, bound: Type<'db>) -> Result<(u16, u16), Infallible> {
        Ok(max_constructor_and_typevar_depth(self.db, self.env, bound))
    }
}
pub(super) struct OrdinaryConstraintDepthCache<'storage, 'env, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) storage: &'storage mut ConstraintSetStorage<'db>,
}
impl<'db> SyncConstraintTypeEffects<'db> for OrdinaryConstraintDepthCache<'_, '_, 'db> {
    type Error = Infallible;
    fn checkpoint(&mut self, _: ConstraintTypeWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn search_bound(&mut self, bound: Type<'db>, search: BoundSearch) -> Result<bool, Infallible> {
        OrdinaryConstraintTypes {
            db: self.db,
            env: self.env,
        }
        .search_bound(bound, search)
    }
    fn materialize_bound(
        &mut self,
        bound: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Infallible> {
        OrdinaryConstraintTypes {
            db: self.db,
            env: self.env,
        }
        .materialize_bound(bound, kind)
    }
    fn type_depth(&mut self, bound: Type<'db>) -> Result<(u16, u16), Infallible> {
        OrdinaryConstraintTypes {
            db: self.db,
            env: self.env,
        }
        .type_depth(bound)
    }
}
impl<'db> SyncConstraintDepthCacheEffects<'db> for OrdinaryConstraintDepthCache<'_, '_, 'db> {
    fn depth_cache_get(&mut self, id: ConstraintId) -> Result<Option<(u16, u16)>, Infallible> {
        Ok(self.storage.constraint_bound_depth_cache.get(&id).copied())
    }
    fn depth_constraint(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Infallible> {
        Ok(self.storage.constraint_data(id))
    }
    fn depth_cache_publish(
        &mut self,
        id: ConstraintId,
        depth: (u16, u16),
    ) -> Result<(), Infallible> {
        self.storage.constraint_bound_depth_cache.insert(id, depth);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use super::super::control::{
        AllocationKind, TddControl, TddError, TddWork, Unrestricted, hash_slots,
        receiver_seen_storage, sequence_growth, unrestricted,
    };
    use super::super::variables::{
        ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, ConstraintProvenance,
        TypeVarEquivalenceBound, TypeVarRangeBound,
    };
    use super::super::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintTypeCursor};
    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::TypeFormType;
    use crate::types::typevar::BindingContext;

    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("source control unexpectedly suspended"),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Request {
        Search(BoundSearch),
        Materialize(MaterializationKind),
        Depth,
        Get,
        Constraint,
        Publish,
    }
    struct Recording<'builder, 'env, 'db> {
        ordinary: OrdinaryConstraintTypes<'env, 'db>,
        builder: Option<&'builder ConstraintSetBuilder<'db>>,
        requests: Vec<Request>,
        refuse: Option<Request>,
    }
    impl<'env, 'db> Recording<'_, 'env, 'db> {
        fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
            Self {
                ordinary: OrdinaryConstraintTypes { db, env },
                builder: None,
                requests: Vec::new(),
                refuse: None,
            }
        }
        fn request(&mut self, request: Request) -> Result<(), Request> {
            self.requests.push(request);
            if self.refuse == Some(request) {
                Err(request)
            } else {
                Ok(())
            }
        }
    }
    impl<'db> ConstraintTypeEffects<'db> for Recording<'_, '_, 'db> {
        type Error = Request;
        async fn checkpoint(&mut self, _: ConstraintTypeWork) -> Result<(), Request> {
            Ok(())
        }
        async fn search_bound(
            &mut self,
            bound: Type<'db>,
            search: BoundSearch,
        ) -> Result<bool, Request> {
            self.request(Request::Search(search))?;
            Ok(match self.ordinary.search_bound(bound, search) {
                Ok(value) => value,
                Err(never) => match never {},
            })
        }
        async fn materialize_bound(
            &mut self,
            bound: Type<'db>,
            kind: MaterializationKind,
        ) -> Result<Type<'db>, Request> {
            self.request(Request::Materialize(kind))?;
            Ok(match self.ordinary.materialize_bound(bound, kind) {
                Ok(value) => value,
                Err(never) => match never {},
            })
        }
        async fn type_depth(&mut self, bound: Type<'db>) -> Result<(u16, u16), Request> {
            if let Some(builder) = self.builder {
                let storage = builder.storage.borrow_mut();
                assert!(storage.constraint_bound_depth_cache.is_empty());
            }
            self.request(Request::Depth)?;
            Ok(match self.ordinary.type_depth(bound) {
                Ok(value) => value,
                Err(never) => match never {},
            })
        }
    }
    impl<'db> ConstraintDepthCacheEffects<'db> for Recording<'_, '_, 'db> {
        async fn depth_cache_get(
            &mut self,
            id: ConstraintId,
        ) -> Result<Option<(u16, u16)>, Request> {
            self.request(Request::Get)?;
            Ok(self
                .builder
                .expect("cache control has a builder")
                .storage
                .borrow()
                .constraint_bound_depth_cache
                .get(&id)
                .copied())
        }
        async fn depth_constraint(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Request> {
            self.request(Request::Constraint)?;
            Ok(self
                .builder
                .expect("cache control has a builder")
                .storage
                .borrow()
                .constraint_data(id))
        }
        async fn depth_cache_publish(
            &mut self,
            id: ConstraintId,
            depth: (u16, u16),
        ) -> Result<(), Request> {
            self.request(Request::Publish)?;
            self.builder
                .expect("cache control has a builder")
                .storage
                .borrow_mut()
                .constraint_bound_depth_cache
                .insert(id, depth);
            Ok(())
        }
    }

    #[test]
    fn classification_effects_preserve_short_circuits() {
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic_self(
            &db,
            Type::object(),
            BindingContext::Synthetic(env.program(&db)),
        );
        let provenance = ConstraintProvenance::Evidence;
        let full = vec![
            Request::Search(BoundSearch::TypeVar),
            Request::Search(BoundSearch::UnspecializedTypeVar),
            Request::Materialize(MaterializationKind::Bottom),
            Request::Materialize(MaterializationKind::Top),
        ];
        for (bound, expected, requests) in [
            (Type::TypeVar(variable), None, full[..1].to_vec()),
            (
                Type::Dynamic(DynamicType::UnspecializedTypeVar),
                None,
                full[..2].to_vec(),
            ),
            (Type::any(), None, full.clone()),
            (Type::int_literal(1), Some(variable), full.clone()),
        ] {
            for constraint in [
                Constraint::ConcreteLower(ConcreteLowerBound::new(provenance, variable, bound)),
                Constraint::ConcreteUpper(ConcreteUpperBound::new(provenance, variable, bound)),
                Constraint::ConcreteEquivalence(ConcreteEquivalenceBound::new(
                    provenance, variable, bound,
                )),
            ] {
                let mut effects = Recording::new(&db, &env);
                assert_eq!(
                    ready(constraint_as_concrete_with(constraint, &mut effects)),
                    Ok(expected)
                );
                assert_eq!(effects.requests, requests);
                for (index, request) in requests.iter().copied().enumerate() {
                    let mut effects = Recording::new(&db, &env);
                    effects.refuse = Some(request);
                    assert_eq!(
                        ready(constraint_as_concrete_with(constraint, &mut effects)),
                        Err(request)
                    );
                    assert_eq!(effects.requests, requests[..=index]);
                }
            }
        }
        for constraint in [
            Constraint::TypeVarRange(TypeVarRangeBound::new(&db, provenance, variable, variable)),
            Constraint::TypeVarEquivalence(TypeVarEquivalenceBound::new(
                &db, provenance, variable, variable,
            )),
        ] {
            let mut effects = Recording::new(&db, &env);
            assert_eq!(
                ready(constraint_as_concrete_with(constraint, &mut effects)),
                Ok(None)
            );
            assert_eq!(
                ready(constraint_bound_depth_with(constraint, &mut effects)),
                Ok((0, 0))
            );
            assert!(effects.requests.is_empty());
        }
    }

    #[test]
    fn depth_cache_releases_storage_across_child() {
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic_self(
            &db,
            Type::object(),
            BindingContext::Synthetic(env.program(&db)),
        );
        let bound = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(variable)));
        let constraint = Constraint::ConcreteLower(ConcreteLowerBound::new(
            ConstraintProvenance::Evidence,
            variable,
            bound,
        ));
        let builder = ConstraintSetBuilder::new();
        let id = builder
            .storage
            .borrow_mut()
            .intern_constraint(&db, &env, constraint);
        let mut effects = Recording::new(&db, &env);
        effects.builder = Some(&builder);
        effects.refuse = Some(Request::Depth);
        assert_eq!(
            ready(cached_constraint_bound_depth_with(id, &mut effects)),
            Err(Request::Depth)
        );
        assert!(
            builder
                .storage
                .borrow()
                .constraint_bound_depth_cache
                .is_empty()
        );
        assert_eq!(
            effects.requests,
            [Request::Get, Request::Constraint, Request::Depth]
        );
        effects.requests.clear();
        effects.refuse = Some(Request::Publish);
        assert_eq!(
            ready(cached_constraint_bound_depth_with(id, &mut effects)),
            Err(Request::Publish)
        );
        assert!(
            builder
                .storage
                .borrow()
                .constraint_bound_depth_cache
                .is_empty()
        );
        assert_eq!(
            effects.requests,
            [
                Request::Get,
                Request::Constraint,
                Request::Depth,
                Request::Publish
            ]
        );
        effects.requests.clear();
        effects.refuse = None;
        assert_eq!(
            ready(cached_constraint_bound_depth_with(id, &mut effects)),
            Ok((1, 1))
        );
        assert_eq!(
            effects.requests,
            [
                Request::Get,
                Request::Constraint,
                Request::Depth,
                Request::Publish
            ]
        );
        assert_eq!(
            builder
                .storage
                .borrow()
                .constraint_bound_depth_cache
                .get(&id),
            Some(&(1, 1))
        );
        effects.requests.clear();
        assert_eq!(
            ready(cached_constraint_bound_depth_with(id, &mut effects)),
            Ok((1, 1))
        );
        assert_eq!(effects.requests, [Request::Get]);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReceiverStep {
        Advance,
        Growth,
        Storage,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ReceiverTable {
        Empty,
        Populated,
    }

    #[derive(Debug)]
    struct RefuseStep {
        refuse: ReceiverStep,
    }

    impl TddControl for RefuseStep {
        type Error = TddWork;

        fn admit(&mut self, work: TddWork) -> Result<(), TddWork> {
            let refuse = match self.refuse {
                ReceiverStep::Advance => work == TddWork::Advance,
                ReceiverStep::Growth => matches!(
                    work,
                    TddWork::Grow {
                        allocation: AllocationKind::TypeWalkReceiverSeen,
                        ..
                    }
                ),
                ReceiverStep::Storage => matches!(work, TddWork::ReceiverSeenStorage { .. }),
            };
            if refuse {
                Err(work)
            } else {
                Ok(())
            }
        }
    }

    #[test_case::test_case(ReceiverStep::Advance, ReceiverTable::Empty)]
    #[test_case::test_case(ReceiverStep::Growth, ReceiverTable::Empty)]
    #[test_case::test_case(ReceiverStep::Storage, ReceiverTable::Empty)]
    #[test_case::test_case(ReceiverStep::Advance, ReceiverTable::Populated)]
    #[test_case::test_case(ReceiverStep::Growth, ReceiverTable::Populated)]
    #[test_case::test_case(ReceiverStep::Storage, ReceiverTable::Populated)]
    fn receiver_constraint_steps_admit_growth(
        refuse: ReceiverStep,
        table: ReceiverTable,
    ) -> anyhow::Result<()> {
        // Refusing any cursor admission preserves its position and table, including when a
        // populated table needs replacement. A funded retry retains duplicate-node steps.
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic_self(
            &db,
            Type::object(),
            BindingContext::Synthetic(env.program(&db)),
        );
        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let a = ConstraintSet::constrain_typevar_lower_bound(
                &db,
                &env,
                builder,
                variable,
                Type::int_literal(1),
            );
            let b = ConstraintSet::constrain_typevar_lower_bound(
                &db,
                &env,
                builder,
                variable,
                Type::int_literal(2),
            );
            (3..35).fold(a.iff(&db, builder, b).negate(&db, builder), |set, value| {
                let next = ConstraintSet::constrain_typevar_lower_bound(
                    &db,
                    &env,
                    builder,
                    variable,
                    Type::int_literal(value),
                );
                set.and(&db, builder, || next)
            })
        });
        let expected: Vec<_> = owned.type_steps().collect();
        assert!(
            expected.iter().any(Option::is_none),
            "fixture must retain repeated decision constraints"
        );
        let mut cursor = OwnedConstraintTypeCursor::new(&owned);
        let mut actual = Vec::new();
        if table == ReceiverTable::Populated {
            loop {
                if !cursor.seen.is_empty()
                    && cursor.seen.len() == cursor.seen.capacity()
                    && expected.get(cursor.next).is_some_and(Option::is_some)
                {
                    break;
                }
                let Some(step) = unrestricted(cursor.next_with(&mut Unrestricted)) else {
                    anyhow::bail!("fixture must require growth after populating the seen table");
                };
                actual.push(step);
            }
        }
        let state = cursor.state();
        let seen = cursor.seen.clone();
        assert!(matches!(
            cursor.next_with(&mut RefuseStep { refuse }),
            Err(TddError::Refused(_))
        ));
        assert_eq!(cursor.state(), state);
        assert_eq!(cursor.seen, seen);
        while let Some(step) = unrestricted(cursor.next_with(&mut Unrestricted)) {
            actual.push(step);
        }
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test_case::test_case(0)]
    #[test_case::test_case(7)]
    fn receiver_storage_quotes_hash_layout_and_retirement(capacity: usize) {
        // The supplement adds hash metadata to logical payload bytes, quotes replacement
        // initialization, and prepays cleanup. A populated table also pays for scanning and
        // retiring its old backing.
        // Work uses table-slot bounds; requested bytes include the ID and control-byte layout.
        let mut plan =
            unrestricted(sequence_growth::<ConstraintId, Infallible>(capacity, capacity + 1));
        plan.relocation_units = capacity;
        let storage = unrestricted(receiver_seen_storage::<Infallible>(capacity, plan));
        let new_slots = unrestricted(hash_slots::<Infallible>(plan.requested_capacity * 2));
        let old_slots = if capacity == 0 {
            0
        } else {
            unrestricted(hash_slots::<Infallible>(capacity))
        };
        let new_bytes = new_slots * (size_of::<ConstraintId>() + 1);
        assert_eq!(
            plan.requested_payload_bytes,
            plan.requested_capacity * size_of::<ConstraintId>(),
        );
        assert_eq!(
            storage.requested_payload_bytes(),
            new_bytes - plan.requested_payload_bytes,
        );
        assert_eq!(storage.work_units(), 2 * old_slots + 2 * new_slots);
        assert_eq!(
            TddWork::Grow {
                allocation: AllocationKind::TypeWalkReceiverSeen,
                plan,
            }
            .work_units(),
            capacity,
        );
    }

    #[test]
    fn receiver_storage_rejects_unrepresentable_layout() {
        // A logical ID payload can fit even though its conservative hash backing does not.
        // The storage quote rejects that layout with a capacity error.
        let capacity = isize::MAX as usize / (16 * size_of::<ConstraintId>());
        let plan =
            unrestricted(sequence_growth::<ConstraintId, Infallible>(capacity, capacity + 1));
        assert_eq!(
            receiver_seen_storage::<Infallible>(capacity, plan),
            Err(TddError::CapacityExhausted),
        );
    }
}
