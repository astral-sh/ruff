use std::cell::{Cell, RefCell};
use std::sync::Arc;

use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::DbWithWritableSystem;
use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
use salsa::execution_probe::{
    BorrowOrCopy, ExecutableRouteProvider, ExecutionAdmission, ExecutionWork, FinalSourceError,
    FinalSourceMemo, FinalSourceRoute, NativeValueOperation, NativeValueQuote, ProviderContext,
    RegistryBuilder, RetainedInput, RunError, RunResult,
};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::prepared_source_probe;
use salsa::{Cycle, DatabaseKeyIndex, Id};

use crate::db::tests::{TestDb, TestDbBuilder};
use crate::definition::{Definition, DefinitionKind, DefinitionState};
use crate::scope::ScopeId;
use crate::{
    Db, PlaceTable, ProgramFile, TestProgramDb, UseDefMap, global_scope, place_table,
    semantic_index, use_def_map,
};

const PATH: &str = "/src/source.py";
// Quoting keeps annotation names out of PlaceTable; equal widths preserve map ranges.
const INITIAL: &str = "x: \"int\"\n";
const ANNOTATION_EDIT: &str = "x: \"str\"\n";
const RENAME: &str = "y: \"str\"\n";

thread_local! {
    static CONTROLLED_BODIES: Cell<[usize; 2]> = const { Cell::new([0, 0]) };
    static ORDINARY_BODIES: Cell<[usize; 2]> = const { Cell::new([0, 0]) };
    static FIELD_READS: RefCell<Vec<Id>> = const { RefCell::new(Vec::new()) };
    static COMPARISONS: RefCell<Vec<MapComparison>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy)]
enum ConsumerKind {
    Maps = 0,
    Field = 1,
}

fn count(counter: &Cell<[usize; 2]>, kind: ConsumerKind) {
    let mut counts = counter.get();
    counts[kind as usize] += 1;
    counter.set(counts);
}

fn definition_of_x<'db>(places: &PlaceTable, uses: &UseDefMap<'db>) -> Option<Definition<'db>> {
    let symbol = places.symbol_id("x")?;
    uses.end_of_scope_symbol_declarations(symbol)
        .find_map(|declaration| match declaration.declaration {
            DefinitionState::Defined(definition) => Some(definition),
            DefinitionState::Undefined | DefinitionState::Deleted => None,
        })
}

fn is_annotated(db: &dyn Db, definition: Definition<'_>) -> bool {
    matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(_))
}

#[salsa::tracked(attempt = ReturnOnly, returns(copy))]
fn map_consumer<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> bool {
    ORDINARY_BODIES.with(|counter| count(counter, ConsumerKind::Maps));
    definition_of_x(place_table(db, scope), use_def_map(db, scope)).is_some()
}

#[salsa::tracked(attempt = ReturnOnly, returns(copy))]
fn field_consumer<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> bool {
    ORDINARY_BODIES.with(|counter| count(counter, ConsumerKind::Field));
    definition_of_x(place_table(db, scope), use_def_map(db, scope))
        .is_some_and(|definition| is_annotated(db, definition))
}

#[derive(Debug, Eq, PartialEq)]
struct MapComparison {
    places_equal: bool,
    uses_equal: bool,
    old_definition: Option<Id>,
    new_definition: Option<Id>,
}

#[derive(Debug, salsa::SalsaValue)]
struct MapSnapshot<'db> {
    places: Arc<PlaceTable>,
    uses: Arc<UseDefMap<'db>>,
    definition: Option<Id>,
}

impl PartialEq for MapSnapshot<'_> {
    fn eq(&self, other: &Self) -> bool {
        let comparison = MapComparison {
            places_equal: self.places == other.places,
            uses_equal: self.uses == other.uses,
            old_definition: self.definition,
            new_definition: other.definition,
        };
        let equal = comparison.places_equal
            && comparison.uses_equal
            && comparison.old_definition == comparison.new_definition;
        COMPARISONS.with_borrow_mut(|comparisons| comparisons.push(comparison));
        equal
    }
}

impl Eq for MapSnapshot<'_> {}

// Salsa owns both lifetime-bearing generations while comparing their complete map values.
#[salsa::tracked(attempt = CompleteOnly, returns(ref))]
fn map_snapshot<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> MapSnapshot<'db> {
    let index = semantic_index(db, scope.program_file(db));
    let places = Arc::clone(&index.place_tables[scope.file_scope_id(db)]);
    let uses = Arc::clone(&index.use_def_maps[scope.file_scope_id(db)]);
    let definition = definition_of_x(&places, &uses).map(|definition| definition.as_id());
    MapSnapshot {
        places,
        uses,
        definition,
    }
}

fn fixture() -> (TestDb, File) {
    let db = TestDbBuilder::new()
        .with_file(PATH, INITIAL)
        .build()
        .unwrap();
    let file = system_path_to_file(&db, PATH).unwrap();
    (db, file)
}

fn prepare<'db>(db: &'db TestDb, file: File, name: &str) -> ScopeId<'db> {
    let program_file = ProgramFile::new(db, file, db.program());
    let scope = global_scope(db, program_file);
    let places = place_table(db, scope);
    let uses = use_def_map(db, scope);
    assert_eq!(
        places
            .symbols()
            .map(|symbol| symbol.name().as_str())
            .collect::<Vec<_>>(),
        [name]
    );
    let snapshot = map_snapshot(db, scope);
    assert!(std::ptr::eq(places, snapshot.places.as_ref()));
    assert!(std::ptr::eq(uses, snapshot.uses.as_ref()));
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            semantic_index::fn_ingredient_(db, db.zalsa()),
            program_file.as_id(),
        ),
        Err(FinalSourceError::OutputBearingMemo)
    ));
    scope
}

fn ordinary(db: &TestDb, file: File) -> [bool; 2] {
    let scope = global_scope(db, ProgramFile::new(db, file, db.program()));
    [map_consumer(db, scope), field_consumer(db, scope)]
}

struct Consumer<'db, P: Configuration, U: Configuration> {
    places: FinalSourceRoute<'db, P>,
    uses: FinalSourceRoute<'db, U>,
    kind: ConsumerKind,
}

impl<'run, 'db: 'run, P, U, C> ExecutableRouteProvider<'run, 'db, C> for Consumer<'db, P, U>
where
    P: Configuration<DbView = dyn Db, Output<'db> = Arc<PlaceTable>>,
    U: Configuration<DbView = dyn Db, Output<'db> = Arc<UseDefMap<'db>>>,
    C: Configuration<DbView = dyn Db, Input<'db> = ScopeId<'db>, Output<'db> = bool>,
{
    async fn native_value<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // ScopeId's generated conversion wraps its Id without cloning stored fields.
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => 1,
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                return Err(RunError::Contract("scope input requires handle conversion"));
            }
            NativeValueOperation::Comparison { .. } => 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> RunResult<bool> {
        CONTROLLED_BODIES.with(|counter| count(counter, self.kind));
        let endpoint = context.endpoint();
        let places = endpoint
            .read_final_source(&self.places, scope.as_id())
            .await;
        let uses = endpoint.read_final_source(&self.uses, scope.as_id()).await;
        let definition = definition_of_x(places, uses);
        Ok(match self.kind {
            ConsumerKind::Maps => definition.is_some(),
            ConsumerKind::Field => match definition {
                Some(definition) => {
                    endpoint
                        .local_call(|| {
                            FIELD_READS.with_borrow_mut(|reads| reads.push(definition.as_id()));
                            Ok(())
                        })
                        .await;
                    let kind = endpoint
                        .read_field(definition.read_fields(db).kind(), &BorrowOrCopy)
                        .await;
                    endpoint
                        .local_call(|| {
                            endpoint.admit_work(1)?;
                            Ok(matches!(kind, DefinitionKind::AnnotatedAssignment(_)))
                        })
                        .await
                }
                None => false,
            },
        })
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _id: Id,
        _input: ScopeId<'db>,
    ) -> RunResult<bool> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Db,
        _cycle: &'call Cycle<'call>,
        _last: &'call bool,
        _value: bool,
        _input: ScopeId<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct Admit;
impl ExecutionAdmission for Admit {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn controlled(db: &TestDb, scope: ScopeId<'_>, expected: [bool; 2], ran: [bool; 2]) {
    let places_ingredient = place_table::fn_ingredient_(db, db.zalsa());
    let uses_ingredient = use_def_map::fn_ingredient_(db, db.zalsa());
    let places = FinalSourceMemo::certify(db as &dyn Db, places_ingredient, scope.as_id()).unwrap();
    let uses = FinalSourceMemo::certify(db as &dyn Db, uses_ingredient, scope.as_id()).unwrap();
    let source_keys = [places.database_key(), uses.database_key()];
    let caller_keys = [
        map_consumer::fn_ingredient_(db, db.zalsa()).database_key_index(scope.as_id()),
        field_consumer::fn_ingredient_(db, db.zalsa()).database_key_index(scope.as_id()),
    ];
    let before = CONTROLLED_BODIES.get();
    let ordinary_before = ORDINARY_BODIES.get();
    let captured = prepared_source_probe::capture(db, || {
        try_with_attempt(db, 100_000, || -> RunResult<[bool; 2]> {
            let map_provider;
            let field_provider;
            let mut registry = RegistryBuilder::new(db, &Admit)?;
            let places =
                registry.register_final_source(db as &dyn Db, places_ingredient, &[places])?;
            let uses = registry.register_final_source(db as &dyn Db, uses_ingredient, &[uses])?;
            map_provider = Consumer {
                places: places.clone(),
                uses: uses.clone(),
                kind: ConsumerKind::Maps,
            };
            field_provider = Consumer {
                places,
                uses,
                kind: ConsumerKind::Field,
            };
            let map_route =
                registry.reserve(db as &dyn Db, map_consumer::fn_ingredient_(db, db.zalsa()))?;
            let field_route = registry.reserve(
                db as &dyn Db,
                field_consumer::fn_ingredient_(db, db.zalsa()),
            )?;
            let map_binding = registry.provider(&map_provider)?;
            let field_binding = registry.provider(&field_provider)?;
            registry.bind_executable(&map_route, &map_binding)?;
            registry.bind_executable(&field_route, &field_binding)?;
            registry.seal()?.run(move |endpoint| async move {
                let maps = *endpoint
                    .provider(map_binding)?
                    .fetch_ref(&map_route, scope.as_id())?
                    .await?;
                let field = *endpoint
                    .provider(field_binding)?
                    .fetch_ref(&field_route, scope.as_id())?
                    .await?;
                Ok([maps, field])
            })
        })
    })
    .unwrap();
    assert_eq!(captured.value, Ok(AttemptOutcome::Complete(Ok(expected))));
    assert_eq!(
        CONTROLLED_BODIES.get(),
        [
            before[0] + usize::from(ran[0]),
            before[1] + usize::from(ran[1])
        ]
    );
    assert_eq!(
        ORDINARY_BODIES.get(),
        ordinary_before,
        "controlled providers must not call ordinary consumer bodies"
    );
    let source_reads: Vec<(DatabaseKeyIndex, Option<DatabaseKeyIndex>)> = captured
        .reads
        .iter()
        .filter(|read| source_keys.contains(&read.key))
        .map(|read| (read.key, read.parent))
        .collect();
    let expected_reads: Vec<_> = caller_keys
        .into_iter()
        .zip(ran)
        .filter(|(_, ran)| *ran)
        .flat_map(|(caller, _)| source_keys.map(|source| (source, Some(caller))))
        .collect();
    assert_eq!(source_reads, expected_reads);
}

fn assert_stale(db: &TestDb, scope_id: Id) {
    let before = CONTROLLED_BODIES.get();
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            place_table::fn_ingredient_(db, db.zalsa()),
            scope_id
        ),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            use_def_map::fn_ingredient_(db, db.zalsa()),
            scope_id
        ),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            map_consumer::fn_ingredient_(db, db.zalsa()),
            scope_id
        ),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    assert_eq!(CONTROLLED_BODIES.get(), before);
}

#[test]
fn quoted_annotation_preserves_complete_maps_and_definition() {
    COMPARISONS.with_borrow_mut(Vec::clear);
    let (mut db, file) = fixture();
    let definition = {
        let scope = prepare(&db, file, "x");
        map_snapshot(&db, scope).definition.unwrap()
    };
    db.write_file(PATH, ANNOTATION_EDIT).unwrap();
    let scope = prepare(&db, file, "x");
    assert_eq!(map_snapshot(&db, scope).definition, Some(definition));
    COMPARISONS.with_borrow(|comparisons| {
        assert_eq!(
            comparisons.as_slice(),
            [MapComparison {
                places_equal: true,
                uses_equal: true,
                old_definition: Some(definition),
                new_definition: Some(definition),
            }]
        )
    });
}

#[test]
fn finalized_scope_maps_preserve_source_and_tracked_field_dependencies() {
    CONTROLLED_BODIES.set([0, 0]);
    ORDINARY_BODIES.set([0, 0]);
    FIELD_READS.with_borrow_mut(Vec::clear);
    COMPARISONS.with_borrow_mut(Vec::clear);
    let (mut db, file) = fixture();
    let (mut reference, reference_file) = fixture();
    let (scope_id, definition) = {
        let scope = prepare(&db, file, "x");
        let definition = map_snapshot(&db, scope).definition.unwrap();
        let expected = ordinary(&reference, reference_file);
        assert_eq!(expected, [true, true]);
        controlled(&db, scope, expected, [true, true]);
        controlled(&db, scope, expected, [false, false]);
        (scope.as_id(), definition)
    };
    assert_eq!(CONTROLLED_BODIES.get(), [1, 1]);
    FIELD_READS.with_borrow(|reads| assert_eq!(reads.as_slice(), [definition]));

    db.write_file(PATH, ANNOTATION_EDIT).unwrap();
    assert_stale(&db, scope_id);
    reference.write_file(PATH, ANNOTATION_EDIT).unwrap();
    {
        let scope = prepare(&db, file, "x");
        assert_eq!(scope.as_id(), scope_id);
        assert_eq!(map_snapshot(&db, scope).definition, Some(definition));
        COMPARISONS.with_borrow(|comparisons| {
            assert_eq!(
                comparisons.as_slice(),
                [MapComparison {
                    places_equal: true,
                    uses_equal: true,
                    old_definition: Some(definition),
                    new_definition: Some(definition),
                }]
            )
        });
        let expected = ordinary(&reference, reference_file);
        assert_eq!(expected, [true, true]);
        controlled(&db, scope, expected, [false, true]);
        controlled(&db, scope, expected, [false, false]);
    }
    assert_eq!(CONTROLLED_BODIES.get(), [1, 2]);
    FIELD_READS.with_borrow(|reads| assert_eq!(reads.as_slice(), [definition, definition]));

    COMPARISONS.with_borrow_mut(Vec::clear);
    db.write_file(PATH, RENAME).unwrap();
    assert_stale(&db, scope_id);
    reference.write_file(PATH, RENAME).unwrap();
    let scope = prepare(&db, file, "y");
    assert_eq!(scope.as_id(), scope_id);
    COMPARISONS.with_borrow(|comparisons| {
        assert_eq!(comparisons.len(), 1);
        assert!(!comparisons[0].places_equal);
        assert_eq!(comparisons[0].old_definition, Some(definition));
        assert_eq!(comparisons[0].new_definition, None);
    });
    let expected = ordinary(&reference, reference_file);
    assert_eq!(expected, [false, false]);
    controlled(&db, scope, expected, [true, true]);
    controlled(&db, scope, expected, [false, false]);
    controlled(&db, scope, expected, [false, false]);
    assert_eq!(CONTROLLED_BODIES.get(), [2, 3]);
    FIELD_READS.with_borrow(|reads| assert_eq!(reads.as_slice(), [definition, definition]));
}
