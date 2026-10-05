use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::execution_probe::{Demand, ExecutionAdmission, RegistryBuilder};
use salsa::prepared_source_probe::Stamp;

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::ConstraintSet;
use crate::types::constructor::expansion_probe;
use crate::types::relation::TypeRelationChecker;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, Type, TypeFormType, TypeRecursionContext, TypeVarVariance,
};

#[derive(Default)]
struct Admission {
    events: RefCell<Vec<ExecutionWork>>,
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.events.borrow_mut().push(work);
        Ok(())
    }
}

fn checker_ids(checker: &TypeRelationChecker<'_, '_, '_>) -> [*const (); 6] {
    [
        std::ptr::from_ref(checker.env).cast(),
        std::ptr::from_ref(checker.constraints).cast(),
        std::ptr::from_ref(checker.relation_visitor).cast(),
        std::ptr::from_ref(checker.disjointness_visitor).cast(),
        std::ptr::from_ref(checker.signature_relation_visitor).cast(),
        std::ptr::from_ref(checker.materialization_visitor).cast(),
    ]
}

fn assert_uninitialized<T>(storage: &FixedStorage<T>) {
    assert!(storage.arena.get().is_none());
    assert_eq!(storage.observed_capacity.get(), None);
    assert_eq!(storage.initialized.get(), 0);
}

fn assert_one_initialized<T>(name: &str, storage: &FixedStorage<T>) {
    assert_eq!(storage.limit.get(), 1);
    assert_eq!(storage.initialized.get(), 1);
    let actual = storage
        .observed_capacity
        .get()
        .expect("initial capacity recorded");
    assert!(actual >= storage.limit.get());
    assert_eq!(
        storage
            .arena
            .get()
            .expect("initialized arena")
            .uninitialized_array()
            .len(),
        actual - 1,
    );
    eprintln!(
        "CALL_RESOURCE {name}: requested_payload={} actual_capacity={actual} initialized=1 limit=1 fixed_metadata={}",
        size_of::<T>(),
        size_of::<FixedStorage<T>>(),
    );
}

#[test]
fn root_and_children_share_original_call_resources() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let baseline_env = ProgramEnvironment::from_program(program);
    let baseline_builder = ConstraintSetBuilder::new();
    let baseline_owners = RelationOwners::new(&baseline_env, &baseline_builder);
    let baseline_checker = baseline_owners.assignability(TypeVarSet::None);
    assert!(std::ptr::eq(baseline_checker.env, &baseline_env));
    assert!(std::ptr::eq(
        baseline_checker.constraints,
        &baseline_builder
    ));
    let baseline = ConstraintSet::from_bool(baseline_checker.constraints, true)
        .is_trivially_always_satisfied();

    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let admission = Admission::default();
    assert_uninitialized(&environments.storage);
    assert_uninitialized(&builders.storage);
    assert_uninitialized(&owners.storage);

    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let admission = &admission;
        RegistryBuilder::new(&db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                assert_uninitialized(&environments.storage);
                assert_uninitialized(&builders.storage);
                assert_uninitialized(&owners.storage);
                let allocation_start = admission.events.borrow().len();
                let env = environments.allocate(&endpoint, program).await;
                let builder = builders.allocate(&endpoint).await;
                let relation = owners.allocate(&endpoint, env, builder).await;
                assert_eq!(
                    &admission.events.borrow()[allocation_start..],
                    &[
                        ExecutionWork::Work { units: 1 },
                        ExecutionWork::Resource {
                            requested_bytes: size_of::<ProgramEnvironment<'_>>()
                        },
                        ExecutionWork::Work { units: 1 },
                        ExecutionWork::Resource {
                            requested_bytes: size_of::<ConstraintSetBuilder<'_>>()
                        },
                        ExecutionWork::Work { units: 1 },
                        ExecutionWork::Resource {
                            requested_bytes: size_of::<RelationOwners<'_, '_, '_>>()
                        },
                    ],
                );
                let checker = relation.assignability(TypeVarSet::None);
                assert!(std::ptr::eq(checker.env, env));
                assert!(std::ptr::eq(checker.constraints, builder));
                let identities = checker_ids(&checker);
                let expected = ConstraintSet::from_bool(builder, true);
                for _ in 0..2 {
                    let child_checker = checker.clone();
                    let actual = endpoint
                        .child_call(|| async {
                            endpoint
                                .demand(move || async move {
                                    assert_eq!(checker_ids(&child_checker), identities);
                                    Ok(ConstraintSet::from_bool(child_checker.constraints, true))
                                })?
                                .await
                        })
                        .await;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert_eq!(environments.storage.initialized.get(), 1);
                    assert_eq!(builders.storage.initialized.get(), 1);
                    assert_eq!(owners.storage.initialized.get(), 1);
                }
                Ok(expected.is_trivially_always_satisfied())
            })
    })
    .0;
    assert!(
        matches!(outcome, Ok(Ok(value)) if value == baseline),
        "{outcome:?}"
    );
    assert_eq!(
        admission
            .events
            .borrow()
            .iter()
            .filter(|event| matches!(event, ExecutionWork::Task { .. }))
            .count(),
        3,
    );
    assert_one_initialized("environment", &environments.storage);
    assert_one_initialized("builder", &builders.storage);
    assert_one_initialized("relation", &owners.storage);
    assert!(!expansion_probe::active());
}

#[derive(Clone, Copy, Debug)]
enum PoolKind {
    Environment,
    Builder,
    Owners,
}

fn assert_two_initialized<T>(name: &str, storage: &FixedStorage<T>) {
    assert_eq!(storage.limit.get(), 2);
    assert_eq!(storage.initialized.get(), 2);
    let actual = storage
        .observed_capacity
        .get()
        .expect("initial capacity recorded");
    assert!(actual >= 2);
    assert_eq!(
        storage
            .arena
            .get()
            .expect("initialized arena")
            .uninitialized_array()
            .len(),
        actual - 2
    );
    eprintln!(
        "CALL_RESOURCE {name}: requested_payload={} actual_capacity={actual} initialized=2 limit=2 fixed_metadata={}",
        2 * size_of::<T>(),
        size_of::<FixedStorage<T>>()
    );
}

#[test]
fn two_calls_keep_addresses_and_refuse_before_any_pool_spills() {
    for exhausted in [PoolKind::Environment, PoolKind::Builder, PoolKind::Owners] {
        let db = setup_db();
        let program = db.program_environment().program(&db);
        let capacity = CallResourceCapacity {
            calls: NonZeroUsize::new(2).expect("positive capacity"),
        };
        let environments = CallEnvironments::with_capacity(capacity);
        let builders = CallBuilders::with_capacity(capacity);
        let owners = CallRelationOwners::with_capacity(capacity);
        let admission = Admission::default();
        let returned = Cell::new(None);
        let after_refusal = Cell::new(false);
        let before_exhaustion = Cell::new(0);
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let environments = &environments;
            let builders = &builders;
            let owners = &owners;
            let admission = &admission;
            let after_refusal = &after_refusal;
            let before_exhaustion = &before_exhaustion;
            let result =
                RegistryBuilder::new(&db, admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let start = admission.events.borrow().len();
                        let first_env = environments.allocate(&endpoint, program).await;
                        let first_builder = builders.allocate(&endpoint).await;
                        let first_owner =
                            owners.allocate(&endpoint, first_env, first_builder).await;
                        let first = first_owner.assignability(TypeVarSet::None);
                        let first_ids = checker_ids(&first);
                        assert_eq!(
                            &admission.events.borrow()[start..],
                            &[
                                ExecutionWork::Work { units: 1 },
                                ExecutionWork::Resource {
                                    requested_bytes: 2 * size_of::<ProgramEnvironment<'_>>()
                                },
                                ExecutionWork::Work { units: 1 },
                                ExecutionWork::Resource {
                                    requested_bytes: 2 * size_of::<ConstraintSetBuilder<'_>>()
                                },
                                ExecutionWork::Work { units: 1 },
                                ExecutionWork::Resource {
                                    requested_bytes: 2 * size_of::<RelationOwners<'_, '_, '_>>()
                                },
                            ]
                        );
                        let start = admission.events.borrow().len();
                        let second_env = environments.allocate(&endpoint, program).await;
                        let second_builder = builders.allocate(&endpoint).await;
                        let second_owner =
                            owners.allocate(&endpoint, second_env, second_builder).await;
                        let second = second_owner.assignability(TypeVarSet::None);
                        assert_eq!(
                            &admission.events.borrow()[start..],
                            &[ExecutionWork::Work { units: 1 }; 3]
                        );
                        assert_eq!(checker_ids(&first), first_ids);
                        let second_ids = checker_ids(&second);
                        assert!(
                            first_ids
                                .iter()
                                .zip(second_ids)
                                .all(|(first, second)| *first != second)
                        );
                        let expected = (
                            ConstraintSet::from_bool(first_builder, true),
                            ConstraintSet::from_bool(second_builder, true),
                        );
                        let actual = endpoint
                            .child_call(|| async {
                                endpoint
                                    .demand(move || async move {
                                        assert_eq!(checker_ids(&first), first_ids);
                                        assert_eq!(checker_ids(&second), second_ids);
                                        Ok((
                                            ConstraintSet::from_bool(first.constraints, true),
                                            ConstraintSet::from_bool(second.constraints, true),
                                        ))
                                    })?
                                    .await
                            })
                            .await;
                        assert!(actual.0.ownership_probe_same_set(expected.0));
                        assert!(actual.1.ownership_probe_same_set(expected.1));
                        assert!(!actual.0.ownership_probe_same_set(actual.1));
                        before_exhaustion.set(admission.events.borrow().len());
                        match exhausted {
                            PoolKind::Environment => {
                                let _ = environments.allocate(&endpoint, program).await;
                            }
                            PoolKind::Builder => {
                                let _ = builders.allocate(&endpoint).await;
                            }
                            PoolKind::Owners => {
                                let _ = owners.allocate(&endpoint, first_env, first_builder).await;
                            }
                        }
                        after_refusal.set(true);
                        Ok(())
                    });
            returned.set(result.as_ref().err().copied());
            result
        })
        .0;
        assert!(
            matches!(outcome, Err(expansion_probe::Incomplete::Allowance)),
            "{exhausted:?}: {outcome:?}"
        );
        assert_eq!(
            returned.get(),
            Some(RunError::Refused(Incomplete::Allowance))
        );
        assert!(!after_refusal.get());
        assert_eq!(
            &admission.events.borrow()[before_exhaustion.get()..],
            &[ExecutionWork::Work { units: 1 }]
        );
        assert_two_initialized("environment", &environments.storage);
        assert_two_initialized("builder", &builders.storage);
        assert_two_initialized("relation", &owners.storage);
    }
}

struct LoggedPool<'a, T> {
    value: Option<T>,
    name: &'static str,
    journal: &'a RefCell<Vec<&'static str>>,
}

impl<'a, T> LoggedPool<'a, T> {
    fn new(value: T, name: &'static str, journal: &'a RefCell<Vec<&'static str>>) -> Self {
        Self {
            value: Some(value),
            name,
            journal,
        }
    }
}

impl<T> Deref for LoggedPool<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value.as_ref().expect("live pool")
    }
}

impl<T> Drop for LoggedPool<'_, T> {
    fn drop(&mut self) {
        drop(self.value.take());
        self.journal.borrow_mut().push(self.name);
    }
}

struct RootDrop<'a>(&'a RefCell<Vec<&'static str>>);

impl Drop for RootDrop<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push("root");
    }
}

struct QueuedObserver<'a>(&'a dyn Fn());

impl Drop for QueuedObserver<'_> {
    fn drop(&mut self) {
        (self.0)();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    BackingRefuse,
    BackingPostAcceptance,
    ChildRefuse,
    ChildCancel,
}

struct FaultAdmission<'run, 'db: 'run> {
    db: &'db TestDb,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    observer: &'run dyn Fn(),
    child_factory_ran: &'run Cell<bool>,
    armed: Cell<bool>,
    fired: Cell<bool>,
    fault: Fault,
}

impl ExecutionAdmission for FaultAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let matches = match self.fault {
            Fault::BackingRefuse | Fault::BackingPostAcceptance => {
                matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes == size_of::<ProgramEnvironment<'_>>())
            }
            Fault::ChildRefuse | Fault::ChildCancel => matches!(work, ExecutionWork::Task { .. }),
        };
        if !self.armed.get() || self.fired.get() || !matches {
            return Ok(());
        }
        self.fired.set(true);
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract("resource fault has no endpoint"))?;
        let child = QueuedObserver(self.observer);
        let child_factory_ran = self.child_factory_ran;
        let pending = endpoint.demand(move || {
            child_factory_ran.set(true);
            async move {
                let _held = child;
                Ok(())
            }
        })?;
        *self.pending.borrow_mut() = Some(pending);
        match self.fault {
            Fault::BackingRefuse | Fault::ChildRefuse => {
                Err(RunError::Refused(Incomplete::Allowance))
            }
            Fault::BackingPostAcceptance => Ok(()),
            Fault::ChildCancel => {
                self.db.cancellation_token().cancel();
                Ok(())
            }
        }
    }
}

struct ResetSlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}

impl Drop for ResetSlots<'_, '_, '_> {
    fn drop(&mut self) {
        let pending = self.pending.borrow_mut().take();
        let endpoint = self.endpoint.borrow_mut().take();
        drop(pending);
        drop(endpoint);
    }
}

#[derive(Debug)]
struct Cleanup {
    initialized: [usize; 3],
    remaining: [Option<usize>; 3],
    identities: Option<[*const (); 6]>,
    visitor_counts: Option<[(usize, usize); 2]>,
    signatures_empty: Option<bool>,
}

fn remaining<T>(storage: &FixedStorage<T>) -> Option<usize> {
    storage
        .arena
        .get()
        .map(|arena| arena.uninitialized_array().len())
}

fn failure_retains_scoped_resources(fault: Fault) {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let stamp = Stamp::current(&db);
    let journal = RefCell::new(Vec::new());
    let cleanup = RefCell::new(Vec::new());
    let original_ids = Cell::new(None);
    {
        let capacity = CallResourceCapacity {
            calls: NonZeroUsize::MIN,
        };
        let environments = LoggedPool::new(
            CallEnvironments::with_capacity(capacity),
            "environments",
            &journal,
        );
        let builders = LoggedPool::new(CallBuilders::with_capacity(capacity), "builders", &journal);
        let owners = LoggedPool::new(
            CallRelationOwners::with_capacity(capacity),
            "owners",
            &journal,
        );
        let selected = RefCell::new(None::<TypeRelationChecker<'_, '_, '_>>);
        let observer = || {
            let selected = selected.borrow();
            cleanup.borrow_mut().push(Cleanup {
                initialized: [
                    environments.storage.initialized.get(),
                    builders.storage.initialized.get(),
                    owners.storage.initialized.get(),
                ],
                remaining: [
                    remaining(&environments.storage),
                    remaining(&builders.storage),
                    remaining(&owners.storage),
                ],
                identities: selected.as_ref().map(checker_ids),
                visitor_counts: selected.as_ref().map(|checker| {
                    [
                        checker.relation_visitor.ownership_probe_counts(),
                        checker.disjointness_visitor.ownership_probe_counts(),
                    ]
                }),
                signatures_empty: selected
                    .as_ref()
                    .map(|checker| checker.signature_relation_visitor.is_empty()),
            });
            journal.borrow_mut().push("child");
        };
        let child_factory_ran = Cell::new(false);
        let target_factory_ran = Cell::new(false);
        let after_await = Cell::new(false);
        let returned = Cell::new(None);
        // ResetSlots takes and drops the endpoint before these scopes retire. Suppressing the
        // slot's automatic endpoint destructor avoids a drop-check cycle through admission.
        let admission;
        let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
        let pending = RefCell::new(None);
        admission = FaultAdmission {
            db: &db,
            endpoint: &endpoint_slot,
            pending: &pending,
            observer: &observer,
            child_factory_ran: &child_factory_ran,
            armed: Cell::new(false),
            fired: Cell::new(false),
            fault,
        };
        let reset = ResetSlots {
            endpoint: &endpoint_slot,
            pending: &pending,
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            expansion_probe::run(&db, usize::MAX, || {
                let environments = &environments;
                let builders = &builders;
                let owners = &owners;
                let selected = &selected;
                let admission = &admission;
                let original_ids = &original_ids;
                let journal = &journal;
                let after_await = &after_await;
                let target_factory_ran = &target_factory_ran;
                let result = RegistryBuilder::new(&db, admission)?
                    .seal()?
                    .run(move |endpoint| {
                        **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                        async move {
                            let _root = RootDrop(journal);
                            if matches!(fault, Fault::BackingRefuse | Fault::BackingPostAcceptance)
                            {
                                admission.armed.set(true);
                            }
                            let env = environments.allocate(&endpoint, program).await;
                            let builder = builders.allocate(&endpoint).await;
                            let relation = owners.allocate(&endpoint, env, builder).await;
                            let checker = relation.assignability(TypeVarSet::None);
                            original_ids.set(Some(checker_ids(&checker)));
                            *selected.borrow_mut() = Some(checker.clone());
                            admission.armed.set(true);
                            let _ = endpoint
                                .child_call(|| async {
                                    endpoint
                                        .demand(move || {
                                            target_factory_ran.set(true);
                                            async move {
                                                Ok(ConstraintSet::from_bool(
                                                    checker.constraints,
                                                    true,
                                                ))
                                            }
                                        })?
                                        .await
                                })
                                .await;
                            after_await.set(true);
                            Ok(())
                        }
                    });
                returned.set(result.as_ref().err().copied());
                result
            })
        }));
        assert!(admission.fired.get());
        match fault {
            Fault::BackingRefuse | Fault::ChildRefuse => {
                let (result, _) = outcome.expect("ordinary refusal");
                assert!(matches!(
                    result,
                    Err(expansion_probe::Incomplete::Allowance)
                ));
                assert_eq!(
                    returned.get(),
                    Some(RunError::Refused(Incomplete::Allowance))
                );
            }
            Fault::BackingPostAcceptance => {
                let (result, _) = outcome.expect("local completion rejection");
                assert!(matches!(
                    result,
                    Err(expansion_probe::Incomplete::Interrupted)
                ));
                assert_eq!(
                    returned.get(),
                    Some(RunError::Contract("completed task retained a child"))
                );
            }
            Fault::ChildCancel => {
                let payload = outcome.expect_err("native cancellation");
                assert!(matches!(
                    payload.downcast_ref::<salsa::Cancelled>(),
                    Some(salsa::Cancelled::Local)
                ));
            }
        }
        assert!(!child_factory_ran.get() && !target_factory_ran.get() && !after_await.get());
        assert_eq!(&*journal.borrow(), &["child", "root"]);
        drop(reset);
        assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
        selected.borrow_mut().take();
    }
    assert_eq!(
        &*journal.borrow(),
        &["child", "root", "owners", "builders", "environments"]
    );
    let cleanup = cleanup.borrow();
    assert_eq!(cleanup.len(), 1);
    let actual = &cleanup[0];
    let counts = match fault {
        Fault::BackingRefuse => [0, 0, 0],
        Fault::BackingPostAcceptance => [1, 0, 0],
        Fault::ChildRefuse | Fault::ChildCancel => [1, 1, 1],
    };
    assert_eq!(actual.initialized, counts);
    for (count, remaining) in counts.into_iter().zip(actual.remaining) {
        assert_eq!(remaining.is_some(), count == 1);
    }
    assert_eq!(actual.identities, original_ids.get());
    if matches!(fault, Fault::ChildRefuse | Fault::ChildCancel) {
        assert_eq!(actual.visitor_counts, Some([(0, 0), (0, 0)]));
        assert_eq!(actual.signatures_empty, Some(true));
    }
    assert!(!expansion_probe::active());
    if fault != Fault::ChildCancel {
        for _ in 0..2 {
            assert!(stamp.belongs_to(&db));
            fresh_scopes_succeed(&db, program);
        }
    }
}

fn fresh_scopes_succeed<'db>(db: &'db TestDb, program: Program<'db>) {
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let admission = Admission::default();
    assert_uninitialized(&environments.storage);
    assert_uninitialized(&builders.storage);
    assert_uninitialized(&owners.storage);
    let outcome = expansion_probe::run(db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                let builder = builders.allocate(&endpoint).await;
                let relation = owners.allocate(&endpoint, env, builder).await;
                let checker = relation.assignability(TypeVarSet::None);
                let expected = ConstraintSet::from_bool(builder, true);
                let actual = endpoint
                    .child_call(|| async {
                        endpoint
                            .demand(move || async move {
                                Ok(ConstraintSet::from_bool(checker.constraints, true))
                            })?
                            .await
                    })
                    .await;
                assert!(actual.ownership_probe_same_set(expected));
                Ok(actual.is_trivially_always_satisfied())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(true))), "{outcome:?}");
    assert_eq!(
        [
            environments.storage.initialized.get(),
            builders.storage.initialized.get(),
            owners.storage.initialized.get()
        ],
        [1, 1, 1]
    );
}

#[test]
fn backing_admission_failures_keep_the_actual_scopes_until_child_cleanup() {
    for fault in [Fault::BackingRefuse, Fault::BackingPostAcceptance] {
        failure_retains_scoped_resources(fault);
    }
}

#[test]
fn child_creation_failures_keep_original_checker_and_visitors_until_cleanup() {
    for fault in [Fault::ChildRefuse, Fault::ChildCancel] {
        failure_retains_scoped_resources(fault);
    }
}

#[test]
fn mapping_visitors_keep_stable_environments_and_fresh_transformation_cells() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::new(2).unwrap(),
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let admission = Admission::default();
    assert_uninitialized(&visitors.storage);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let environments = &environments;
        let visitors = &visitors;
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                let first = visitors.allocate(&endpoint, env).await;
                let second = visitors.allocate(&endpoint, env).await;
                assert!(!std::ptr::eq(first, second));
                assert!(std::ptr::eq(first.env, env));
                assert!(std::ptr::eq(second.env, env));
                for visitor in [first, second] {
                    assert!(visitor.recursion_context.is_none());
                    assert!(visitor.materialize_typevar_bounds_and_defaults);
                    assert!(visitor.top_materialization.get().is_none());
                    assert!(visitor.bottom_materialization.get().is_none());
                }
                endpoint
                    .child_call(|| async {
                        endpoint
                            .demand(move || async move {
                                assert!(std::ptr::eq(first.env, second.env));
                                assert!(std::ptr::eq(first.env, env));
                                Ok(())
                            })?
                            .await
                    })
                    .await;
                Ok(())
            })
    })
    .0;
    assert!(matches!(result, Ok(Ok(()))), "{result:?}");
    assert_eq!(visitors.storage.initialized.get(), 2);
    assert_eq!(environments.storage.initialized.get(), 1);
}

#[test]
fn mapping_visitor_capacity_refuses_before_arena_spill() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let admission = Admission::default();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let environments = &environments;
        let visitors = &visitors;
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                visitors.allocate(&endpoint, env).await;
                visitors.allocate(&endpoint, env).await;
                Ok(())
            })
    })
    .0;
    assert!(
        matches!(
            result,
            Err(expansion_probe::Incomplete::Allowance)
                | Ok(Err(RunError::Refused(Incomplete::Allowance)))
        ),
        "{result:?}"
    );
    assert_one_initialized("mapping visitor", &visitors.storage);
}

fn nonterminal_owned_set<'db>(db: &'db TestDb) -> OwnedConstraintSet<'db> {
    let env = db.program_environment();
    let variable = BoundTypeVarInstance::synthetic(
        db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let bound = TypeFormType::from_type_expression(db, Type::int_literal(1));
    ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, variable, bound)
    })
}

#[test]
fn terminal_owned_packaging_preserves_the_live_builder() {
    let db = setup_db();
    let env = db.program_environment();
    let owned = nonterminal_owned_set(&db);
    let view = owned.query_view();
    let (builder, original) = view.parts();
    assert!(original.to_owned_terminal().is_none());
    let original_display = original.display(&db, &env).to_string();

    for answer in [false, true] {
        let expected = ConstraintSetBuilder::new()
            .into_owned(|builder| ConstraintSet::from_bool(builder, answer));
        let actual = ConstraintSet::from_bool(builder, answer).to_owned_terminal();
        assert_eq!(actual, Some(expected));
        assert!(view.ownership_probe_matches(&owned));
        assert!(original.ownership_probe_same_set(view.parts().1));
        assert_eq!(original.display(&db, &env).to_string(), original_display);
    }
}

#[test]
fn owned_query_views_keep_the_actual_root_and_shared_arenas() {
    let db = setup_db();
    let owned = nonterminal_owned_set(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let builders = CallBuilders::with_capacity(capacity);
    let retained = Cell::new(None);
    let admission = Admission::default();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let builders = &builders;
        let retained = &retained;
        let owned = &owned;
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let temporary = owned.clone();
                let view = builders.allocate_owned_query(&endpoint, &temporary).await;
                assert!(view.ownership_probe_matches(&temporary));
                drop(temporary);
                let (builder, root) = view.parts();
                assert!(root.to_owned_terminal().is_none());
                let private = builders.allocate(&endpoint).await;
                assert!(!std::ptr::eq(builder, private));
                retained.set(Some(view));
                endpoint
                    .child_call(|| async {
                        endpoint
                            .demand(move || async move {
                                assert!(view.ownership_probe_matches(owned));
                                assert!(root.ownership_probe_same_set(view.parts().1));
                                Ok(())
                            })?
                            .await
                    })
                    .await;
                Ok(())
            })
    })
    .0;
    assert!(matches!(result, Ok(Ok(()))), "{result:?}");
    let Some(view) = retained.get() else {
        panic!("the completed allocation must retain its query view");
    };
    assert!(view.ownership_probe_matches(&owned));
    let env = db.program_environment();
    let original = owned.query(|_, root| root.display(&db, &env).to_string());
    drop(owned);
    assert_eq!(view.parts().1.display(&db, &env).to_string(), original);
    assert_one_initialized("owned query view", &builders.query_views);
    assert_one_initialized("private builder", &builders.storage);
}

#[test]
fn owned_query_view_capacity_refuses_without_disturbing_the_first_view() {
    let db = setup_db();
    let owned = nonterminal_owned_set(&db);
    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let retained = Cell::new(None);
    let admission = Admission::default();
    let returned = Cell::new(None);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let builders = &builders;
        let retained = &retained;
        let owned = &owned;
        let inner =
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    retained.set(Some(builders.allocate_owned_query(&endpoint, owned).await));
                    builders.allocate_owned_query(&endpoint, owned).await;
                    Ok(())
                });
        returned.set(inner.as_ref().err().copied());
        inner
    })
    .0;
    assert!(
        matches!(result, Err(expansion_probe::Incomplete::Allowance)),
        "{result:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(Incomplete::Allowance))
    );
    assert!(
        retained
            .get()
            .is_some_and(|view| view.ownership_probe_matches(&owned))
    );
    assert_one_initialized("owned query view", &builders.query_views);
    assert_uninitialized(&builders.storage);
}

#[test]
fn materialization_roots_share_the_equivalence_guard_and_refresh_every_cache() {
    let db = setup_db();
    let env = db.program_environment();
    let context = TypeRecursionContext::default();
    let mut original = ApplyTypeMappingVisitor::new(&env).with_recursion_context(Some(&context));
    original.materialize_typevar_bounds_and_defaults = false;
    for cell in [
        &original.default,
        &original.top_materialization,
        &original.bottom_materialization,
        &original.top_specialization_materialization,
        &original.bottom_specialization_materialization,
        &original.promotion,
        &original.skip_promotion,
    ] {
        assert!(cell.set(Box::default()).is_ok());
    }
    let visitors = CallMappingVisitors::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::new(2).unwrap(),
    });
    let admission = Admission::default();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let visitors = &visitors;
        let original = &original;
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let first = visitors
                    .allocate_materialization_root(&endpoint, original)
                    .await;
                let second = visitors
                    .allocate_materialization_root(&endpoint, original)
                    .await;
                assert!(!std::ptr::eq(first, second));
                for root in [first, second] {
                    assert!(std::ptr::eq(root.env, original.env));
                    assert!(
                        root.recursion_context
                            .zip(original.recursion_context)
                            .is_some_and(|(actual, expected)| std::ptr::eq(actual, expected))
                    );
                    assert!(!root.materialize_typevar_bounds_and_defaults);
                    assert!(
                        root.materialization_equivalence
                            .get()
                            .zip(original.materialization_equivalence.get())
                            .is_some_and(|(actual, expected)| Rc::ptr_eq(actual, expected))
                    );
                    for cell in [
                        &root.default,
                        &root.top_materialization,
                        &root.bottom_materialization,
                        &root.top_specialization_materialization,
                        &root.bottom_specialization_materialization,
                        &root.promotion,
                        &root.skip_promotion,
                    ] {
                        assert!(cell.get().is_none());
                    }
                }
                Ok(())
            })
    })
    .0;
    assert!(matches!(result, Ok(Ok(()))), "{result:?}");
    assert_eq!(visitors.storage.initialized.get(), 2);
    let guard_bytes = CallMappingVisitors::materialization_guard_bytes()
        .expect("the fixed detector allocation has a valid layout");
    assert_eq!(admission.events.borrow().iter().filter(|event| {
        matches!(event, ExecutionWork::Resource { requested_bytes } if *requested_bytes == guard_bytes)
    }).count(), 1);
}

struct RefuseResource {
    bytes: usize,
    armed: Cell<bool>,
    fired: Cell<bool>,
}

impl ExecutionAdmission for RefuseResource {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.armed.get()
            && matches!(work, ExecutionWork::Resource { requested_bytes } if requested_bytes == self.bytes)
        {
            self.fired.set(true);
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

#[test]
fn materialization_guard_refusal_leaves_the_original_visitor_uninitialized() {
    let db = setup_db();
    let env = db.program_environment();
    let original = ApplyTypeMappingVisitor::new(&env);
    let visitors = CallMappingVisitors::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let admission = RefuseResource {
        bytes: CallMappingVisitors::materialization_guard_bytes()
            .expect("the fixed detector allocation has a valid layout"),
        armed: Cell::new(false),
        fired: Cell::new(false),
    };
    let returned = Cell::new(None);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let visitors = &visitors;
        let original = &original;
        let admission = &admission;
        let inner = RegistryBuilder::new(&db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                admission.armed.set(true);
                visitors
                    .allocate_materialization_root(&endpoint, original)
                    .await;
                Ok(())
            });
        returned.set(inner.as_ref().err().copied());
        inner
    })
    .0;
    assert!(
        matches!(result, Err(expansion_probe::Incomplete::Allowance)),
        "{result:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(Incomplete::Allowance))
    );
    assert!(admission.fired.get());
    assert!(original.materialization_equivalence.get().is_none());
    assert_uninitialized(&visitors.storage);
}

#[test]
fn owned_query_view_backing_refusal_preserves_the_owned_source() {
    let db = setup_db();
    let owned = nonterminal_owned_set(&db);
    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let admission = RefuseResource {
        bytes: size_of::<OwnedConstraintSetQuery<'_>>(),
        armed: Cell::new(false),
        fired: Cell::new(false),
    };
    let returned = Cell::new(None);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let builders = &builders;
        let owned = &owned;
        let admission = &admission;
        let inner = RegistryBuilder::new(&db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                admission.armed.set(true);
                builders.allocate_owned_query(&endpoint, owned).await;
                Ok(())
            });
        returned.set(inner.as_ref().err().copied());
        inner
    })
    .0;
    assert!(
        matches!(result, Err(expansion_probe::Incomplete::Allowance)),
        "{result:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(Incomplete::Allowance))
    );
    assert!(admission.fired.get());
    assert_uninitialized(&builders.query_views);
    assert_uninitialized(&builders.storage);
    assert!(owned.query_view().ownership_probe_matches(&owned));
    assert!(owned.query(|_, root| root.to_owned_terminal().is_none()));
}

#[test]
fn materialization_root_capacity_refuses_before_initializing_another_guard() {
    let db = setup_db();
    let env = db.program_environment();
    let first = ApplyTypeMappingVisitor::new(&env);
    let second = ApplyTypeMappingVisitor::new(&env);
    let visitors = CallMappingVisitors::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let admission = Admission::default();
    let returned = Cell::new(None);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let visitors = &visitors;
        let first = &first;
        let second = &second;
        let inner =
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    visitors
                        .allocate_materialization_root(&endpoint, first)
                        .await;
                    visitors
                        .allocate_materialization_root(&endpoint, second)
                        .await;
                    Ok(())
                });
        returned.set(inner.as_ref().err().copied());
        inner
    })
    .0;
    assert!(
        matches!(result, Err(expansion_probe::Incomplete::Allowance)),
        "{result:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(Incomplete::Allowance))
    );
    assert!(first.materialization_equivalence.get().is_some());
    assert!(second.materialization_equivalence.get().is_none());
    assert_one_initialized("directional mapping visitor", &visitors.storage);
}

#[test]
fn lazy_equivalence_reuses_the_original_checker_resources() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let eager = owners.equivalence();
    let lazy = owners.constraint_set_equivalence();
    assert_eq!(
        eager.typevar_evaluation,
        super::super::TypeVarEvaluation::Eager
    );
    assert_eq!(
        lazy.typevar_evaluation,
        super::super::TypeVarEvaluation::Lazy
    );
    assert!(std::ptr::eq(eager.env, lazy.env));
    assert!(std::ptr::eq(eager.constraints, lazy.constraints));
    assert!(std::ptr::eq(eager.relation_visitor, lazy.relation_visitor));
    assert!(std::ptr::eq(
        eager.disjointness_visitor,
        lazy.disjointness_visitor
    ));
    assert!(std::ptr::eq(
        eager.signature_relation_visitor,
        lazy.signature_relation_visitor
    ));
    assert!(std::ptr::eq(
        eager.materialization_visitor,
        lazy.materialization_visitor
    ));
    assert!(lazy.given.ownership_probe_same_set(eager.given));
    assert!(lazy.given.is_trivially_never_satisfied());
    assert!(lazy.perform_expensive_checks);
    assert!(lazy.observations.is_none());
}
