use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use rustc_hash::FxHashSet;
use ty_python_core::platform::PythonPlatform;
use ty_python_core::semantic_index;

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::callable::scheduled_probe::run_with;
use crate::types::infer::type_parameter_header::TypeParameterHeader;
use crate::types::{KnownInstanceType, Type, inferred_declaration};

const ORDERS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];

fn prepare_functions<'db>(
    index: &SemanticIndex<'db>,
    statements: &[ast::Stmt],
) -> anyhow::Result<(PreparedSources<'db>, Vec<Definition<'db>>)> {
    let mut sources = PreparedSources::default();
    let mut owners = Vec::new();
    for statement in statements {
        let ast::Stmt::FunctionDef(function) = statement else {
            anyhow::bail!("expected a function declaration");
        };
        let owner = index.expect_single_definition(function);
        if let Some(parameters) = &function.type_params {
            sources.insert_type_params(index, owner, parameters);
        } else {
            sources.generic_contexts.insert(owner, Box::default());
        }
        owners.push(owner);
    }
    Ok((sources, owners))
}

fn assert_no_inference(db: &TestDb) {
    let events = db.clone().take_salsa_events();
    for query in [
        "infer_implicit_alias_type",
        "infer_definition_types",
        "infer_deferred_types",
        "infer_function_default_types",
        "infer_scope_types_impl",
        "infer_expression_types_impl",
        "infer_expression_type_impl",
        "infer_statement_types_impl",
        "infer_unpack_types",
        "infer_protocol_variance",
        "function_known_decorators",
    ] {
        assert_function_query_was_not_run_by_name(db, query, None, &events);
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Observation<'db> {
    consumer: Option<Result<GenericContext<'db>, Boundary>>,
    headers: FxHashMap<Definition<'db>, Result<TypeParameterHeader<'db>, Boundary>>,
    header_pending: FxHashSet<Definition<'db>>,
    contexts: FxHashMap<Definition<'db>, Result<GenericContext<'db>, Boundary>>,
    context_pending: FxHashSet<Definition<'db>>,
    header_polls: FxHashMap<Definition<'db>, usize>,
    context_polls: FxHashMap<Definition<'db>, usize>,
    consumer_polls: usize,
    work: usize,
    boundaries: Vec<usize>,
}

fn observe_context<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    sources: &PreparedSources<'db>,
    owner: Definition<'db>,
    budget: usize,
    order: (bool, bool),
) -> anyhow::Result<Observation<'db>> {
    let router = Router::with_sources(sources.clone());
    let snapshot = run_with(db, env, &router, budget, order.0, order.1, |router| async {
        router.consumer_generic_context_demand(owner).await
    })
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    Ok(Observation {
        consumer: snapshot
            .consumer
            .map(|answer| answer.and_then(|fact| fact.value(&router))),
        headers: snapshot
            .graph
            .header_values
            .into_iter()
            .map(|(key, answer)| (key, answer.and_then(|fact| fact.value(&router))))
            .collect(),
        header_pending: snapshot.graph.header_pending,
        contexts: snapshot
            .graph
            .generic_context_values
            .into_iter()
            .map(|(key, answer)| (key, answer.and_then(|fact| fact.value(&router))))
            .collect(),
        context_pending: snapshot.graph.generic_context_pending,
        header_polls: snapshot.header_polls,
        context_polls: snapshot.generic_context_polls,
        consumer_polls: snapshot.consumer_polls,
        work: snapshot.graph.work,
        boundaries: snapshot.graph.boundaries,
    })
}

#[test]
fn queued_headers_and_contexts_match_ordinary_declarations_without_evaluating_annotations()
-> anyhow::Result<()> {
    let mut db = setup_db();
    let source = "\
def mixed[Z, A, *Ts, **P](): ...
def bounded[T: MissingBound[tuple[int, ...]]](): ...
def constrained[T: (MissingA[int], MissingB[str])](): ...
def defaulted[T = MissingDefault[list[int]]](): ...
def paramspec[**P = [Missing[int]]](): ...
def typevartuple[*Ts = *tuple[Missing[int], ...]](): ...
def invalid_empty[T: ()](): ...
def invalid_single[T: (Missing,) = MissingDefault[int]](): ...
";
    db.write_file("/src/queued_headers.py", source)?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_headers.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let observations = owners
        .iter()
        .map(|owner| observe_context(&db, &env, &sources, *owner, 10_000, ORDERS[0]))
        .collect::<anyhow::Result<Vec<_>>>()?;
    assert_no_inference(&db);

    let mut invalid_constraint_ranges = Vec::new();
    for ((owner, statement), observation) in owners.iter().zip(module.suite()).zip(observations) {
        let ast::Stmt::FunctionDef(function) = statement else {
            anyhow::bail!("expected a function declaration");
        };
        let parameters = function
            .type_params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("expected type parameters"))?;
        let context = observation
            .consumer
            .ok_or_else(|| anyhow::anyhow!("generic context did not finish"))?
            .map_err(|boundary| anyhow::anyhow!("generic context failed: {boundary:?}"))?;
        let definitions = &sources.generic_contexts[owner];
        let mut expected_variables = Vec::new();
        for (definition, parameter) in definitions.iter().zip(parameters.iter()) {
            let expected = infer_type_parameter_header(&db, *definition, parameter.into());
            assert_eq!(observation.headers[definition], Ok(expected));
            if let Some(range) = expected.invalid_constraint_count {
                invalid_constraint_ranges.push(range);
            }
            assert_eq!(observation.header_polls[definition], 1);
            assert_eq!(
                expected.variable.identity(&db).definition(&db),
                Some(*definition),
            );
            expected_variables.push(expected.variable.with_binding_context(&db, *owner));
            assert_eq!(
                inferred_declaration(&db, *definition)
                    .declared()
                    .map(|declared| declared.inner_type()),
                Some(Type::KnownInstance(KnownInstanceType::TypeVar(
                    expected.variable
                ))),
            );
        }
        assert_eq!(
            context.variables(&db).collect::<Vec<_>>(),
            expected_variables
        );
        assert_eq!(
            context,
            GenericContext::from_type_params(&db, index, *owner, parameters),
        );
        assert!(observation.header_pending.is_empty());
        assert!(observation.context_pending.is_empty());
    }
    assert_eq!(
        invalid_constraint_ranges
            .iter()
            .map(|range| &source[*range])
            .collect::<Vec<_>>(),
        ["()", "(Missing,)"],
    );
    let diagnostics = crate::types::check_types(&db, file);
    let mut diagnostic_ranges = Vec::new();
    for diagnostic in diagnostics.iter().filter(|diagnostic| {
        diagnostic
            .id()
            .is_lint_named("invalid-type-variable-constraints")
    }) {
        assert_eq!(
            diagnostic.headline_message(),
            "TypeVar must have at least two constrained types",
        );
        let span = diagnostic
            .primary_span()
            .ok_or_else(|| anyhow::anyhow!("constraint diagnostic has no primary span"))?;
        assert_eq!(
            span.file(),
            &ruff_db::diagnostic::UnifiedFile::Ty(file.file(&db)),
        );
        diagnostic_ranges.push(
            span.range()
                .ok_or_else(|| anyhow::anyhow!("constraint diagnostic has no source range"))?,
        );
    }
    diagnostic_ranges.sort_unstable_by_key(|range| range.start());
    assert_eq!(diagnostic_ranges, invalid_constraint_ranges);
    Ok(())
}

#[test]
fn every_allowance_and_round_order_is_independent_of_legacy_inference_warmth() -> anyhow::Result<()>
{
    let mut db = setup_db();
    db.write_file(
        "/src/queued_warmth.py",
        "def generic[Z: MissingBound[int], A = MissingDefault[str], *Ts, **P](): ...\n",
    )?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_warmth.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let owner = owners[0];
    let complete = observe_context(&db, &env, &sources, owner, 10_000, ORDERS[0])?;
    assert!(matches!(complete.consumer, Some(Ok(_))));
    let baseline = (0..=complete.work)
        .map(|budget| observe_context(&db, &env, &sources, owner, budget, ORDERS[0]))
        .collect::<anyhow::Result<Vec<_>>>()?;
    assert_no_inference(&db);

    for warmth in 0..3 {
        if warmth == 1 {
            inferred_declaration(&db, sources.generic_contexts[&owner][0]);
        } else if warmth == 2 {
            for definition in &sources.generic_contexts[&owner] {
                inferred_declaration(&db, *definition);
            }
        }
        db.clone().take_salsa_events();
        for (budget, expected) in baseline.iter().enumerate() {
            for order in ORDERS {
                let actual = observe_context(&db, &env, &sources, owner, budget, order)?;
                assert_eq!(
                    &actual, expected,
                    "warmth={warmth}, budget={budget}, order={order:?}"
                );
                assert!(actual.work <= budget);
                if budget < complete.work {
                    assert!(actual.consumer.is_none());
                }
            }
        }
        assert_no_inference(&db);
    }
    Ok(())
}

#[test]
fn missing_early_or_later_headers_preserve_independent_siblings_at_every_allowance()
-> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file("/src/queued_missing.py", "def generic[Z, A, B](): ...\n")?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_missing.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let owner = owners[0];
    let definitions = &sources.generic_contexts[&owner];
    for missing in [definitions[0], definitions[2]] {
        let mut incomplete = sources.clone();
        incomplete.headers.remove(&missing);
        let complete = observe_context(&db, &env, &incomplete, owner, 10_000, ORDERS[0])?;
        assert_eq!(complete.consumer, Some(Err(Boundary::SourcePreparation)));
        assert_eq!(complete.contexts[&owner], Err(Boundary::SourcePreparation));
        for definition in definitions {
            if *definition == missing {
                assert_eq!(
                    complete.headers[definition],
                    Err(Boundary::SourcePreparation)
                );
            } else {
                assert!(complete.headers[definition].is_ok());
            }
            assert_eq!(complete.header_polls[definition], 1);
        }
        for budget in 0..=complete.work {
            let expected = observe_context(&db, &env, &incomplete, owner, budget, ORDERS[0])?;
            for order in ORDERS {
                let actual = observe_context(&db, &env, &incomplete, owner, budget, order)?;
                assert_eq!(
                    actual, expected,
                    "missing={missing:?}, budget={budget}, order={order:?}"
                );
                assert!(!actual.contexts.values().any(Result::is_ok));
            }
        }
    }
    let unprepared = observe_context(
        &db,
        &env,
        &PreparedSources::default(),
        owner,
        10_000,
        ORDERS[0],
    )?;
    assert_eq!(unprepared.consumer, Some(Err(Boundary::SourcePreparation)));
    assert!(unprepared.headers.is_empty());
    assert_no_inference(&db);
    Ok(())
}

#[test]
fn repeated_demands_share_published_facts_and_reject_support_from_another_root()
-> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file("/src/queued_support.py", "def generic[T](): ...\n")?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_support.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let owner = owners[0];
    let definition = sources.generic_contexts[&owner][0];
    let mut counts = Vec::new();
    let mut facts = Vec::new();
    let routers = [
        Router::with_sources(sources.clone()),
        Router::with_sources(sources),
    ];
    for (router, repetitions) in routers.iter().zip([1, 3]) {
        let unpublished = evaluate_header(&db, &env, router, definition)
            .map_err(|boundary| anyhow::anyhow!("header failed: {boundary:?}"))?;
        assert_eq!(unpublished.value(router), Err(Boundary::SourceSupport));
        let snapshot = run_with(&db, &env, router, 10_000, false, false, |router| async {
            let first_header = router.consumer_header_demand(definition).await;
            let first_context = router.consumer_generic_context_demand(owner).await;
            for _ in 1..repetitions {
                assert_eq!(
                    router.consumer_header_demand(definition).await,
                    first_header
                );
                assert_eq!(
                    router.consumer_generic_context_demand(owner).await,
                    first_context
                );
            }
            (first_header, first_context)
        })
        .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
        let (header, context) = snapshot
            .consumer
            .ok_or_else(|| anyhow::anyhow!("consumer did not finish"))?;
        let header = header.map_err(|boundary| anyhow::anyhow!("header failed: {boundary:?}"))?;
        let context =
            context.map_err(|boundary| anyhow::anyhow!("context failed: {boundary:?}"))?;
        assert!(header.value(router).is_ok());
        assert!(context.value(router).is_ok());
        assert_eq!(snapshot.header_polls[&definition], 1);
        assert_eq!(router.headers.borrow().len(), 1);
        assert_eq!(router.generic_contexts.borrow().len(), 1);
        counts.push((snapshot.header_polls, snapshot.generic_context_polls));
        facts.push((header, context));
    }
    assert_eq!(counts[0], counts[1]);
    assert_eq!(facts[0].0.value(&routers[1]), Err(Boundary::SourceSupport));
    assert_eq!(facts[0].1.value(&routers[1]), Err(Boundary::SourceSupport));
    assert_eq!(facts[1].0.value(&routers[0]), Err(Boundary::SourceSupport));
    assert_eq!(facts[1].1.value(&routers[0]), Err(Boundary::SourceSupport));
    assert_no_inference(&db);
    Ok(())
}

#[test]
fn source_payload_debit_grows_with_names_and_parameter_counts() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file(
        "/src/queued_debit.py",
        "def short[T](): ...\ndef long[LongParameterName](): ...\ndef wide[T, U, V](): ...\ndef empty(): ...\n",
    )?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_debit.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let mut work = Vec::new();
    let mut context_debits = Vec::new();
    let mut header_debits = Vec::new();
    for owner in owners {
        let router = Router::with_sources(sources.clone());
        let snapshot = run_with(&db, &env, &router, 10_000, false, false, |router| async {
            router.consumer_generic_context_demand(owner).await
        })
        .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
        let context = snapshot
            .consumer
            .ok_or_else(|| anyhow::anyhow!("consumer did not finish"))?
            .map_err(|boundary| anyhow::anyhow!("context failed: {boundary:?}"))?;
        let value = context
            .value(&router)
            .map_err(|boundary| anyhow::anyhow!("invalid context support: {boundary:?}"))?;
        assert_eq!(
            value.variables(&db).len(),
            sources.generic_contexts[&owner].len()
        );
        context_debits.push(context.support.logical_debit);
        let debit = router
            .headers
            .borrow()
            .values()
            .try_fold(0, |total, entry| {
                let answer = entry
                    .answer
                    .ok_or_else(|| anyhow::anyhow!("header did not finish"))?;
                let fact =
                    answer.map_err(|boundary| anyhow::anyhow!("header failed: {boundary:?}"))?;
                Ok::<_, anyhow::Error>(total + fact.support.logical_debit)
            })?;
        header_debits.push(debit);
        work.push(snapshot.graph.work);
    }
    assert_eq!(context_debits[0], context_debits[1]);
    assert_eq!(
        header_debits[1] - header_debits[0],
        "LongParameterName".len() - "T".len()
    );
    assert_eq!(work[1] - work[0], header_debits[1] - header_debits[0]);
    assert!(context_debits[2] > context_debits[0]);
    assert!(header_debits[2] > header_debits[0]);
    assert!(work[2] > work[0]);
    assert!(context_debits[3] > 0);
    assert_eq!(header_debits[3], 0);
    assert!(work[3] < work[0]);
    assert_no_inference(&db);
    Ok(())
}

#[test]
fn source_requests_reject_a_different_program() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file("/src/queued_program.py", "def generic[T](): ...\n")?;
    let file = db.program_file(system_path_to_file(&db, "/src/queued_program.py")?);
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let index = semantic_index(&db, file);
    let env = db.program_environment();
    let (sources, owners) = prepare_functions(index, module.suite())?;
    let owner = owners[0];
    let definition = sources.generic_contexts[&owner][0];
    let other_program =
        crate::Program::new(&db, &PythonPlatform::All, env.resolver_environment(&db));
    assert_ne!(other_program, env.program(&db));
    let other_env = ProgramEnvironment::from_program(other_program);
    let router = Router::with_sources(sources);
    let snapshot = run_with(
        &db,
        &other_env,
        &router,
        10_000,
        false,
        false,
        |router| async {
            (
                router.consumer_header_demand(definition).await,
                router.consumer_generic_context_demand(owner).await,
            )
        },
    )
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    assert_eq!(
        snapshot.consumer,
        Some((Err(Boundary::ProgramDomain), Err(Boundary::ProgramDomain)))
    );
    assert_eq!(snapshot.header_polls[&definition], 1);
    assert_eq!(snapshot.generic_context_polls[&owner], 1);
    assert_no_inference(&db);
    Ok(())
}
