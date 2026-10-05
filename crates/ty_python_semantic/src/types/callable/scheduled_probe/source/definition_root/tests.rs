use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::{
    assert_function_query_was_not_run_by_name, find_will_execute_event_by_name,
};
use ruff_python_ast::{self as ast, PythonVersion};
use salsa::Database;
use salsa::plumbing::AsId;
use ty_module_resolver::{
    KnownModule, ModuleGlobSet, SearchPathSettings, resolve_module_confident,
};
use ty_python_core::program::{FallibleStrategy, Program};
use ty_python_core::{ProgramFile, TestProgramDb, semantic_index};

use super::*;
use crate::AnalysisSettings;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::Type;
use crate::types::infer::{
    SourceDefinitionEffect, infer_definition_types, scheduled_definition_builder_counts,
    scheduled_definition_live_builders,
};

const ORDERS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];
const COMPLETE: RootPolicy = RootPolicy {
    allowance: 100_000,
    reverse_execution: false,
    reverse_merge: false,
};

fn fixture(sources: &[(&str, &str)]) -> anyhow::Result<TestDb> {
    let mut builder = TestDbBuilder::new().with_python_version(PythonVersion::PY313);
    for (path, source) in sources {
        builder = builder.with_file(*path, source);
    }
    builder.build()
}

fn ring_fixture(length: usize) -> anyhow::Result<TestDb> {
    let sources: Vec<_> = (0..length)
        .map(|index| {
            (
                format!("/src/m{index}.py"),
                format!("from m{} import X\n", (index + 1) % length),
            )
        })
        .collect();
    let borrowed: Vec<_> = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    fixture(&borrowed)
}

fn chain_fixture(length: usize) -> anyhow::Result<TestDb> {
    let sources: Vec<_> = (0..length)
        .map(|index| {
            (
                format!("/src/m{index}.py"),
                if index + 1 == length {
                    "class X: ...\n".to_owned()
                } else {
                    format!("from m{} import X\n", index + 1)
                },
            )
        })
        .collect();
    let borrowed: Vec<_> = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    fixture(&borrowed)
}

fn definition<'db>(db: &'db TestDb, path: &str) -> anyhow::Result<Definition<'db>> {
    let file = db.program_file(system_path_to_file(db, path)?);
    definition_in_file(db, file, None)
}

fn named_definition<'db>(
    db: &'db TestDb,
    path: &str,
    name: &str,
) -> anyhow::Result<Definition<'db>> {
    let file = db.program_file(system_path_to_file(db, path)?);
    definition_in_file(db, file, Some(name))
}

fn known_definition<'db>(
    db: &'db TestDb,
    module: KnownModule,
    name: &str,
) -> anyhow::Result<Definition<'db>> {
    let environment = db.program_environment();
    let module = resolve_module_confident(db, environment.resolver_environment(db), &module.name())
        .ok_or_else(|| anyhow::anyhow!("fixture module did not resolve"))?;
    let file = module
        .file(db)
        .ok_or_else(|| anyhow::anyhow!("fixture module has no source file"))?;
    definition_in_file(db, db.program_file(file), Some(name))
}

fn custom_typeshed_definition<'db>(db: &'db TestDb, path: &str) -> anyhow::Result<Definition<'db>> {
    let search_paths = SearchPathSettings {
        custom_typeshed: Some("/typeshed".into()),
        ..SearchPathSettings::new(vec!["/src".into()])
    }
    .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?;
    search_paths.try_register_static_roots(db);
    let mut settings = db.program_settings().clone();
    settings.search_paths = search_paths;
    let program = Program::from_settings(db, &settings);
    let file = program.program_file(db, system_path_to_file(db, path)?);
    definition_in_file(db, file, None)
}

fn definition_in_file<'db>(
    db: &'db TestDb,
    file: ProgramFile<'db>,
    name: Option<&str>,
) -> anyhow::Result<Definition<'db>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    let definitions = module
        .suite()
        .iter()
        .filter_map(|statement| match statement {
            ast::Stmt::ClassDef(class) if name.is_none_or(|name| class.name.as_str() == name) => {
                Some(index.expect_single_definition(class))
            }
            ast::Stmt::FunctionDef(function)
                if name.is_none_or(|name| function.name.as_str() == name) =>
            {
                Some(index.expect_single_definition(function))
            }
            ast::Stmt::ImportFrom(import) => import
                .names
                .iter()
                .find(|alias| {
                    name.is_none_or(|name| alias.asname.as_ref().unwrap_or(&alias.name) == name)
                })
                .map(|alias| index.expect_single_definition(alias)),
            _ => None,
        });
    // Selecting the final binding lets a fixture exercise its recorded preceding name use.
    let definition = if name.is_some() {
        definitions.last()
    } else {
        definitions.into_iter().next()
    };
    definition.ok_or_else(|| anyhow::anyhow!("fixture has no matching class, function or import"))
}

fn label<'db>(db: &'db TestDb, definition: Definition<'db>) -> String {
    definition.file(db).path(db).to_string()
}

fn binding<'db>(
    db: &'db TestDb,
    owner: Definition<'db>,
    inference: &DefinitionInference<'db>,
) -> String {
    match inference.binding_type(owner) {
        Type::FunctionLiteral(function) => format!("<function '{}'>", function.name(db)),
        ty => ty
            .display(db, &ProgramEnvironment::from_definition(owner))
            .to_string(),
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    root: Result<String, Incomplete>,
    completed: FxHashMap<String, String>,
    boundaries: FxHashMap<String, Boundary>,
    pending: FxHashSet<String>,
    edges: FxHashSet<(String, String)>,
    polls: FxHashMap<String, usize>,
    starts: FxHashMap<String, usize>,
    source_work_polls: usize,
    work: usize,
}

fn observe<'db>(
    db: &'db TestDb,
    owner: Definition<'db>,
    outcome: &DefinitionRootOutcome<'db>,
) -> Observation {
    Observation {
        root: match &outcome.root {
            Completion::Complete(inference) => Ok(binding(db, owner, inference)),
            Completion::Incomplete(reason) => Err(*reason),
        },
        completed: outcome
            .completed
            .iter()
            .map(|(owner, inference)| (label(db, *owner), binding(db, *owner, inference)))
            .collect(),
        boundaries: outcome
            .boundaries
            .iter()
            .map(|(owner, boundary)| (label(db, *owner), *boundary))
            .collect(),
        pending: outcome
            .pending
            .iter()
            .map(|owner| label(db, *owner))
            .collect(),
        edges: outcome
            .pending_edges
            .iter()
            .map(|(parent, child)| (label(db, *parent), label(db, *child)))
            .collect(),
        polls: outcome
            .definition_polls
            .iter()
            .map(|(owner, count)| (label(db, *owner), *count))
            .collect(),
        starts: outcome
            .definition_starts
            .iter()
            .map(|(owner, count)| (label(db, *owner), *count))
            .collect(),
        source_work_polls: outcome.source_work_polls,
        work: outcome.work,
    }
}

fn assert_no_legacy_inference(db: &TestDb, events: &[salsa::Event]) {
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
        assert_function_query_was_not_run_by_name(db, query, None, events);
    }
}

fn evaluate(
    db: &mut TestDb,
    path: &str,
    policy: RootPolicy,
    executes: bool,
) -> anyhow::Result<Observation> {
    let owner = definition(db, path)?;
    Ok(evaluate_owner(db, owner, policy, executes))
}

fn evaluate_owner<'db>(
    db: &'db TestDb,
    owner: Definition<'db>,
    policy: RootPolicy,
    executes: bool,
) -> Observation {
    db.clone().clear_salsa_events();
    let request = DefinitionRootRequest::new(db, owner, policy);
    let observation = observe(db, owner, evaluate_definition_root(db, request));
    let request_id = request.as_id();
    let events = db.clone().take_salsa_events();
    assert_eq!(
        find_will_execute_event_by_name(db, "evaluate_definition_root", Some(request_id), &events)
            .is_some(),
        executes,
        "{events:#?}",
    );
    assert_no_legacy_inference(db, &events);
    assert!(observation.work <= policy.allowance);
    assert_eq!(scheduled_definition_live_builders(), 0);
    observation
}

fn assert_matches_legacy<'db>(
    db: &'db TestDb,
    owner: Definition<'db>,
    policy: RootPolicy,
) -> anyhow::Result<()> {
    db.clone().clear_salsa_events();
    let request = DefinitionRootRequest::new(db, owner, policy);
    let Completion::Complete(inference) = &evaluate_definition_root(db, request).root else {
        anyhow::bail!("definition did not complete");
    };
    assert_no_legacy_inference(db, &db.clone().take_salsa_events());
    assert_eq!(inference.as_ref(), infer_definition_types(db, owner));
    Ok(())
}

#[test]
fn originating_definition_completes_the_real_class_and_matches_legacy() -> anyhow::Result<()> {
    let mut db = fixture(&[("/src/a.py", "class X: ...\n")])?;
    let actual = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(actual.root, Ok("<class 'X'>".to_owned()));
    assert_eq!(actual.completed.len(), 1);
    assert!(actual.pending.is_empty());
    assert!(actual.edges.is_empty());
    assert_eq!(actual.starts.get("/src/a.py"), Some(&1));
    assert!(actual.source_work_polls > 0);

    // The legacy query runs only after the queued result is complete. Equality includes the
    // declaration, deferred regions and diagnostics owned by the finalized inference result.
    let owner = definition(&db, "/src/a.py")?;
    let request = DefinitionRootRequest::new(&db, owner, COMPLETE);
    let Completion::Complete(inference) = &evaluate_definition_root(&db, request).root else {
        anyhow::bail!("class did not complete");
    };
    assert_eq!(inference.as_ref(), infer_definition_types(&db, owner));
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, actual);
    Ok(())
}

#[test]
fn originating_definition_completes_imports_and_matches_the_whole_legacy_result()
-> anyhow::Result<()> {
    for source in [
        "class X: ...\n",
        "def X(value: Unavailable) -> AlsoUnavailable: ...\n",
    ] {
        let mut db = fixture(&[("/src/a.py", "from b import X\n"), ("/src/b.py", source)])?;
        let actual = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
        assert!(actual.root.is_ok(), "{actual:#?}");
        assert_eq!(actual.completed.len(), 2);
        assert!(actual.boundaries.is_empty());
        assert!(actual.pending.is_empty());
        assert!(actual.edges.is_empty());
        assert_eq!(actual.starts.len(), 2);
        assert!(actual.starts.values().all(|starts| *starts == 1));
        assert_matches_legacy(&db, definition(&db, "/src/a.py")?, COMPLETE)?;
        assert_matches_legacy(&db, definition(&db, "/src/b.py")?, COMPLETE)?;
        assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, actual);
    }
    Ok(())
}

#[test]
fn originating_definition_completes_a_function_without_forcing_its_annotations()
-> anyhow::Result<()> {
    let mut db = fixture(&[(
        "/src/a.py",
        "def f(value: Unavailable) -> AlsoUnavailable:\n    return value\n",
    )])?;
    let actual = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert!(actual.root.is_ok(), "{actual:#?}");
    assert_eq!(actual.completed.len(), 1);
    assert!(actual.boundaries.is_empty());
    assert!(actual.pending.is_empty());
    assert!(actual.edges.is_empty());
    assert_eq!(actual.starts.get("/src/a.py"), Some(&1));
    assert_matches_legacy(&db, definition(&db, "/src/a.py")?, COMPLETE)?;
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, actual);
    Ok(())
}

#[test]
fn originating_definition_completes_vendored_type_through_its_imported_decorator()
-> anyhow::Result<()> {
    let db = fixture(&[])?;
    let owner = known_definition(&db, KnownModule::Builtins, "type")?;
    let imported = known_definition(&db, KnownModule::Builtins, "disjoint_base")?;
    let decorator = known_definition(&db, KnownModule::TypingExtensions, "disjoint_base")?;
    assert!(owner.file(&db).path(&db).is_vendored_path());
    assert!(decorator.file(&db).path(&db).is_vendored_path());
    let policy = RootPolicy {
        allowance: 4_000_000,
        ..COMPLETE
    };
    let actual = evaluate_owner(&db, owner, policy, true);
    assert_eq!(actual.root, Ok("<class 'type'>".to_owned()));
    assert!(actual.pending.is_empty());
    assert!(actual.edges.is_empty());
    assert!(actual.boundaries.is_empty());
    let outcome = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, policy));
    for dependency in [owner, imported, decorator] {
        assert_eq!(outcome.definition_starts.get(&dependency), Some(&1));
        assert!(outcome.completed.contains_key(&dependency));
    }
    assert_eq!(outcome.definition_starts.len(), 3);
    assert_eq!(outcome.completed.len(), 3);
    for (reverse_execution, reverse_merge) in ORDERS.into_iter().skip(1) {
        let reordered_policy = RootPolicy {
            reverse_execution,
            reverse_merge,
            ..policy
        };
        assert_eq!(evaluate_owner(&db, owner, reordered_policy, true), actual,);
        assert_eq!(
            evaluate_definition_root(
                &db,
                DefinitionRootRequest::new(&db, owner, reordered_policy)
            ),
            outcome,
        );
    }
    for dependency in [owner, imported, decorator] {
        assert_matches_legacy(&db, dependency, policy)?;
    }
    let warm_policy = RootPolicy {
        allowance: policy.allowance + 1,
        ..policy
    };
    assert_eq!(evaluate_owner(&db, owner, warm_policy, true), actual);
    assert_eq!(
        evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, warm_policy)),
        outcome,
    );
    assert_eq!(evaluate_owner(&db, owner, policy, false), actual);
    Ok(())
}

#[test]
fn originating_definition_shares_one_completed_decorator_between_imports() -> anyhow::Result<()> {
    let policy = RootPolicy {
        allowance: 4_000_000,
        ..COMPLETE
    };
    let db = fixture(&[(
        "/src/a.py",
        "from typing_extensions import disjoint_base as first\n\
         from typing_extensions import disjoint_base as second\n\
         @first\n\
         @second\n\
         class X: ...\n",
    )])?;
    let owner = named_definition(&db, "/src/a.py", "X")?;
    let first = named_definition(&db, "/src/a.py", "first")?;
    let second = named_definition(&db, "/src/a.py", "second")?;
    let decorator = known_definition(&db, KnownModule::TypingExtensions, "disjoint_base")?;
    let actual = evaluate_owner(&db, owner, policy, true);
    assert_eq!(actual.root, Ok("<class 'X'>".to_owned()));
    let outcome = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, policy));
    assert_eq!(outcome.definition_starts.len(), 4);
    assert_eq!(outcome.completed.len(), 4);
    for dependency in [owner, first, second, decorator] {
        assert_eq!(outcome.definition_starts.get(&dependency), Some(&1));
        assert!(outcome.completed.contains_key(&dependency));
    }
    for (reverse_execution, reverse_merge) in ORDERS.into_iter().skip(1) {
        let reordered_policy = RootPolicy {
            reverse_execution,
            reverse_merge,
            ..policy
        };
        let _reordered = evaluate_owner(&db, owner, reordered_policy, true);
        assert_eq!(
            evaluate_definition_root(
                &db,
                DefinitionRootRequest::new(&db, owner, reordered_policy)
            ),
            outcome,
        );
    }
    assert_matches_legacy(&db, owner, policy)?;
    Ok(())
}

#[test]
fn originating_definition_repeated_decorators_keep_subquadratic_work() -> anyhow::Result<()> {
    let policy = RootPolicy {
        allowance: 100_000_000,
        ..COMPLETE
    };
    let mut work = [0; 3];
    for (index, count) in [32, 128, 512].into_iter().enumerate() {
        let source = format!(
            "from typing_extensions import disjoint_base as d\n{}class X: ...\n",
            "@d\n".repeat(count),
        );
        let db = fixture(&[("/src/a.py", &source)])?;
        let owner = named_definition(&db, "/src/a.py", "X")?;
        let imported = named_definition(&db, "/src/a.py", "d")?;
        let decorator = known_definition(&db, KnownModule::TypingExtensions, "disjoint_base")?;
        let actual = evaluate_owner(&db, owner, policy, true);
        assert_eq!(actual.root, Ok("<class 'X'>".to_owned()));
        assert!(actual.boundaries.is_empty());
        assert!(actual.pending.is_empty());
        assert!(actual.edges.is_empty());
        let outcome = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, policy));
        assert_eq!(outcome.definition_starts.len(), 3);
        assert_eq!(outcome.completed.len(), 3);
        for dependency in [owner, imported, decorator] {
            assert_eq!(outcome.definition_starts.get(&dependency), Some(&1));
            assert!(outcome.completed.contains_key(&dependency));
        }
        work[index] = actual.work;
    }

    // Each decorator records a distinct expression while sharing the same imported function.
    // Compare added work to remove the fixed dependency cost. Quadrupling the decorator count
    // permits linear work plus final sorting, but rejects quadratic expression accumulation.
    let [small, medium, large] = work;
    assert!(small < medium && medium < large, "{work:?}");
    assert!(large - medium < 6 * (medium - small), "{work:?}");
    Ok(())
}

#[test]
fn originating_definition_reads_custom_module_type_declarations_and_source_edits()
-> anyhow::Result<()> {
    let mut db = fixture(&[
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "class X: ...\n"),
        ("/typeshed/stdlib/types.pyi", "class ModuleType: ...\n"),
        ("/typeshed/stdlib/VERSIONS", "types: 3.0-\n"),
    ])?;
    let owner = custom_typeshed_definition(&db, "/src/a.py")?;
    let absent = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(absent.root, Ok("<class 'X'>".to_owned()));
    assert_eq!(
        absent.completed.get("/typeshed/stdlib/types.pyi"),
        Some(&"<class 'ModuleType'>".to_owned())
    );
    assert_matches_legacy(&db, owner, COMPLETE)?;

    // An unannotated class binding does not declare an implicit module attribute.
    db.write_file(
        "/typeshed/stdlib/types.pyi",
        "class ModuleType:\n    X = 1\n",
    )?;
    let owner = custom_typeshed_definition(&db, "/src/a.py")?;
    let undeclared = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(undeclared.root, absent.root);
    assert!(undeclared.boundaries.is_empty());
    assert_matches_legacy(&db, owner, COMPLETE)?;

    // The annotation is an actual source dependency. Its unsupported inference cannot be
    // replaced with the successful negative lookup from either previous revision.
    db.write_file(
        "/typeshed/stdlib/types.pyi",
        "class ModuleType:\n    X: int\n",
    )?;
    let owner = custom_typeshed_definition(&db, "/src/a.py")?;
    let declared = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(
        declared.root,
        Err(Incomplete::Source(Boundary::SourceDefinition(
            SourceDefinitionEffect::AnnotatedAssignment,
        ))),
    );
    assert_eq!(
        declared.completed.get("/src/b.py"),
        Some(&"<class 'X'>".to_owned())
    );
    assert!(!declared.completed.contains_key("/src/a.py"));
    assert!(declared.pending.is_empty());
    assert!(declared.edges.is_empty());
    assert_eq!(evaluate_owner(&db, owner, COMPLETE, false), declared);

    db.write_file("/typeshed/stdlib/types.pyi", "class ModuleType: ...\n")?;
    let owner = custom_typeshed_definition(&db, "/src/a.py")?;
    let restored = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(restored, absent);
    Ok(())
}

#[test]
fn originating_definition_does_not_identify_decorators_by_their_name() -> anyhow::Result<()> {
    let mut db = fixture(&[(
        "/src/a.py",
        "def disjoint_base(cls): ...\n@disjoint_base\nclass X: ...\n",
    )])?;
    let owner = named_definition(&db, "/src/a.py", "X")?;
    let actual = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(
        actual.root,
        Err(Incomplete::Source(Boundary::SourceDefinition(
            SourceDefinitionEffect::ClassMetadata,
        ))),
    );
    let function = named_definition(&db, "/src/a.py", "disjoint_base")?;
    let raw = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, COMPLETE));
    assert!(raw.completed.contains_key(&function));
    assert!(!raw.completed.contains_key(&owner));

    db.write_file(
        "/src/a.py",
        "from typing_extensions import disjoint_base as decorate\n@decorate\nclass X: ...\n",
    )?;
    let owner = named_definition(&db, "/src/a.py", "X")?;
    let edited = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(edited.root, Ok("<class 'X'>".to_owned()));
    assert_matches_legacy(&db, owner, COMPLETE)?;
    Ok(())
}

#[test]
fn originating_definition_waits_for_a_functions_preceding_binding() -> anyhow::Result<()> {
    let mut db = fixture(&[("/src/a.py", "f = 1\ndef f(): ...\n")])?;
    let owner = named_definition(&db, "/src/a.py", "f")?;
    let blocked = evaluate_owner(&db, owner, COMPLETE, true);
    assert_eq!(
        blocked.root,
        Err(Incomplete::Source(Boundary::SourceDefinition(
            SourceDefinitionEffect::Assignment,
        ))),
    );
    let raw = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, COMPLETE));
    assert_eq!(raw.definition_starts.len(), 2);
    assert!(raw.completed.is_empty());

    db.write_file("/src/a.py", "class f: ...\ndef f(): ...\n")?;
    let owner = named_definition(&db, "/src/a.py", "f")?;
    let edited = evaluate_owner(&db, owner, COMPLETE, true);
    assert!(edited.root.is_ok(), "{edited:#?}");
    let raw = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, COMPLETE));
    assert_eq!(raw.definition_starts.len(), 2);
    assert_eq!(raw.completed.len(), 2);
    assert_matches_legacy(&db, owner, COMPLETE)?;
    Ok(())
}

#[test]
fn originating_definition_reenters_the_real_import_ring() -> anyhow::Result<()> {
    let mut db = fixture(&[
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "from a import X\n"),
    ])?;
    let outcome = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(outcome.root, Err(Incomplete::DependencyCycle));
    assert!(outcome.completed.is_empty());
    assert!(outcome.boundaries.is_empty());
    assert_eq!(outcome.pending.len(), 2);
    assert_eq!(outcome.starts.len(), 2);
    assert!(outcome.starts.values().all(|starts| *starts == 1));
    assert_eq!(
        outcome.edges,
        FxHashSet::from_iter([
            ("/src/a.py".to_owned(), "/src/b.py".to_owned()),
            ("/src/b.py".to_owned(), "/src/a.py".to_owned()),
        ]),
    );
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, outcome);
    Ok(())
}

fn allowances(work: usize) -> impl Iterator<Item = usize> {
    (0..FIXED_TRANSPORT - 1).chain(
        (FIXED_TRANSPORT..=work)
            .step_by(1 + TRANSPORT_PER_SEMANTIC_UNIT)
            .flat_map(|allowance| [allowance - 1, allowance]),
    )
}

fn check_all_cutoffs(db: &mut TestDb, path: &str) -> anyhow::Result<()> {
    let complete = evaluate(db, path, COMPLETE, true)?;
    for allowance in allowances(complete.work) {
        let policy = RootPolicy {
            allowance,
            ..COMPLETE
        };
        let baseline = evaluate(db, path, policy, true)?;
        if baseline.root.is_err() {
            assert!(baseline.pending.contains(path) || baseline.boundaries.contains_key(path));
        }
        assert!(
            baseline
                .completed
                .iter()
                .all(|(owner, value)| complete.completed.get(owner) == Some(value))
        );
        assert!(baseline.edges.is_subset(&complete.edges));
        assert!(baseline.starts.values().all(|starts| *starts == 1));
        if allowance == complete.work {
            assert_eq!(baseline, complete);
        } else if baseline.root != complete.root {
            assert_eq!(baseline.root, Err(Incomplete::Allowance));
        }
        for (reverse_execution, reverse_merge) in ORDERS.into_iter().skip(1) {
            let reordered = evaluate(
                db,
                path,
                RootPolicy {
                    reverse_execution,
                    reverse_merge,
                    ..policy
                },
                true,
            )?;
            assert_eq!(reordered, baseline, "allowance {allowance}");
        }
    }
    Ok(())
}

#[test]
fn originating_definition_cutoffs_share_one_allowance_in_every_order() -> anyhow::Result<()> {
    let mut class = fixture(&[("/src/a.py", "class X: ...\n")])?;
    check_all_cutoffs(&mut class, "/src/a.py")?;
    let mut function = fixture(&[(
        "/src/a.py",
        "def f(value: Unavailable) -> AlsoUnavailable: ...\n",
    )])?;
    check_all_cutoffs(&mut function, "/src/a.py")?;
    for length in [2, 4] {
        let mut db = ring_fixture(length)?;
        check_all_cutoffs(&mut db, "/src/m0.py")?;
    }
    Ok(())
}

#[test]
fn originating_definition_retains_completed_dependency_at_an_explicit_boundary()
-> anyhow::Result<()> {
    let mut db = fixture(&[
        ("/src/a.py", "X: int\nfrom b import X\n"),
        ("/src/b.py", "class X: ...\n"),
    ])?;
    let outcome = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(
        outcome.root,
        Err(Incomplete::Source(Boundary::SourceDefinition(
            SourceDefinitionEffect::AnnotatedAssignment,
        ))),
    );
    assert_eq!(
        outcome.completed.get("/src/b.py"),
        Some(&"<class 'X'>".to_owned())
    );
    assert!(!outcome.completed.contains_key("/src/a.py"));
    let owner = definition(&db, "/src/a.py")?;
    let raw = evaluate_definition_root(&db, DefinitionRootRequest::new(&db, owner, COMPLETE));
    assert_eq!(raw.definition_starts.len(), 3);
    assert!(outcome.starts.values().all(|starts| *starts == 1));
    assert!(outcome.pending.is_empty());
    assert!(outcome.edges.is_empty());
    Ok(())
}

#[test]
fn originating_definition_complete_and_incomplete_results_observe_source_edits()
-> anyhow::Result<()> {
    let mut db = fixture(&[("/src/a.py", "class X: ...\n")])?;
    let first = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert!(first.root.is_ok());
    db.write_file("/src/a.py", "class Y: ...\n")?;
    let edited = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(edited.root, Ok("<class 'Y'>".to_owned()));

    let mut db = fixture(&[
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "from a import X\n"),
    ])?;
    let cycle = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(cycle.root, Err(Incomplete::DependencyCycle));
    db.write_file("/src/b.py", "class X: ...\n")?;
    let edited = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(edited.root, Ok("<class 'X'>".to_owned()));
    assert!(edited.completed.contains_key("/src/b.py"));
    assert!(edited.edges.is_empty());
    Ok(())
}

#[test]
fn originating_definition_resolution_changes_invalidate_incomplete_results() -> anyhow::Result<()> {
    let mut db = fixture(&[
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "from a import X\n"),
    ])?;
    let cycle = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(cycle.root, Err(Incomplete::DependencyCycle));
    db.write_file("/src/b.pyi", "class X: ...\n")?;
    let stubbed = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(stubbed.root, Ok("<class 'X'>".to_owned()));
    assert!(stubbed.completed.contains_key("/src/b.pyi"));
    assert!(!stubbed.completed.contains_key("/src/b.py"));
    db.write_file("/src/b.py", "class Shadowed: ...\n")?;
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, stubbed);
    Ok(())
}

#[test]
fn originating_definition_warm_legacy_queries_do_not_supply_evidence() -> anyhow::Result<()> {
    for child in ["class X: ...\n", "from a import X\n"] {
        let mut db = fixture(&[("/src/a.py", "from b import X\n"), ("/src/b.py", child)])?;
        let cold = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
        let owner = definition(&db, "/src/a.py")?;
        let _legacy = infer_definition_types(&db, owner);
        let warm = evaluate(
            &mut db,
            "/src/a.py",
            RootPolicy {
                reverse_execution: true,
                ..COMPLETE
            },
            true,
        )?;
        assert_eq!(warm, cold);
    }
    Ok(())
}

#[test]
fn originating_definition_cancellation_discards_the_suspended_builder() -> anyhow::Result<()> {
    let mut db = fixture(&[("/src/a.py", "class X: ...\n")])?;
    let owner = definition(&db, "/src/a.py")?;
    let request = DefinitionRootRequest::new(&db, owner, COMPLETE);
    assert_eq!(scheduled_definition_live_builders(), 0);
    let (created, dropped) = scheduled_definition_builder_counts();
    // The consumer's registration costs one unit; the first definition poll costs two and
    // suspends with its owned builder at the first source-work request.
    CANCEL_NEXT_ROOT.set(Some((3, db.cancellation_token())));
    let cancelled = salsa::Cancelled::catch(|| evaluate_definition_root(&db, request));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert!(CANCEL_NEXT_ROOT.take().is_none());
    assert_eq!(scheduled_definition_live_builders(), 0);
    assert_eq!(
        scheduled_definition_builder_counts(),
        (created + 1, dropped + 1)
    );
    assert_no_legacy_inference(&db, &db.clone().take_salsa_events());

    let retried = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    let mut cold_db = fixture(&[("/src/a.py", "class X: ...\n")])?;
    let cold = evaluate(&mut cold_db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(retried, cold);
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, retried);
    Ok(())
}

#[test]
fn originating_definition_cancellation_discards_a_real_dependency_wait() -> anyhow::Result<()> {
    let sources = [
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "from a import X\n"),
    ];
    let mut probe = fixture(&sources)?;
    let cold = evaluate(&mut probe, "/src/a.py", COMPLETE, true)?;
    let mut suspension = None;
    for allowance in allowances(cold.work) {
        let outcome = evaluate(
            &mut probe,
            "/src/a.py",
            RootPolicy {
                allowance,
                ..COMPLETE
            },
            true,
        )?;
        if outcome
            .edges
            .contains(&("/src/a.py".to_owned(), "/src/b.py".to_owned()))
            && outcome.starts.len() == 2
            && outcome.polls.len() == 1
        {
            suspension = Some((outcome.work - FIXED_TRANSPORT) / (1 + TRANSPORT_PER_SEMANTIC_UNIT));
            break;
        }
    }
    let suspension = suspension.ok_or_else(|| anyhow::anyhow!("no suspended dependency demand"))?;
    let mut db = fixture(&sources)?;
    let owner = definition(&db, "/src/a.py")?;
    let request = DefinitionRootRequest::new(&db, owner, COMPLETE);
    let (created, dropped) = scheduled_definition_builder_counts();
    CANCEL_NEXT_ROOT.set(Some((suspension, db.cancellation_token())));
    let cancelled = salsa::Cancelled::catch(|| evaluate_definition_root(&db, request));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert!(CANCEL_NEXT_ROOT.take().is_none());
    assert_eq!(scheduled_definition_live_builders(), 0);
    assert_eq!(
        scheduled_definition_builder_counts(),
        (created + 1, dropped + 1)
    );
    assert_no_legacy_inference(&db, &db.clone().take_salsa_events());

    let retried = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(retried, cold);
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, retried);
    Ok(())
}

#[test]
fn originating_definition_long_source_rings_keep_linear_work_and_single_producers()
-> anyhow::Result<()> {
    let policy = RootPolicy {
        allowance: 4_000_000,
        ..COMPLETE
    };
    let mut small_db = ring_fixture(2)?;
    let small = evaluate(&mut small_db, "/src/m0.py", policy, true)?;
    assert_eq!(small.root, Err(Incomplete::DependencyCycle));
    for length in [16, 64, 128] {
        let mut db = ring_fixture(length)?;
        let outcome = evaluate(&mut db, "/src/m0.py", policy, true)?;
        assert_eq!(outcome.root, Err(Incomplete::DependencyCycle));
        assert!(outcome.completed.is_empty());
        assert!(outcome.boundaries.is_empty());
        assert_eq!(outcome.pending.len(), length);
        assert_eq!(outcome.edges.len(), length);
        assert_eq!(outcome.starts.len(), length);
        assert!(outcome.starts.values().all(|starts| *starts == 1));
        // Longer module names add source work. After accounting for node count, work stays
        // within a factor of two of the two-module fixture despite the greater dependency depth.
        assert!(outcome.work <= small.work * length);
        assert!(outcome.work * 4 >= small.work * length);
    }
    Ok(())
}

#[test]
fn originating_definition_completed_import_chains_keep_proportional_work() -> anyhow::Result<()> {
    let policy = RootPolicy {
        allowance: 4_000_000,
        ..COMPLETE
    };
    let mut small_db = chain_fixture(2)?;
    let small = evaluate(&mut small_db, "/src/m0.py", policy, true)?;
    assert_eq!(small.root, Ok("<class 'X'>".to_owned()));
    for length in [4, 16, 64] {
        let mut db = chain_fixture(length)?;
        let outcome = evaluate(&mut db, "/src/m0.py", policy, true)?;
        assert_eq!(outcome.root, small.root);
        assert_eq!(outcome.completed.len(), length);
        assert_eq!(outcome.starts.len(), length);
        assert!(outcome.starts.values().all(|starts| *starts == 1));
        assert!(outcome.boundaries.is_empty());
        assert!(outcome.pending.is_empty());
        assert!(outcome.edges.is_empty());
        // Each module adds one import transaction, including its real ModuleType lookup.
        assert!(outcome.work <= small.work * length);
        assert!(outcome.work * 4 >= small.work * length);
        assert_eq!(evaluate(&mut db, "/src/m0.py", policy, false)?, outcome);
    }
    Ok(())
}

#[test]
fn originating_definition_completed_chain_cutoffs_and_orders_preserve_committed_dependencies()
-> anyhow::Result<()> {
    let mut db = chain_fixture(3)?;
    let complete = evaluate(&mut db, "/src/m0.py", COMPLETE, true)?;
    assert_eq!(complete.root, Ok("<class 'X'>".to_owned()));
    for allowance in [
        0,
        FIXED_TRANSPORT,
        complete.work / 2,
        complete.work - 1,
        complete.work,
    ] {
        let policy = RootPolicy {
            allowance,
            ..COMPLETE
        };
        let baseline = evaluate(&mut db, "/src/m0.py", policy, true)?;
        assert!(
            baseline
                .completed
                .iter()
                .all(|(owner, value)| complete.completed.get(owner) == Some(value))
        );
        assert!(baseline.starts.values().all(|starts| *starts == 1));
        assert!(baseline.boundaries.is_empty());
        if allowance == complete.work {
            assert_eq!(baseline, complete);
        } else if baseline.root.is_err() {
            assert_eq!(baseline.root, Err(Incomplete::Allowance));
            assert!(!baseline.completed.contains_key("/src/m0.py"));
        } else {
            assert_eq!(baseline.root, complete.root);
            assert_eq!(baseline.completed, complete.completed);
        }
        for (reverse_execution, reverse_merge) in ORDERS.into_iter().skip(1) {
            assert_eq!(
                evaluate(
                    &mut db,
                    "/src/m0.py",
                    RootPolicy {
                        reverse_execution,
                        reverse_merge,
                        ..policy
                    },
                    true
                )?,
                baseline,
            );
        }
    }
    Ok(())
}

#[test]
fn originating_definition_cancellation_after_a_dependency_completes_retries_from_a_cold_session()
-> anyhow::Result<()> {
    let sources = [
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "class X: ...\n"),
    ];
    let mut probe = fixture(&sources)?;
    let cold = evaluate(&mut probe, "/src/a.py", COMPLETE, true)?;
    let mut suspension = None;
    for allowance in allowances(cold.work) {
        let outcome = evaluate(
            &mut probe,
            "/src/a.py",
            RootPolicy {
                allowance,
                ..COMPLETE
            },
            true,
        )?;
        if outcome.completed.contains_key("/src/b.py") && outcome.pending.contains("/src/a.py") {
            suspension = Some((outcome.work - FIXED_TRANSPORT) / (1 + TRANSPORT_PER_SEMANTIC_UNIT));
            break;
        }
    }
    let suspension = suspension
        .ok_or_else(|| anyhow::anyhow!("no completed dependency before root publication"))?;
    let mut db = fixture(&sources)?;
    let owner = definition(&db, "/src/a.py")?;
    let request = DefinitionRootRequest::new(&db, owner, COMPLETE);
    let (created, dropped) = scheduled_definition_builder_counts();
    CANCEL_NEXT_ROOT.set(Some((suspension, db.cancellation_token())));
    let cancelled = salsa::Cancelled::catch(|| evaluate_definition_root(&db, request));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert!(CANCEL_NEXT_ROOT.take().is_none());
    assert_eq!(scheduled_definition_live_builders(), 0);
    assert_eq!(
        scheduled_definition_builder_counts(),
        (created + 2, dropped + 2)
    );
    assert_no_legacy_inference(&db, &db.clone().take_salsa_events());

    let retried = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(retried, cold);
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, retried);
    Ok(())
}

#[test]
fn originating_definition_import_policy_changes_invalidate_the_root() -> anyhow::Result<()> {
    let mut db = fixture(&[
        ("/src/a.py", "from b import X\n"),
        ("/src/b.py", "from a import X\n"),
    ])?;
    let original = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(original.root, Err(Incomplete::DependencyCycle));
    assert_eq!(original.starts.len(), 2);
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, original);

    // Any configured pattern stops at the policy boundary before matching or resolving the
    // imported module. This includes a pattern that would not match the imported module.
    for pattern in ["b", "unrelated"] {
        db.set_analysis_settings(AnalysisSettings {
            replace_imports_with_any: ModuleGlobSet::from_patterns([pattern])?,
            ..AnalysisSettings::default()
        });
        let configured = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
        assert_eq!(
            configured.root,
            Err(Incomplete::Source(Boundary::SourceDefinition(
                SourceDefinitionEffect::ImportPolicy,
            ))),
        );
        assert!(configured.completed.is_empty());
        assert!(configured.pending.is_empty());
        assert!(configured.edges.is_empty());
        assert_eq!(configured.boundaries.len(), 1);
        assert_eq!(configured.starts.len(), 1);
        assert_eq!(configured.starts.get("/src/a.py"), Some(&1));
        assert_eq!(configured.polls.len(), 1);
        assert!(configured.work < original.work);
        assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, configured);
    }

    db.set_analysis_settings(AnalysisSettings::default());
    let restored = evaluate(&mut db, "/src/a.py", COMPLETE, true)?;
    assert_eq!(restored, original);
    assert_eq!(evaluate(&mut db, "/src/a.py", COMPLETE, false)?, restored);
    Ok(())
}
