//! Exercises the inheritance-cycle query after ordinary class-identity preparation.
//! Each controlled body test verifies that the requested cycle memo is still missing after setup.

mod graphs;
mod retained;

use std::cell::RefCell;

use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe::{Read, Status};

use super::super::inheritance_cycle::InheritanceCycleConfiguration;
use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::static_literal::{InheritanceCycle, inheritance_cycle_inner_ingredient};

#[derive(Clone, Copy)]
struct Request<'db>(StaticClassLiteral<'db>);

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = Option<InheritanceCycle>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.inheritance_cycle(self.0).await
    }
}

fn database(source: &str) -> TestDb {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.pyi", source)
        .build()
        .unwrap()
}

fn prepare(db: &TestDb) -> PreparedAnalysisFile<'_> {
    prepare_file(db, system_path_to_file(db, "src/main.pyi").unwrap()).unwrap()
}

fn class_in_file<'db>(
    db: &'db TestDb,
    file: ProgramFile<'db>,
    name: &str,
) -> StaticClassLiteral<'db> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let node = module
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap();
    let definition = semantic_index(db, file).expect_single_definition(node);
    let Some(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).original_class_type(definition)
    else {
        panic!("fixture definition is not a static class");
    };
    class
}

fn cold_class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> StaticClassLiteral<'db> {
    let class = class_in_file(db, prepared.program_file(), "Product");
    assert_missing(db, class);
    class
}

fn assert_missing(db: &TestDb, class: StaticClassLiteral<'_>) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            inheritance_cycle_inner_ingredient(db),
            class.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
        "requested inheritance-cycle query already has a completed memo",
    );
}

fn assert_read(reads: &[Read], stamp: Stamp, key: salsa::DatabaseKeyIndex) -> Read {
    let read = *reads.iter().find(|read| read.key == key).unwrap();
    assert_eq!(read.status, Status::Final);
    assert_eq!(read.stamp, stamp);
    read
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    class: StaticClassLiteral<'db>,
) -> Result<AnalysisOutcome<Option<InheritanceCycle>>, AnalysisFailure> {
    controlled_member_operation(prepared, Request(class), &funded())
}

#[derive(Clone, Default)]
struct Journal {
    class: Option<salsa::Id>,
    bodies: usize,
    native_quotes: usize,
    first_native_remaining: Option<usize>,
    traversals: usize,
    dropped_traversals: usize,
    live_traversals: usize,
    snapshots: Vec<TraversalSnapshot>,
    cancel_at: Option<RetainedPoint>,
}

#[derive(Clone, Copy, Debug)]
struct TraversalSnapshot {
    frames: usize,
    active: usize,
    visited: usize,
    frame_capacity: usize,
    active_slots: usize,
    active_ordered_capacity: usize,
    visited_slots: usize,
    visited_ordered_capacity: usize,
    after_enter: bool,
    remaining_work: usize,
}

#[derive(Clone, Copy, Debug)]
enum RetainedPoint {
    Frames,
    Revisited,
}

impl RetainedPoint {
    fn reached(self, snapshot: &TraversalSnapshot) -> bool {
        match self {
            Self::Frames => snapshot.frames >= 6 && snapshot.active >= 5 && snapshot.visited >= 5,
            Self::Revisited => {
                snapshot.after_enter && snapshot.active == 0 && snapshot.visited == 3
            }
        }
    }
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

struct Recording;

impl Recording {
    fn start(class: StaticClassLiteral<'_>) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal {
                class: Some(class.as_id()),
                ..Journal::default()
            });
        });
        Self
    }

    fn counts(&self) -> (usize, usize) {
        JOURNAL.with_borrow(|journal| {
            let journal = journal.as_ref().unwrap();
            (journal.bodies, journal.native_quotes)
        })
    }

    fn snapshot(&self) -> Journal {
        JOURNAL.with_borrow(|journal| journal.as_ref().unwrap().clone())
    }

    fn cancel_at(&self, point: RetainedPoint) {
        JOURNAL.with_borrow_mut(|journal| journal.as_mut().unwrap().cancel_at = Some(point));
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
}

pub(in crate::types::infer) fn observe_body(_db: &dyn Db, class: StaticClassLiteral<'_>) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            assert_eq!(journal.class, Some(class.as_id()));
            journal.bodies += 1;
        }
    });
}

pub(in crate::types::infer) fn observe_native(db: &dyn Db) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.native_quotes += 1;
            journal.first_native_remaining.get_or_insert_with(|| {
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap()
            });
        }
    });
}

pub(in crate::types::infer) fn observe_traversal_created() {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.traversals += 1;
            journal.live_traversals += 1;
        }
    });
}

pub(in crate::types::infer) fn observe_traversal_drop() {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.dropped_traversals += 1;
            journal.live_traversals -= 1;
        }
    });
}

pub(in crate::types::infer) fn observe_traversal(
    db: &dyn Db,
    frames: usize,
    active: usize,
    visited: usize,
    frame_capacity: usize,
    active_slots: usize,
    active_ordered_capacity: usize,
    visited_slots: usize,
    visited_ordered_capacity: usize,
    after_enter: bool,
) {
    let cancel = JOURNAL.with_borrow_mut(|journal| {
        let Some(journal) = journal else {
            return false;
        };
        let snapshot = TraversalSnapshot {
            frames,
            active,
            visited,
            frame_capacity,
            active_slots,
            active_ordered_capacity,
            visited_slots,
            visited_ordered_capacity,
            after_enter,
            remaining_work: salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
        };
        let cancel = journal
            .cancel_at
            .is_some_and(|point| point.reached(&snapshot));
        if cancel {
            journal.cancel_at = None;
        }
        journal.snapshots.push(snapshot);
        cancel
    });
    if cancel {
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
    }
}

/// Controlled cycle detection agrees with an independent ordinary database and reuses the direct key.
/// Only class identities are prepared ordinarily in the controlled database; the requested cycle
/// memo must be missing before its first controlled fetch.
#[test]
fn cold_cycles_preserve_classification_and_canonical_memos() {
    for (source, expected) in [
        ("class Product: ...\n", None),
        ("class Base: ...\nclass Product(Base): ...\n", None),
        (
            "class Product(Product): ...\n",
            Some(InheritanceCycle::Participant),
        ),
        (
            "class Product(Other): ...\nclass Other(Product): ...\n",
            Some(InheritanceCycle::Participant),
        ),
        (
            "class First(Second): ...\nclass Second(First): ...\nclass Product(First): ...\n",
            Some(InheritanceCycle::Inherited),
        ),
    ] {
        let ordinary_db = database(source);
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_class = cold_class(&ordinary_db, &ordinary_prepared);
        assert_eq!(
            ordinary_class.inheritance_cycle(&ordinary_db),
            expected,
            "{source}"
        );

        let db = database(source);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let key = inheritance_cycle_inner_ingredient(&db).database_key_index(class.as_id());
        let recording = Recording::start(class);
        let cold = capture(&db, || controlled(&prepared, class)).unwrap();
        cold.check_root_reads().unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(expected)),
            "{source}"
        );
        assert_eq!(recording.counts().0, 1);
        let first = assert_read(&cold.reads, cold.stamp, key);

        let hit = capture(&db, || controlled(&prepared, class)).unwrap();
        assert_eq!(hit.value, cold.value);
        assert_eq!(recording.counts().0, 1);
        assert_eq!(
            assert_read(&hit.reads, hit.stamp, key).memo_address,
            first.memo_address
        );
        assert_eq!(class.inheritance_cycle(&db), expected);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

fn assert_none_initial<'db, C: InheritanceCycleConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    class: StaticClassLiteral<'db>,
) {
    assert_eq!(C::cycle_initial(db, class.as_id(), class), None);
}

/// The canonical cycle initializer returns None without evaluating or publishing the query body.
#[test]
fn canonical_cycle_initialization_preserves_none() {
    let db = database("class Product: ...\n");
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    assert_none_initial(&db, inheritance_cycle_inner_ingredient(&db), class);
    assert_missing(&db, class);
}

/// Refusing the native quotation publishes no cycle result; the same class and revision can retry.
/// Work is measured before that quotation, and fresh databases locate the first byte budget that reaches it.
#[test]
fn native_refusal_retries_the_same_cold_class() {
    let measured = database("class Base: ...\nclass Product(Base): ...\n");
    let measured_prepared = prepare(&measured);
    let measured_class = cold_class(&measured, &measured_prepared);
    let recording = Recording::start(measured_class);
    assert_eq!(
        controlled(&measured_prepared, measured_class),
        Ok(AnalysisOutcome::Complete(None))
    );
    let work = funded().semantic_work_limit - recording.snapshot().first_native_remaining.unwrap();
    drop(recording);

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = database("class Base: ...\nclass Product(Base): ...\n");
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let recording = Recording::start(class);
        let result = controlled_member_operation(
            &prepared,
            Request(class),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(None))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{result:?}"
        );
        if recording.counts().1 > 0 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_no_active_attempt();
    }

    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: lower,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let db = database("class Base: ...\nclass Product(Base): ...\n");
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(class);
        assert_eq!(
            controlled_member_operation(&prepared, Request(class), &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            }),
        );
        assert_eq!(recording.counts(), (0, 1));
        assert_missing(&db, class);
        assert_no_active_attempt();

        let retry = capture(&db, || controlled(&prepared, class)).unwrap();
        assert_eq!(retry.value, Ok(AnalysisOutcome::Complete(None)));
        assert_eq!(recording.counts().0, 1);
        let key = inheritance_cycle_inner_ingredient(&db).database_key_index(class.as_id());
        assert_read(&retry.reads, retry.stamp, key);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// A class from another program is rejected before either a cold fetch or a cached result is read.
#[test]
fn foreign_program_is_rejected_before_cached_cycle_results() {
    let db = database("class Base: ...\nclass Product(Base): ...\n");
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
    let file = ProgramFile::new(&db, prepared.program_file().file(&db), foreign);
    let class = class_in_file(&db, file, "Product");
    assert_missing(&db, class);
    let key = inheritance_cycle_inner_ingredient(&db).database_key_index(class.as_id());
    let recording = Recording::start(class);
    for cached in [false, true] {
        if cached {
            assert_eq!(class.inheritance_cycle(&db), None);
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    inheritance_cycle_inner_ingredient(&db),
                    class.as_id(),
                )
                .is_ok()
            );
        }
        let rejected = capture(&db, || controlled(&prepared, class)).unwrap();
        assert_eq!(
            rejected.value,
            Err(AnalysisFailure::Execution(RunError::Contract(
                "source program is foreign"
            ))),
        );
        assert!(rejected.reads.iter().all(|read| read.key != key));
        assert_eq!(recording.counts(), (0, 0));
        assert_no_active_attempt();
    }
}
