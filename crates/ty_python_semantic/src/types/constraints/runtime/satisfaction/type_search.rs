//! Bound classification uses the shared ordered walk and the caller's execution owner.

use std::cell::RefCell;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{BorrowOrCopy, RunError, RunResult, TaskEndpoint};

use super::{
    BoundSearch, Incomplete, UnsupportedSatisfactionOperation, runtime_error, runtime_incomplete,
};
use crate::Db;
use crate::types::constraints::control::attempt::ExecutionControl;
use crate::types::constraints::control::{
    AllocationKind, TddControl, TddError, TddWork, reserve_smallvec,
};
use crate::types::constraints::runtime::EndpointAdmission;
use crate::types::constraints::support::Support;
use crate::types::constraints::variables::Constraint;
use crate::types::constraints::{ConstraintId, ConstraintSetBuilder, ConstraintSetStorage};
use crate::types::instance::{
    NominalClassEffects, NominalClassFacts, NominalInstanceClass, nominal_class_with,
};
use crate::types::tuple::TupleType;
use crate::types::visitor::runtime::{
    RuntimeTypeSearch, RuntimeTypeWalk as SharedRuntimeTypeWalk, TypeSearchUnavailable,
};
use crate::types::visitor::{
    SearchOperation, TypeDepthEffects, TypeSearchMode, TypeSupportEffects, TypeWalkFacts,
    enter_depth_active_with, leave_depth_active_with, search_type_with, static_eligible_with,
    support_type_with, type_depth_with,
};
use crate::types::{BoundTypeVarInstance, ClassType, DynamicType, NominalInstanceType, Type};

pub(super) type RuntimeTypeWalk<'call, 'run, 'db, Q = BoundSearch> =
    SharedRuntimeTypeWalk<'call, 'run, 'db, Q, ()>;

impl TypeSearchUnavailable for () {
    async fn unavailable<T>(
        &self,
        db: &dyn Db,
        endpoint: &TaskEndpoint<'_, '_>,
        operation: SearchOperation,
    ) -> RunResult<T> {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Err(runtime_incomplete(
                    db,
                    Incomplete::UnsupportedSearchOperation(operation),
                ))
            })
            .await)
    }

    fn error(&self, db: &dyn Db, error: TddError<RunError>) -> RunError {
        runtime_error(db, error)
    }
}

pub(super) async fn search_bound<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    bound: Type<'db>,
    search: BoundSearch,
) -> RunResult<bool> {
    let mode = match search {
        BoundSearch::TypeVar => TypeSearchMode::SkipLazyAttributes,
        BoundSearch::UnspecializedTypeVar => TypeSearchMode::IncludeAliasArguments,
    };
    let mut effects = RuntimeTypeWalk::new(db, endpoint, search);
    search_type_with(
        bound,
        mode,
        TypeWalkFacts,
        &mut effects,
    )
    .await
}

impl<'call, 'run, 'db> RuntimeTypeWalk<'call, 'run, 'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        search: BoundSearch,
    ) -> Self {
        Self {
            db,
            endpoint,
            query: search,
            unavailable: (),
        }
    }
}
impl<'call, 'run, 'db, Q> RuntimeTypeWalk<'call, 'run, 'db, Q> {
    async fn refuse_depth<T>(&self) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Err(runtime_incomplete(
                    self.db,
                    Incomplete::UnsupportedSatisfactionOperation(
                        UnsupportedSatisfactionOperation::BoundDepth,
                    ),
                ))
            })
            .await)
    }
}

impl<'db> RuntimeTypeSearch<'db> for BoundSearch {
    fn predicate(&self, ty: Type<'db>) -> bool {
        match self {
            Self::TypeVar => matches!(ty, Type::TypeVar(_)),
            Self::UnspecializedTypeVar => {
                matches!(ty, Type::Dynamic(DynamicType::UnspecializedTypeVar))
            }
        }
    }
}

/// Runs the original active-path depth fold without creating a solver result or cache entry.
pub(super) async fn type_depth<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
) -> RunResult<(u16, u16)> {
    let mut effects = RuntimeTypeWalk::new(db, endpoint, BoundSearch::TypeVar);
    type_depth_with(ty, TypeWalkFacts, &mut effects).await
}

pub(super) async fn static_eligible<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    ty: Type<'db>,
) -> RunResult<bool> {
    let mut effects = RuntimeTypeWalk::new(db, endpoint, BoundSearch::TypeVar);
    static_eligible_with(ty, TypeWalkFacts, &mut effects).await
}

struct RuntimeSupport<'a, 'db> {
    storage: &'a RefCell<ConstraintSetStorage<'db>>,
    support: &'a mut Support,
}
impl<'db> TypeSupportEffects<'db> for RuntimeTypeWalk<'_, '_, 'db, RuntimeSupport<'_, 'db>> {
    async fn record_occurrence(&mut self, occurrence: BoundTypeVarInstance<'db>) -> RunResult<()> {
        let identity = self
            .endpoint
            .read_field(
                occurrence.identity_request(self.endpoint.field_request_context()),
                &BorrowOrCopy,
            )
            .await;
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                let admission = EndpointAdmission(self.endpoint);
                let mut control = ExecutionControl::new(&admission);
                let id = self
                    .query
                    .storage
                    .borrow_mut()
                    .intern_typevar_identity_controlled(identity, occurrence, &mut control)
                    .map_err(|error| runtime_error(self.db, error))?;
                let additional =
                    Support::words_needed(id).saturating_sub(self.query.support.words().len());
                reserve_smallvec(
                    self.query.support.words_mut(),
                    additional,
                    AllocationKind::SupportWords,
                    &mut control,
                )
                .map_err(|error| runtime_error(self.db, error))?;
                control.admit(TddWork::SupportWords { words: additional })?;
                control.admit(TddWork::SupportWords { words: 1 })?;
                self.query.support.insert(id);
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn skipped_lazy(&mut self) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.query.support.mark_incomplete();
                Ok(())
            })
            .await;
        Ok(())
    }
}

/// Collects actual support and publishes it in the supplied builder after admission.
/// The caller retains the temporary support across suspension, including a cache hit.
pub(super) async fn intern_constraint<'call, 'run: 'call, 'db: 'run>(
    db: &'db dyn Db,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    builder: &ConstraintSetBuilder<'db>,
    constraint: Constraint<'db>,
    support: &mut Support,
) -> RunResult<ConstraintId> {
    {
        let mut effects = RuntimeTypeWalk {
            db,
            endpoint,
            query: RuntimeSupport {
                storage: &builder.storage,
                support,
            },
            unavailable: (),
        };
        let bound = match constraint {
            Constraint::ConcreteLower(constraint) => {
                effects.record_occurrence(constraint.typevar).await?;
                Some(constraint.bound)
            }
            Constraint::ConcreteUpper(constraint) => {
                effects.record_occurrence(constraint.typevar).await?;
                Some(constraint.bound)
            }
            Constraint::ConcreteEquivalence(constraint) => {
                effects.record_occurrence(constraint.typevar).await?;
                Some(constraint.bound)
            }
            Constraint::TypeVarRange(constraint) => {
                effects.record_occurrence(constraint.left).await?;
                effects.record_occurrence(constraint.right).await?;
                None
            }
            Constraint::TypeVarEquivalence(constraint) => {
                effects.record_occurrence(constraint.left).await?;
                effects.record_occurrence(constraint.right).await?;
                None
            }
        };
        if let Some(bound) = bound {
            support_type_with(bound, TypeWalkFacts, &mut effects).await?;
        }
    }
    Ok(endpoint
        .local_call(|| {
            builder
                .storage
                .borrow_mut()
                .intern_constraint_controlled(
                    constraint,
                    support,
                    &mut ExecutionControl::new(&EndpointAdmission(endpoint)),
                )
                .map_err(|error| runtime_error(db, error))
        })
        .await)
}

impl<'db> TypeDepthEffects<'db> for RuntimeTypeWalk<'_, '_, 'db> {
    async fn nominal_class(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        nominal_class_with(instance, NominalClassFacts, self).await
    }
    async fn enter_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        Ok(self
            .endpoint
            .local_call(|| {
                enter_depth_active_with(
                    active,
                    ty,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| runtime_error(self.db, error))
            })
            .await)
    }
    async fn leave_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                leave_depth_active_with(
                    active,
                    ty,
                    &mut ExecutionControl::new(&EndpointAdmission(self.endpoint)),
                )
                .map_err(|error| runtime_error(self.db, error))
            })
            .await;
        Ok(())
    }
}

impl<'db> NominalClassEffects<'db> for RuntimeTypeWalk<'_, '_, 'db> {
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.endpoint
            .local_call(|| self.endpoint.admit_work(1))
            .await;
        Ok(())
    }
    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        let class = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(class)
            })
            .await;
        Ok(match class {
            NominalInstanceClass::Plain(class) => class,
            NominalInstanceClass::InheritsFromExplicitAny(class) => {
                self.endpoint
                    .read_field(
                        class
                            .field_requests(self.endpoint.field_request_context())
                            .class(),
                        &BorrowOrCopy,
                    )
                    .await
            }
        })
    }
    async fn tuple_class(&self, _: TupleType<'db>) -> RunResult<ClassType<'db>> {
        self.refuse_depth().await
    }
    async fn version_class(&self) -> RunResult<Option<ClassType<'db>>> {
        self.refuse_depth().await
    }
    async fn object_class(&self) -> RunResult<ClassType<'db>> {
        self.refuse_depth().await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::mem::ManuallyDrop;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use ruff_db::files::system_path_to_file;
    use ruff_index::Idx;
    use salsa::execution_probe::{Demand, ExecutionAdmission, ExecutionWork, RegistryBuilder};
    use salsa::prepared_source_probe;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
    use crate::place::global_symbol;
    use crate::types::constraints::variables::{
        ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, ConstraintProvenance,
        TypeVarEquivalenceBound, TypeVarRangeBound,
    };
    use crate::types::constructor::expansion_probe;
    use crate::types::{
        ClassLiteral, GenericAlias, GenericContext, TypeFormType, TypeVarBoundOrConstraints,
        TypeVarVariance,
    };

    #[derive(Clone, Copy, Debug)]
    enum Operation<'db> {
        Record(BoundTypeVarInstance<'db>),
        Skip,
        Import(Constraint<'db>),
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Value {
        Recorded,
        Imported(ConstraintId),
    }
    #[derive(Clone, Copy)]
    struct Fault {
        index: usize,
        panic: bool,
    }
    #[derive(Debug)]
    struct SupportPanic;

    struct ChildCleanup<'a>(&'a dyn Fn());
    impl Drop for ChildCleanup<'_> {
        fn drop(&mut self) {
            (self.0)();
        }
    }
    struct Admission<'run, 'db: 'run> {
        events: &'run RefCell<Vec<ExecutionWork>>,
        fault: Option<Fault>,
        fired: Cell<bool>,
        endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
        pending: &'run RefCell<Option<Demand<()>>>,
        cleanup: &'run dyn Fn(),
        child_started: &'run Cell<bool>,
    }
    impl ExecutionAdmission for Admission<'_, '_> {
        fn admit(&self, work: ExecutionWork) -> RunResult<()> {
            let index = self.events.borrow().len();
            self.events.borrow_mut().push(work);
            if let Some(fault) = self.fault
                && fault.index == index
                && !self.fired.replace(true)
            {
                let endpoint = self
                    .endpoint
                    .borrow()
                    .as_ref()
                    .cloned()
                    .ok_or(RunError::Contract("support fault has no active endpoint"))?;
                let cleanup = ChildCleanup(self.cleanup);
                let started = self.child_started;
                *self.pending.borrow_mut() = Some(endpoint.demand(move || {
                    started.set(true);
                    async move {
                        let _cleanup = cleanup;
                        Ok(())
                    }
                })?);
                if fault.panic {
                    std::panic::panic_any(SupportPanic);
                }
                return Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance,
                ));
            }
            Ok(())
        }
    }
    struct Reset<'a, 'run, 'db: 'run> {
        endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
        pending: &'a RefCell<Option<Demand<()>>>,
    }
    impl Drop for Reset<'_, '_, '_> {
        fn drop(&mut self) {
            let pending = self.pending.borrow_mut().take();
            let endpoint = self.endpoint.borrow_mut().take();
            drop(pending);
            drop(endpoint);
        }
    }
    struct SupportOwner<'a> {
        support: Support,
        live: &'a Cell<bool>,
        journal: &'a RefCell<Vec<&'static str>>,
        snapshot: &'a RefCell<Option<Support>>,
    }
    impl Drop for SupportOwner<'_> {
        fn drop(&mut self) {
            *self.snapshot.borrow_mut() = Some(self.support.clone());
            self.live.set(false);
            self.journal.borrow_mut().push("support");
        }
    }
    struct SupportRun {
        result: Result<Result<RunResult<Value>, Incomplete>, bool>,
        work: Vec<ExecutionWork>,
        operation_range: std::ops::Range<usize>,
        support: Support,
    }
    fn run_operation<'db>(
        db: &'db TestDb,
        builder: &ConstraintSetBuilder<'db>,
        operation: Operation<'db>,
        fault: Option<Fault>,
    ) -> SupportRun {
        let live = Cell::new(false);
        let delivered = Cell::new(false);
        let journal = RefCell::new(Vec::new());
        let snapshot = RefCell::new(None);
        let child_started = Cell::new(false);
        let child_drops = Cell::new(0);
        let observation = Cell::new(None);
        let work = RefCell::new(Vec::new());
        let start = Cell::new(0);
        let end = Cell::new(0);
        let cleanup = || {
            observation.set(Some((
                live.get(),
                delivered.get(),
                snapshot.borrow().is_none(),
                builder.storage.try_borrow().is_ok(),
            )));
            child_drops.set(child_drops.get() + 1);
            journal.borrow_mut().push("child");
        };
        let captured = prepared_source_probe::capture(db, || {
            catch_unwind(AssertUnwindSafe(|| {
                expansion_probe::run(db, usize::MAX, || {
                    let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
                    let pending = RefCell::new(None);
                    let admission = Admission {
                        events: &work,
                        fault,
                        fired: Cell::new(false),
                        endpoint: &endpoint_slot,
                        pending: &pending,
                        cleanup: &cleanup,
                        child_started: &child_started,
                    };
                    let _reset = Reset {
                        endpoint: &endpoint_slot,
                        pending: &pending,
                    };
                    let registry = RegistryBuilder::new(db, &admission)?;
                    let admission = &admission;
                    let live = &live;
                    let delivered = &delivered;
                    let journal = &journal;
                    let snapshot = &snapshot;
                    let start = &start;
                    let end = &end;
                    registry.seal()?.run(move |endpoint| {
                        **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                        async move {
                            live.set(true);
                            let mut owner = SupportOwner {
                                support: Support::default(),
                                live,
                                journal,
                                snapshot,
                            };
                            start.set(admission.events.borrow().len());
                            let value = match operation {
                                Operation::Import(constraint) => Value::Imported(
                                    intern_constraint(
                                        db,
                                        &endpoint,
                                        builder,
                                        constraint,
                                        &mut owner.support,
                                    )
                                    .await?,
                                ),
                                Operation::Record(occurrence) => {
                                    let mut effects = RuntimeTypeWalk {
                                        db,
                                        endpoint: &endpoint,
                                        query: RuntimeSupport {
                                            storage: &builder.storage,
                                            support: &mut owner.support,
                                        },
                                        unavailable: (),
                                    };
                                    effects.record_occurrence(occurrence).await?;
                                    Value::Recorded
                                }
                                Operation::Skip => {
                                    let mut effects = RuntimeTypeWalk {
                                        db,
                                        endpoint: &endpoint,
                                        query: RuntimeSupport {
                                            storage: &builder.storage,
                                            support: &mut owner.support,
                                        },
                                        unavailable: (),
                                    };
                                    effects.skipped_lazy().await?;
                                    Value::Recorded
                                }
                            };
                            end.set(admission.events.borrow().len());
                            delivered.set(true);
                            Ok(value)
                        }
                    })
                })
            }))
        })
        .expect("support capture");
        assert!(captured.reads.is_empty());
        assert!(!live.get());
        assert!(!child_started.get());
        assert_eq!(child_drops.get(), usize::from(fault.is_some()));
        if fault.is_some() {
            assert_eq!(observation.get(), Some((true, false, true, true)));
            assert!(!delivered.get());
            assert_eq!(&*journal.borrow(), &["child", "support"]);
        } else {
            assert_eq!(observation.get(), None);
            assert!(delivered.get());
            assert_eq!(&*journal.borrow(), &["support"]);
        }
        assert_storage_consistent(db, &builder.storage.borrow());
        SupportRun {
            result: captured
                .value
                .map(|value| value.0)
                .map_err(|payload| payload.is::<SupportPanic>()),
            work: work.into_inner(),
            operation_range: start.get()..end.get(),
            support: snapshot
                .into_inner()
                .expect("the actual support owner was destroyed"),
        }
    }

    fn variable<'db>(db: &'db TestDb, name: Name) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            name,
            TypeVarVariance::Invariant,
        )
    }
    fn assert_storage_consistent<'db>(db: &'db TestDb, storage: &ConstraintSetStorage<'db>) {
        // Skipped source attributes legitimately leave a published support incomplete.
        // Its IDs and cache entries still have to describe the actual retained arenas.
        assert_eq!(storage.typevars.len(), storage.typevar_cache.len());
        for (id, variable) in storage.typevars.iter_enumerated() {
            assert_eq!(storage.typevar_cache.get(&variable.identity(db)), Some(&id));
        }
        assert_eq!(storage.constraints.len(), storage.constraint_cache.len());
        assert_eq!(storage.constraints.len(), storage.constraint_supports.len());
        for (id, constraint) in storage.constraints.iter_enumerated() {
            assert_eq!(storage.constraint_cache.get(constraint), Some(&id));
            let support = storage.constraint_support(id);
            assert!(support.iter().all(|id| id.index() < storage.typevars.len()));
        }
        assert!(storage.nodes.is_empty());
        assert!(storage.source_orders.is_empty());
    }
    fn concrete_constraints<'db>(
        subject: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
        provenance: ConstraintProvenance,
    ) -> [Constraint<'db>; 3] {
        [
            ConcreteLowerBound::new(provenance, subject, bound).into(),
            ConcreteUpperBound::new(provenance, subject, bound).into(),
            ConcreteEquivalenceBound::new(provenance, subject, bound).into(),
        ]
    }
    #[derive(Default)]
    struct Work(RefCell<Vec<ExecutionWork>>);
    impl ExecutionAdmission for Work {
        fn admit(&self, work: ExecutionWork) -> RunResult<()> {
            self.0.borrow_mut().push(work);
            Ok(())
        }
    }
    fn eligible<'db>(
        db: &'db TestDb,
        ty: Type<'db>,
        allowance: usize,
    ) -> Result<RunResult<bool>, Incomplete> {
        let work = Work::default();
        expansion_probe::run(db, allowance, || {
            RegistryBuilder::new(db, &work)?
                .seal()?
                .run(|endpoint| async move { static_eligible(db, &endpoint, ty).await })
        })
        .0
    }
    fn check_constraint<'db>(
        db: &'db TestDb,
        constraint: Constraint<'db>,
        expected: &[BoundTypeVarInstance<'db>],
        complete: bool,
    ) {
        let builder = ConstraintSetBuilder::new();
        let ordinary = ConstraintSetBuilder::new();
        let ordinary_id = ordinary.storage.borrow_mut().intern_constraint(
            db,
            &db.program_environment(),
            constraint,
        );
        let run = run_operation(db, &builder, Operation::Import(constraint), None);
        let Ok(Ok(Ok(Value::Imported(id)))) = run.result else {
            panic!("support import did not complete: {:?}", run.result);
        };
        assert_eq!(builder.storage.borrow().constraint_data(id), constraint);
        let storage = builder.storage.borrow();
        let support = storage.constraint_support(id);
        assert_eq!(
            support
                .iter()
                .map(|id| storage.typevar_data(id))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(support.is_complete(), complete);
        assert_eq!(
            support,
            ordinary.storage.borrow().constraint_support(ordinary_id)
        );
        assert_eq!(run.support, Support::default());
        drop(storage);
        let retry = run_operation(db, &builder, Operation::Import(constraint), None);
        assert_eq!(retry.result, Ok(Ok(Ok(Value::Imported(id)))));
        assert_eq!(builder.storage.borrow().constraints.len(), 1);
        assert_eq!(
            retry.support,
            *builder.storage.borrow().constraint_support(id)
        );
    }

    #[test]
    fn controlled_support_uses_subject_first_actual_storage() {
        let db = setup_db();
        let env = db.program_environment();
        let [subject, first, last] =
            ["Subject", "First", "Last"].map(|name| variable(&db, Name::new_static(name)));
        let shared = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(first)));
        let root = Type::tuple(TupleType::heterogeneous(
            &db,
            &env,
            [
                Type::TypeVar(first),
                Type::TypeVar(first),
                shared,
                shared,
                Type::TypeVar(last),
                Type::TypeVar(subject),
            ],
        ));
        for provenance in [
            ConstraintProvenance::Evidence,
            ConstraintProvenance::Validity,
        ] {
            for constraint in concrete_constraints(subject, root, provenance) {
                check_constraint(&db, constraint, &[subject, first, last], true);
            }
            for (constraint, expected) in {
                let range = TypeVarRangeBound::new(&db, provenance, subject, first);
                let equality = TypeVarEquivalenceBound::new(&db, provenance, subject, first);
                [
                    (Constraint::from(range), [range.left, range.right]),
                    (Constraint::from(equality), [equality.left, equality.right]),
                ]
            } {
                check_constraint(&db, constraint, &expected, true);
            }
        }
        assert_eq!(eligible(&db, root, usize::MAX), Ok(Ok(true)));
        assert_eq!(eligible(&db, root, 1), Err(Incomplete::Allowance));
        assert_eq!(eligible(&db, root, usize::MAX), Ok(Ok(true)));
        let gradual = Type::TypeForm(TypeFormType::new(&db, Type::any()));
        assert_eq!(eligible(&db, gradual, usize::MAX), Ok(Ok(false)));
    }

    #[test]
    fn controlled_support_preserves_skips_and_alias_declaration_policy() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().with_file("/src/support.py", "class Scope[T]: ...\nRecursive = tuple[int, \"Recursive | None\"]\nrecursive: Recursive\ntype Alias = int\nalias: Alias\n").build()?;
        let env = db.program_environment();
        let module = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/support.py")?,
            env.program(&db),
        );
        let subject = variable(&db, Name::new_static("Subject"));
        for name in ["recursive", "alias"] {
            let ty = global_symbol(&db, module, name).place.expect_type();
            assert!(
                matches!(ty, Type::Recursive(_))
                    || (name == "alias" && matches!(ty, Type::TypeAlias(_)))
            );
            check_constraint(
                &db,
                concrete_constraints(subject, ty, ConstraintProvenance::Evidence)[0],
                &[subject],
                false,
            );
            assert_eq!(eligible(&db, ty, usize::MAX), Ok(Ok(true)));
        }
        let Type::ClassLiteral(ClassLiteral::Static(origin)) =
            global_symbol(&db, module, "Scope").place.expect_type()
        else {
            anyhow::bail!("Scope is not a static class");
        };
        let declaration = variable(&db, Name::new_static("Declaration"))
            .map_bound_or_constraints(&db, |_| {
                Some(TypeVarBoundOrConstraints::UpperBound(Type::any()))
            });
        let argument = variable(&db, Name::new_static("Argument"));
        let context = GenericContext::from_typevar_instances(&db, &env, [declaration]);
        let alias = Type::GenericAlias(GenericAlias::new(
            &db,
            origin,
            context.specialize(&db, vec![Type::TypeVar(argument)]),
        ));
        check_constraint(
            &db,
            concrete_constraints(subject, alias, ConstraintProvenance::Evidence)[0],
            &[subject, argument],
            true,
        );
        assert_eq!(
            eligible(&db, Type::TypeVar(declaration), usize::MAX),
            Ok(Ok(true))
        );
        assert_eq!(eligible(&db, alias, usize::MAX), Ok(Ok(false)));
        Ok(())
    }

    fn seeded<'db>(db: &'db TestDb) -> ConstraintSetBuilder<'db> {
        let builder = ConstraintSetBuilder::new();
        for index in 0..(2 * usize::BITS) {
            let seed = variable(db, Name::new(format!("Seed{index}")));
            builder.storage.borrow_mut().intern_typevar(db, seed);
        }
        builder
    }

    #[test]
    fn controlled_support_cutoffs_preserve_mutations_and_cleanup_order() {
        let db = setup_db();
        let target = variable(&db, Name::new_static("Target"));
        for operation in [Operation::Record(target), Operation::Skip] {
            let complete_builder = seeded(&db);
            let complete = run_operation(&db, &complete_builder, operation, None);
            assert_eq!(complete.result, Ok(Ok(Ok(Value::Recorded))));
            assert!(!complete.operation_range.is_empty());
            match operation {
                Operation::Record(_) => {
                    assert_eq!(complete.support.words().len(), 3);
                    assert!(complete.support.is_complete());
                    assert_eq!(
                        complete
                            .support
                            .iter()
                            .map(|id| complete_builder.storage.borrow().typevar_data(id))
                            .collect::<Vec<_>>(),
                        [target]
                    );
                }
                Operation::Skip => {
                    assert!(complete.support.words().is_empty());
                    assert!(!complete.support.is_complete());
                }
                Operation::Import(_) => panic!("this fixture isolates support mutation"),
            }
            for index in complete.operation_range.clone() {
                let builder = seeded(&db);
                let refused = run_operation(
                    &db,
                    &builder,
                    operation,
                    Some(Fault {
                        index,
                        panic: false,
                    }),
                );
                assert_eq!(refused.result, Ok(Err(Incomplete::Allowance)));
                assert_eq!(refused.support, Support::default());
                assert_eq!(refused.work[index], complete.work[index]);
                let retry = run_operation(&db, &builder, operation, None);
                assert_eq!(retry.result, complete.result);
                assert_eq!(retry.support, complete.support);
            }
            let builder = seeded(&db);
            let panicked = run_operation(
                &db,
                &builder,
                operation,
                Some(Fault {
                    index: complete.operation_range.start,
                    panic: true,
                }),
            );
            assert_eq!(panicked.result, Err(true));
            assert_eq!(panicked.support, Support::default());
            let retry = run_operation(&db, &builder, operation, None);
            assert_eq!(retry.support, complete.support);
        }
    }

    #[test]
    fn controlled_support_import_cutoffs_leave_consistent_completed_prefixes() {
        let db = setup_db();
        let subject = variable(&db, Name::new_static("Subject"));
        let bound_var = variable(&db, Name::new_static("Bound"));
        let bound = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(bound_var)));
        let constraint = concrete_constraints(subject, bound, ConstraintProvenance::Evidence)[2];
        let complete = run_operation(
            &db,
            &ConstraintSetBuilder::new(),
            Operation::Import(constraint),
            None,
        );
        for index in complete.operation_range.clone() {
            let builder = ConstraintSetBuilder::new();
            let refused = run_operation(
                &db,
                &builder,
                Operation::Import(constraint),
                Some(Fault {
                    index,
                    panic: false,
                }),
            );
            assert_eq!(refused.result, Ok(Err(Incomplete::Allowance)));
            let storage = builder.storage.borrow();
            let types = storage.typevars.iter().copied().collect::<Vec<_>>();
            assert_eq!(types, [subject, bound_var][..types.len()]);
            let recorded = refused
                .support
                .iter()
                .map(|id| storage.typevar_data(id))
                .collect::<Vec<_>>();
            assert_eq!(recorded, [subject, bound_var][..recorded.len()]);
            assert!(refused.support.is_complete());
            assert!(storage.constraints.is_empty());
            drop(storage);
            let retry = run_operation(&db, &builder, Operation::Import(constraint), None);
            let Ok(Ok(Ok(Value::Imported(id)))) = retry.result else {
                panic!("same-builder retry failed: {:?}", retry.result);
            };
            let storage = builder.storage.borrow();
            assert_eq!(storage.constraint_data(id), constraint);
            assert_eq!(
                storage
                    .constraint_support(id)
                    .iter()
                    .map(|id| storage.typevar_data(id))
                    .collect::<Vec<_>>(),
                [subject, bound_var]
            );
            assert!(storage.constraint_support(id).is_complete());
        }
    }
}
