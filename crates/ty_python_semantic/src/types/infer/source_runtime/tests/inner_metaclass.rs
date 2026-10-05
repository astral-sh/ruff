//! Exercises the canonical inner metaclass query after ordinary class-identity preparation.
//! Each controlled body test checks that preparation leaves the requested metaclass memo absent.

mod native;
pub(in crate::types::infer) mod reconciliation;

use std::cell::RefCell;
use std::panic::AssertUnwindSafe;

use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe::{Read, Status};

use super::super::inner_metaclass::InnerMetaclassConfiguration;
use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::metaclass_selection::MetaclassSelectionResult;
use crate::types::class::static_literal::try_metaclass_inner_ingredient;
use crate::types::class::{ClassMetaclass, MetaclassErrorKind};
use crate::types::{KnownClass, StaticClassLiteral};

#[derive(Clone, Copy, Debug)]
struct Request<'db>(StaticClassLiteral<'db>);

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = MetaclassSelectionResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.inner_metaclass(self.0).await
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
            try_metaclass_inner_ingredient(db),
            class.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
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
) -> Result<AnalysisOutcome<MetaclassSelectionResult<'db>>, AnalysisFailure> {
    controlled_member_operation(prepared, Request(class), &funded())
}

#[derive(Clone, Debug, Default)]
struct Journal {
    class: Option<salsa::Id>,
    bodies: usize,
    native_quotes: usize,
    first_native_remaining: Option<usize>,
    bases_created: usize,
    bases_dropped: usize,
    live_bases: usize,
    retained_remaining: Option<usize>,
    cancel: bool,
    cancelled: bool,
}

thread_local! { static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) }; }

#[derive(Debug)]
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

    fn snapshot(&self) -> Journal {
        JOURNAL.with_borrow(|journal| journal.as_ref().unwrap().clone())
    }

    fn cancel(&self) {
        JOURNAL.with_borrow_mut(|journal| journal.as_mut().unwrap().cancel = true);
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
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

pub(in crate::types::infer) fn observe_body(_db: &dyn Db, class: StaticClassLiteral<'_>) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.class == Some(class.as_id())
        {
            journal.bodies += 1;
        }
    });
}

pub(in crate::types::infer) fn observe_bases_created(
    _db: &dyn Db,
    class: StaticClassLiteral<'_>,
) -> Option<fn()> {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.class == Some(class.as_id())
        {
            journal.bases_created += 1;
            journal.live_bases += 1;
            Some(observe_bases_dropped as fn())
        } else {
            None
        }
    })
}

pub(in crate::types::infer) fn observe_bases_ready(db: &dyn Db, class: StaticClassLiteral<'_>) {
    let cancel = JOURNAL.with_borrow_mut(|journal| {
        let Some(journal) = journal else {
            return false;
        };
        if journal.class != Some(class.as_id()) {
            return false;
        }
        journal.retained_remaining.get_or_insert_with(|| {
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

fn observe_bases_dropped() {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            journal.bases_dropped += 1;
            journal.live_bases -= 1;
        }
    });
}

fn assert_drained(journal: &Journal) {
    assert_eq!(journal.live_bases, 0);
    assert_eq!(journal.bases_created, journal.bases_dropped);
}

fn assert_type_metaclass<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    result: &MetaclassSelectionResult<'db>,
) {
    assert_eq!(
        *result,
        Ok((
            ClassMetaclass::Selected(
                KnownClass::Type
                    .to_class_literal(db, &ProgramEnvironment::from_file(prepared.program_file()))
            ),
            None
        ))
    );
}

/// Ordinary inheritance and explicit object bases publish the same canonical metaclass used by later reads.
/// Class identities are prepared ordinarily, while the requested inner metaclass memo remains cold.
#[test]
fn cold_inner_metaclasses_publish_and_reuse_the_direct_key() {
    for source in [
        "class Product(object): ...\n",
        "class Base: ...\nclass Product(Base): ...\n",
        "class Left: ...\nclass Right: ...\nclass Product(Left, Right): ...\n",
    ] {
        let ordinary = database(source);
        let ordinary_prepared = prepare(&ordinary);
        let ordinary_class = cold_class(&ordinary, &ordinary_prepared);
        assert_type_metaclass(
            &ordinary,
            &ordinary_prepared,
            &ordinary_class.try_metaclass(&ordinary),
        );
        let db = database(source);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let recording = Recording::start(class);
        let key = try_metaclass_inner_ingredient(&db).database_key_index(class.as_id());
        let cold = capture(&db, || controlled(&prepared, class)).unwrap();
        cold.check_root_reads().unwrap();
        let Ok(AnalysisOutcome::Complete(result)) = &cold.value else {
            panic!("cold metaclass: {:?}", cold.value);
        };
        assert_type_metaclass(&db, &prepared, result);
        assert_eq!(recording.snapshot().bodies, 1);
        let first = assert_read(&cold.reads, cold.stamp, key);
        let hit = capture(&db, || controlled(&prepared, class)).unwrap();
        assert_eq!(hit.value, cold.value);
        assert_eq!(
            assert_read(&hit.reads, hit.stamp, key).memo_address,
            first.memo_address
        );
        assert_eq!(class.try_metaclass(&db), *result);
        assert_eq!(recording.snapshot().bodies, 1);
        assert_drained(&recording.snapshot());
        assert_no_active_attempt();
    }
}

fn assert_cycle_initial<'db, C: InnerMetaclassConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    class: StaticClassLiteral<'db>,
) {
    assert!(
        matches!(C::cycle_initial(db, class.as_id(), class), Err(error) if matches!(error.reason(), MetaclassErrorKind::Cycle))
    );
}

/// The generated metaclass initializer retains its Cycle error without publishing a body result.
#[test]
fn inner_metaclass_cycle_initialization_preserves_the_error() {
    let db = database("class Product: ...\n");
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    assert_cycle_initial(&db, try_metaclass_inner_ingredient(&db), class);
    assert_missing(&db, class);
}

/// Metaclass transform metadata follows the selected metaclass's MRO and records who supplied the candidate.
/// An explicit subclass of the transformer inherits its parameters; an ordinary class base supplies its own candidate.
#[test]
fn inner_metaclass_preserves_inherited_transform_parameters() {
    for (tail, explicit) in [
        (
            "class Derived(Meta): ...\nclass Product(metaclass=Derived): ...\n",
            true,
        ),
        (
            "class Base(metaclass=Meta): ...\nclass Product(Base): ...\n",
            false,
        ),
    ] {
        let source = format!(
            "from typing import dataclass_transform\n@dataclass_transform()\nclass Meta(type): ...\n{tail}"
        );
        let db = database(&source);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let recording = Recording::start(class);
        let result = controlled(&prepared, class);
        let Ok(AnalysisOutcome::Complete(Ok((_, Some(info))))) = result else {
            panic!("metaclass transform inference: {result:?}");
        };
        let meta = class_in_file(&db, prepared.program_file(), "Meta");
        assert_eq!(Some(info.params), meta.dataclass_transformer_params(&db));
        assert_eq!(info.from_explicit_metaclass, explicit);
        assert_drained(&recording.snapshot());
        assert_no_active_attempt();
    }
}

/// Work refusal drains the retained base cursor; byte refusal at native entry leaves the requested
/// inner-metaclass memo unpublished.
/// Both attempts retry successfully with the same class and revision under the normal limits.
#[test]
fn inner_metaclass_refusal_drains_and_retries() {
    let source = "class Base: ...\nclass Product(Base): ...\n";
    let measured = database(source);
    let prepared = prepare(&measured);
    let class = cold_class(&measured, &prepared);
    let recording = Recording::start(class);
    assert!(matches!(
        controlled(&prepared, class),
        Ok(AnalysisOutcome::Complete(Ok(_)))
    ));
    let work = funded().semantic_work_limit - recording.snapshot().retained_remaining.unwrap();
    drop(recording);
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = database(source);
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
                Ok(AnalysisOutcome::Complete(_))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{result:?}"
        );
        if recording.snapshot().native_quotes > 0 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_drained(&recording.snapshot());
        assert_no_active_attempt();
    }
    for (policy, reason, retained) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
            true,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: lower,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
            false,
        ),
    ] {
        let db = database(source);
        let prepared = prepare(&db);
        let class = cold_class(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(class);
        assert_eq!(
            controlled_member_operation(&prepared, Request(class), &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert_missing(&db, class);
        let journal = recording.snapshot();
        assert_eq!(journal.bases_created > 0, retained);
        assert!(journal.native_quotes > 0);
        assert_drained(&journal);
        assert_no_active_attempt();
        let Ok(AnalysisOutcome::Complete(result)) = controlled(&prepared, class) else {
            panic!("metaclass retry did not complete");
        };
        assert_type_metaclass(&db, &prepared, &result);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_drained(&recording.snapshot());
        assert_no_active_attempt();
    }
}

/// Cancellation after base retention drains the cursor and permits a same-revision retry.
/// The retry reuses a completed canonical memo or runs the interrupted query again.
#[test]
fn cancelled_inner_metaclass_drains_and_retries() {
    let db = database("class Base: ...\nclass Product(Base): ...\n");
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(class);
    recording.cancel();
    assert!(matches!(
        salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, class))),
        Err(salsa::Cancelled::Local)
    ));
    let journal = recording.snapshot();
    assert!(journal.cancelled);
    assert_drained(&journal);
    let completed = FinalSourceMemo::certify(
        &db as &dyn Db,
        try_metaclass_inner_ingredient(&db),
        class.as_id(),
    )
    .is_ok();
    assert_no_active_attempt();
    drop(recording);
    let recording = Recording::start(class);
    let Ok(AnalysisOutcome::Complete(result)) = controlled(&prepared, class) else {
        panic!("cancelled metaclass retry did not complete");
    };
    assert_type_metaclass(&db, &prepared, &result);
    assert_eq!(recording.snapshot().bodies, usize::from(!completed));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            try_metaclass_inner_ingredient(&db),
            class.as_id(),
        )
        .is_ok()
    );
    let Ok(AnalysisOutcome::Complete(warm)) = controlled(&prepared, class) else {
        panic!("completed metaclass retry was not reusable");
    };
    assert_eq!(warm, result);
    assert_eq!(recording.snapshot().bodies, usize::from(!completed));
    assert_drained(&recording.snapshot());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// A direct Protocol base in a user stub selects `_ProtocolMeta`, matching ordinary inference.
/// Cancellation after retaining the bases drains the cursor and leaves the requested memo cold.
/// A same-revision retry publishes the canonical result, which controlled and ordinary reads reuse.
#[test]
fn protocol_inner_metaclass_publishes_after_cancellation_and_reuses_the_direct_key() {
    let source = "from typing import Protocol\nclass Product(Protocol): ...\n";
    let ordinary = database(source);
    let ordinary_prepared = prepare(&ordinary);
    let ordinary_class = cold_class(&ordinary, &ordinary_prepared);
    assert_eq!(
        ordinary_class.try_metaclass(&ordinary),
        Ok((
            ClassMetaclass::Selected(KnownClass::ProtocolMeta.to_class_literal(
                &ordinary,
                &ProgramEnvironment::from_file(ordinary_prepared.program_file()),
            )),
            None,
        )),
    );

    let db = database(source);
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let key = try_metaclass_inner_ingredient(&db).database_key_index(class.as_id());
    let recording = Recording::start(class);
    recording.cancel();
    let cancelled = capture(&db, || {
        salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, class)))
    })
    .unwrap();
    assert!(matches!(cancelled.value, Err(salsa::Cancelled::Local)));
    assert!(cancelled.belongs_to(&db));
    assert!(
        cancelled
            .reads
            .iter()
            .all(|read| read.key != key || read.status != Status::Final)
    );
    let journal = recording.snapshot();
    assert!(journal.cancelled);
    assert_eq!(journal.bodies, 1);
    assert!(journal.bases_created > 0);
    assert_drained(&journal);
    assert_missing(&db, class);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    drop(recording);

    let recording = Recording::start(class);
    let retry = capture(&db, || controlled(&prepared, class)).unwrap();
    retry.check_root_reads().unwrap();
    assert!(retry.belongs_to(&db));
    assert_eq!(retry.stamp, cancelled.stamp);
    let Ok(AnalysisOutcome::Complete(result)) = &retry.value else {
        panic!("Protocol metaclass retry did not complete: {:?}", retry.value);
    };
    assert_eq!(
        *result,
        Ok((
            ClassMetaclass::Selected(KnownClass::ProtocolMeta.to_class_literal(
                &db,
                &ProgramEnvironment::from_file(prepared.program_file()),
            )),
            None,
        )),
    );
    let first = assert_read(&retry.reads, retry.stamp, key);
    assert_eq!(first.parent, None);
    assert_ne!(first.memo_address, 0);
    assert_eq!(recording.snapshot().bodies, 1);
    assert!(recording.snapshot().bases_created > 0);
    assert_drained(&recording.snapshot());
    assert_no_active_attempt();

    let hit = capture(&db, || controlled(&prepared, class)).unwrap();
    hit.check_root_reads().unwrap();
    assert_eq!(hit.stamp, retry.stamp);
    assert_eq!(hit.value, retry.value);
    let hit_read = assert_read(&hit.reads, hit.stamp, key);
    assert_eq!(hit_read.parent, None);
    assert_eq!(hit_read.memo_address, first.memo_address);
    let ordinary_read = capture(&db, || class.try_metaclass(&db)).unwrap();
    ordinary_read.check_root_reads().unwrap();
    assert_eq!(ordinary_read.stamp, retry.stamp);
    assert_eq!(ordinary_read.value, *result);
    let ordinary_memo = assert_read(&ordinary_read.reads, ordinary_read.stamp, key);
    assert_eq!(ordinary_memo.parent, None);
    assert_eq!(ordinary_memo.memo_address, first.memo_address);
    assert_eq!(recording.snapshot().bodies, 1);
    assert_drained(&recording.snapshot());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Foreign-program classes are rejected before the canonical query is read, even when its memo is cached.
#[test]
fn foreign_inner_metaclass_is_rejected_before_cached_reads() {
    let db = database("class Product(object): ...\n");
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
    let key = try_metaclass_inner_ingredient(&db).database_key_index(class.as_id());
    for cached in [false, true] {
        if cached {
            class.try_metaclass(&db).unwrap();
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    try_metaclass_inner_ingredient(&db),
                    class.as_id()
                )
                .is_ok()
            );
        }
        let recording = Recording::start(class);
        let rejected = capture(&db, || controlled(&prepared, class)).unwrap();
        assert_eq!(
            rejected.value,
            Err(AnalysisFailure::Execution(RunError::Contract(
                "source program is foreign"
            )))
        );
        assert!(rejected.reads.iter().all(|read| read.key != key));
        assert_eq!(recording.snapshot().native_quotes, 0);
        assert_no_active_attempt();
    }
}
