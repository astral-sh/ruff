use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use salsa::execution_probe::{
    ExecutionAdmission, ExecutionWork, FinalSourceError, FinalSourceMemo, RegistryBuilder,
};
use salsa::plumbing::ZalsaDatabase;
use salsa::prepared_source_probe::{self, Read, Stamp, Status};
use salsa::{DatabaseKeyIndex, EventKind};
use ty_python_core::definition::DefinitionState;
use ty_python_core::finalized_sources::{place_table_ingredient, use_def_map_ingredient};
use ty_python_core::{global_scope, place_table, semantic_index, use_def_map};

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::class::{
    explicit_bases_ingredient, known_class_to_class_literal_ingredient,
    known_class_to_class_literal_key, pep695_generic_context_ingredient,
    static_class_generic_context_ingredient, try_mro_unspecialized_ingredient,
};
use crate::types::infer::{definition_inference_ingredient, infer_definition_types};
use crate::types::protocol_class::{
    cached_protocol_interface, protocol_interface_ingredient, protocol_interface_memo_ingredient,
};
use crate::types::{KnownClass, TypeQualifiers};

mod object;

const PATH: &str = "/src/protocol.py";
#[derive(Clone, Copy)]
enum MemberFixture {
    Inferred,
    Declared,
}
impl MemberFixture {
    fn source(self) -> &'static str {
        match self {
            Self::Inferred => "from typing import Protocol\nclass P(Protocol):\n    value = 0\n",
            Self::Declared => {
                "from typing import Protocol\nclass P(Protocol):\n    value: int = 0\n"
            }
        }
    }
    fn cold_mro(self) -> bool {
        matches!(self, Self::Inferred)
    }
}

#[derive(Default)]
struct Admission(RefCell<Vec<ExecutionWork>>);
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.0.borrow_mut().push(work);
        Ok(())
    }
}

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(PATH, source)
        .build()
}

fn only_definition<'db>(states: impl Iterator<Item = DefinitionState<'db>>) -> Definition<'db> {
    let mut definitions = states.filter_map(|state| match state {
        DefinitionState::Defined(definition) => Some(definition),
        _ => None,
    });
    let definition = definitions.next().expect("the fixture defines this symbol");
    assert!(definitions.next().is_none());
    definition
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = db.program_file(system_path_to_file(db, PATH)?);
    let _ = semantic_index(db, file);
    let scope = global_scope(db, file);
    let places = place_table(db, scope);
    let uses = use_def_map(db, scope);
    let definition = only_definition(
        uses.end_of_scope_symbol_bindings(places.symbol_id(name).unwrap())
            .map(|binding| binding.binding),
    );
    let Type::ClassLiteral(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).binding_type(definition)
    else {
        panic!("the undecorated fixture has a static class literal");
    };
    Ok(class)
}

struct Prepared<'db> {
    root: StaticClassLiteral<'db>,
    classes: Vec<StaticClassLiteral<'db>>,
    definitions: Vec<Definition<'db>>,
    program: Program<'db>,
    object: StaticClassLiteral<'db>,
    object_id: salsa::Id,
}

fn prepare<'db>(
    db: &'db TestDb,
    members: &[(&str, &[&str])],
    declared: bool,
) -> anyhow::Result<Prepared<'db>> {
    let mut classes = Vec::new();
    let mut definitions = Vec::new();
    for (name, names) in members {
        let class = class(db, name)?;
        let body = class.body_scope(db);
        let places = place_table(db, body);
        let uses = use_def_map(db, body);
        for name in *names {
            let symbol = places.symbol_id(name).unwrap();
            let definition = if declared {
                only_definition(
                    uses.end_of_scope_symbol_declarations(symbol)
                        .map(|declaration| declaration.declaration),
                )
            } else {
                only_definition(
                    uses.end_of_scope_symbol_bindings(symbol)
                        .map(|binding| binding.binding),
                )
            };
            let _ = infer_definition_types(db, definition);
            definitions.push(definition);
        }
        assert_eq!(class.generic_context(db), None);
        let _ = class.explicit_bases(db);
        classes.push(class);
    }
    let root = *classes.last().expect("the last fixture class is the root");
    let env = ProgramEnvironment::from_file(root.program_file(db));
    let program = env.program(db);
    let object = KnownClass::Object.try_to_class_literal(db, &env).unwrap();
    let object_id = known_class_to_class_literal_key(db, KnownClass::Object, program);
    Ok(Prepared {
        root,
        classes,
        definitions,
        program,
        object,
        object_id,
    })
}

fn single<'db>(db: &'db TestDb, fixture: MemberFixture) -> anyhow::Result<Prepared<'db>> {
    prepare(db, &[("P", &["value"])], !fixture.cold_mro())
}

#[derive(Clone, Copy)]
struct ClassKeys {
    mro: DatabaseKeyIndex,
    context: DatabaseKeyIndex,
    bases: DatabaseKeyIndex,
    places: DatabaseKeyIndex,
    uses: DatabaseKeyIndex,
}
fn class_keys<'db>(db: &'db TestDb, class: StaticClassLiteral<'db>) -> ClassKeys {
    let body = class.body_scope(db).as_id();
    ClassKeys {
        mro: try_mro_unspecialized_ingredient(db).database_key_index(class.as_id()),
        context: static_class_generic_context_ingredient(db).database_key_index(class.as_id()),
        bases: explicit_bases_ingredient(db).database_key_index(class.as_id()),
        places: place_table_ingredient(db).database_key_index(body),
        uses: use_def_map_ingredient(db).database_key_index(body),
    }
}
fn interface_key<'db>(db: &'db TestDb, prepared: &Prepared<'db>) -> DatabaseKeyIndex {
    protocol_interface_ingredient(db)
        .database_key_index(ClassType::NonGeneric(prepared.root.into()).as_id())
}
fn definition_key<'db>(db: &'db TestDb, definition: Definition<'db>) -> DatabaseKeyIndex {
    definition_inference_ingredient(db).database_key_index(definition.as_id())
}
fn object_key<'db>(db: &'db TestDb, prepared: &Prepared<'db>) -> DatabaseKeyIndex {
    known_class_to_class_literal_ingredient(db).database_key_index(prepared.object_id)
}
fn mro_final<'db>(db: &'db TestDb, class: StaticClassLiteral<'db>) -> bool {
    match FinalSourceMemo::certify(
        db as &dyn Db,
        try_mro_unspecialized_ingredient(db),
        class.as_id(),
    ) {
        Ok(_) => true,
        Err(FinalSourceError::MissingMemo) => false,
        other => panic!("acyclic MRO must be final or absent: {other:?}"),
    }
}
fn assert_interface_missing<'db>(db: &'db TestDb, prepared: &Prepared<'db>) {
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            protocol_interface_ingredient(db),
            ClassType::NonGeneric(prepared.root.into()).as_id()
        ),
        Err(FinalSourceError::MissingMemo)
    ));
}
fn sorted<'db, C: Configuration>(
    mut memos: Vec<FinalSourceMemo<'db, C>>,
) -> Vec<FinalSourceMemo<'db, C>> {
    memos.sort_by_key(|memo| memo.database_key().key_index());
    memos.dedup_by_key(|memo| memo.database_key().key_index());
    memos
}
fn absent_source<'db, C: Configuration>(
    _ingredient: &'db salsa::plumbing::function::IngredientImpl<C>,
) -> Option<FinalSourceRoute<'db, C>> {
    None
}

struct AttemptRecord<'db> {
    outcome: Result<RunResult<ProtocolInterface<'db>>, Incomplete>,
    raw_error: Option<RunError>,
    reads: Vec<Read>,
    stamp: Stamp,
    preparation: Vec<salsa::Event>,
    events: Vec<salsa::Event>,
    work: Vec<ExecutionWork>,
}
impl<'db> AttemptRecord<'db> {
    fn complete(&self) -> ProtocolInterface<'db> {
        assert!(self.raw_error.is_none());
        match self.outcome {
            Ok(Ok(value)) => value,
            ref other => panic!("interface did not complete: {other:?}"),
        }
    }
    fn refused(&self, key: DatabaseKeyIndex) {
        assert_eq!(self.outcome, Err(Incomplete::Allowance));
        assert_eq!(
            self.raw_error,
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance
            ))
        );
        assert!(
            !self
                .reads
                .iter()
                .any(|read| read.parent.is_none() && read.key == key)
        );
        assert!(
            self.reads
                .iter()
                .all(|read| read.status == Status::Final && read.stamp == self.stamp)
        );
    }
}

fn run_interface<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    allowance: usize,
) -> AttemptRecord<'db> {
    let pt = place_table_ingredient(db);
    let ud = use_def_map_ingredient(db);
    let di = definition_inference_ingredient(db);
    let gc = static_class_generic_context_ingredient(db);
    let eb = explicit_bases_ingredient(db);
    let pc = pep695_generic_context_ingredient(db);
    let kc = known_class_to_class_literal_ingredient(db);
    let cm = try_mro_unspecialized_ingredient(db);
    let ci = protocol_interface_ingredient(db);
    let places = sorted(
        prepared
            .classes
            .iter()
            .map(|class| {
                FinalSourceMemo::certify(
                    db as &dyn ty_python_core::Db,
                    pt,
                    class.body_scope(db).as_id(),
                )
                .unwrap()
            })
            .collect(),
    );
    let uses = sorted(
        prepared
            .classes
            .iter()
            .map(|class| {
                FinalSourceMemo::certify(
                    db as &dyn ty_python_core::Db,
                    ud,
                    class.body_scope(db).as_id(),
                )
                .unwrap()
            })
            .collect(),
    );
    let definitions = sorted(
        prepared
            .definitions
            .iter()
            .map(|definition| {
                FinalSourceMemo::certify(db as &dyn Db, di, definition.as_id()).unwrap()
            })
            .collect(),
    );
    let contexts = sorted(
        prepared
            .classes
            .iter()
            .map(|class| FinalSourceMemo::certify(db as &dyn Db, gc, class.as_id()).unwrap())
            .collect(),
    );
    let bases = sorted(
        prepared
            .classes
            .iter()
            .map(|class| FinalSourceMemo::certify(db as &dyn Db, eb, class.as_id()).unwrap())
            .collect(),
    );
    let object = FinalSourceMemo::certify(db as &dyn Db, kc, prepared.object_id).unwrap();
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let admission = Admission::default();
    let raw_error = Cell::new(None);
    let class_type = ClassType::NonGeneric(prepared.root.into());
    let captured = prepared_source_probe::capture(db, || {
        expansion_probe::run(db, allowance, || {
            let sources;
            let mut registry = RegistryBuilder::new(db, &admission)?;
            sources = ProtocolSources {
                places: registry.register_final_source(
                    db as &dyn ty_python_core::Db,
                    pt,
                    &places,
                )?,
                uses: registry.register_final_source(db as &dyn ty_python_core::Db, ud, &uses)?,
                definitions: if definitions.is_empty() {
                    None
                } else {
                    Some(registry.register_final_source(db as &dyn Db, di, &definitions)?)
                },
                contexts: registry.register_final_source(db as &dyn Db, gc, &contexts)?,
                bases: registry.register_final_source(db as &dyn Db, eb, &bases)?,
                pep695: absent_source(pc),
                object: registry.register_final_source(db as &dyn Db, kc, &[object])?,
                object_program: prepared.program,
                object_id: prepared.object_id,
            };
            let mro_route = registry.reserve_callable(db as &dyn Db, cm)?;
            let interface_route = registry.reserve_callable(db as &dyn Db, ci)?;
            registry.bind_callable(
                &mro_route,
                ProtocolMroProvider {
                    route: mro_route.clone(),
                    sources: &sources,
                },
            )?;
            let values = registry.finite_interned_values(
                db as &dyn Db,
                ProtocolInterface::ingredient(db.zalsa()),
                protocol_interface_memo_ingredient(db),
            )?;
            registry.bind_callable(
                &interface_route,
                ProtocolInterfaceProvider {
                    sources: &sources,
                    mro_route,
                    values,
                },
            )?;
            let result = registry.seal()?.run(move |endpoint| async move {
                endpoint.local_call(|| endpoint.admit_work(1)).await;
                Ok(endpoint
                    .child_call(|| async {
                        Ok(*endpoint
                            .fetch_ref(&interface_route, class_type.as_id())?
                            .await?)
                    })
                    .await)
            });
            raw_error.set(result.as_ref().err().copied());
            result
        })
    })
    .unwrap();
    if matches!(captured.value.0, Ok(Ok(_))) {
        captured.check_root_reads().unwrap();
    }
    AttemptRecord {
        outcome: captured.value.0,
        raw_error: raw_error.get(),
        reads: captured.reads,
        stamp: captured.stamp,
        preparation,
        events: reader.take_salsa_events(),
        work: admission.0.into_inner(),
    }
}

fn executed(events: &[salsa::Event]) -> Vec<DatabaseKeyIndex> {
    events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect()
}
fn assert_query_reads(
    reads: &[Read],
    parent: Option<DatabaseKeyIndex>,
    expected: &[DatabaseKeyIndex],
) {
    let selected = reads
        .iter()
        .filter(|read| read.parent == parent)
        .map(|read| {
            assert_eq!(read.status, Status::Final);
            read.key
        })
        .collect::<HashSet<_>>();
    assert_eq!(selected, expected.iter().copied().collect::<HashSet<_>>());
}
fn assert_parents(reads: &[Read], expected: &[DatabaseKeyIndex]) {
    assert!(
        reads
            .iter()
            .all(|read| read.parent.is_none_or(|parent| expected.contains(&parent))),
        "unexpected parent in the actual query-read trace"
    );
}
fn single_source_keys<'db>(db: &'db TestDb, prepared: &Prepared<'db>) -> [DatabaseKeyIndex; 6] {
    let keys = class_keys(db, prepared.root);
    [
        keys.places,
        keys.uses,
        definition_key(db, prepared.definitions[0]),
        keys.context,
        keys.bases,
        object_key(db, prepared),
    ]
}
fn single_edges<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    record: &AttemptRecord<'db>,
    cold_mro: bool,
) {
    let ci = interface_key(db, prepared);
    let cm = class_keys(db, prepared.root).mro;
    let [places, uses, definition, context, bases, object] = single_source_keys(db, prepared);
    assert_query_reads(&record.reads, None, &[ci]);
    assert_query_reads(
        &record.reads,
        Some(ci),
        &[cm, context, bases, uses, places, definition],
    );
    let mro_sources = [context, bases, object];
    assert_query_reads(
        &record.reads,
        Some(cm),
        if cold_mro { &mro_sources } else { &[] },
    );
    assert_parents(&record.reads, &[ci, cm]);
}
fn assert_member<'db>(
    db: &'db TestDb,
    actual: ProtocolInterface<'db>,
    name: &str,
    definition: Definition<'db>,
    ty: Type<'db>,
    qualifiers: TypeQualifiers,
) {
    let member = &actual.inner(db)[name];
    assert_eq!(member.definition, Some(definition));
    assert_eq!(member.qualifiers, qualifiers);
    assert_eq!(
        member
            .kind
            .member_types()
            .map(|member| member.ty())
            .collect::<Vec<_>>(),
        [ty]
    );
}
fn expected_member<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    fixture: MemberFixture,
) -> (Type<'db>, TypeQualifiers) {
    let definition = prepared.definitions[0];
    let inference = infer_definition_types(db, definition);
    match fixture {
        MemberFixture::Inferred => {
            assert!(
                inference
                    .inferred_declaration(definition)
                    .declared()
                    .is_none()
            );
            (inference.binding_type(definition), TypeQualifiers::empty())
        }
        MemberFixture::Declared => {
            let declared = inference
                .inferred_declaration(definition)
                .declared()
                .unwrap();
            (declared.inner_type(), declared.qualifiers())
        }
    }
}
fn assert_ordinary<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    actual: ProtocolInterface<'db>,
    source: &str,
) -> anyhow::Result<()> {
    let ordinary = database(source)?;
    let ordinary_class = class(&ordinary, "P")?;
    let expected =
        cached_protocol_interface(&ordinary, ClassType::NonGeneric(ordinary_class.into()));
    let ordinary_env = ProgramEnvironment::from_file(ordinary_class.program_file(&ordinary));
    let env = ProgramEnvironment::from_file(prepared.root.program_file(db));
    assert_eq!(
        actual.display(db, &env).to_string(),
        expected.display(&ordinary, &ordinary_env).to_string()
    );
    Ok(())
}
fn assert_single_result<'db>(
    db: &'db TestDb,
    prepared: &Prepared<'db>,
    actual: ProtocolInterface<'db>,
    fixture: MemberFixture,
) {
    let (ty, qualifiers) = expected_member(db, prepared, fixture);
    assert!(qualifiers.is_empty());
    assert_eq!(actual.inner(db).len(), 1);
    assert_member(db, actual, "value", prepared.definitions[0], ty, qualifiers);
    assert_eq!(
        cached_protocol_interface(db, ClassType::NonGeneric(prepared.root.into())),
        actual
    );
    assert_eq!(
        prepared
            .root
            .try_mro(db, None)
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [
            ClassBase::Class(ClassType::NonGeneric(prepared.root.into())),
            ClassBase::Protocol,
            ClassBase::Generic,
            ClassBase::Class(ClassType::NonGeneric(prepared.object.into()))
        ]
    );
}

#[test]
fn cold_data_interface_uses_real_mro_sources_and_canonical_output() -> anyhow::Result<()> {
    data_interface_uses_real_sources(MemberFixture::Inferred)
}
#[test]
fn annotated_data_interface_uses_final_declaration_and_existing_mro() -> anyhow::Result<()> {
    data_interface_uses_real_sources(MemberFixture::Declared)
}
fn data_interface_uses_real_sources(fixture: MemberFixture) -> anyhow::Result<()> {
    let db = database(fixture.source())?;
    let prepared = single(&db, fixture)?;
    assert_eq!(
        prepared.root.explicit_bases(&db),
        [Type::SpecialForm(crate::types::SpecialFormType::Protocol)]
    );
    assert_eq!(mro_final(&db, prepared.root), !fixture.cold_mro());
    assert_interface_missing(&db, &prepared);
    let record = run_interface(&db, &prepared, usize::MAX);
    let actual = record.complete();
    let ci = interface_key(&db, &prepared);
    let cm = class_keys(&db, prepared.root).mro;
    let preparation = executed(&record.preparation);
    assert_eq!(preparation.contains(&cm), !fixture.cold_mro());
    assert!(!preparation.contains(&ci));
    for key in single_source_keys(&db, &prepared) {
        assert!(
            preparation.contains(&key),
            "missing prepared source {key:?}"
        );
    }
    assert_eq!(
        executed(&record.events),
        if fixture.cold_mro() {
            vec![ci, cm]
        } else {
            vec![ci]
        }
    );
    single_edges(&db, &prepared, &record, fixture.cold_mro());
    assert_single_result(&db, &prepared, actual, fixture);
    assert_ordinary(&db, &prepared, actual, fixture.source())?;
    let mut reader = db.clone();
    assert!(
        executed(&reader.take_salsa_events())
            .iter()
            .all(|key| *key != cm && *key != ci)
    );
    assert_eq!(
        record
            .work
            .iter()
            .filter(|event| matches!(event, ExecutionWork::Task { .. }))
            .count(),
        3,
        "root and the two registered query fetches"
    );
    assert!(
        record
            .work
            .iter()
            .any(|event| matches!(event, ExecutionWork::Work { units } if *units > 0))
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum Milestone {
    Mro,
    Interface,
}
fn cold_probe(allowance: usize) -> anyhow::Result<(bool, bool, usize)> {
    let db = database(MemberFixture::Inferred.source())?;
    let prepared = single(&db, MemberFixture::Inferred)?;
    assert!(!mro_final(&db, prepared.root));
    assert_interface_missing(&db, &prepared);
    let record = run_interface(&db, &prepared, allowance);
    let complete = matches!(record.outcome, Ok(Ok(_)));
    if !complete {
        record.refused(interface_key(&db, &prepared));
        assert_interface_missing(&db, &prepared);
    }
    let work = record
        .work
        .iter()
        .try_fold(0usize, |sum, work| {
            sum.checked_add(match work {
                ExecutionWork::Work { units } => *units,
                _ => 0,
            })
        })
        .expect("the small fixture's observed work fits usize");
    Ok((mro_final(&db, prepared.root), complete, work))
}
fn first_allowance(upper: usize, milestone: Milestone) -> anyhow::Result<usize> {
    let mut low = 0;
    let mut high = upper;
    let mut probes = 0;
    while high - low > 1 {
        probes += 1;
        assert!(probes <= usize::BITS);
        let middle = low + (high - low) / 2;
        // A failed attempt may retain completed children; each calibration needs fresh cold memos.
        let (mro, interface, _) = cold_probe(middle)?;
        if match milestone {
            Milestone::Mro => mro,
            Milestone::Interface => interface,
        } {
            high = middle;
        } else {
            low = middle;
        }
    }
    Ok(high)
}
fn memo_address(
    record: &AttemptRecord<'_>,
    parent: DatabaseKeyIndex,
    key: DatabaseKeyIndex,
) -> usize {
    let addresses = record
        .reads
        .iter()
        .filter(|read| read.parent == Some(parent) && read.key == key)
        .map(|read| read.memo_address)
        .collect::<HashSet<_>>();
    assert_eq!(addresses.len(), 1);
    *addresses.iter().next().unwrap()
}

#[test]
fn real_allowance_retains_completed_mro_and_allows_same_revision_retry() -> anyhow::Result<()> {
    let (mro, interface, upper) = cold_probe(usize::MAX)?;
    assert!(mro && interface && upper > 0);
    let (mro, interface, _) = cold_probe(upper)?;
    assert!(
        mro && interface,
        "reported Work is an upper bound, not the actual debit total"
    );
    let (mro, interface, _) = cold_probe(0)?;
    assert!(!mro && !interface);
    let mro_allowance = first_allowance(upper, Milestone::Mro)?;
    let interface_allowance = first_allowance(upper, Milestone::Interface)?;
    assert!(
        0 < mro_allowance && mro_allowance < interface_allowance && interface_allowance <= upper
    );

    for (allowance, retained_mro) in [(mro_allowance - 1, false), (interface_allowance - 1, true)] {
        let db = database(MemberFixture::Inferred.source())?;
        let prepared = single(&db, MemberFixture::Inferred)?;
        assert!(!mro_final(&db, prepared.root));
        assert_interface_missing(&db, &prepared);
        let ci = interface_key(&db, &prepared);
        let cm = class_keys(&db, prepared.root).mro;
        let failed = run_interface(&db, &prepared, allowance);
        failed.refused(ci);
        assert_eq!(executed(&failed.events), [ci, cm]);
        assert_interface_missing(&db, &prepared);
        assert_eq!(mro_final(&db, prepared.root), retained_mro);
        let [places, uses, definition, context, bases, object] = single_source_keys(&db, &prepared);
        let mut interface_reads = vec![context, bases, uses, places, definition];
        if retained_mro {
            interface_reads.push(cm);
        }
        assert_query_reads(&failed.reads, None, &[]);
        assert_query_reads(&failed.reads, Some(ci), &interface_reads);
        let mro_reads = failed
            .reads
            .iter()
            .filter(|read| read.parent == Some(cm))
            .collect::<Vec<_>>();
        assert!(
            !mro_reads.is_empty(),
            "the refusal must occur after actual MRO source work"
        );
        assert!(
            mro_reads
                .iter()
                .all(|read| [context, bases, object].contains(&read.key))
        );
        if retained_mro {
            assert_query_reads(&failed.reads, Some(cm), &[context, bases, object]);
        }
        assert_parents(&failed.reads, &[ci, cm]);
        let retained_address = retained_mro.then(|| memo_address(&failed, ci, cm));

        // This helper recertifies the original sources and creates new registry owners; it runs no producer.
        let retry = run_interface(&db, &prepared, usize::MAX);
        assert_eq!(retry.stamp, failed.stamp);
        assert!(executed(&retry.preparation).is_empty());
        assert_eq!(
            executed(&retry.events),
            if retained_mro { vec![ci] } else { vec![ci, cm] }
        );
        let actual = retry.complete();
        single_edges(&db, &prepared, &retry, !retained_mro);
        if let Some(address) = retained_address {
            assert_eq!(memo_address(&retry, ci, cm), address);
        }
        assert!(mro_final(&db, prepared.root));
        assert_single_result(&db, &prepared, actual, MemberFixture::Inferred);
        assert_ordinary(&db, &prepared, actual, MemberFixture::Inferred.source())?;
    }
    Ok(())
}

#[test]
fn edited_declaration_recertifies_sources_and_invalidates_the_same_interface() -> anyhow::Result<()>
{
    const EDITED: &str = "from typing import Protocol\nclass P(Protocol):\n    value: str = \"\"\n";
    let mut db = database(MemberFixture::Declared.source())?;
    let (old_key, old_definition, old_stamp, old_rendering, old_member) = {
        let prepared = single(&db, MemberFixture::Declared)?;
        assert!(mro_final(&db, prepared.root));
        assert_interface_missing(&db, &prepared);
        let first = run_interface(&db, &prepared, usize::MAX);
        let actual = first.complete();
        assert_eq!(executed(&first.events), [interface_key(&db, &prepared)]);
        single_edges(&db, &prepared, &first, false);
        assert_single_result(&db, &prepared, actual, MemberFixture::Declared);
        let env = ProgramEnvironment::from_file(prepared.root.program_file(&db));
        let (ty, _) = expected_member(&db, &prepared, MemberFixture::Declared);
        (
            interface_key(&db, &prepared),
            definition_key(&db, prepared.definitions[0]),
            first.stamp,
            actual.display(&db, &env).to_string(),
            ty.display(&db, &env).to_string(),
        )
    };
    db.write_file(PATH, EDITED)?;
    let prepared = single(&db, MemberFixture::Declared)?;
    let ci = interface_key(&db, &prepared);
    assert_eq!(
        ci, old_key,
        "the edit must invalidate the existing canonical query key"
    );
    assert_eq!(definition_key(&db, prepared.definitions[0]), old_definition);
    assert_ne!(Stamp::current(&db), old_stamp);
    assert!(mro_final(&db, prepared.root));
    assert!(matches!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            protocol_interface_ingredient(&db),
            ClassType::NonGeneric(prepared.root.into()).as_id()
        ),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    let edited = run_interface(&db, &prepared, usize::MAX);
    assert!(!executed(&edited.preparation).contains(&ci));
    assert_eq!(executed(&edited.events), [ci]);
    let actual = edited.complete();
    single_edges(&db, &prepared, &edited, false);
    assert_single_result(&db, &prepared, actual, MemberFixture::Declared);
    let env = ProgramEnvironment::from_file(prepared.root.program_file(&db));
    let (ty, _) = expected_member(&db, &prepared, MemberFixture::Declared);
    assert_ne!(ty.display(&db, &env).to_string(), old_member);
    assert_ne!(actual.display(&db, &env).to_string(), old_rendering);
    assert_ordinary(&db, &prepared, actual, EDITED)?;
    let warm = run_interface(&db, &prepared, usize::MAX);
    assert_eq!(warm.stamp, edited.stamp);
    assert_eq!(warm.complete(), actual);
    assert!(executed(&warm.events).is_empty());
    assert_query_reads(&warm.reads, None, &[ci]);
    assert!(warm.reads.iter().all(|read| read.parent.is_none()));
    Ok(())
}

#[test]
fn inherited_members_follow_cold_mro_order_and_nearest_definition() -> anyhow::Result<()> {
    const SOURCE: &str = "from typing import Protocol\nclass Base(Protocol):\n    value = 0\n    inherited = 2\nclass P(Base, Protocol):\n    value = 1\n    own = 3\n";
    let db = database(SOURCE)?;
    let prepared = prepare(
        &db,
        &[("Base", &["value", "inherited"]), ("P", &["value", "own"])],
        false,
    )?;
    let base_class = prepared.classes[0];
    assert!(prepared.classes.iter().all(|class| !mro_final(&db, *class)));
    assert_interface_missing(&db, &prepared);
    let ci = interface_key(&db, &prepared);
    let root = class_keys(&db, prepared.root);
    let base = class_keys(&db, base_class);
    let record = run_interface(&db, &prepared, usize::MAX);
    let preparation = executed(&record.preparation);
    assert!(
        [ci, root.mro, base.mro]
            .iter()
            .all(|key| !preparation.contains(key))
    );
    assert_eq!(executed(&record.events), [ci, root.mro, base.mro]);
    let actual = record.complete();
    assert_eq!(actual.inner(&db).len(), 3);
    for (name, index, value) in [("value", 2, 1), ("inherited", 1, 2), ("own", 3, 3)] {
        let definition = prepared.definitions[index];
        let ty = infer_definition_types(&db, definition).binding_type(definition);
        assert_eq!(ty, Type::int_literal(value));
        assert_member(&db, actual, name, definition, ty, TypeQualifiers::empty());
    }
    assert_eq!(
        cached_protocol_interface(&db, ClassType::NonGeneric(prepared.root.into())),
        actual
    );
    assert_eq!(
        prepared
            .root
            .try_mro(&db, None)
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [
            ClassBase::Class(ClassType::NonGeneric(prepared.root.into())),
            ClassBase::Class(ClassType::NonGeneric(base_class.into())),
            ClassBase::Protocol,
            ClassBase::Generic,
            ClassBase::Class(ClassType::NonGeneric(prepared.object.into()))
        ]
    );
    assert_ordinary(&db, &prepared, actual, SOURCE)?;
    let definitions = prepared
        .definitions
        .iter()
        .map(|definition| definition_key(&db, *definition))
        .collect::<Vec<_>>();
    let object = object_key(&db, &prepared);
    let mut expected = vec![
        root.mro,
        root.context,
        root.bases,
        base.bases,
        root.places,
        root.uses,
        base.places,
        base.uses,
    ];
    expected.extend_from_slice(&definitions);
    assert_query_reads(&record.reads, None, &[ci]);
    assert_query_reads(&record.reads, Some(ci), &expected);
    assert_query_reads(
        &record.reads,
        Some(root.mro),
        &[root.context, root.bases, base.context, base.mro, object],
    );
    assert_query_reads(
        &record.reads,
        Some(base.mro),
        &[base.context, base.bases, object],
    );
    assert_parents(&record.reads, &[ci, root.mro, base.mro]);
    let position = |key| {
        record
            .reads
            .iter()
            .position(|read| read.parent == Some(ci) && read.key == key)
            .unwrap()
    };
    assert!(position(root.places) < position(base.places));
    assert!(position(root.uses) < position(base.uses));
    assert!(position(definitions[2]) < position(definitions[0]));
    Ok(())
}
