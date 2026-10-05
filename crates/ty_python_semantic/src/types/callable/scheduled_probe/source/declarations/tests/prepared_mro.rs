use std::panic::AssertUnwindSafe;

use salsa::Database as _;

use super::member_lookup::alias;
use super::*;
use crate::place::{Place, PlaceAndQualifiers, PublicTypePolicy};
use crate::types::MemberLookupPolicy;
use crate::types::callable::scheduled_probe::member_lookup::{
    LookupFailure, LookupOperation, PreparedMroCursor, PreparedMroMemberEffects,
    class_member_from_mro,
};
use crate::types::class::member_lookup::{MroMemberEffects, MroMemberWork};
use crate::types::generics::Specialization;
use crate::types::mapping::effects::MappingOperation;

#[derive(Clone, Copy)]
struct Request<'db> {
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
}

impl<'db> Request<'db> {
    async fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        router: &Router<'db, '_>,
    ) -> Result<PlaceAndQualifiers<'db>, LookupFailure<'db>> {
        class_member_from_mro(
            db,
            env,
            router,
            self.class,
            self.specialization,
            "__init__",
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
        )
        .await
    }

    fn ordinary(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> PlaceAndQualifiers<'db> {
        self.class.class_member_from_mro(
            db,
            env,
            "__init__",
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            self.class.iter_mro(db, self.specialization),
        )
    }
}

fn run_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    request: Request<'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Result<PlaceAndQualifiers<'db>, LookupFailure<'db>>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            request.evaluate(db, env, router)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "source reads: {:?}",
        observed.reads
    );
    assert!(!router.consumer_active.get());
    observed.value
}

#[test]
fn forwarding_mro_lookup_preserves_metadata_without_a_prepared_tail() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mro.py", code(markdown, CASES[2])?)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let function = prepared.globals["check"]
            .value
            .place
            .expect_type()
            .as_function_literal()
            .unwrap();
        let forward = alias(
            prepared.signature(function).unwrap().overloads[0]
                .parameters()
                .get_positional(0)
                .unwrap()
                .annotated_type(),
        );
        for facts in prepared.classes.values_mut() {
            facts.proper_mro = None;
        }
        let prepared = Rc::new(prepared);
        let first_request = Request {
            class: forward.origin(&db),
            specialization: Some(forward.specialization(&db)),
        };
        let first = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            first_request,
            100_000,
            (false, false),
        );
        let first_member = *first.consumer.as_ref().unwrap().as_ref().unwrap();
        assert!(first.static_mro_polls.is_empty());
        let next = alias(first_member.place.expect_type());
        let second_request = Request {
            class: next.origin(&db),
            specialization: Some(next.specialization(&db)),
        };
        let second = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            second_request,
            100_000,
            (false, false),
        );
        let second_member = *second.consumer.as_ref().unwrap().as_ref().unwrap();
        assert!(second.static_mro_polls.is_empty());
        assert_eq!(first_member, first_request.ordinary(&db, &env));
        assert_eq!(second_member, second_request.ordinary(&db, &env));

        for (request, full) in [(first_request, first), (second_request, second)] {
            for budget in [0, 1, full.work() - 1] {
                let short = run_member(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    request,
                    budget,
                    (false, false),
                );
                assert!(short.graph.exhausted);
                assert!(short.consumer.is_none());
            }
            for order in [(false, false), (true, false), (false, true), (true, true)] {
                let retry =
                    run_member(&db, &env, Rc::clone(&prepared), request, full.work(), order);
                assert_eq!(retry.consumer, full.consumer);
                assert_eq!(retry.work(), full.work());
            }
        }
    }
    Ok(())
}

#[test]
fn missing_constructor_context_is_distinct_from_missing_ordinary_context() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/mro.py", "class Root:\n    __init__: int = 1\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let class = class_named(&prepared, "Root");
    let inherited = prepared.inherited_generic_context(class).unwrap();
    assert_eq!(inherited, class.inherited_generic_context(&db));
    assert!(
        !prepared.classes[&class]
            .inherited_generic_context
            .support
            .is_empty()
    );
    prepared.classes.remove(&class);
    let observed = probe::capture(&db, || {
        assert_eq!(
            prepared.context(class),
            Err(MissingDeclaration(DeclarationKey::Context(class)))
        );
        assert_eq!(
            prepared.inherited_generic_context(class),
            Err(MissingDeclaration(DeclarationKey::ClassInput(
                class,
                ClassInput::InheritedGenericContext
            )))
        );
    })
    .unwrap();
    assert!(observed.reads.is_empty());
    let result = run_member(
        &db,
        &env,
        Rc::new(prepared),
        Request {
            class,
            specialization: None,
        },
        100_000,
        (false, false),
    );
    assert_eq!(
        result.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::ClassInput(class, ClassInput::InheritedGenericContext)
        ))))
    );
    Ok(())
}

#[test]
fn mro_lookup_preserves_earlier_unavailable_dependencies() -> anyhow::Result<()> {
    for source in [
        "class Root[T]:\n    def __init__(self, value: T): ...\n",
        "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass Root(Generic[T]):\n    def __init__(self, value: T): ...\n",
    ] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let class = class_named(&prepared, "Root");
        let default = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            Request {
                class,
                specialization: None,
            },
            100_000,
            (false, false),
        );
        assert_eq!(
            default.consumer,
            Some(Err(LookupFailure::Unsupported(
                LookupOperation::DefaultSpecialization
            )))
        );
        let specialization = prepared
            .context(class)
            .unwrap()
            .unwrap()
            .specialize(&db, &[KnownClass::Int.to_instance(&db, &env)]);
        let explicit = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            Request {
                class,
                specialization: Some(specialization),
            },
            100_000,
            (false, false),
        );
        assert_eq!(
            explicit.consumer,
            Some(Err(LookupFailure::Unsupported(
                LookupOperation::ConstructorContext
            )))
        );
    }

    Ok(())
}

#[test]
fn inherited_function_constructor_reaches_mapping_with_the_root_context() -> anyhow::Result<()> {
    for (source, generic_parent) in [
        (
            "class Parent:\n    def __init__(self, value: int): ...\nclass Child(Parent): ...\n",
            false,
        ),
        (
            "class Parent[T]:\n    def __init__(self, value: T): ...\nclass Child(Parent[int]): ...\n",
            true,
        ),
        (
            "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass Parent(Generic[T]):\n    def __init__(self, value: T): ...\nclass Child(Parent[int]): ...\n",
            true,
        ),
    ] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let child = class_named(&prepared, "Child");
        let parent = class_named(&prepared, "Parent");
        assert!(
            prepared
                .namespace(child, "__init__")
                .unwrap()
                .is_undefined()
        );
        assert!(prepared.code_generator(child).unwrap().is_none());
        assert!(prepared.inherited_generic_context(child).unwrap().is_none());
        assert_eq!(
            prepared
                .inherited_generic_context(parent)
                .unwrap()
                .is_some(),
            generic_parent
        );
        let request = Request {
            class: child,
            specialization: None,
        };

        // Generic ancestors need owner mapping. Retaining the child's context lets them reach
        // that dependency instead of stopping earlier at constructor-context work.
        let full = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            100_000,
            (false, false),
        );
        if generic_parent {
            assert_eq!(
                full.consumer,
                Some(Err(LookupFailure::Boundary(Boundary::MappingOperation(
                    MappingOperation::FunctionParamSpecPrelude,
                )))),
                "{source}"
            );
        } else {
            let member = *full.consumer.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(member, request.ordinary(&db, &env));
            let Place::Defined(defined) = member.place else {
                panic!("the inherited function constructor is defined");
            };
            assert_eq!(defined.public_type_policy, PublicTypePolicy::Raw);
        }

        for budget in [0, 1, full.work() - 1] {
            let short = run_member(
                &db,
                &env,
                Rc::clone(&prepared),
                request,
                budget,
                (false, false),
            );
            assert!(short.graph.exhausted);
            assert!(short.consumer.is_none());
        }
        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry = run_member(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer);
            assert_eq!(retry.work(), full.work());
        }
    }
    Ok(())
}

#[test]
fn inherited_forwarding_constructor_traverses_canonical_absence() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut source = code(markdown, CASES[1])?;
        source.push_str("\nclass Parent(Base[int, int]): ...\nclass Child(Parent): ...\n");
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let child = class_named(&prepared, "Child");
        let parent = class_named(&prepared, "Parent");
        for class in [child, parent] {
            assert!(
                prepared
                    .namespace(class, "__init__")
                    .unwrap()
                    .is_undefined()
            );
            assert!(prepared.code_generator(class).unwrap().is_none());
            assert!(prepared.inherited_generic_context(class).unwrap().is_none());
        }
        let request = Request {
            class: child,
            specialization: None,
        };

        // Both absent local constructors must be checked before the inherited callable object
        // is specialized. Capture this lookup before the ordinary oracle can warm its queries.
        let full = run_member(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            100_000,
            (false, false),
        );
        let member = *full.consumer.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(member, request.ordinary(&db, &env));
        let Place::Defined(defined) = member.place else {
            panic!("the inherited callable-object constructor is defined");
        };
        assert_eq!(defined.public_type_policy, PublicTypePolicy::Raw);
        assert_eq!(alias(defined.ty).origin(&db), class_named(&prepared, "C"));

        for budget in [0, 1, full.work() - 1] {
            let short = run_member(
                &db,
                &env,
                Rc::clone(&prepared),
                request,
                budget,
                (false, false),
            );
            assert!(short.graph.exhausted);
            assert!(short.consumer.is_none());
        }
        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry = run_member(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer);
            assert_eq!(retry.work(), full.work());
        }
    }
    Ok(())
}

#[test]
fn cancelled_mro_lookup_does_not_publish_a_member() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/mro.py", "class Root:\n    __init__: int = 1\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
    let request = Request {
        class: class_named(&prepared, "Root"),
        specialization: None,
    };
    let router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
    let cancellation = db.cancellation_token();
    let published = Cell::new(false);
    let observed = probe::capture(&db, || {
        salsa::Cancelled::catch(AssertUnwindSafe(|| {
            run_with(&db, &env, &router, 100_000, false, false, |router| async {
                router.consumer_checkpoint(1).await.unwrap();
                cancellation.cancel();
                let result = request.evaluate(&db, &env, router).await;
                published.set(true);
                result
            })
        }))
    })
    .unwrap();
    assert!(observed.reads.is_empty());
    assert!(observed.value.is_err());
    assert!(!published.get());
    assert!(!router.consumer_active.get());
    Ok(())
}

#[test]
fn mro_cursor_reads_the_proper_tail_only_after_the_first_entry() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/mro.py",
        "class Root: ...\nclass Child(Root): ...\nclass Generic[T]: ...\n",
    )?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let child = class_named(&prepared, "Child");
    let generic = class_named(&prepared, "Generic");
    let specialization = prepared
        .context(generic)
        .unwrap()
        .unwrap()
        .specialize(&db, &[KnownClass::Int.to_instance(&db, &env)]);

    // Looking up the first entry does not require a prepared tail; advancing past it does.
    let run = |prepared, root, specialization, first_only| {
        let router = Router::with_declarations(&db, &env, prepared).unwrap();
        let observed = probe::capture(&db, || {
            run_with(&db, &env, &router, 100_000, false, false, |router| async {
                let effects = PreparedMroMemberEffects::new(&db, &env, router, "__init__")?;
                let mut cursor = PreparedMroCursor::new(ClassLiteral::Static(root), specialization);
                let mut entries = Vec::new();
                loop {
                    MroMemberEffects::checkpoint(&effects, MroMemberWork::Advance).await?;
                    let Some(entry) = MroMemberEffects::advance(&effects, &mut cursor).await?
                    else {
                        break;
                    };
                    entries.push(entry);
                    if first_only {
                        break;
                    }
                }
                Ok::<_, LookupFailure<'_>>(entries)
            })
            .unwrap()
        })
        .unwrap();
        assert!(
            observed.reads.is_empty(),
            "source reads: {:?}",
            observed.reads
        );
        assert!(!router.consumer_active.get());
        observed.value.consumer.unwrap()
    };
    let complete = run(
        Rc::new(PreparedDeclarations::prepare(&db, file).unwrap()),
        child,
        None,
        false,
    )
    .unwrap();
    assert_eq!(complete, child.iter_mro(&db, None).collect::<Vec<_>>());
    prepared.classes.get_mut(&child).unwrap().proper_mro = None;
    let prepared = Rc::new(prepared);
    assert_eq!(
        run(Rc::clone(&prepared), child, None, true).unwrap(),
        vec![ClassBase::Class(ClassType::NonGeneric(child.into()))]
    );
    assert_eq!(
        run(Rc::clone(&prepared), child, None, false),
        Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::ProperMro(child)
        )))
    );
    let generic_first = run(Rc::clone(&prepared), generic, Some(specialization), true).unwrap();
    assert_eq!(
        generic_first,
        vec![generic.iter_mro(&db, Some(specialization)).next().unwrap()]
    );
    assert_eq!(
        run(Rc::clone(&prepared), generic, Some(specialization), false),
        Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::ExplicitBases(generic)
        )))
    );
    assert_eq!(
        run(prepared, generic, None, true),
        Err(LookupFailure::Unsupported(
            LookupOperation::DefaultSpecialization
        ))
    );
    Ok(())
}
