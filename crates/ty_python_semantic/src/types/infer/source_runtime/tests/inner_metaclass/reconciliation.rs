//! Checks direct-class owner identity and interruption of retained metaclass ancestor storage.

use super::*;
use crate::types::GenericAlias;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::relation::source::RelationSourceOperation;
use crate::types::relation::source::resources::{
    ClassRelation, RelationResourceAccess, observations as class_observations,
};
use crate::types::relation::source::retained::observations as task_observations;
use crate::types::relation::{RelationOwners, TypeRelation};
use crate::types::typevar::TypeVarSet;

/// Compares the same class operands in both directions within one resource-owning run.
#[derive(Clone, Copy, Debug)]
struct ClassConditions<'db> {
    source: ClassType<'db>,
    target: ClassType<'db>,
    relation: ClassRelation,
}

impl<'db> MemberOperation<'db> for ClassConditions<'db> {
    type Output = [bool; 2];

    /// Runs two fresh direct-class conditions while keeping their owners alive for inspection.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = effects
            .initialize_value(|| ProgramEnvironment::from_program(program))
            .await?;
        let forward = access
            .resources()
            .class_condition(
                access.db(),
                &env,
                self.source,
                self.target,
                self.relation,
                &effects,
            )
            .await?;
        let reverse = access
            .resources()
            .class_condition(
                access.db(),
                &env,
                self.target,
                self.source,
                self.relation,
                &effects,
            )
            .await?;
        Ok([forward, reverse])
    }
}

/// Computes the ordinary direct-class result with a fresh builder and all four relation visitors.
fn ordinary_condition<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    source: ClassType<'db>,
    target: ClassType<'db>,
    relation: ClassRelation,
) -> bool {
    let constraints = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(env, &constraints);
    let checker = match relation {
        ClassRelation::Subtyping => owners.subtyping(TypeVarSet::None),
        ClassRelation::Assignability => owners.assignability(TypeVarSet::None),
    };
    checker
        .check_class_pair(db, source, target)
        .is_always_satisfied(db, env)
}

/// Direct-class conditions match ordinary comparison while each direction has fresh owners.
/// Queued class pairs keep their root's checker state, and satisfaction keeps its original terminal.
#[test_case::test_case("Base", "Base", ClassRelation::Subtyping; "equal subtyping")]
#[test_case::test_case("Child", "Base", ClassRelation::Subtyping; "asymmetric subtyping")]
#[test_case::test_case("Other", "Base", ClassRelation::Subtyping; "unrelated subtyping")]
#[test_case::test_case("Base", "Base", ClassRelation::Assignability; "equal assignability")]
#[test_case::test_case("Child", "Base", ClassRelation::Assignability; "asymmetric assignability")]
#[test_case::test_case("Other", "Base", ClassRelation::Assignability; "unrelated assignability")]
fn direct_class_conditions_preserve_owners_and_terminals(
    source_name: &str,
    target_name: &str,
    relation: ClassRelation,
) {
    let source = "class Base: ...\nclass Child(Base): ...\nclass Other: ...\n";
    let ordinary = database(source);
    let ordinary_prepared = prepare(&ordinary);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_source = ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
        &ordinary,
        ordinary_prepared.program_file(),
        source_name,
    )));
    let ordinary_target = ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
        &ordinary,
        ordinary_prepared.program_file(),
        target_name,
    )));
    let expected = [
        ordinary_condition(
            &ordinary,
            &ordinary_env,
            ordinary_source,
            ordinary_target,
            relation,
        ),
        ordinary_condition(
            &ordinary,
            &ordinary_env,
            ordinary_target,
            ordinary_source,
            relation,
        ),
    ];
    let db = database(source);
    let prepared = prepare(&db);
    let request = ClassConditions {
        source: ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
            &db,
            prepared.program_file(),
            source_name,
        ))),
        target: ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
            &db,
            prepared.program_file(),
            target_name,
        ))),
        relation,
    };
    class_observations::reset_class_conditions();
    task_observations::reset(None);
    assert_eq!(
        controlled_member_operation(&prepared, request, &funded()),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    let journal = class_observations::class_condition_snapshot();
    assert_eq!(journal.root_count, 2);
    assert_eq!(journal.pair_count, 2);
    assert_eq!(journal.result_count, 2);
    let [Some(first), Some(second), ..] = journal.roots else {
        panic!("both direct-class roots must be observed");
    };
    assert_ne!(first.builder, second.builder);
    assert_ne!(first.environment, second.environment);
    assert!(
        first
            .visitors
            .into_iter()
            .zip(second.visitors)
            .all(|(left, right)| left != right)
    );
    let expected_relation = match relation {
        ClassRelation::Subtyping => TypeRelation::Subtyping,
        ClassRelation::Assignability => TypeRelation::Assignability,
    };
    assert_eq!(first.relation, expected_relation);
    assert_eq!(second.relation, expected_relation);
    assert_eq!(first.inferable, None);
    assert_eq!(second.inferable, None);
    let [Some(first_pair), Some(second_pair), ..] = journal.pairs else {
        panic!("both queued direct-class pairs must be observed");
    };
    assert_eq!(first_pair.identity, first);
    assert_eq!(second_pair.identity, second);
    assert_ne!(first_pair.checker, second_pair.checker);
    let [Some(first_result), Some(second_result), ..] = journal.results else {
        panic!("both direct-class results must be observed");
    };
    assert_eq!(first_result.builder, first.builder);
    assert_eq!(second_result.builder, second.builder);
    assert_eq!(first_result.terminal, Some(expected[0]));
    assert_eq!(second_result.terminal, Some(expected[1]));
    assert!(first_result.original_terminal);
    assert!(second_result.original_terminal);
    let (live, entered, _) = task_observations::progress();
    assert_eq!(live, 0);
    assert!(entered >= 2);
    assert_no_active_attempt();
}

/// A reached generic-specialization child refuses before either directional boolean is returned.
#[test_case::test_case(ClassRelation::Subtyping; "subtyping")]
#[test_case::test_case(ClassRelation::Assignability; "assignability")]
fn direct_generic_class_condition_preserves_typed_refusal(relation: ClassRelation) {
    let db = database("class Product[T]: ...\n");
    let prepared = prepare(&db);
    let origin = class_in_file(&db, prepared.program_file(), "Product");
    let Some(context) = origin.generic_context(&db) else {
        panic!("Product fixture must have a generic context");
    };
    let source = ClassType::Generic(GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, &[Type::int_literal(1)]),
    ));
    let target = ClassType::Generic(GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, &[Type::bool_literal(true)]),
    ));
    class_observations::reset_class_conditions();
    task_observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            ClassConditions {
                source,
                target,
                relation
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::UnavailableOperation(OperationId::Relation(
                RelationSourceOperation::ClassSpecialization,
            )),
            completed: (),
        }),
    );
    let journal = class_observations::class_condition_snapshot();
    assert_eq!(journal.root_count, 1);
    assert_eq!(journal.pair_count, 1);
    assert_eq!(journal.result_count, 0);
    assert_eq!(task_observations::progress().0, 0);
    assert_no_active_attempt();
}

/// Checks that a real suspension precedes retirement of the specified retained comparison.
fn assert_retired_after_pending(snapshot: &task_observations::Snapshot, child: usize) {
    assert!(!snapshot.overflowed);
    let events = &snapshot.events[..snapshot.count];
    let pending = events
        .iter()
        .position(
            |event| matches!(event, Some(task_observations::Event::Pending(id)) if *id == child),
        )
        .unwrap();
    let retired = events
        .iter()
        .position(
            |event| matches!(event, Some(task_observations::Event::Retired(id)) if *id == child),
        )
        .unwrap();
    assert!(pending < retired);
}

/// Prepares an asymmetric direct-class comparison whose source MRO requires a canonical child.
fn child_to_base<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> ClassConditions<'db> {
    ClassConditions {
        source: ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
            db,
            prepared.program_file(),
            "Child",
        ))),
        target: ClassType::NonGeneric(ClassLiteral::Static(class_in_file(
            db,
            prepared.program_file(),
            "Base",
        ))),
        relation: ClassRelation::Subtyping,
    }
}

/// Cancellation after a retained class comparison actually suspends retires its child task.
/// Retrying the interrupted pair of conditions returns both results at the same revision.
#[test]
fn cancelled_pending_class_child_drains_and_retries() {
    let source = "class Base: ...\nclass Child(Base): ...\n";
    let measured = database(source);
    let prepared = prepare(&measured);
    let request = child_to_base(&measured, &prepared);
    task_observations::reset(None);
    assert_eq!(
        controlled_member_operation(&prepared, request, &funded()),
        Ok(AnalysisOutcome::Complete([true, false])),
    );
    let snapshot = task_observations::snapshot();
    assert!(!snapshot.overflowed);
    let child = snapshot.events[..snapshot.count]
        .iter()
        .find_map(|event| match event {
            Some(task_observations::Event::Pending(child)) => Some(*child),
            Some(task_observations::Event::Entered(_) | task_observations::Event::Retired(_))
            | None => None,
        })
        .expect("the class MRO child must suspend the retained comparison");
    assert_retired_after_pending(&snapshot, child);
    assert_eq!(task_observations::progress().0, 0);
    assert_no_active_attempt();

    let db = database(source);
    let prepared = prepare(&db);
    let request = child_to_base(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    task_observations::reset(None);
    task_observations::set_cancel_at_pending(Some(child));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert_retired_after_pending(&task_observations::snapshot(), child);
    assert_eq!(task_observations::progress().0, 0);
    assert_no_active_attempt();
    task_observations::reset(None);
    task_observations::set_cancel_at_pending(None);
    assert_eq!(
        controlled_member_operation(&prepared, request, &funded()),
        Ok(AnalysisOutcome::Complete([true, false])),
    );
    assert_eq!(task_observations::progress().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Checker retention succeeds with a small work allowance despite its larger representation.
/// The normal requested-byte allowance still admits the checker's storage and transfers.
#[test]
fn checker_retention_uses_scalar_construction_work() {
    let db = database("class Product: ...\n");
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let storage = CheckerStorage::new();
    let result = with_analysis_session(&prepared, &funded(), |session| {
        let db = &db;
        let storage = &storage;
        let owners = &owners;
        RegistryBuilder::with_budget(session.db(), session.budget())?
            .seal()?
            .run(|endpoint| async move {
                let remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
                endpoint
                    .local_call(|| endpoint.admit_work(remaining - 32))
                    .await;
                let _checker = storage
                    .allocate(&endpoint, || owners.subtyping(TypeVarSet::None))
                    .await;
                Ok(())
            })
    });
    assert_eq!(result, Ok(AnalysisOutcome::Complete(())));
    let Some((retained, bytes)) = storage.retained_payload() else {
        panic!("checker storage must report its retained payload");
    };
    assert_eq!(retained, 1);
    assert!(bytes > 0);
    assert_no_active_attempt();
}

/// Records target-ancestor owner construction, destructor entry, and its first admitted insertion.
#[derive(Clone, Copy, Debug, Default)]
struct AncestorJournal {
    created: usize,
    dropped: usize,
    live: usize,
    first_remaining_work: Option<usize>,
    cancel: bool,
    cancelled: bool,
}

thread_local! {
    static ANCESTORS: RefCell<Option<AncestorJournal>> = const { RefCell::new(None) };
}

/// Keeps ancestor observations enabled until all registered work has drained.
#[derive(Debug)]
struct AncestorRecording;

impl AncestorRecording {
    /// Starts an isolated ancestor-set journal for the current test thread.
    fn start() -> Self {
        ANCESTORS.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(AncestorJournal::default());
        });
        Self
    }

    /// Copies the currently recorded ownership and insertion state.
    fn snapshot(&self) -> AncestorJournal {
        ANCESTORS.with_borrow(|journal| journal.unwrap())
    }

    /// Requests cancellation at the first completed insertion while its set is live.
    fn cancel(&self) {
        ANCESTORS.with_borrow_mut(|journal| journal.as_mut().unwrap().cancel = true);
    }
}

impl Drop for AncestorRecording {
    fn drop(&mut self) {
        ANCESTORS.with_borrow_mut(|journal| *journal = None);
    }
}

/// Records a new set and returns the callback that records entry to its owner's destructor.
pub(in crate::types::infer) fn observe_ancestors_created(_db: &dyn Db) -> Option<fn()> {
    ANCESTORS.with_borrow_mut(|journal| {
        let journal = journal.as_mut()?;
        journal.created += 1;
        journal.live += 1;
        Some(observe_ancestors_drop as fn())
    })
}

/// Captures the first insertion's remaining allowance and optionally cancels while storage is live.
pub(in crate::types::infer) fn observe_ancestors_ready(
    db: &dyn Db,
    _len: usize,
    _capacity: usize,
    _table_slots: usize,
    _ordered_capacity: usize,
    _peak_attempt: usize,
) {
    let cancel = ANCESTORS.with_borrow_mut(|journal| {
        let Some(journal) = journal else {
            return false;
        };
        assert!(journal.live > 0);
        journal.first_remaining_work.get_or_insert_with(|| {
            salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap()
        });
        if journal.cancel {
            journal.cancel = false;
            journal.cancelled = true;
            true
        } else {
            false
        }
    });
    if cancel {
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
    }
}

/// Records ancestor-owner destructor entry without requiring a further fallible admission.
fn observe_ancestors_drop() {
    ANCESTORS.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.dropped += 1;
            journal.live -= 1;
        }
    });
}

/// Prepares shared gradual ancestry so reconciliation must retain a target-ancestor set.
fn ancestor_database() -> TestDb {
    database(
        "from typing import Any\nbase: Any\nclass RootMeta(base): ...\nclass LeftMeta(RootMeta): ...\nclass RightMeta(RootMeta): ...\nclass Left(metaclass=LeftMeta): ...\nclass Right(metaclass=RightMeta): ...\nclass Product(Left, Right): ...\n",
    )
}

/// Checks after a run that every ancestor owner entered its destructor and no relation child remains live.
fn assert_ancestors_drained(journal: AncestorJournal) {
    assert_eq!(journal.live, 0);
    assert_eq!(journal.created, journal.dropped);
    assert_eq!(task_observations::progress().0, 0);
    assert_no_active_attempt();
}

/// Requires the shared-ancestor fixture to complete with its ordinary metaclass conflict.
fn assert_conflict(result: Result<AnalysisOutcome<MetaclassSelectionResult<'_>>, AnalysisFailure>) {
    let Ok(AnalysisOutcome::Complete(Err(error))) = result else {
        panic!("expected completed metaclass conflict, got {result:?}");
    };
    assert!(matches!(
        error.reason(),
        MetaclassErrorKind::Conflict { .. }
    ));
}

/// Finds whether a fresh run reaches a live ancestor set within the requested-byte limit.
fn reaches_ancestors_with_bytes(bytes: usize) -> bool {
    let db = ancestor_database();
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let recording = AncestorRecording::start();
    let result = controlled_member_operation(
        &prepared,
        Request(class),
        &AnalysisPolicy {
            requested_bytes_limit: bytes,
            ..funded()
        },
    );
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(_))
                | Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                })
        ),
        "{result:?}"
    );
    let journal = recording.snapshot();
    assert_ancestors_drained(journal);
    journal.first_remaining_work.is_some()
}

/// Work refusal after an ancestor insertion retires storage without publishing a partial result.
/// A fully funded retry completes in the same revision.
#[test]
fn live_ancestor_work_refusal_drains_and_retries() {
    let measured = ancestor_database();
    let prepared = prepare(&measured);
    let class = cold_class(&measured, &prepared);
    let recording = AncestorRecording::start();
    assert_conflict(controlled(&prepared, class));
    let work = funded().semantic_work_limit - recording.snapshot().first_remaining_work.unwrap();
    assert_ancestors_drained(recording.snapshot());
    drop(recording);
    assert_refusal_then_retry(
        AnalysisPolicy {
            semantic_work_limit: work,
            ..funded()
        },
        AnalysisIncomplete::WorkLimit,
    );
}

/// Byte refusal after an ancestor insertion retires storage without publishing a partial result.
/// Fresh databases locate the first live-set boundary without reusing canonical child memos.
#[test]
fn live_ancestor_byte_refusal_drains_and_retries() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    assert!(reaches_ancestors_with_bytes(upper));
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if reaches_ancestors_with_bytes(middle) {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    assert_refusal_then_retry(
        AnalysisPolicy {
            requested_bytes_limit: lower,
            ..funded()
        },
        AnalysisIncomplete::RequestedAllocationLimit,
    );
}

/// Checks live-set refusal, absence of a parent memo, and funded same-revision retry.
fn assert_refusal_then_retry(policy: AnalysisPolicy, reason: AnalysisIncomplete) {
    let db = ancestor_database();
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = AncestorRecording::start();
    assert_eq!(
        controlled_member_operation(&prepared, Request(class), &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: ()
        })
    );
    assert!(recording.snapshot().first_remaining_work.is_some());
    assert_ancestors_drained(recording.snapshot());
    assert_missing(&db, class);
    assert_conflict(controlled(&prepared, class));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_ancestors_drained(recording.snapshot());
}

/// Cancellation while the ancestor set is live retires it and permits same-revision retry.
/// A canonical result published before cancellation is delivered remains reusable.
#[test]
fn cancelled_live_ancestors_drain_and_retry() {
    let db = ancestor_database();
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = AncestorRecording::start();
    recording.cancel();
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, class)));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(recording.snapshot().cancelled);
    assert_ancestors_drained(recording.snapshot());
    assert_conflict(controlled(&prepared, class));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_ancestors_drained(recording.snapshot());
}
