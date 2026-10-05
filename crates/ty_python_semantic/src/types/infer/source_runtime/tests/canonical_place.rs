use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FinalSourceMemo;
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe::Status;
use ty_python_core::place::ScopedPlaceId;

use super::*;
use crate::place::{
    ConsideredDefinitions, Definedness, Place, Provenance, RequiresExplicitReExport, place_by_id,
    place_by_id_ingredient,
};
use crate::types::member_lookup::runtime_profile::PlaceConfiguration;
use crate::types::normalization::source::observations as normalization_observations;

type Input<'db> = (
    ScopeId<'db>,
    ScopedPlaceId,
    RequiresExplicitReExport,
    ConsideredDefinitions,
);

#[derive(Clone, Debug, Default)]
struct Recovery {
    initial: Vec<salsa::Id>,
    entered: Vec<(salsa::Id, u32)>,
    completed: Vec<salsa::Id>,
    first_remaining: Option<usize>,
    cancellation_requested: bool,
}

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static CANCEL_RECOVERY: Cell<bool> = const { Cell::new(false) };
    static RECOVERY: RefCell<Recovery> = RefCell::new(Recovery::default());
}

struct Recording;

impl Recording {
    fn start() -> Self {
        Self::with_cancellation(false)
    }

    fn with_cancellation(cancel: bool) -> Self {
        assert!(!RECORDING.replace(true));
        CANCEL_RECOVERY.set(cancel);
        RECOVERY.with_borrow_mut(|recovery| *recovery = Recovery::default());
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
        CANCEL_RECOVERY.set(false);
    }
}

pub(in crate::types::infer::source_runtime) fn observe_initial(id: salsa::Id) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| recovery.initial.push(id));
    }
}

pub(in crate::types::infer::source_runtime) fn observe_recovery(
    db: &dyn Db,
    id: salsa::Id,
    iteration: u32,
) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| {
            recovery.entered.push((id, iteration));
            if recovery.first_remaining.is_none() {
                recovery.first_remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            }
        });
        if CANCEL_RECOVERY.replace(false) {
            RECOVERY.with_borrow_mut(|recovery| recovery.cancellation_requested = true);
            db.cancellation_token().cancel();
        }
    }
}

pub(in crate::types::infer::source_runtime) fn observe_recovered(id: salsa::Id) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| recovery.completed.push(id));
    }
}

fn database(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db
}

fn input<'db>(prepared: &PreparedAnalysisFile<'db>, name: &str) -> Input<'db> {
    let index = prepared.semantic_index();
    let symbol = index
        .place_table(FileScopeId::global())
        .symbol_id(name)
        .unwrap();
    (
        index.scope_id(FileScopeId::global()),
        ScopedPlaceId::Symbol(symbol),
        RequiresExplicitReExport::No,
        ConsideredDefinitions::EndOfScope,
    )
}

fn ordinary<'db>(db: &'db TestDb, input: Input<'db>) -> PlaceAndQualifiers<'db> {
    place_by_id(db, input.0, input.1, input.2, input.3)
}

fn existing_key<'db, C: PlaceConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    input: &Input<'db>,
) -> Option<salsa::Id> {
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| entry.value().fields() == input);
    let id = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    id
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    input: Input<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<PlaceAndQualifiers<'db>>, AnalysisFailure> {
    normalization_observations::reset(None);
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let weak = Rc::downgrade(&routes);
        let query_routes = Rc::clone(&routes);
        let values = &values;
        let result = catch_unwind(AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                access.place_by_id(input.0, input.1, input.2, input.3).await
            })
        }));
        assert_eq!(observations::counts().0, 0);
        let normalization = normalization_observations::snapshot();
        assert_eq!(normalization.children, normalization.dropped);
        assert_eq!(normalization.buffers, normalization.dropped_buffers);
        assert_eq!(normalization.live_buffers, 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

#[derive(Debug, PartialEq, Eq)]
enum ProvenanceIdentity {
    Unknown,
    MultipleDefinitions,
    SingleDefinition { file: String, range: TextRange },
}

impl ProvenanceIdentity {
    fn of(db: &TestDb, provenance: Provenance<'_>) -> Self {
        match provenance {
            Provenance::Unknown => Self::Unknown,
            Provenance::MultipleDefinitions => Self::MultipleDefinitions,
            Provenance::SingleDefinition(definition) => {
                let module = parsed_module(db, definition.python_file(db)).load(db);
                Self::SingleDefinition {
                    file: definition.file(db).path(db).to_string(),
                    range: definition.full_range(db, &module).range(),
                }
            }
        }
    }
}

fn assert_equivalent(
    db: &TestDb,
    prepared: &PreparedAnalysisFile<'_>,
    actual: PlaceAndQualifiers<'_>,
    ordinary_db: &TestDb,
    ordinary_prepared: &PreparedAnalysisFile<'_>,
    expected: PlaceAndQualifiers<'_>,
) {
    assert_eq!(actual.qualifiers, expected.qualifiers);
    match (actual.place, expected.place) {
        (Place::Undefined, Place::Undefined) => {}
        (Place::Defined(actual), Place::Defined(expected)) => {
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
            assert_eq!(
                actual.ty.display(db, &env).to_string(),
                expected.ty.display(ordinary_db, &ordinary_env).to_string()
            );
            assert_eq!(actual.definedness, expected.definedness);
            assert_eq!(actual.origin, expected.origin);
            assert_eq!(actual.public_type_policy, expected.public_type_policy);
            assert_eq!(
                ProvenanceIdentity::of(db, actual.provenance),
                ProvenanceIdentity::of(ordinary_db, expected.provenance),
            );
        }
        _ => panic!("different definedness: {actual:?}, {expected:?}"),
    }
}

#[test]
fn cold_place_keys_preserve_scope_symbol_reexport_and_definition_selection() {
    let source = "value = True\nvalue = False\nother = True\n";
    let mut db = database(source);
    db.write_file("src/other.py", source).unwrap();
    let prepared = prepare(&db);
    let other_file = system_path_to_file(&db, "src/other.py").unwrap();
    let other_prepared = prepare_file(&db, other_file).unwrap();
    let mut ordinary_db = database(source);
    ordinary_db.write_file("src/other.py", source).unwrap();
    let ordinary_prepared = prepare(&ordinary_db);
    let other_file = system_path_to_file(&ordinary_db, "src/other.py").unwrap();
    let ordinary_other = prepare_file(&ordinary_db, other_file).unwrap();
    let ingredient = place_by_id_ingredient(&db);
    let mut keys = Vec::new();
    for (prepared, ordinary_prepared) in [
        (&prepared, &ordinary_prepared),
        (&other_prepared, &ordinary_other),
    ] {
        for name in ["value", "other"] {
            for reexport in [RequiresExplicitReExport::No, RequiresExplicitReExport::Yes] {
                for considered in [
                    ConsideredDefinitions::EndOfScope,
                    ConsideredDefinitions::AllReachable,
                ] {
                    let mut request = input(prepared, name);
                    request.2 = reexport;
                    request.3 = considered;
                    assert!(existing_key(&db, ingredient, &request).is_none());
                    observations::reset(None);
                    let cold = capture(&db, || controlled(prepared, request, &funded())).unwrap();
                    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
                        panic!("{request:?}: {:?}", cold.value);
                    };
                    cold.check_root_reads().unwrap();
                    let key = existing_key(&db, ingredient, &request).unwrap();
                    assert!(!keys.contains(&key));
                    keys.push(key);
                    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
                    let mut ordinary_request = input(ordinary_prepared, name);
                    ordinary_request.2 = reexport;
                    ordinary_request.3 = considered;
                    assert_equivalent(
                        &db,
                        prepared,
                        actual,
                        &ordinary_db,
                        ordinary_prepared,
                        ordinary(&ordinary_db, ordinary_request),
                    );
                    assert_no_active_attempt();
                }
            }
        }
    }
    assert_eq!(keys.len(), 16);
}

#[test]
fn imported_places_record_direct_dependencies_and_reuse_the_ordinary_memo() {
    let mut db = database("from dependency import value\n");
    db.write_file("src/dependency.py", "value = True\n")
        .unwrap();
    let prepared = prepare(&db);
    let request = input(&prepared, "value");
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let cold = capture(&db, || controlled(&prepared, request, &funded())).unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
        panic!("{:?}", cold.value);
    };
    assert!(
        matches!(actual.place, Place::Defined(place) if place.ty == Type::bool_literal(true)
        && place.definedness == Definedness::AlwaysDefined)
    );
    cold.check_root_reads().unwrap();
    let ingredient = place_by_id_ingredient(&db);
    let id = existing_key(&db, ingredient, &request).unwrap();
    let root_key = ingredient.database_key_index(id);
    let root = cold
        .reads
        .iter()
        .find(|read| read.key == root_key && read.parent.is_none())
        .unwrap();
    let definition = cold
        .reads
        .iter()
        .find(|read| {
            read.parent == Some(root_key)
                && db.ingredient_debug_name(read.key.ingredient_index()) == "infer_definition_types"
        })
        .unwrap();
    let imported = cold
        .reads
        .iter()
        .find(|read| {
            read.parent == Some(definition.key)
                && read.key != root_key
                && db.ingredient_debug_name(read.key.ingredient_index()) == "place_by_id"
        })
        .unwrap();
    assert!(cold.reads.iter().any(|read| {
        read.parent == Some(imported.key)
            && db.ingredient_debug_name(read.key.ingredient_index()) == "infer_definition_types"
    }));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, imported.key.key_index()).is_ok());

    let mut reader = db.clone();
    reader.take_salsa_events();
    let native = capture(&db, || ordinary(&db, request)).unwrap();
    assert_eq!(native.value, actual);
    let reused = native
        .reads
        .iter()
        .find(|read| read.key == root_key && read.parent.is_none())
        .unwrap();
    assert_eq!(reused.status, Status::Final);
    assert_eq!(reused.memo_address, root.memo_address);
    assert_eq!(reused.stamp, root.stamp);
    assert_eq!(controlled(&prepared, request, &funded()), cold.value);
    let events = reader.take_salsa_events();
    for query in ["place_by_id", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn cyclic_module_name_reaches_the_actual_place_recovery_provider() {
    // The module attribute refers back to the public place currently being inferred. The
    // concrete operand lets recovery retain a value after removing the divergent cycle seed.
    let source = "import main\nvalue = main.value or True\n";
    let db = database(source);
    let prepared = prepare(&db);
    let request = input(&prepared, "value");
    observations::reset(None);
    let recording = Recording::start();
    let cold = capture(&db, || controlled(&prepared, request, &funded())).unwrap();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
        panic!("natural place cycle: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let ingredient = place_by_id_ingredient(&db);
    let id = existing_key(&db, ingredient, &request).unwrap();
    let recovery = RECOVERY.with_borrow(Clone::clone);
    assert!(recovery.initial.contains(&id), "{recovery:?}");
    assert!(
        recovery.entered.iter().any(|(key, _)| *key == id),
        "{recovery:?}"
    );
    assert_eq!(
        recovery.completed,
        recovery
            .entered
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>()
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    let ordinary_db = database(source);
    let ordinary_prepared = prepare(&ordinary_db);
    let expected = ordinary(&ordinary_db, input(&ordinary_prepared, "value"));
    assert_equivalent(
        &db,
        &prepared,
        actual,
        &ordinary_db,
        &ordinary_prepared,
        expected,
    );
    let recording = Recording::start();
    assert_eq!(controlled(&prepared, request, &funded()), cold.value);
    drop(recording);
    assert!(RECOVERY.with_borrow(|recovery| recovery.entered.is_empty()));
    assert_no_active_attempt();
}

#[test]
fn interrupted_place_queries_release_children_and_allow_same_revision_retry() {
    let measured = database("value = True\n");
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert!(matches!(
        controlled(
            &measured_prepared,
            input(&measured_prepared, "value"),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(_))
    ));
    // Work exhaustion stops after the initializer is stored. Prepared dependencies mask native
    // cancellation until publication, so retry reuses the completed place in that case.
    let stored_work = funded().semantic_work_limit - observations::stored_remaining().unwrap();

    for cancel in [false, true] {
        let db = database("value = True\n");
        let prepared = prepare(&db);
        let request = input(&prepared, "value");
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::Stored));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        };
        let result =
            salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, request, &policy)));
        if cancel {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            );
        }
        let ingredient = place_by_id_ingredient(&db);
        let id = existing_key(&db, ingredient, &request).unwrap();
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok(),
            cancel
        );
        assert_no_active_attempt();
        observations::reset(None);
        let mut reader = db.clone();
        reader.take_salsa_events();
        let Ok(AnalysisOutcome::Complete(actual)) = controlled(&prepared, request, &funded())
        else {
            panic!("funded place retry did not complete");
        };
        assert!(
            matches!(actual.place, Place::Defined(place) if place.ty == Type::bool_literal(true))
        );
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        let events = reader.take_salsa_events();
        if cancel {
            for query in ["place_by_id", "infer_definition_types"] {
                assert_function_query_was_not_run_by_name(&db, query, None, &events);
            }
        } else {
            assert!(
                find_will_execute_event_by_name(&db, "place_by_id", Some(id), &events).is_some()
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn interrupted_natural_place_recovery_cleans_up_and_retries_in_the_same_revision() {
    // Work exhaustion at the first recovery entry prevents publication, so retry repeats recovery.
    // Prepared dependencies defer cancellation requested there until publication; retry then
    // reuses the completed memo.
    let source = "import main\nvalue = main.value or True\n";
    let measured = database(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let recording = Recording::start();
    assert!(matches!(
        controlled(
            &measured_prepared,
            input(&measured_prepared, "value"),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(_))
    ));
    drop(recording);
    let recovery_work = funded().semantic_work_limit
        - RECOVERY.with_borrow(|recovery| recovery.first_remaining.unwrap());

    for cancel in [false, true] {
        let db = database(source);
        let prepared = prepare(&db);
        let request = input(&prepared, "value");
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::with_cancellation(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: recovery_work,
                ..funded()
            }
        };
        let result =
            salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, request, &policy)));
        drop(recording);
        let recovery = RECOVERY.with_borrow(Clone::clone);
        assert!(!recovery.entered.is_empty(), "{recovery:?}");
        assert_eq!(recovery.cancellation_requested, cancel);
        if cancel {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            assert_eq!(recovery.completed.len(), recovery.entered.len());
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            );
            assert!(recovery.completed.is_empty(), "{recovery:?}");
        }
        let ingredient = place_by_id_ingredient(&db);
        let id = existing_key(&db, ingredient, &request).unwrap();
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok(),
            cancel
        );
        assert_no_active_attempt();

        observations::reset(None);
        let mut reader = db.clone();
        reader.take_salsa_events();
        let recording = Recording::start();
        let retry = capture(&db, || controlled(&prepared, request, &funded())).unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(actual)) = retry.value else {
            panic!("funded natural place recovery retry: {:?}", retry.value);
        };
        retry.check_root_reads().unwrap();
        let recovery = RECOVERY.with_borrow(Clone::clone);
        let events = reader.take_salsa_events();
        if cancel {
            assert!(recovery.entered.is_empty(), "{recovery:?}");
            for query in ["place_by_id", "infer_definition_types"] {
                assert_function_query_was_not_run_by_name(&db, query, None, &events);
            }
        } else {
            assert!(
                recovery.entered.iter().any(|(key, _)| *key == id),
                "{recovery:?}"
            );
            assert_eq!(recovery.completed.len(), recovery.entered.len());
            assert!(
                find_will_execute_event_by_name(&db, "place_by_id", Some(id), &events).is_some()
            );
        }
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        let ordinary_db = database(source);
        let ordinary_prepared = prepare(&ordinary_db);
        let expected = ordinary(&ordinary_db, input(&ordinary_prepared, "value"));
        assert_equivalent(
            &db,
            &prepared,
            actual,
            &ordinary_db,
            &ordinary_prepared,
            expected,
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
