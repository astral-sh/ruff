use std::panic::AssertUnwindSafe;

use salsa::Database as _;

use super::*;
use crate::db::tests::TestDb;
use crate::types::callable::scheduled_probe::member_lookup::{
    LookupFailure, LookupOperation, static_synthesized_member,
};
use crate::types::class::CodeGeneratorKind;
use crate::types::class::own_member::OwnMemberLookupRequest;

fn fixture() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_third_party_packages()
        .with_file(
            "/.venv/lib/python3.13/site-packages/pydantic/__init__.pyi",
            "from .main import BaseModel as BaseModel\n",
        )
        .with_file(
            "/.venv/lib/python3.13/site-packages/pydantic/main.pyi",
            r#"
from typing import dataclass_transform

@dataclass_transform(kw_only_default=True)
class ModelMetaclass(type): ...

class BaseModel(metaclass=ModelMetaclass): ...
"#,
        )
        .with_file(
            "/src/synthesis.py",
            r#"
from dataclasses import dataclass
from functools import total_ordering
from typing import NamedTuple, TypedDict
from pydantic import BaseModel

class Plain: ...

@total_ordering
class Ordered:
    def __lt__(self, other: object) -> bool:
        return False

@dataclass
class Data:
    value: int

class Named(NamedTuple):
    value: int

class Typed(TypedDict):
    value: int

class Model(BaseModel):
    value: int
"#,
        )
        .build()
}

fn prepare<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>) -> PreparedDeclarations<'db> {
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/synthesis.py").unwrap(),
        env.program(db),
    );
    PreparedDeclarations::prepare(db, file).unwrap()
}

fn synthesis_request<'a, 'db>(
    prepared: &PreparedDeclarations<'db>,
    class: &str,
    name: &'a str,
) -> OwnMemberLookupRequest<'a, 'db> {
    OwnMemberLookupRequest {
        class: class_named(prepared, class),
        name,
        inherited_generic_context: None,
        specialization: None,
    }
}

fn run_synthesis<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    request: OwnMemberLookupRequest<'_, 'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Result<Option<Type<'db>>, LookupFailure<'db>>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            static_synthesized_member(db, env, router, request)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "synthesis entered source queries: {:?}",
        observed.reads
    );
    assert!(!router.consumer_active.get());
    observed.value
}

#[test]
fn prepared_synthesis_returns_canonical_absence_for_non_generators() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    assert!(class_named(&prepared, "Ordered").total_ordering(&db));

    for (class, name) in [
        ("Plain", "__init__"),
        ("Plain", "__delete__"),
        ("Plain", "not_generated"),
        ("Ordered", "__init__"),
    ] {
        let request = synthesis_request(&prepared, class, name);
        let full = run_synthesis(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            10_000,
            (false, false),
        );
        assert_eq!(full.consumer, Some(Ok(None)), "{class}.{name}");
        let ordinary = request
            .class
            .own_synthesized_member(&db, &env, None, None, name);
        assert_eq!(full.consumer, Some(Ok(ordinary)), "{class}.{name}");
    }
    Ok(())
}

#[test]
fn prepared_synthesis_reports_the_selected_opaque_dependency() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));

    for (class, name, operation) in [
        ("Ordered", "__gt__", LookupOperation::TotalOrderingMember),
        (
            "Plain",
            "__setattr__",
            LookupOperation::FrozenDataclassSubclassMember,
        ),
        (
            "Plain",
            "__delattr__",
            LookupOperation::FrozenDataclassSubclassMember,
        ),
    ] {
        let full = run_synthesis(
            &db,
            &env,
            Rc::clone(&prepared),
            synthesis_request(&prepared, class, name),
            10_000,
            (false, false),
        );
        assert_eq!(
            full.consumer,
            Some(Err(LookupFailure::Unsupported(operation))),
            "{class}.{name}"
        );
    }

    for class_name in ["Data", "Named", "Typed", "Model"] {
        let class = class_named(&prepared, class_name);
        assert!(matches!(
            (class_name, prepared.code_generator(class).unwrap()),
            ("Data", Some(CodeGeneratorKind::DataclassLike(_)))
                | ("Named", Some(CodeGeneratorKind::NamedTuple))
                | ("Typed", Some(CodeGeneratorKind::TypedDict))
                | ("Model", Some(CodeGeneratorKind::Pydantic(_)))
        ));
        for name in ["__init__", "not_generated"] {
            let full = run_synthesis(
                &db,
                &env,
                Rc::clone(&prepared),
                synthesis_request(&prepared, class_name, name),
                10_000,
                (false, false),
            );
            assert_eq!(
                full.consumer,
                Some(Err(LookupFailure::Unsupported(
                    LookupOperation::GeneratedMember
                ))),
                "{class_name}.{name}"
            );
            if name == "not_generated" {
                // The canonical generated body runs its prelude even when its final dispatch
                // returns no member, so the prepared request still reaches that dependency.
                assert_eq!(
                    class.own_synthesized_member(&db, &env, None, None, name),
                    None,
                    "{class_name}.{name}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn prepared_synthesis_preserves_missing_generator_facts_and_earlier_guards() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    for (class, name, earlier_operation) in [
        ("Plain", "__init__", None),
        ("Ordered", "__init__", None),
        (
            "Ordered",
            "__gt__",
            Some(LookupOperation::TotalOrderingMember),
        ),
        (
            "Plain",
            "__setattr__",
            Some(LookupOperation::FrozenDataclassSubclassMember),
        ),
    ] {
        let mut prepared = prepare(&db, &env);
        let request = synthesis_request(&prepared, class, name);
        prepared.classes.remove(&request.class);
        let full = run_synthesis(
            &db,
            &env,
            Rc::new(prepared),
            request,
            10_000,
            (false, false),
        );
        let expected = earlier_operation.map_or_else(
            || {
                LookupFailure::Missing(MissingDeclaration(DeclarationKey::ClassInput(
                    request.class,
                    ClassInput::CodeGenerator,
                )))
            },
            LookupFailure::Unsupported,
        );
        assert_eq!(full.consumer, Some(Err(expected)), "{class}.{name}");
    }
    Ok(())
}

#[test]
fn synthesis_budget_and_cancellation_do_not_publish_incomplete_results() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    for (class, name) in [
        ("Plain", "__init__"),
        ("Ordered", "__gt__"),
        ("Plain", "__delattr__"),
        ("Data", "not_generated"),
    ] {
        let request = synthesis_request(&prepared, class, name);
        let full = run_synthesis(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            10_000,
            (false, false),
        );
        assert!(full.consumer.is_some());
        for budget in [0, full.work() - 1].into_iter().chain(
            full.graph
                .boundaries
                .iter()
                .copied()
                .filter(|work| *work < full.work()),
        ) {
            let short = run_synthesis(
                &db,
                &env,
                Rc::clone(&prepared),
                request,
                budget,
                (false, false),
            );
            assert!(short.graph.exhausted, "{class}.{name}: {budget}");
            assert!(short.consumer.is_none(), "{class}.{name}: {budget}");
        }

        // The last suspended checkpoint guards either successful publication or the selected
        // opaque dependency. A canceled database stays canceled, so it owns a separate fixture.
        let cut = full.graph.boundaries[full.graph.boundaries.len() - 2];
        let cancelled_db = fixture()?;
        let cancelled_env = cancelled_db.program_environment();
        let cancelled_prepared = Rc::new(prepare(&cancelled_db, &cancelled_env));
        let cancelled_request = synthesis_request(&cancelled_prepared, class, name);
        let router =
            Router::with_declarations(&cancelled_db, &cancelled_env, cancelled_prepared).unwrap();
        router.cancel_at(cut, cancelled_db.cancellation_token());
        let published = Cell::new(false);
        let observed = probe::capture(&cancelled_db, || {
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                run_with(
                    &cancelled_db,
                    &cancelled_env,
                    &router,
                    10_000,
                    false,
                    false,
                    |router| async {
                        let result = static_synthesized_member(
                            &cancelled_db,
                            &cancelled_env,
                            router,
                            cancelled_request,
                        )
                        .await;
                        published.set(true);
                        result
                    },
                )
            }))
        })
        .unwrap();
        assert!(observed.reads.is_empty());
        assert!(matches!(observed.value, Err(salsa::Cancelled::Local)));
        assert!(!published.get());
        assert!(!router.consumer_active.get());

        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry = run_synthesis(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer, "{class}.{name}");
            assert_eq!(retry.work(), full.work(), "{class}.{name}");
        }
    }
    Ok(())
}
