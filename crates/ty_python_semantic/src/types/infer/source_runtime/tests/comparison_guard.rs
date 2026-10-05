//! Exercises comparison visitor growth through cold source inference and its real admission policy.
//! Rust controls are needed to observe collection capacity, individual work/byte refusals and native
//! scope destruction; mdtests can check the resulting type but cannot inspect those runtime states.

use std::cell::RefCell;

use super::*;
use crate::types::cyclic::CycleDetectorStorageProbe;
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::infer::comparisons::BinaryComparisonVisitor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Storage {
    Active,
    Cache,
}

impl Storage {
    const fn index(self) -> usize {
        match self {
            Self::Active => 0,
            Self::Cache => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    BeforeVisit,
    Pending,
    BeforeFinish,
    Prepared,
    Committed,
}

#[derive(Clone, Copy, Debug)]
struct State {
    stage: Stage,
    storage: CycleDetectorStorageProbe,
}

#[derive(Clone, Copy, Debug, Default)]
struct Relocation {
    started: usize,
    accepted: usize,
}

/// Keeps passive snapshots of one live comparison visitor without retaining that visitor.
#[derive(Clone, Copy, Debug)]
struct Journal {
    visitor: Option<usize>,
    states: [Option<State>; 24],
    count: usize,
    overflowed: bool,
    relocations: [Relocation; 2],
}

impl Journal {
    const fn new() -> Self {
        Self {
            visitor: None,
            states: [None; 24],
            count: 0,
            overflowed: false,
            relocations: [Relocation { started: 0, accepted: 0 }; 2],
        }
    }

    fn states(&self) -> impl Iterator<Item = State> + '_ {
        self.states[..self.count].iter().flatten().copied()
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static CANCEL: Cell<Option<Storage>> = const { Cell::new(None) };
    static JOURNAL: RefCell<Journal> = const { RefCell::new(Journal::new()) };
}

/// Enables bounded observations and optional cancellation until the source operation drains.
#[derive(Debug)]
struct Recording;

impl Recording {
    /// Starts recording and optionally arms cancellation after storage growth.
    /// `Some(Storage::Active)` cancels with two pending scopes; `Some(Storage::Cache)` cancels
    /// after cache reservation has been accepted, before the third result commits.
    fn start(cancel: Option<Storage>) -> Self {
        assert!(!ENABLED.replace(true));
        CANCEL.set(cancel);
        JOURNAL.with_borrow_mut(|journal| *journal = Journal::new());
        observations::reset(None);
        lifetime_observations::reset();
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ENABLED.set(false);
        CANCEL.set(None);
        lifetime_observations::stop();
    }
}

/// Records the adapter's actual visitor storage at an existing lookup or finish boundary.
/// Also requests cancellation when a post-growth boundary armed by `Recording::start` is reached.
pub(in crate::types::infer) fn observe(
    db: &dyn crate::Db,
    visitor: &BinaryComparisonVisitor<'_>,
    stage: Stage,
) {
    if !ENABLED.get() {
        return;
    }
    let address = std::ptr::from_ref(visitor).addr();
    let storage = visitor.ownership_probe_storage();
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(previous) = journal.visitor {
            assert_eq!(previous, address, "fixture must use one comparison visitor");
        } else {
            journal.visitor = Some(address);
            lifetime_observations::relation_conversion(address, visitor.ownership_probe_counts());
        }
        if let Some(slot) = journal.states.get_mut(journal.count) {
            *slot = Some(State { stage, storage });
            journal.count += 1;
        } else {
            journal.overflowed = true;
        }
    });
    let cancellation_boundary = match CANCEL.get() {
        Some(Storage::Active) => stage == Stage::Pending && storage.active_len == 2,
        Some(Storage::Cache) => {
            stage == Stage::Prepared && storage.cache_len == 2 && storage.cache_capacity.is_some()
        }
        None => false,
    };
    if cancellation_boundary {
        CANCEL.set(None);
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn relocation_started(storage: Storage) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.relocations[storage.index()].started += 1);
    }
}

pub(in crate::types::infer) fn relocation_accepted(storage: Storage) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.relocations[storage.index()].accepted += 1);
    }
}

fn snapshot() -> Journal {
    let journal = JOURNAL.with_borrow(|journal| *journal);
    assert!(!journal.overflowed, "comparison observation buffer overflowed");
    journal
}

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = (((3,),),) >= (((2,),),)\n")
        .unwrap();
    db
}

/// Checks the literal result against ordinary inference in a separate, initially cold database.
fn ordinary_result() -> bool {
    let db = fixture();
    let expression = expression(&db);
    let inferred = infer_expression_types(&db, expression, TypeContext::default());
    assert_eq!(
        inferred.expression_type(expression.node_ref(&db)),
        Type::bool_literal(true)
    );
    true
}

/// Checks that cold execution completed three real tuple keys and spilled both native collections.
fn assert_spilled() {
    let journal = snapshot();
    assert!(journal.states().any(|state| {
        state.stage == Stage::Pending
            && state.storage.active_len == 3
            && state.storage.active_capacity > 1
    }), "{journal:?}");
    assert!(journal.states().any(|state| {
        state.stage == Stage::Prepared
            && state.storage.active_len == 1
            && state.storage.cache_len == 2
            && state.storage.cache_capacity.is_some()
    }), "{journal:?}");
    assert!(journal.states().any(|state| {
        state.stage == Stage::Committed
            && state.storage.active_len == 0
            && state.storage.cache_len == 3
            && state.storage.cache_capacity.is_some()
    }), "{journal:?}");
    assert_eq!(journal.relocations[Storage::Active.index()].accepted, 1);
    assert_eq!(journal.relocations[Storage::Cache.index()].accepted, 1);
}

/// Verifies all observed comparison scopes and source builders have drained.
fn assert_cleanup() {
    let journal = snapshot();
    let lifetime = lifetime_observations::snapshot();
    assert!(!lifetime.overflowed, "native lifetime observation buffer overflowed");
    if let Some(visitor) = journal.visitor
        && let Some(counts) = lifetime.events[..lifetime.count].iter().rev().find_map(|event| {
            match event {
                Some(lifetime_observations::Event::RelationDropAfter { visitor: actual, counts, .. })
                    if *actual == visitor => Some(counts),
                _ => None,
            }
        })
    {
        assert_eq!(counts.0, 0, "comparison active entries survived drainage");
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Verifies native unfinished-scope removal and source-owner drainage after interruption.
fn assert_drained(active_at_failure: usize, cached: usize) {
    let journal = snapshot();
    let visitor = journal.visitor.unwrap();
    let lifetime = lifetime_observations::snapshot();
    assert!(!lifetime.overflowed, "native lifetime observation buffer overflowed");
    let events = &lifetime.events[..lifetime.count];
    let before = events.iter().position(|event| matches!(event,
        Some(lifetime_observations::Event::RelationDropBefore { visitor: actual, counts, had_item: true })
            if *actual == visitor && *counts == (active_at_failure, cached)
    )).expect("unfinished comparison scope remained present until native drop");
    let after = events.iter().rposition(|event| matches!(event,
        Some(lifetime_observations::Event::RelationDropAfter { visitor: actual, counts, had_item: true })
            if *actual == visitor && *counts == (0, cached)
    )).expect("all unfinished comparison scopes removed their active entries");
    assert!(before < after, "{events:?}");
    if active_at_failure == 2 {
        let child = events.iter().position(|event| matches!(event,
            Some(lifetime_observations::Event::RelationDropAfter { visitor: actual, counts, had_item: true })
                if *actual == visitor && *counts == (1, cached)
        )).expect("nested scope drained before enclosing scope");
        assert!(before < child && child < after, "{events:?}");
    }
    let source_owner = events.iter().rposition(|event| matches!(event,
        Some(lifetime_observations::Event::SourceChildDropped { .. })
    )).expect("source expression builder drained");
    assert!(after < source_owner, "{events:?}");
    assert_cleanup();
}

/// Checks canonical publication and reuse after a successful cold attempt or same-revision retry.
fn assert_canonical(db: &TestDb, prepared: &PreparedAnalysisFile<'_>, expected: Type<'_>) {
    let expression = expression(db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let canonical = infer_expression_types(db, expression, TypeContext::default());
    assert_eq!(canonical.expression_type(expression.node_ref(db)), expected);
    assert!(canonical.extra.is_none());
    assert_eq!(
        expression_type_with_policy(prepared, expression_key(prepared), &funded()),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert!(std::ptr::eq(
        canonical,
        infer_expression_types(db, expression, TypeContext::default())
    ));
    assert_function_query_was_not_run_by_name(
        db,
        "infer_expression_types_impl",
        None,
        &events_db.take_salsa_events(),
    );
    assert_cleanup();
}

/// Nested tuple ordering must spill the real comparison visitor and publish the ordinary result.
/// Rust observations establish both growth branches rather than inferring them from tuple depth.
#[test]
fn cold_nested_comparison_spills_active_stack_and_completed_cache() {
    let expected = Type::bool_literal(ordinary_result());
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let _recording = Recording::start(None);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_spilled();
    assert_canonical(&db, &prepared, expected);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Budget {
    Work,
    Bytes,
}

impl Budget {
    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy { semantic_work_limit: limit, ..funded() },
            Self::Bytes => AnalysisPolicy { requested_bytes_limit: limit, ..funded() },
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Finds the first policy limit that accepts the selected real relocation admission.
/// Every sample uses a fresh database, so earlier attempts cannot supply canonical warm memos.
fn relocation_threshold(storage: Storage, budget: Budget) -> usize {
    let mut lower = 0;
    let mut upper = budget.limit();
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let _recording = Recording::start(None);
        match expression_type_with_policy(&prepared, expression_key(&prepared), &budget.policy(middle)) {
            Ok(AnalysisOutcome::Complete(ty)) => assert_eq!(ty, Type::bool_literal(true)),
            Ok(AnalysisOutcome::Incomplete { reason, completed: () }) => assert_eq!(reason, budget.reason()),
            other => panic!("{other:?}"),
        }
        if snapshot().relocations[storage.index()].accepted == 1 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_cleanup();
    }
    assert!(upper > 0);
    upper
}

/// Each budget independently refuses a real active/cache relocation while retaining its scope.
/// Rust controls check the exact admission and native cleanup; retry must still publish the
/// canonical ordinary result without changing the revision or increasing the funded limits.
#[test_case::test_case(Storage::Active, Budget::Work; "active work")]
#[test_case::test_case(Storage::Active, Budget::Bytes; "active bytes")]
#[test_case::test_case(Storage::Cache, Budget::Work; "cache work")]
#[test_case::test_case(Storage::Cache, Budget::Bytes; "cache bytes")]
fn relocation_refusal_drains_scopes_and_retries(storage: Storage, budget: Budget) {
    let expected = Type::bool_literal(ordinary_result());
    let threshold = relocation_threshold(storage, budget);
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    {
        let _recording = Recording::start(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &budget.policy(threshold - 1)),
            Ok(AnalysisOutcome::Incomplete { reason: budget.reason(), completed: () }),
        );
        let journal = snapshot();
        assert_eq!(journal.relocations[storage.index()].started, 1);
        assert_eq!(journal.relocations[storage.index()].accepted, 0);
        let cached = match storage {
            Storage::Active => {
                assert!(journal.states().any(|state| {
                    state.stage == Stage::BeforeVisit
                        && state.storage.active_len == 1
                        && state.storage.active_capacity == 1
                }), "{journal:?}");
                0
            }
            Storage::Cache => {
                assert!(journal.states().any(|state| {
                    state.stage == Stage::BeforeFinish
                        && state.storage.active_len == 1
                        && state.storage.cache_len == 2
                        && state.storage.cache_capacity.is_none()
                }), "{journal:?}");
                2
            }
        };
        assert_drained(1, cached);
    }
    {
        let _recording = Recording::start(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_spilled();
        assert_canonical(&db, &prepared, expected);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// Cancellation after real storage growth drains unfinished native scopes before a successful
/// same-revision retry. The cache case cancels after reservation but before the third result commits.
#[test_case::test_case(Storage::Active; "active stack")]
#[test_case::test_case(Storage::Cache; "completed cache")]
fn cancellation_after_spill_drains_scopes_and_retries(storage: Storage) {
    let expected = Type::bool_literal(ordinary_result());
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    {
        let _recording = Recording::start(Some(storage));
        let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        }));
        assert!(matches!(outcome, Err(salsa::Cancelled::Local)), "{outcome:?}");
        assert_eq!(CANCEL.get(), None);
        let journal = snapshot();
        assert_eq!(journal.relocations[storage.index()].accepted, 1);
        match storage {
            Storage::Active => assert_drained(2, 0),
            Storage::Cache => assert_drained(1, 2),
        }
    }
    {
        let _recording = Recording::start(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_spilled();
        assert_canonical(&db, &prepared, expected);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}
