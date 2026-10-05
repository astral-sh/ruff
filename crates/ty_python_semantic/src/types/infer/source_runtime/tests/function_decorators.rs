use std::cell::RefCell;
use std::panic::AssertUnwindSafe;

use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::types::function::{FunctionDecorators, FunctionType, OverloadLiteral};
use crate::types::infer::{
    FunctionDecoratorInference, InferenceFlags, function_decorator_inference_ingredient,
    function_known_decorators, infer_deferred_types,
};

const CALLS: &str = "from typing import Any\ndef first(value: Any) -> Any: ...\ndef second(value: Any) -> Any: ...\n@first(True)\n@second(False)\ndef target(): ...\n";

#[derive(Clone, Copy)]
enum Cancellation {
    None,
    Decorator,
}

#[derive(Clone, Debug)]
struct Completed {
    range: TextRange,
    flags: InferenceFlags,
    remaining: usize,
    live: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IngestedResults {
    expressions: usize,
    bindings: usize,
    called: usize,
    aliases: usize,
    flags: FunctionDecorators,
    unknown: bool,
    diagnostics: usize,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    completed: Vec<Completed>,
    finalizing: Option<(usize, usize)>,
    retired: Vec<(usize, usize)>,
    cancellation_check_returned: bool,
    ingested_results: Vec<IngestedResults>,
}

thread_local! {
    static TARGET: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CANCEL: Cell<Cancellation> = const { Cell::new(Cancellation::None) };
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

struct Recording;

impl Recording {
    fn start(_db: &dyn Db, definition: Definition<'_>, cancel: Cancellation) -> Self {
        assert!(TARGET.replace(Some(definition.as_id())).is_none());
        CANCEL.set(cancel);
        JOURNAL.with_borrow_mut(|journal| *journal = Snapshot::default());
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        TARGET.set(None);
        CANCEL.set(Cancellation::None);
    }
}

pub(in crate::types::infer) struct OwnerLifetime {
    baseline: Option<usize>,
}

impl OwnerLifetime {
    pub(in crate::types::infer) fn new(definition: Definition<'_>) -> Self {
        Self {
            baseline: (TARGET.get() == Some(definition.as_id())).then(|| observations::counts().0),
        }
    }
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        if let Some(baseline) = self.baseline {
            JOURNAL.with_borrow_mut(|journal| {
                journal.retired.push((baseline, observations::counts().0));
            });
        }
    }
}

pub(in crate::types::infer) fn decorator_completed(
    db: &dyn Db,
    definition: Definition<'_>,
    range: TextRange,
    flags: InferenceFlags,
) {
    if TARGET.get() != Some(definition.as_id()) {
        return;
    }
    JOURNAL.with_borrow_mut(|journal| {
        journal.completed.push(Completed {
            range,
            flags,
            remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
            live: observations::counts().0,
        });
    });
    if matches!(CANCEL.get(), Cancellation::Decorator) {
        CANCEL.set(Cancellation::None);
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
        JOURNAL.with_borrow_mut(|journal| journal.cancellation_check_returned = true);
    }
}

pub(in crate::types::infer) fn finalizing(db: &dyn Db, definition: Definition<'_>) {
    if TARGET.get() == Some(definition.as_id()) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.finalizing = Some((
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap(),
                observations::counts().0,
            ));
        });
    }
}

pub(in crate::types::infer) fn ingestion_results(inference: &FunctionDecoratorInference<'_>) {
    if TARGET.get().is_some() {
        JOURNAL.with_borrow_mut(|journal| {
            journal.ingested_results.push(IngestedResults {
                expressions: inference.expression_types().count(),
                bindings: inference.bindings().len(),
                called: inference.called_functions().len(),
                aliases: inference.implicit_aliases().len(),
                flags: inference.known_decorators(),
                unknown: inference.has_unknown_decorators(),
                diagnostics: inference.diagnostics().into_iter().len(),
            });
        });
    }
}

fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

pub(super) fn database(source: &str) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file("src/main.pyi", source).unwrap();
    db
}

pub(super) fn prepared(db: &TestDb) -> PreparedAnalysisFile<'_> {
    prepare_file(db, system_path_to_file(db, "src/main.pyi").unwrap()).unwrap()
}

pub(super) fn selected_function<'a>(
    prepared: &'a PreparedAnalysisFile<'_>,
) -> &'a ast::StmtFunctionDef {
    match prepared.parsed_module().syntax().body.last().unwrap() {
        Stmt::FunctionDef(function) => function,
        Stmt::ClassDef(class) => class.body.last().unwrap().as_function_def_stmt().unwrap(),
        _ => panic!("fixture function"),
    }
}

pub(super) fn definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    match prepared.parsed_module().syntax().body.last().unwrap() {
        Stmt::Assign(assignment) => assignment_definition(prepared, assignment),
        _ => prepared
            .semantic_index()
            .expect_single_definition(selected_function(prepared)),
    }
}

#[derive(Clone, Copy)]
pub(super) enum Request<'a, 'db> {
    Decorators,
    Deferred,
    Definition,
    Overloads,
    Metadata,
    RuntimeVisibility,
    Signature,
    Binding(&'a DefinitionInference<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Value<'db> {
    Decorators(&'db FunctionDecoratorInference<'db>),
    Definition(&'db DefinitionInference<'db>),
    Overloads(
        FunctionType<'db>,
        &'db (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
    ),
    Metadata(
        FunctionType<'db>,
        &'db [OverloadLiteral<'db>],
        Option<OverloadLiteral<'db>>,
    ),
    RuntimeVisibility(bool),
    Signature(FunctionType<'db>, &'db CallableSignature<'db>),
    Binding(Type<'db>),
}

pub(super) fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    request: Request<'_, 'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Value<'db>>, AnalysisFailure> {
    let definition = definition(prepared);
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
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            match request {
                Request::Decorators => access
                    .function_known_decorators(definition)
                    .await
                    .map(Value::Decorators),
                Request::Deferred => access
                    .deferred_definition(definition)
                    .await
                    .map(Value::Definition),
                Request::Definition => access.definition(definition).await.map(Value::Definition),
                Request::RuntimeVisibility => access.runtime_visibility(definition).await.map(Value::RuntimeVisibility),
                Request::Binding(inference) => {
                    let effects = SourceEffects::new(&access, access.routes.program);
                    crate::types::runtime_visibility::RuntimeVisibilityEffects::binding_type(
                        &effects, inference, definition,
                    ).await.map(Value::Binding)
                }
                Request::Overloads | Request::Metadata | Request::Signature => {
                    let inference = access.definition(definition).await?;
                    let function = access.endpoint.local_call(|| {
                        access.endpoint.admit_work(2)?;
                        access.endpoint.check_completion()?;
                        inference.function_type(definition)
                            .ok_or(RunError::Contract("overload fixture is not a function"))
                    }).await;
                    let literal = access.endpoint.read_field(
                        function.field_requests(session.db()).literal(),
                        &BorrowOrCopy,
                    ).await;
                    let last = access.endpoint.local_call(|| {
                        access.endpoint.admit_work(2)?;
                        access.endpoint.check_completion()?;
                        super::overload_collection::select(literal.last_definition);
                        Ok(literal.last_definition)
                    }).await;
                    if matches!(request, Request::Signature) {
                        access.function_signature(function).await
                            .map(|value| Value::Signature(function, value))
                    } else if matches!(request, Request::Metadata) {
                        let effects = SourceEffects::new(&access, access.routes.program);
                        let (overloads, implementation) = function.overloads_and_implementation_with(session.db(), &effects).await?;
                        Ok(Value::Metadata(function, overloads, implementation))
                    } else {
                        access.function_overloads(last).await
                            .map(|value| Value::Overloads(function, value))
                    }
                }
            }
        })
    })
}

#[derive(Debug, Eq, PartialEq)]
struct Payload {
    expressions: Vec<String>,
    bindings: usize,
    called: Vec<String>,
    aliases: usize,
    flags: FunctionDecorators,
    unknown: bool,
    diagnostics: usize,
}

fn payload<'db>(db: &'db TestDb, value: &FunctionDecoratorInference<'db>) -> Payload {
    let env = db.program_environment();
    Payload {
        expressions: value
            .expression_types()
            .map(|(_, ty)| ty.display(db, &env).to_string())
            .collect(),
        bindings: value.bindings().len(),
        called: value
            .called_functions()
            .iter()
            .map(|function| function.name(db).to_string())
            .collect(),
        aliases: value.implicit_aliases().len(),
        flags: value.known_decorators(),
        unknown: value.has_unknown_decorators(),
        diagnostics: value.diagnostics().into_iter().len(),
    }
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert!(
        snapshot()
            .retired
            .iter()
            .all(|(before, after)| before == after)
    );
}

#[test]
fn cold_decorator_queries_preserve_results_and_reuse_the_canonical_memo() {
    for (source, flags, unknown, calls) in [
        ("def target(): ...\n", FunctionDecorators::empty(), false, 0),
        (
            "@True\n@False\ndef target(): ...\n",
            FunctionDecorators::empty(),
            true,
            0,
        ),
        (
            "from typing import no_type_check, overload\n@no_type_check\n@overload\ndef target(): ...\n",
            FunctionDecorators::NO_TYPE_CHECK | FunctionDecorators::OVERLOAD,
            false,
            0,
        ),
        (
            "@staticmethod\n@classmethod\ndef target(): ...\n",
            FunctionDecorators::STATICMETHOD | FunctionDecorators::CLASSMETHOD,
            false,
            0,
        ),
        (
            "@property\ndef target(): ...\n",
            FunctionDecorators::empty(),
            false,
            0,
        ),
        (
            "class C:\n    @property\n    def target(self): ...\n",
            FunctionDecorators::empty(),
            false,
            0,
        ),
        (CALLS, FunctionDecorators::empty(), true, 2),
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(&db, definition, Cancellation::None);
        let cold = capture(&db, || {
            controlled(&prepared, Request::Decorators, &funded())
        })
        .unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Decorators(inference))) = cold.value else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        let actual = payload(&db, inference);
        assert_eq!(actual.flags, flags, "{source}");
        assert_eq!(actual.unknown, unknown, "{source}");
        assert_eq!(actual.called.len(), calls, "{source}");
        assert_eq!(actual.bindings, 0);
        assert_eq!(actual.aliases, 0);
        assert_eq!(actual.diagnostics, 0);
        let journal = snapshot();
        assert_eq!(
            journal.completed.len(),
            selected_function(&prepared).decorator_list.len()
        );
        assert_eq!(
            journal
                .completed
                .iter()
                .map(|entry| entry.range)
                .collect::<Vec<_>>(),
            selected_function(&prepared)
                .decorator_list
                .iter()
                .map(|decorator| decorator.expression.range())
                .collect::<Vec<_>>()
        );
        assert!(journal.completed.iter().all(|entry| entry.live > 0));
        assert!(journal.finalizing.is_some_and(|(_, live)| live > 0));
        if flags.contains(FunctionDecorators::NO_TYPE_CHECK) {
            assert!(
                !journal.completed[0]
                    .flags
                    .contains(InferenceFlags::IN_NO_TYPE_CHECK)
            );
            assert!(
                journal.completed[1]
                    .flags
                    .contains(InferenceFlags::IN_NO_TYPE_CHECK)
            );
        }
        assert_cleanup();

        let ordinary_db = database(source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        let ordinary =
            function_known_decorators(&ordinary_db, self::definition(&ordinary_prepared));
        assert_eq!(actual, payload(&ordinary_db, ordinary), "{source}");
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                function_decorator_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert!(std::ptr::eq(
            inference,
            function_known_decorators(&db, definition)
        ));
        assert_eq!(
            controlled(&prepared, Request::Decorators, &funded()),
            cold.value
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "function_known_decorators",
            Some(definition.as_id()),
            &events_db.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn non_function_decorator_query_preserves_the_ordinary_default() {
    let db = database("target = True\n");
    let prepared = prepared(&db);
    observations::reset(None);
    let result = controlled(&prepared, Request::Decorators, &funded());
    let Ok(AnalysisOutcome::Complete(Value::Decorators(value))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(
        payload(&db, value),
        Payload {
            expressions: Vec::new(),
            bindings: 0,
            called: Vec::new(),
            aliases: 0,
            flags: FunctionDecorators::empty(),
            unknown: true,
            diagnostics: 0,
        }
    );
    let ordinary_db = database("target = True\n");
    let ordinary_prepared = self::prepared(&ordinary_db);
    assert_eq!(
        payload(&db, value),
        payload(
            &ordinary_db,
            function_known_decorators(&ordinary_db, definition(&ordinary_prepared))
        )
    );
    assert_cleanup();
}

#[test]
fn deferred_annotations_read_the_same_decorator_query_for_functions_and_receivers() {
    // These fixtures exercise the decorator query used for function annotation flags
    // and method receiver classification, with names from local and enclosing scopes.
    for source in [
        "from typing import Any, no_type_check\n@no_type_check\ndef target(value: Any) -> Any: ...\n",
        "class C:\n    from builtins import staticmethod\n    from typing import Any\n    @staticmethod\n    def target(value: Any) -> Any: ...\n",
        "class C:\n    from builtins import classmethod\n    from typing import Any\n    @classmethod\n    def target(cls: Any, value: Any) -> Any: ...\n",
        "from typing import Any\nclass C:\n    @staticmethod\n    def target(value: Any) -> Any: ...\n",
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        observations::reset(None);
        let cold = capture(&db, || controlled(&prepared, Request::Deferred, &funded())).unwrap();
        let Ok(AnalysisOutcome::Complete(Value::Definition(value))) = cold.value else {
            panic!("{source}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        let decorator_key =
            function_decorator_inference_ingredient(&db).database_key_index(definition.as_id());
        let deferred_key =
            deferred_definition_inference_ingredient(&db).database_key_index(definition.as_id());
        assert!(
            cold.reads
                .iter()
                .any(|read| read.key == decorator_key && read.parent == Some(deferred_key))
        );
        let ordinary_db = database(source);
        let ordinary_prepared = self::prepared(&ordinary_db);
        let ordinary = infer_deferred_types(&ordinary_db, self::definition(&ordinary_prepared));
        let function = selected_function(&prepared);
        let ordinary_function = selected_function(&ordinary_prepared);
        for (parameter, ordinary_parameter) in function
            .parameters
            .iter_non_variadic_params()
            .zip(ordinary_function.parameters.iter_non_variadic_params())
        {
            let annotation = parameter.annotation().unwrap();
            let ordinary_annotation = ordinary_parameter.annotation().unwrap();
            assert_eq!(
                value.expression_type(annotation),
                ordinary.expression_type(ordinary_annotation)
            );
            assert_eq!(
                value.type_expression_flags(annotation),
                ordinary.type_expression_flags(ordinary_annotation)
            );
        }
        assert_eq!(
            value.expression_type(function.returns.as_deref().unwrap()),
            Type::any()
        );
        assert_cleanup();
    }
}

#[test]
fn decorator_expression_and_later_definition_refusals_remain_precise() {
    for (source, request, operation) in [
        (
            "@missing\ndef target(): ...\n",
            Request::Decorators,
            OperationId::UnresolvedReference,
        ),
        (
            "@(seen := True)\ndef target(): ...\n",
            Request::Decorators,
            OperationId::ExpressionKind,
        ),
        (
            "@True\ndef target(): ...\n",
            Request::Definition,
            OperationId::CallBindings,
        ),
    ] {
        let db = database(source);
        let prepared = prepared(&db);
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..2 {
            observations::reset(None);
            assert_eq!(
                controlled(&prepared, request, &funded()),
                Ok(unavailable(operation)),
                "{source}"
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_cleanup();
        }
        if matches!(request, Request::Definition) {
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    function_decorator_inference_ingredient(&db),
                    definition(&prepared).as_id(),
                )
                .is_ok()
            );
        }
    }
}

#[test]
fn recursive_decorator_query_ingests_the_cycle_seed_before_later_definition_refusal() {
    let db = database("def make() -> target: ...\n@make()\ndef target(): ...\n");
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        let recording = Recording::start(&db, definition, Cancellation::None);
        let ingestion_recording = super::function_decorator_ingestion::Recording::start(definition, false);
        let result = controlled(&prepared, Request::Decorators, &funded());
        drop(recording);
        drop(ingestion_recording);
        assert_eq!(result, Ok(unavailable(OperationId::CallBindings)));
        // The recursive definition merges the initial empty decorator result, then
        // refuses the unknown fallback's transformation before publishing a definition.
        assert_eq!(
            snapshot().ingested_results,
            vec![IngestedResults {
                expressions: 0,
                bindings: 0,
                called: 0,
                aliases: 0,
                flags: FunctionDecorators::empty(),
                unknown: true,
                diagnostics: 0,
            }]
        );
        super::function_decorator_ingestion::assert_empty_cycle_seed_was_ingested();
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                function_decorator_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_err()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn decorator_work_refusal_and_cancellation_retire_the_owner_before_same_revision_retry() {
    let measured = database(CALLS);
    let measured_prepared = prepared(&measured);
    observations::reset(None);
    let recording = Recording::start(
        &measured,
        definition(&measured_prepared),
        Cancellation::None,
    );
    assert!(matches!(
        controlled(&measured_prepared, Request::Decorators, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Decorators(_)))
    ));
    drop(recording);
    let journal = snapshot();
    assert_eq!(journal.completed.len(), 2);
    let after_first = funded().semantic_work_limit - journal.completed[0].remaining;
    let before_finalization = funded().semantic_work_limit - journal.finalizing.unwrap().0;
    assert_cleanup();

    for (limit, cancel) in [
        (after_first, false),
        (before_finalization, false),
        (funded().semantic_work_limit, true),
    ] {
        let db = database(CALLS);
        let prepared = prepared(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(
            &db,
            definition,
            if cancel {
                Cancellation::Decorator
            } else {
                Cancellation::None
            },
        );
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                Request::Decorators,
                &AnalysisPolicy {
                    semantic_work_limit: limit,
                    ..funded()
                },
            )
        }));
        drop(recording);
        if cancel {
            // Fixpoint queries mask Local cancellation. With both decorators in prepared
            // source, the query finishes and retains its memo before cancellation propagates.
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            assert!(snapshot().cancellation_check_returned);
            assert_eq!(snapshot().completed.len(), 2);
            assert!(snapshot().finalizing.is_some());
        } else {
            assert!(
                matches!(
                    result,
                    Ok(Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    }))
                ),
                "{result:?}"
            );
        }
        assert!(!snapshot().completed.is_empty());
        assert!(!snapshot().retired.is_empty());
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                function_decorator_inference_ingredient(&db),
                definition.as_id()
            )
            .is_ok(),
            cancel,
        );
        assert_cleanup();
        assert!(matches!(
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                salsa::prepared_source_probe::try_with_preparation(&db, || ())
            })),
            Ok(Ok(()))
        ));
        assert!(matches!(
            controlled(&prepared, Request::Decorators, &funded()),
            Ok(AnalysisOutcome::Complete(Value::Decorators(_)))
        ));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn cancellation_in_the_next_cold_import_preserves_the_outer_decorator_owner() {
    // Preparing an unprepared import parks semantic queries and unmasks Local cancellation.
    // The request after the first decorator therefore interrupts the still-active owner.
    let mut db = database(
        "from typing import Any\nfrom cold import second\ndef first(value: Any) -> Any: ...\n@first(True)\n@second(False)\ndef target(): ...\n",
    );
    db.write_file(
        "src/cold.pyi",
        "from typing import Any\ndef second(value: Any) -> Any: ...\n",
    )
    .unwrap();
    let prepared = prepared(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(&db, definition, Cancellation::Decorator);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Decorators, &funded())
    }));
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let journal = snapshot();
    assert_eq!(journal.completed.len(), 1);
    assert!(journal.completed[0].live > 0);
    assert!(journal.cancellation_check_returned);
    assert!(journal.finalizing.is_none());
    assert!(!journal.retired.is_empty());
    assert_cleanup();
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_decorator_inference_ingredient(&db),
            definition.as_id()
        )
        .is_err()
    );
    assert!(matches!(
        controlled(&prepared, Request::Decorators, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Decorators(_)))
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}
