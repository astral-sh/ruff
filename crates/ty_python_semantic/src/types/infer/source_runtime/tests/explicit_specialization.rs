use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::class::{
    generic_alias_try_mro_ingredient, instance_flags_inner_ingredient,
    try_mro_unspecialized_ingredient,
};
use crate::types::mro::source::source_alias_mro_ingredient;

thread_local! {
    static ARGUMENTS: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static LAST_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
    static CANCEL_AT: Cell<usize> = const { Cell::new(1) };
    static C3_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static C3_ARGUMENTS: Cell<Option<usize>> = const { Cell::new(None) };
    static CANCEL_C3: Cell<bool> = const { Cell::new(false) };
    static C3_ORIGIN_ENTERED: Cell<usize> = const { Cell::new(0) };
    static C3_ORIGIN_RETURNED: Cell<usize> = const { Cell::new(0) };
    static C3_ORIGIN_ALIASES: Cell<[Option<salsa::Id>; 2]> = const { Cell::new([None; 2]) };
    static C3_ORIGIN_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static C3_ORIGIN_ARGUMENTS: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(in crate::types::infer) fn observe_c3_append(db: &dyn Db) {
    if C3_REMAINING.get().is_none() {
        C3_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        C3_ARGUMENTS.set(Some(ARGUMENTS.get()));
    }
    if CANCEL_C3.replace(false) {
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn observe_c3_origin(
    db: &dyn Db,
    alias: GenericAlias<'_>,
    returned: bool,
) {
    if returned {
        C3_ORIGIN_RETURNED.set(C3_ORIGIN_RETURNED.get() + 1);
        return;
    }
    let ordinal = C3_ORIGIN_ENTERED.get();
    C3_ORIGIN_ENTERED.set(ordinal + 1);
    if ordinal < 2 {
        let mut aliases = C3_ORIGIN_ALIASES.get();
        aliases[ordinal] = Some(alias.as_id());
        C3_ORIGIN_ALIASES.set(aliases);
    }
    if ordinal == 1 {
        C3_ORIGIN_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        C3_ORIGIN_ARGUMENTS.set(Some(ARGUMENTS.get()));
    }
}

pub(in crate::types::infer) fn observe_argument(db: &dyn Db) {
    let previous = ARGUMENTS.get();
    ARGUMENTS.set(previous + 1);
    LAST_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
        db,
    ));
    if previous == 0 {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        ACTIVE.set(observations::counts().0);
    }
    if previous + 1 == CANCEL_AT.get() && CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    ARGUMENTS.set(0);
    REMAINING.set(None);
    LAST_REMAINING.set(None);
    ACTIVE.set(0);
    CANCEL.set(cancel);
    CANCEL_AT.set(1);
    C3_REMAINING.set(None);
    C3_ARGUMENTS.set(None);
    CANCEL_C3.set(false);
    C3_ORIGIN_ENTERED.set(0);
    C3_ORIGIN_RETURNED.set(0);
    C3_ORIGIN_ALIASES.set([None; 2]);
    C3_ORIGIN_REMAINING.set(None);
    C3_ORIGIN_ARGUMENTS.set(None);
    observations::reset(None);
}

fn fixture() -> TestDb {
    specialization_fixture(false)
}

fn specialization_fixture(nested: bool) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.pyi",
        &format!(
            "from typing import Generic, TypeVar\n\
         T = TypeVar(\"T\")\n\
         class Box(Generic[T]): ...\n\
         class Leaf: ...\n\
         left = right = Box[{}]\n",
            if nested { "Box[Leaf]" } else { "Leaf" }
        ),
    )
    .unwrap();
    db
}

fn prepare_fixture(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.pyi").unwrap();
    prepare_file(db, file).unwrap()
}

fn selected_expression<'db>(prepared: &PreparedAnalysisFile<'db>) -> Expression<'db> {
    prepared
        .semantic_index()
        .expression(expression_key(prepared))
}

fn class_definition<'db>(prepared: &PreparedAnalysisFile<'db>, name: &str) -> Definition<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap();
    prepared.semantic_index().expect_single_definition(class)
}

fn canonical_class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> StaticClassLiteral<'db> {
    let definition = class_definition(prepared, name);
    let ingredient = definition_inference_ingredient(db);
    assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, definition.as_id()).is_ok());
    let inference = salsa::attach(db, || {
        ingredient.fetch(
            db as &dyn Db,
            (db as &dyn Db).zalsa(),
            (db as &dyn Db).zalsa_local(),
            definition.as_id(),
        )
    });
    let Some(ClassLiteral::Static(class)) = inference.original_class_type(definition) else {
        panic!("fixture class {name}");
    };
    class
}

fn assert_shape<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    alias: GenericAlias<'db>,
) {
    assert_specialization_shape(db, prepared, alias, false);
}

fn assert_specialization_shape<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    alias: GenericAlias<'db>,
    nested: bool,
) {
    let origin = canonical_class(db, prepared, "Box");
    let leaf = canonical_class(db, prepared, "Leaf");
    assert_eq!(alias.origin(db), origin);
    let ingredient = static_class_generic_context_ingredient(db);
    assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, origin.as_id()).is_ok());
    let context = salsa::attach(db, || {
        *ingredient.fetch(
            db as &dyn Db,
            (db as &dyn Db).zalsa(),
            (db as &dyn Db).zalsa_local(),
            origin.as_id(),
        )
    })
    .unwrap();
    assert_eq!(context.variables(db).len(), 1);
    let specialization = alias.specialization(db);
    assert_eq!(specialization.generic_context(db), context);
    assert_eq!(specialization.materialization_kind(db), None);
    assert!(specialization.tuple(db).is_none());
    let [argument] = specialization.types(db) else {
        panic!("fixture specialization has one argument");
    };
    let Type::NominalInstance(instance) = argument else {
        panic!("fixture Leaf instance: {argument:?}");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert!(!instance.inherits_from_explicit_any());
    if nested {
        let ClassType::Generic(inner) = instance.class(db, &env) else {
            panic!("fixture inner Box instance: {argument:?}");
        };
        assert_shape(db, prepared, inner);
    } else {
        assert_eq!(instance.class_literal(db, &env), ClassLiteral::Static(leaf));
    }
    assert_eq!(
        specialization,
        Specialization::new(db, context, vec![*argument].into_boxed_slice(), None, None),
    );
    assert_eq!(alias, GenericAlias::new(db, origin, specialization));
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn completed_alias<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> (GenericAlias<'db>, usize) {
    let revision = salsa::plumbing::current_revision(db);
    for preceding_work_limit_attempts in 0..4 {
        reset(false);
        let result = expression_type_with_policy(prepared, expression_key(prepared), &funded());
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(db), revision);
        match result {
            Ok(AnalysisOutcome::Complete(Type::GenericAlias(alias))) => {
                return (alias, preceding_work_limit_attempts);
            }
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }) => {}
            other => panic!("{other:?}"),
        }
    }
    panic!("explicit specialization did not complete within four funded caller attempts");
}

fn expression_ran(db: &dyn Db, expression: Expression<'_>, events: &[salsa::Event]) -> bool {
    let key = expression_inference_ingredient(db)
        .database_key_index(InferExpression::Bare(expression).as_id());
    events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    })
}

#[test]
fn cold_flat_specialization_preserves_canonical_identity_payload_and_reuse() {
    cold_specialization_preserves_canonical_identity_payload_and_reuse(false);
}

#[test]
fn cold_nested_specialization_preserves_canonical_identity_payload_and_reuse() {
    cold_specialization_preserves_canonical_identity_payload_and_reuse(true);
}

fn cold_specialization_preserves_canonical_identity_payload_and_reuse(nested: bool) {
    let db = specialization_fixture(nested);
    let prepared = prepare_fixture(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let expression = selected_expression(&prepared);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let (alias, _) = completed_alias(&db, &prepared);
    assert_eq!(ARGUMENTS.get(), if nested { 2 } else { 1 });
    assert!(ACTIVE.get() > 0);
    let events = events_db.take_salsa_events();
    assert!(expression_ran(&db, expression, &events));
    let ingredient = expression_inference_ingredient(&db);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            ingredient,
            InferExpression::Bare(expression).as_id()
        )
        .is_ok()
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            generic_alias_try_mro_ingredient(&db),
            alias.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            source_alias_mro_ingredient(&db),
            alias.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let origin = canonical_class(&db, &prepared, "Box");
    let flags = instance_flags_inner_ingredient(&db);
    let mro = try_mro_unspecialized_ingredient(&db);
    let classification = nested.then(|| {
        assert!(FinalSourceMemo::certify(&db as &dyn Db, flags, origin.as_id()).is_ok());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, mro, origin.as_id()).is_ok());
        assert!(events.iter().any(|event| matches!(event.kind,
            salsa::EventKind::WillExecute { database_key }
                if database_key == flags.database_key_index(origin.as_id()))));
        assert!(events.iter().any(|event| matches!(event.kind,
            salsa::EventKind::WillExecute { database_key }
                if database_key == mro.database_key_index(origin.as_id()))));
        salsa::attach(&db, || {
            let db = &db as &dyn Db;
            let flags = flags.fetch(db, db.zalsa(), db.zalsa_local(), origin.as_id());
            let mro = mro.fetch(db, db.zalsa(), db.zalsa_local(), origin.as_id());
            assert!(flags.is_empty());
            assert_eq!(mro.as_ref().unwrap().len(), 3);
            (flags, mro)
        })
    });
    assert_specialization_shape(&db, &prepared, alias, nested);
    let canonical = infer_expression_types(&db, expression, TypeContext::default());
    assert_eq!(
        canonical.expression_type(expression_key(&prepared)),
        Type::GenericAlias(alias)
    );
    assert_eq!(completed_alias(&db, &prepared), (alias, 0));
    assert_eq!(ARGUMENTS.get(), 0);
    assert!(std::ptr::eq(
        canonical,
        infer_expression_types(&db, expression, TypeContext::default())
    ));
    let events = events_db.take_salsa_events();
    assert!(!expression_ran(&db, expression, &events));
    for query in [
        "infer_definition_types",
        "infer_deferred_types",
        "infer_expression_types_impl",
        "static_class_generic_context",
        "instance_flags_inner",
        "try_mro_unspecialized",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
    if let Some((canonical_flags, canonical_mro)) = classification {
        salsa::attach(&db, || {
            let db = &db as &dyn Db;
            assert!(std::ptr::eq(
                canonical_flags,
                flags.fetch(db, db.zalsa(), db.zalsa_local(), origin.as_id())
            ));
            assert!(std::ptr::eq(
                canonical_mro,
                mro.fetch(db, db.zalsa(), db.zalsa_local(), origin.as_id())
            ));
        });
    }

    let program_file = prepared.program_file();
    let env = ProgramEnvironment::from_file(program_file);
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Expression(expression, TypeContext::default()),
        program_file.file(&db),
        program_file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_expression();
    assert_eq!(canonical, &ordinary);

    let ordinary_db = specialization_fixture(nested);
    let ordinary_prepared = prepare_fixture(&ordinary_db);
    let ordinary = infer_expression_types(
        &ordinary_db,
        selected_expression(&ordinary_prepared),
        TypeContext::default(),
    );
    let Type::GenericAlias(ordinary_alias) =
        ordinary.expression_type(expression_key(&ordinary_prepared))
    else {
        panic!("ordinary fixture generic alias");
    };
    assert_specialization_shape(&ordinary_db, &ordinary_prepared, ordinary_alias, nested);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    assert_eq!(
        Type::GenericAlias(alias).display(&db, &env).to_string(),
        Type::GenericAlias(ordinary_alias)
            .display(&ordinary_db, &ordinary_env)
            .to_string(),
    );
}

#[test]
fn interrupted_specialization_drains_owners_and_reuses_completed_children() {
    interrupted_specialization(false, Interruption::Argument);
}

#[test]
fn interrupted_nested_specialization_reuses_completed_classification() {
    interrupted_specialization(true, Interruption::Argument);
}

#[test]
fn interrupted_nested_c3_keeps_partial_classification_unpublished_and_retries() {
    interrupted_specialization(true, Interruption::C3Append);
}

#[test]
fn interrupted_nested_c3_origin_read_keeps_partial_classification_unpublished_and_retries() {
    interrupted_specialization(true, Interruption::C3Origin);
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Interruption {
    Argument,
    C3Append,
    C3Origin,
}

impl Interruption {
    fn remaining(self) -> Option<usize> {
        match self {
            Self::Argument => LAST_REMAINING.get(),
            Self::C3Append => C3_REMAINING.get(),
            Self::C3Origin => C3_ORIGIN_REMAINING.get(),
        }
    }

    fn arguments(self) -> Option<usize> {
        match self {
            Self::Argument => Some(ARGUMENTS.get()),
            Self::C3Append => C3_ARGUMENTS.get(),
            Self::C3Origin => C3_ORIGIN_ARGUMENTS.get(),
        }
    }
}

fn c3_interruption_boundary(
    db: &TestDb,
    prepared: &PreparedAnalysisFile<'_>,
    boundary: Interruption,
) -> (usize, usize, usize) {
    let revision = salsa::plumbing::current_revision(db);
    for preceding_attempts in 0..4 {
        reset(false);
        let result = expression_type_with_policy(prepared, expression_key(prepared), &funded());
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(Type::GenericAlias(_)))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        ..
                    })
            ),
            "{result:?}"
        );
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(db), revision);
        if let Some(remaining) = boundary.remaining() {
            return (
                preceding_attempts,
                funded().semantic_work_limit - remaining,
                boundary.arguments().unwrap(),
            );
        }
    }
    panic!("nested specialization did not reach a nonempty C3 output");
}

fn interrupted_specialization(nested: bool, boundary: Interruption) {
    let c3 = boundary != Interruption::Argument;
    let measured = specialization_fixture(nested);
    let measured_prepared = prepare_fixture(&measured);
    let arguments = if nested { 2 } else { 1 };
    let (preceding_work_limit_attempts, work, boundary_arguments) = if c3 {
        c3_interruption_boundary(&measured, &measured_prepared, boundary)
    } else {
        let (_, preceding) = completed_alias(&measured, &measured_prepared);
        assert_eq!(ARGUMENTS.get(), arguments);
        let remaining = if nested {
            LAST_REMAINING.get()
        } else {
            REMAINING.get()
        };
        (
            preceding,
            funded().semantic_work_limit - remaining.unwrap(),
            arguments,
        )
    };

    for cancel in [false, true] {
        if cancel && boundary == Interruption::C3Origin {
            continue;
        }
        let db = specialization_fixture(nested);
        let prepared = prepare_fixture(&db);
        let expression = selected_expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        for _ in 0..preceding_work_limit_attempts {
            reset(false);
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                }),
            );
            assert_cleanup();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        events_db.take_salsa_events();
        reset(cancel && !c3);
        CANCEL_C3.set(cancel && boundary == Interruption::C3Append);
        CANCEL_AT.set(arguments);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            ),
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert_eq!(boundary.arguments(), Some(boundary_arguments), "cancel={cancel}");
        if !cancel {
            assert_eq!(ARGUMENTS.get(), boundary_arguments);
        }
        if c3 {
            assert!(boundary.remaining().is_some());
        }
        if boundary == Interruption::C3Origin {
            assert_eq!(C3_ORIGIN_ENTERED.get(), 2);
            assert_eq!(C3_ORIGIN_RETURNED.get(), 1);
            let [first, second] = C3_ORIGIN_ALIASES.get();
            assert!(first.is_some());
            assert_eq!(first, second);
            assert_eq!(C3_REMAINING.get(), None);
        }
        assert!(ACTIVE.get() > 0);
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        let origin = canonical_class(&db, &prepared, "Box");
        let leaf = canonical_class(&db, &prepared, "Leaf");
        if nested {
            let flags = FinalSourceMemo::certify(
                &db as &dyn Db, instance_flags_inner_ingredient(&db), origin.as_id()
            ).map(|_| ());
            let mro = FinalSourceMemo::certify(
                &db as &dyn Db, try_mro_unspecialized_ingredient(&db), origin.as_id()
            ).map(|_| ());
            if c3 && !cancel {
                assert_eq!(flags, Err(FinalSourceError::MissingMemo), "cancel={cancel}");
                assert_eq!(mro, Err(FinalSourceError::MissingMemo), "cancel={cancel}");
            } else {
                // Query-cycle masking defers native cancellation until classification completes.
                assert!(flags.is_ok());
                assert!(mro.is_ok());
            }
        }
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                static_class_generic_context_ingredient(&db),
                origin.as_id()
            )
            .is_ok()
        );
        let events = events_db.take_salsa_events();
        assert!(expression_ran(&db, expression, &events));
        if !cancel {
            assert_eq!(boundary.remaining(), Some(0));
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    expression_inference_ingredient(&db),
                    InferExpression::Bare(expression).as_id()
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
        }
        let (alias, _) = completed_alias(&db, &prepared);
        assert_specialization_shape(&db, &prepared, alias, nested);
        assert_eq!(alias.origin(&db), origin);
        assert_eq!(canonical_class(&db, &prepared, "Leaf"), leaf);
        let events = events_db.take_salsa_events();
        for name in ["Box", "Leaf"] {
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_definition_types",
                Some(class_definition(&prepared, name).as_id()),
                &events,
            );
        }
        assert_function_query_was_not_run_by_name(
            &db,
            "static_class_generic_context",
            Some(origin.as_id()),
            &events,
        );
        if nested && (!c3 || cancel) {
            for query in ["instance_flags_inner", "try_mro_unspecialized"] {
                assert_function_query_was_not_run_by_name(
                    &db, query, Some(origin.as_id()), &events,
                );
            }
        } else if c3 {
            for query in ["instance_flags_inner", "try_mro_unspecialized"] {
                assert!(find_will_execute_event_by_name(
                    &db, query, Some(origin.as_id()), &events,
                ).is_some());
            }
            assert!(FinalSourceMemo::certify(
                &db as &dyn Db, instance_flags_inner_ingredient(&db), origin.as_id()
            ).is_ok());
            assert!(FinalSourceMemo::certify(
                &db as &dyn Db, try_mro_unspecialized_ingredient(&db), origin.as_id()
            ).is_ok());
        }
        if !cancel {
            assert!(expression_ran(&db, expression, &events));
            assert_eq!(ARGUMENTS.get(), arguments);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn reinterned_generic_alias_preserves_its_real_attached_mro_memo() {
    let db = fixture();
    let prepared = prepare_fixture(&db);
    let ordinary =
        infer_expression_types(&db, selected_expression(&prepared), TypeContext::default());
    let Type::GenericAlias(alias) = ordinary.expression_type(expression_key(&prepared)) else {
        panic!("ordinary fixture generic alias");
    };
    let origin = alias.origin(&db);
    let specialization = alias.specialization(&db);
    assert!(alias.try_mro(&db).is_ok());
    let ingredient = generic_alias_try_mro_ingredient(&db);
    let key = ingredient.database_key_index(alias.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, alias.as_id())
            .unwrap()
            .database_key(),
        key
    );
    let canonical = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            alias.as_id(),
        )
    });
    let source_ingredient = source_alias_mro_ingredient(&db);
    let source_key = source_ingredient.database_key_index(alias.as_id());
    let source_canonical = salsa::attach(&db, || {
        source_ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            alias.as_id(),
        )
    });
    assert!(source_canonical.is_ok());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, source_ingredient, alias.as_id())
            .unwrap()
            .database_key(),
        source_key
    );
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let result = with_analysis_session(&prepared, &funded(), |session| {
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let aliases = register_generic_alias_values(session.db(), &mut registry)?;
        registry.seal()?.run(|endpoint| async move {
            Ok(endpoint
                .intern_value(&aliases, (origin, specialization))
                .await)
        })
    });
    assert_eq!(result, Ok(AnalysisOutcome::Complete(alias)));
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, alias.as_id())
            .unwrap()
            .database_key(),
        key
    );
    let reused = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            alias.as_id(),
        )
    });
    assert!(std::ptr::eq(canonical, reused));
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, source_ingredient, alias.as_id())
            .unwrap()
            .database_key(),
        source_key
    );
    let source_reused = salsa::attach(&db, || {
        source_ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            alias.as_id(),
        )
    });
    assert!(std::ptr::eq(source_canonical, source_reused));
    assert!(alias.try_mro(&db).is_ok());
    let events = events_db.take_salsa_events();
    assert!(!events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key || database_key == source_key)
    }));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}
