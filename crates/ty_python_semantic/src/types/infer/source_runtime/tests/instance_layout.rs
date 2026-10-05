use std::cell::RefCell;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, TaskEndpoint};
use ty_python_core::definition::Definition;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::interpret_class_literal_lookup;
use crate::types::class::member_source::InlineMemberSourceEffects;
use crate::types::class::slots::{
    InstanceLayout, SynchronousSlotSelectorEffects, instance_dictionary_sync,
    instance_layout_ingredient,
};

#[derive(Clone, Copy)]
enum Request<'db> {
    Definition(Definition<'db>),
    Known(KnownClass),
}

impl<'db> Request<'db> {
    fn for_fixture(prepared: &PreparedAnalysisFile<'db>, known: Option<KnownClass>) -> Self {
        if let Some(known) = known {
            return Self::Known(known);
        }
        let class = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .find_map(Stmt::as_class_def_stmt)
            .unwrap();
        Self::Definition(prepared.semantic_index().expect_single_definition(class))
    }

    fn ordinary_class(self, db: &'db TestDb, program: Program<'db>) -> StaticClassLiteral<'db> {
        match self {
            Self::Definition(definition) => {
                let Some(ClassLiteral::Static(class)) =
                    infer_definition_types(db, definition).original_class_type(definition)
                else {
                    panic!("fixture definition is not a static class");
                };
                class
            }
            Self::Known(known) => known
                .try_to_class_literal(db, &ProgramEnvironment::from_program(program))
                .unwrap(),
        }
    }
}

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = (StaticClassLiteral<'db>, &'db InstanceLayout);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let endpoint = access.endpoint();
        let class = match self {
            Self::Definition(definition) => {
                let inference = access.definition(definition).await?;
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        let Some(ClassLiteral::Static(class)) =
                            inference.original_class_type(definition)
                        else {
                            return Err(RunError::Contract(
                                "fixture definition is not a static class",
                            ));
                        };
                        Ok(class)
                    })
                    .await
            }
            Self::Known(known) => {
                let lookup = access.known_class_lookup(program, known).await?;
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        interpret_class_literal_lookup(lookup)
                            .ok_or(RunError::Contract("fixture known class is unavailable"))
                    })
                    .await
            }
        };
        if RECORDING.get() {
            JOURNAL.with_borrow_mut(|journal| journal.key = Some(class.as_id()));
        }
        Ok((class, access.instance_layout(class).await?))
    }
}

fn fixture(slots: bool) -> TestDb {
    TestDbBuilder::new()
        .with_file(
            "src/main.py",
            if slots {
                "class Product:\n    __slots__ = ('value',)\n"
            } else {
                "class Product:\n    pass\n"
            },
        )
        .build()
        .unwrap()
}

fn ordinary_layout<'db>(db: &'db TestDb, class: StaticClassLiteral<'db>) -> &'db InstanceLayout {
    let Ok(layout) =
        SynchronousSlotSelectorEffects::instance_layout(&InlineMemberSourceEffects::new(db), class);
    layout
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    OwnerCreated,
    MroBase,
    ChildCreated,
    ChildDropped,
    OwnerDropped,
}

#[derive(Clone, Debug, Default)]
struct Journal {
    key: Option<salsa::Id>,
    body_entries: usize,
    owners: usize,
    live_owners: usize,
    children: usize,
    live_children: usize,
    yielded_bases: usize,
    first_base_remaining: Option<usize>,
    first_child_remaining: Option<usize>,
    child_owner_counts: Vec<usize>,
    owner_child_counts: Vec<usize>,
    owner_builder_counts: Vec<usize>,
    events: Vec<Event>,
}

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static JOURNAL: RefCell<Journal> = RefCell::new(Journal::default());
}

struct Recording;

impl Recording {
    fn start() -> Self {
        assert!(!RECORDING.replace(true));
        JOURNAL.with_borrow_mut(|journal| *journal = Journal::default());
        observations::reset(None);
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
    }
}

pub(in crate::types::infer) struct LayoutLifetime {
    recorded: bool,
}

impl LayoutLifetime {
    pub(in crate::types::infer) fn new() -> Self {
        let recorded = RECORDING.get();
        if recorded {
            JOURNAL.with_borrow_mut(|journal| {
                journal.owners += 1;
                journal.live_owners += 1;
                journal.events.push(Event::OwnerCreated);
            });
        }
        Self { recorded }
    }
}

impl Drop for LayoutLifetime {
    fn drop(&mut self) {
        if self.recorded {
            JOURNAL.with_borrow_mut(|journal| {
                journal.owner_child_counts.push(journal.live_children);
                journal.owner_builder_counts.push(observations::counts().0);
                journal.live_owners -= 1;
                journal.events.push(Event::OwnerDropped);
            });
        }
    }
}

pub(in crate::types::infer) fn observe_body(_db: &dyn Db, class: StaticClassLiteral<'_>) {
    if RECORDING.get() {
        JOURNAL.with_borrow_mut(|journal| {
            if journal.key == Some(class.as_id()) {
                journal.body_entries += 1;
            }
        });
    }
}

pub(in crate::types::infer) fn observe_mro_advance(
    db: &dyn Db,
    _endpoint: &TaskEndpoint<'_, '_>,
    has_base: bool,
) -> RunResult<()> {
    if RECORDING.get() && has_base {
        JOURNAL.with_borrow_mut(|journal| {
            journal.yielded_bases += 1;
            journal.events.push(Event::MroBase);
            if journal.first_base_remaining.is_none() {
                journal.first_base_remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            }
        });
    }
    Ok(())
}

pub(in crate::types::infer) struct MroChildLifetime {
    recorded: bool,
}

impl Drop for MroChildLifetime {
    fn drop(&mut self) {
        if self.recorded {
            JOURNAL.with_borrow_mut(|journal| {
                journal.child_owner_counts.push(journal.live_owners);
                journal.live_children -= 1;
                journal.events.push(Event::ChildDropped);
            });
        }
    }
}

pub(in crate::types::infer) fn observe_mro_child(db: &dyn Db) -> MroChildLifetime {
    let recorded = RECORDING.get()
        && JOURNAL.with_borrow(|journal| journal.live_owners > 0 && journal.yielded_bases > 0);
    if recorded {
        JOURNAL.with_borrow_mut(|journal| {
            journal.children += 1;
            journal.live_children += 1;
            journal.events.push(Event::ChildCreated);
            if journal.first_child_remaining.is_none() {
                journal.first_child_remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            }
        });
    }
    MroChildLifetime { recorded }
}

fn journal() -> Journal {
    JOURNAL.with_borrow(Clone::clone)
}

fn assert_cleanup() {
    let journal = journal();
    assert_eq!(journal.live_owners, 0, "{journal:?}");
    assert_eq!(journal.live_children, 0, "{journal:?}");
    assert_eq!(journal.owner_child_counts, vec![0; journal.owners]);
    assert_eq!(journal.owner_builder_counts, vec![0; journal.owners]);
    assert_eq!(journal.child_owner_counts, vec![1; journal.children]);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Cold layout requests for `type`, `object`, and a plain class match ordinary inference on a
/// separate database. Both ordinary and controlled reads then reuse the canonical layout memo.
#[test]
fn cold_layouts_match_ordinary_inference_and_reuse_the_canonical_memos() {
    for known in [Some(KnownClass::Type), Some(KnownClass::Object), None] {
        let db = fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let request = Request::for_fixture(&prepared, known);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let recording = Recording::start();
        let cold = capture(&db, || {
            controlled_member_operation(&prepared, request, &funded())
        })
        .unwrap();
        drop(recording);
        cold.check_root_reads().unwrap();
        let Ok(AnalysisOutcome::Complete((class, layout))) = cold.value else {
            panic!("{known:?}: {:?}", cold.value);
        };
        assert!(layout.slot_names().is_empty());
        assert_eq!(journal().body_entries, 1);
        assert_eq!(journal().owners, 1);
        assert!(journal().yielded_bases > 0);
        assert_cleanup();
        let ingredient = instance_layout_ingredient(&db);
        let key = ingredient.database_key_index(class.as_id());
        let query_name = db.ingredient_debug_name(key.ingredient_index());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, class.as_id()).is_ok());
        let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
        assert!(
            find_will_execute_event_by_name(
                &db,
                &query_name,
                Some(class.as_id()),
                &events_db.take_salsa_events(),
            )
            .is_some()
        );

        let ordinary_db = fixture(false);
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_class = Request::for_fixture(&ordinary_prepared, known).ordinary_class(
            &ordinary_db,
            ordinary_prepared.program_file().program(&ordinary_db),
        );
        assert_eq!(ordinary_layout(&ordinary_db, ordinary_class), layout);
        let Ok(dictionary) = instance_dictionary_sync(
            ordinary_class,
            &InlineMemberSourceEffects::new(&ordinary_db),
        );
        assert_eq!(dictionary, known != Some(KnownClass::Object));

        let native = capture(&db, || ordinary_layout(&db, class)).unwrap();
        assert!(std::ptr::eq(native.value, layout));
        assert!(native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        }));
        let recording = Recording::start();
        let warm = capture(&db, || {
            controlled_member_operation(&prepared, request, &funded())
        })
        .unwrap();
        drop(recording);
        warm.check_root_reads().unwrap();
        assert_eq!(warm.value, cold.value);
        assert_eq!(journal().body_entries, 0);
        assert_eq!(journal().owners, 0);
        assert!(warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        }));
        assert_function_query_was_not_run_by_name(
            &db,
            &query_name,
            Some(class.as_id()),
            &events_db.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// The controlled runtime does not yet produce slot definitions. A declared slot reaches that
/// unavailable child on every attempt because refusal leaves its layout unpublished.
#[test]
fn explicit_slots_refuse_at_the_slot_definition_dependency() {
    let db = fixture(true);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let request = Request::for_fixture(&prepared, None);
    for _ in 0..2 {
        let recording = Recording::start();
        assert_eq!(
            controlled_member_operation(&prepared, request, &funded()),
            Ok(unavailable(OperationId::ClassCheck(
                ClassCheckOperation::SlotDefinition
            ))),
        );
        drop(recording);
        let observed = journal();
        assert_eq!(observed.body_entries, 1);
        assert_eq!(observed.owners, 1);
        assert_eq!(observed.yielded_bases, 1);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                instance_layout_ingredient(&db),
                observed.key.unwrap(),
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// The layout owns its ordered slot-name collection while its MRO cursor yields the class itself
/// and then requests the canonical MRO query for inherited entries. Exhaustion at either point
/// retires that collection without publishing a layout. The MRO child drops before the collection
/// owner, and retry completes in the same revision with the original funded policy.
#[test]
fn interrupted_layouts_drain_children_before_the_owner_and_retry_in_the_same_revision() {
    let measured = fixture(false);
    let measured_prepared = prepare(&measured);
    let recording = Recording::start();
    assert!(matches!(
        controlled_member_operation(
            &measured_prepared,
            Request::for_fixture(&measured_prepared, None),
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete(_)),
    ));
    drop(recording);
    let measured_journal = journal();
    assert_cleanup();
    let first_base_work =
        funded().semantic_work_limit - measured_journal.first_base_remaining.unwrap();
    let child_work = funded().semantic_work_limit - measured_journal.first_child_remaining.unwrap();
    assert!(child_work > first_base_work);

    for (limit, in_child) in [(first_base_work, false), (child_work, true)] {
        let db = fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let request = Request::for_fixture(&prepared, None);
        let recording = Recording::start();
        assert_eq!(
            controlled_member_operation(
                &prepared,
                request,
                &AnalysisPolicy {
                    semantic_work_limit: limit,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        drop(recording);
        let interrupted = journal();
        assert_eq!(interrupted.body_entries, 1);
        assert_eq!(interrupted.owners, 1);
        assert_eq!(interrupted.yielded_bases, 1);
        assert_eq!(interrupted.children, usize::from(in_child));
        assert_eq!(
            if in_child {
                interrupted.first_child_remaining
            } else {
                interrupted.first_base_remaining
            },
            Some(0),
        );
        let expected_events = if in_child {
            vec![
                Event::OwnerCreated,
                Event::MroBase,
                Event::ChildCreated,
                Event::ChildDropped,
                Event::OwnerDropped,
            ]
        } else {
            vec![Event::OwnerCreated, Event::MroBase, Event::OwnerDropped]
        };
        assert_eq!(interrupted.events, expected_events);
        let key = interrupted.key.unwrap();
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, instance_layout_ingredient(&db), key)
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        assert_cleanup();

        let recording = Recording::start();
        let retry = capture(&db, || {
            controlled_member_operation(&prepared, request, &funded())
        })
        .unwrap();
        drop(recording);
        retry.check_root_reads().unwrap();
        let Ok(AnalysisOutcome::Complete((class, layout))) = retry.value else {
            panic!("layout retry: {:?}", retry.value);
        };
        assert_eq!(class.as_id(), key);
        assert_eq!(journal().body_entries, 1);
        assert_eq!(journal().yielded_bases, measured_journal.yielded_bases);
        assert!(layout.slot_names().is_empty());
        assert_eq!(
            ordinary_layout(
                &measured,
                Request::for_fixture(&measured_prepared, None).ordinary_class(
                    &measured,
                    measured_prepared.program_file().program(&measured),
                )
            ),
            layout
        );
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, instance_layout_ingredient(&db), key).is_ok()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}
