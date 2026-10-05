use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;

use super::*;
use crate::db::tests::TestDbBuilder;
use crate::types::callable::scheduled_probe::{Boundary, ConsumerSnapshot, Router, run_with};
use crate::types::callable::{CallableConversionRequest, CallableTypes, UpcastPolicy};
use crate::types::{InternedType, KnownBoundMethodType, KnownClass};

mod c3;
mod mapping;
mod member_lookup;
mod mro_collection;
mod mro_construction;
mod mro_facts;
mod mro_tasks;
mod prepared_mro;
mod promotion;
mod public_promotion;
mod synthesized_member;

const CASES: [&str; 4] = [
    "Constructor stopping types introduced by forwarding",
    "Constructor stopping types supplied through inherited aliases",
    "Constructor stopping types changed by later forwarding",
    "Alternative constructor paths with different stopping types",
];

const PEP695: &str =
    include_str!("../../../../../../resources/mdtest/generics/pep695/callables.md");
const LEGACY: &str =
    include_str!("../../../../../../resources/mdtest/generics/legacy/callables.md");

fn code(markdown: &str, heading: &str) -> anyhow::Result<String> {
    let (_, section) = markdown
        .split_once(&format!("## {heading}\n"))
        .ok_or_else(|| anyhow::anyhow!("missing section {heading}"))?;
    let section = section.split("\n## ").next().unwrap_or(section);
    let (_, code) = section
        .split_once("```py\n")
        .ok_or_else(|| anyhow::anyhow!("missing code for {heading}"))?;
    Ok(code.split("\n```").next().unwrap_or(code).to_owned())
}

fn class_named<'db>(prepared: &PreparedDeclarations<'db>, name: &str) -> StaticClassLiteral<'db> {
    let ty = prepared.globals[name].value.place.expect_type();
    let Type::ClassLiteral(ClassLiteral::Static(class)) = ty else {
        panic!("expected a static class: {name}");
    };
    class
}

#[test]
fn ordinary_and_owner_specialization_do_not_read_variance() -> anyhow::Result<()> {
    for source in [
        "class Box[T]:\n    value: T\nclass Owner[T]:\n    def method(self, value: Box[T]) -> Box[int]: ...\n",
        "from typing import Generic\nfrom typing_extensions import TypeVar\nT = TypeVar('T', infer_variance=True)\nclass Box(Generic[T]):\n    value: T\nclass Owner(Generic[T]):\n    def method(self, value: Box[T]) -> Box[int]: ...\n",
    ] {
        for specialize_self_domain in [false, true] {
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/mapping.py", source)?;
            let file = system_path_to_file(&db, "/src/mapping.py")?;
            let env = db.program_environment();
            let file = ProgramFile::new(&db, file, env.program(&db));
            let prepared = PreparedDeclarations::prepare(&db, file).unwrap();
            let owner = class_named(&prepared, "Owner");
            let method = prepared
                .namespace(owner, "method")
                .unwrap()
                .ignore_possibly_undefined()
                .unwrap()
                .as_function_literal()
                .unwrap();
            let signature = &prepared.signature(method).unwrap().overloads[0];
            let input = signature
                .parameters()
                .get_positional(1)
                .unwrap()
                .annotated_type();
            let expected = signature.return_ty;
            let specialization = prepared.classes[&owner]
                .context
                .value
                .unwrap()
                .specialize(&db, [KnownClass::Int.to_instance(&db, &env)].as_slice());
            let observed = probe::capture(&db, || {
                input.apply_specialization_impl(&db, specialization, specialize_self_domain)
            })
            .unwrap();
            assert_eq!(observed.value, expected);
            let variance_reads: Vec<_> = observed
                .reads
                .iter()
                .filter_map(|read| {
                    let name =
                        salsa::Database::ingredient_debug_name(&db, read.key.ingredient_index());
                    name.contains("variance").then_some(name)
                })
                .collect();
            assert!(
                variance_reads.is_empty(),
                "unnecessary variance reads: {variance_reads:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn prepared_declarations_original_forwarding_cases() -> anyhow::Result<()> {
    for (syntax, markdown) in [("pep695", PEP695), ("legacy", LEGACY)] {
        for heading in CASES {
            let source = code(markdown, heading)?;
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", &source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let env = db.program_environment();
            let file = ProgramFile::new(&db, file, env.program(&db));
            let started = Instant::now();
            let prepared = PreparedDeclarations::prepare(&db, file)
                .map_err(|error| anyhow::anyhow!("{syntax} {heading}: {error:?}"))?;
            let cold = started.elapsed();
            let initializer = class_named(&prepared, "Initializer");
            let method = prepared
                .namespace(initializer, "__get__")
                .unwrap()
                .ignore_possibly_undefined()
                .unwrap()
                .as_function_literal()
                .unwrap();
            let signature = prepared.signature(method).unwrap();
            assert_eq!(signature.overloads.len(), 2);
            assert!(
                prepared
                    .namespace(initializer, "__delete__")
                    .unwrap()
                    .is_undefined()
            );
            for facts in prepared.classes.values() {
                let proper_mro = facts.proper_mro.as_ref().unwrap();
                assert!(!proper_mro.value.is_empty());
                assert!(!facts.context.support.is_empty());
                assert!(!proper_mro.support.is_empty());
            }
            let started = Instant::now();
            let warm = PreparedDeclarations::prepare(&db, file)
                .map_err(|error| anyhow::anyhow!("{syntax} {heading} warm: {error:?}"))?;
            assert_eq!(warm.classes.len(), prepared.classes.len());
            assert_eq!(warm.signature(method).unwrap(), signature);
            eprintln!(
                "{syntax} {heading}: preparation cold={cold:?} warm={:?} classes={} signatures={}",
                started.elapsed(),
                prepared.classes.len(),
                prepared.signatures.len()
            );
        }
    }
    Ok(())
}

#[test]
fn prepared_declarations_missing_facts_never_reenter_queries() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/forwarding.py", code(PEP695, CASES[2])?)?;
    let file = system_path_to_file(&db, "/src/forwarding.py")?;
    let env = db.program_environment();
    let file = ProgramFile::new(&db, file, env.program(&db));
    let mut prepared =
        PreparedDeclarations::prepare(&db, file).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let initializer = class_named(&prepared, "Initializer");
    let function = prepared
        .namespace(initializer, "__get__")
        .unwrap()
        .ignore_possibly_undefined()
        .unwrap()
        .as_function_literal()
        .unwrap();
    prepared
        .classes
        .get_mut(&initializer)
        .unwrap()
        .namespace
        .remove("__get__");
    prepared.signatures.remove(&function);

    for _ in 0..2 {
        let missing = probe::capture(&db, || {
            assert_eq!(
                prepared.namespace(initializer, "__get__").unwrap_err(),
                MissingDeclaration(DeclarationKey::Namespace(initializer, Name::new("__get__")))
            );
            assert_eq!(
                prepared.signature(function).unwrap_err(),
                MissingDeclaration(DeclarationKey::Signature(function))
            );
            assert!(
                prepared
                    .namespace(initializer, "__delete__")
                    .unwrap()
                    .is_undefined()
            );
        })
        .unwrap();
        assert!(missing.reads.is_empty(), "a capsule lookup entered Salsa");
    }
    Ok(())
}

fn queued<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Option<Rc<PreparedDeclarations<'db>>>,
    request: CallableConversionRequest<'db>,
    budget: usize,
) -> ConsumerSnapshot<'db, Result<Option<CallableTypes<'db>>, Boundary>> {
    let router = prepared.map_or_else(Router::default, |prepared| {
        Router::with_declarations(db, env, prepared).unwrap()
    });
    let observed = probe::capture(db, || {
        run_with(
            db,
            env,
            &router,
            budget,
            false,
            false,
            |router| async move { router.consumer_demand(request).await },
        )
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "queued conversion entered source queries: {:?}",
        observed.reads
    );
    observed.value.unwrap()
}

#[test]
fn prepared_function_conversion_preserves_forwarding_overloads_and_retries() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        for heading in CASES {
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", code(markdown, heading)?)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let env = db.program_environment();
            let file = ProgramFile::new(&db, file, env.program(&db));
            let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
            let initializer = class_named(&prepared, "Initializer");
            let ty = prepared
                .namespace(initializer, "__get__")
                .unwrap()
                .ignore_possibly_undefined()
                .unwrap();
            let request = CallableConversionRequest::new(ty, UpcastPolicy::Unsound);
            let expected = request.evaluate(&db, &env, None);
            let full = queued(&db, &env, Some(Rc::clone(&prepared)), request, 1_000);
            assert_eq!(full.consumer, Some(Ok(expected.clone())));
            assert_eq!(full.graph.values.get(&request), Some(&Ok(expected.clone())));
            assert!(full.graph.pending.is_empty());

            let short = queued(&db, &env, Some(Rc::clone(&prepared)), request, 1);
            assert!(short.consumer.is_none());
            assert!(!short.graph.values.contains_key(&request));
            assert!(short.graph.exhausted);
            for _ in 0..2 {
                let retry = queued(
                    &db,
                    &env,
                    Some(Rc::clone(&prepared)),
                    request,
                    full.graph.work,
                );
                assert_eq!(retry.consumer, Some(Ok(expected.clone())));
                assert_eq!(retry.graph.work, full.graph.work);
            }

            // An implementation definition is not contained in the public overload signatures.
            // Queued conversion rejects that source demand before overload discovery runs.
            let function = ty.as_function_literal().unwrap();
            let recursive = CallableConversionRequest {
                recursive_definition: Some(function.last_definition(&db)),
                ..request
            };
            assert_eq!(
                queued(&db, &env, Some(prepared), recursive, 1_000).consumer,
                Some(Err(Boundary::SourceSignature))
            );
            assert_eq!(
                recursive.evaluate(&db, &env, None),
                Some(CallableTypes::one(CallableType::bottom(&db)))
            );
        }
    }
    Ok(())
}

#[test]
fn prepared_function_conversion_missing_fact_and_foreign_database() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/functions.py",
        "def callback(value: int) -> str: ...\n",
    )?;
    let file = system_path_to_file(&db, "/src/functions.py")?;
    let env = db.program_environment();
    let file = ProgramFile::new(&db, file, env.program(&db));
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let ty = prepared.globals["callback"].value.place.expect_type();
    let function = ty.as_function_literal().unwrap();
    let request = CallableConversionRequest::new(ty, UpcastPolicy::Unsound);
    prepared.signatures.remove(&function);
    let prepared = Rc::new(prepared);
    for capsule in [None, Some(Rc::clone(&prepared)), Some(Rc::clone(&prepared))] {
        let result = queued(&db, &env, capsule, request, 1_000);
        assert_eq!(result.consumer, Some(Err(Boundary::SourceSignature)));
        assert_eq!(
            result.graph.values.get(&request),
            Some(&Err(Boundary::SourceSignature))
        );
    }

    let foreign = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    let foreign_env = foreign.program_environment();
    assert!(matches!(
        Router::with_declarations(&foreign, &foreign_env, Rc::clone(&prepared)),
        Err(Boundary::SourceSupport)
    ));
    let router = Router::with_declarations(&db, &env, prepared).unwrap();
    let factory_ran = Cell::new(false);
    let result = run_with(&foreign, &foreign_env, &router, 1_000, false, false, |_| {
        factory_ran.set(true);
        async {}
    });
    assert!(matches!(result, Err(Boundary::SourceSupport)));
    assert!(!factory_ran.get());
    Ok(())
}

#[test]
fn prepared_function_conversion_preserves_method_kinds() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/methods.py", "class Methods:\n    def ordinary(self, value: int) -> int: ...\n    @classmethod\n    def class_method(cls, value: int) -> int: ...\n    @staticmethod\n    def static_method(value: int) -> int: ...\n")?;
    let file = system_path_to_file(&db, "/src/methods.py")?;
    let env = db.program_environment();
    let file = ProgramFile::new(&db, file, env.program(&db));
    let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
    let class = class_named(&prepared, "Methods");
    for name in ["ordinary", "class_method", "static_method"] {
        let ty = prepared
            .namespace(class, name)
            .unwrap()
            .ignore_possibly_undefined()
            .unwrap();
        assert!(ty.is_function_literal());
        let request = CallableConversionRequest::new(ty, UpcastPolicy::Unsound);
        let expected = request.evaluate(&db, &env, None);
        assert_eq!(
            queued(&db, &env, Some(Rc::clone(&prepared)), request, 1_000).consumer,
            Some(Ok(expected))
        );
    }
    Ok(())
}

#[test]
fn prepared_function_conversion_width_is_paid_only_when_rehashing() -> anyhow::Result<()> {
    let mut measurements = Vec::new();
    for width in [2, 64] {
        let mut source = "from typing import overload, Literal, Any\n".to_owned();
        for index in 0..width {
            source.push_str(&format!(
                "@overload\ndef callback(long_parameter_name: Literal[{index}]) -> int: ...\n"
            ));
        }
        source.push_str("def callback(long_parameter_name: Any) -> Any: ...\n");
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/wide.py", source)?;
        let file = system_path_to_file(&db, "/src/wide.py")?;
        let env = db.program_environment();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let ty = prepared.globals["callback"].value.place.expect_type();
        let request = CallableConversionRequest::new(ty, UpcastPolicy::Unsound);
        let plain = queued(&db, &env, Some(Rc::clone(&prepared)), request, 100_000);
        assert!(matches!(plain.consumer, Some(Ok(Some(_)))));
        let wrapped_request = CallableConversionRequest::new(
            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(InternedType::new(&db, ty))),
            UpcastPolicy::Unsound,
        );
        let wrapped = queued(
            &db,
            &env,
            Some(Rc::clone(&prepared)),
            wrapped_request,
            100_000,
        );
        assert_eq!(
            wrapped.consumer,
            Some(Ok(wrapped_request.evaluate(&db, &env, None)))
        );
        if let Some((_, narrow_work)) = measurements.first() {
            let short = queued(&db, &env, Some(prepared), wrapped_request, *narrow_work);
            assert!(short.consumer.is_none());
            assert!(!short.graph.values.contains_key(&wrapped_request));
            assert!(short.graph.exhausted);
        }
        measurements.push((plain.graph.work, wrapped.graph.work));
    }
    assert_eq!(measurements[0].0, measurements[1].0);
    assert!(measurements[1].1 > measurements[0].1);
    eprintln!("prepared function width (plain, rehash): {measurements:?}");
    Ok(())
}

#[test]
fn prepared_function_conversion_reprepares_after_an_edit() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    let mut previous_stamp = None;
    for return_type in ["int", "str"] {
        db.write_file(
            "/src/edited.py",
            format!("def callback(value: int) -> {return_type}: ...\n"),
        )?;
        let file = system_path_to_file(&db, "/src/edited.py")?;
        let env = db.program_environment();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        if let Some(previous) = previous_stamp {
            assert_ne!(prepared.stamp, previous);
        }
        previous_stamp = Some(prepared.stamp);
        let ty = prepared.globals["callback"].value.place.expect_type();
        let request = CallableConversionRequest::new(ty, UpcastPolicy::Unsound);
        let completed = queued(&db, &env, Some(prepared), request, 1_000)
            .consumer
            .unwrap()
            .unwrap()
            .unwrap();
        let signatures: Vec<_> = completed.signatures(&db).collect();
        assert_eq!(signatures.len(), 1);
        assert_eq!(
            signatures[0].return_ty.display(&db, &env).to_string(),
            return_type
        );
    }
    Ok(())
}
