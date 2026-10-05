use super::*;
use crate::types::callable::scheduled_probe::member_lookup::{LookupFailure, static_own_member};
use crate::types::class::own_member::OwnMemberLookupRequest;
use crate::types::{ClassType, GenericAlias, SubclassOfInner};

pub(super) fn alias(ty: Type<'_>) -> GenericAlias<'_> {
    let Type::SubclassOf(subclass) = ty else {
        panic!("expected a class-object type");
    };
    let SubclassOfInner::Class(ClassType::Generic(alias)) = subclass.subclass_of() else {
        panic!("expected a specialized nominal class");
    };
    alias
}

pub(super) async fn own_initializer<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    class: GenericAlias<'db>,
) -> Result<Member<'db>, LookupFailure<'db>> {
    let member = static_own_member(
        db,
        env,
        router,
        OwnMemberLookupRequest {
            class: class.origin(db),
            name: "__init__",
            inherited_generic_context: None,
            specialization: Some(class.specialization(db)),
        },
    )
    .await?;
    if let Some(ty) = member.inner.place.raw_type() {
        router.consumer_checkpoint(8).await?;
        let request = router.mapping_root(ty, class.specialization(db), true)?;
        let mapped = router.consumer_mapping_demand(request).await?;
        Ok(member.map_type(|_| mapped))
    } else {
        Ok(member)
    }
}

fn run_own_members<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    forward: GenericAlias<'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Result<(Member<'db>, Member<'db>), LookupFailure<'db>>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(
            db,
            env,
            &router,
            budget,
            order.0,
            order.1,
            |router| async move {
                let first = own_initializer(db, env, router, forward).await?;
                let next = alias(first.inner.place.raw_type().unwrap());
                let second = own_initializer(db, env, router, next).await?;
                Ok((first, second))
            },
        )
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "lookup entered source queries: {:?}",
        observed.reads
    );
    assert!(!router.consumer_active.get());
    observed.value
}

#[test]
fn prepared_own_member_steps_preserve_real_forwarding_members() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut fact_counts = None;
        let mut previous_work = 0;
        for depth in [4, 8, 16, 32] {
            let source = code(markdown, CASES[2])?.replace(
                "list[list[list[list[V]]]]",
                &format!("{}V{}", "list[".repeat(depth), "]".repeat(depth)),
            );
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", source)?;
            let env = db.program_environment();
            let file = ProgramFile::new(
                &db,
                system_path_to_file(&db, "/src/forwarding.py")?,
                env.program(&db),
            );
            let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
            let counts = (prepared.classes.len(), prepared.signatures.len());
            if let Some(prior) = fact_counts {
                assert_eq!(counts, prior);
            }
            fact_counts = Some(counts);
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

            // Run lookup before its ordinary oracle so source queries cannot hide behind warm caches.
            let full = run_own_members(
                &db,
                &env,
                Rc::clone(&prepared),
                forward,
                100_000,
                (false, false),
            );
            let (first, second) = full.consumer.as_ref().unwrap().as_ref().unwrap();
            let next = alias(first.inner.place.raw_type().unwrap());
            assert_eq!(
                *first,
                ClassType::Generic(forward).own_class_member(&db, &env, None, "__init__")
            );
            assert_eq!(
                *second,
                ClassType::Generic(next).own_class_member(&db, &env, None, "__init__")
            );
            assert!(full.work() > previous_work);
            previous_work = full.work();

            for budget in [0, 1, full.work() - 1] {
                let short = run_own_members(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    forward,
                    budget,
                    (false, false),
                );
                assert!(short.graph.exhausted);
                assert!(short.consumer.is_none());
            }
            for order in [
                (false, false),
                (false, false),
                (true, false),
                (false, true),
                (true, true),
            ] {
                let repeated =
                    run_own_members(&db, &env, Rc::clone(&prepared), forward, full.work(), order);
                assert_eq!(repeated.consumer, full.consumer);
                assert_eq!(repeated.work(), full.work());
            }
            eprintln!(
                "own member depth={depth}: work={} facts={counts:?}",
                full.work()
            );
        }
    }
    Ok(())
}

#[test]
fn prepared_own_member_missing_inputs_are_distinct_from_canonical_absence() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/forwarding.py", code(PEP695, CASES[2])?)?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/forwarding.py")?,
        env.program(&db),
    );
    for missing in [Some("namespace"), Some("member-input"), Some("class"), None] {
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let forward = class_named(&prepared, "Forward");
        let request = OwnMemberLookupRequest {
            class: forward,
            name: if missing.is_some() {
                "__init__"
            } else {
                "__delete__"
            },
            inherited_generic_context: None,
            specialization: None,
        };
        let expected = match missing {
            Some("namespace") => {
                prepared
                    .classes
                    .get_mut(&forward)
                    .unwrap()
                    .namespace
                    .remove("__init__");
                Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::Namespace(forward, Name::new("__init__")),
                )))
            }
            Some("member-input") => {
                prepared
                    .classes
                    .get_mut(&forward)
                    .unwrap()
                    .member_inputs
                    .remove("__init__");
                Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::MemberInput(
                        forward,
                        Name::new("__init__"),
                        MemberInput::OwnSlot,
                    ),
                )))
            }
            Some("class") => {
                prepared.classes.remove(&forward);
                Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::ClassInput(forward, ClassInput::CodeGenerator),
                )))
            }
            _ => Ok(Member::unbound()),
        };
        let prepared = Rc::new(prepared);
        for _ in 0..2 {
            let router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
            let observed = probe::capture(&db, || {
                run_with(&db, &env, &router, 10_000, false, false, |router| {
                    static_own_member(&db, &env, router, request)
                })
                .unwrap()
            })
            .unwrap();
            assert!(observed.reads.is_empty());
            assert_eq!(observed.value.consumer.as_ref(), Some(&expected));
            if missing.is_none() {
                assert_eq!(
                    observed.value.consumer.unwrap().unwrap(),
                    ClassType::NonGeneric(forward.into()).own_class_member(
                        &db,
                        &env,
                        None,
                        "__delete__"
                    )
                );
            }
            assert!(!router.consumer_active.get());
        }
    }
    Ok(())
}

#[test]
fn consumer_lookup_work_requires_an_active_drive() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let router = Router::default();
    let poll_checkpoint = || {
        let mut work = std::pin::pin!(router.consumer_checkpoint(4));
        std::future::Future::poll(
            work.as_mut(),
            &mut std::task::Context::from_waker(std::task::Waker::noop()),
        )
    };
    assert_eq!(
        poll_checkpoint(),
        std::task::Poll::Ready(Err(Boundary::MappingDomain))
    );
    let completed = run_with(&db, &env, &router, 100, false, false, |router| async {
        router.consumer_checkpoint(4).await
    })
    .unwrap();
    assert_eq!(completed.consumer, Some(Ok(())));
    assert_eq!(
        poll_checkpoint(),
        std::task::Poll::Ready(Err(Boundary::MappingDomain))
    );

    let router = Router::default();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_with(&db, &env, &router, 100, false, false, |router| async {
            router.consumer_checkpoint(4).await.unwrap();
            panic!("consumer fails after admitted work");
        })
    }));
    assert!(panicked.is_err());
    assert!(!router.consumer_active.get());
    Ok(())
}
