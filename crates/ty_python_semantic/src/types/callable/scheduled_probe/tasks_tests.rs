use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ty_python_core::ProgramFile;
use ty_python_core::platform::PythonPlatform;

use super::*;
use crate::db::tests::setup_db;
use crate::place::global_symbol;
use crate::types::{
    AttributeKind, BoundMethodType, DescriptorGetResult, DescriptorOrigin, KnownClass,
    SlotDescriptorType,
};

fn descriptor_request(ty: Type<'_>) -> DescriptorRequest<'_> {
    DescriptorRequest {
        ty,
        instance: Some(Type::int_literal(1)),
        owner: Type::int_literal(2),
    }
}

#[test]
fn generic_constructor_conversion_reaches_member_dependency() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file(
        "/src/constructor.py",
        "class Product[T]: ...\nfactory = Product[int]\n",
    )?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/constructor.py")?,
        env.program(&db),
    );
    let factory = global_symbol(&db, file, "factory").place.expect_type();
    assert!(matches!(factory, Type::GenericAlias(_)));
    let router = Router::default();
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        router.consumer_demand(request(factory)).await
    })
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    assert_eq!(
        result.consumer,
        Some(Err(Boundary::ConstructorEffect(
            ConstructorEffect::MetaclassCall
        )))
    );
    assert_eq!(result.constructor_polls.len(), 1);
    let constructors = router.constructors.borrow();
    let (key, entry) = constructors
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing constructor task"))?;
    assert_eq!(key.receiver, factory);
    assert_eq!(
        entry.answer,
        Some(Err(Boundary::ConstructorEffect(
            ConstructorEffect::MetaclassCall
        )))
    );
    Ok(())
}

#[test]
fn constructor_tasks_reject_a_different_program_domain() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let other_program =
        crate::Program::new(&db, &PythonPlatform::All, env.resolver_environment(&db));
    assert_ne!(other_program, env.program(&db));
    let other_env = ProgramEnvironment::from_program(other_program);
    let receiver = KnownClass::Int.to_subclass_of(&db, &other_env);
    let literal = KnownClass::Int.to_class_literal(&db, &other_env);
    let class = literal
        .to_class_type(&db)
        .ok_or_else(|| anyhow::anyhow!("expected int class"))?;
    let request = ConstructorCallableRequest { class, receiver };
    let router = Router::default();
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        router.constructor_demand(Key::Consumer, request).await
    })
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    assert_eq!(result.consumer, Some(Err(Boundary::ProgramDomain)));
    assert_eq!(result.constructor_polls.get(&request), Some(&1));
    Ok(())
}

#[test]
fn completed_constructor_and_descriptor_tasks_share_only_exact_requests() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let receiver = KnownClass::Int.to_class_literal(&db, &env);
    let class = receiver
        .to_class_type(&db)
        .ok_or_else(|| anyhow::anyhow!("expected int class"))?;
    let constructors = [
        (
            ConstructorCallableRequest { class, receiver },
            ConstructorEffect::InstanceApproximation,
        ),
        (
            ConstructorCallableRequest {
                class,
                receiver: KnownClass::Int.to_subclass_of(&db, &env),
            },
            ConstructorEffect::MetaclassCall,
        ),
    ];
    let slot = Type::SlotDescriptor(SlotDescriptorType::new(&db, Type::int_literal(3)));
    let base = descriptor_request(slot);
    let descriptors = [
        base,
        DescriptorRequest {
            instance: None,
            ..base
        },
        DescriptorRequest {
            instance: Some(Type::int_literal(4)),
            ..base
        },
        DescriptorRequest {
            owner: Type::int_literal(5),
            ..base
        },
        DescriptorRequest {
            ty: Type::SlotDescriptor(SlotDescriptorType::new(&db, Type::int_literal(6))),
            ..base
        },
    ];
    let router = Router::default();
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        for (request, effect) in constructors {
            let first = router.constructor_demand(Key::Consumer, request).await;
            assert_eq!(first, Err(Boundary::ConstructorEffect(effect)));
            assert_eq!(
                router.constructor_demand(Key::Consumer, request).await,
                first
            );
        }
        for request in descriptors {
            let first = router.descriptor_demand_from(Key::Consumer, request).await;
            assert!(matches!(first, Ok(Ok(Some(_)))));
            assert_eq!(
                router.descriptor_demand_from(Key::Consumer, request).await,
                first
            );
        }
    })
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    assert_eq!(result.consumer, Some(()));
    assert_eq!(result.constructor_polls.len(), constructors.len());
    assert_eq!(result.descriptor_polls.len(), descriptors.len());
    assert!(result.constructor_polls.values().all(|polls| *polls == 1));
    assert!(result.descriptor_polls.values().all(|polls| *polls == 1));
    assert_eq!(router.constructors.borrow().len(), constructors.len());
    assert_eq!(router.descriptors.borrow().len(), descriptors.len());
    Ok(())
}

#[test]
fn admitted_descriptor_entries_preserve_return_type_kind_origin_and_absence() {
    let db = setup_db();
    let env = db.program_environment();
    let value = Type::int_literal(3);
    let slot = Type::SlotDescriptor(SlotDescriptorType::new(&db, value));
    let bound_method = Type::BoundMethod(BoundMethodType::from_callable(
        &db,
        Type::unknown(),
        env.program(&db),
        Type::int_literal(1),
    ));
    let cases = [
        (descriptor_request(slot), Some(value)),
        (
            DescriptorRequest {
                instance: None,
                ..descriptor_request(slot)
            },
            Some(slot),
        ),
        (descriptor_request(Type::unknown()), Some(Type::unknown())),
        (descriptor_request(Type::any()), Some(Type::any())),
        (descriptor_request(bound_method), None),
    ];
    let router = Router::default();
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        for (request, return_type) in cases {
            assert_eq!(
                router.descriptor_demand_from(Key::Consumer, request).await,
                Ok(Ok(return_type.map(|return_type| DescriptorGetResult {
                    return_type,
                    origin: DescriptorOrigin::default(),
                    kind: AttributeKind::DataDescriptor,
                })))
            );
        }
    });
    assert!(matches!(result, Ok(result) if result.consumer == Some(())));
    assert_eq!(router.descriptors.borrow().len(), cases.len());
}

#[test]
fn source_and_semantic_boundaries_remain_distinct_from_descriptor_absence() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file("/src/scheduled_descriptor.py", "def function(): ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/scheduled_descriptor.py")?,
        env.program(&db),
    );
    let function = global_symbol(&db, file, "function").place.expect_type();
    assert!(matches!(function, Type::FunctionLiteral(_)));
    let ordinary_value = Type::int_literal(1);
    let router = Router::default();
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        assert_eq!(
            router.consumer_demand(request(function)).await,
            Err(Boundary::SourceSignature)
        );
        assert_eq!(
            router.consumer_demand(request(ordinary_value)).await,
            Err(Boundary::SemanticOperation)
        );
        assert_eq!(
            router
                .descriptor_demand_from(Key::Consumer, descriptor_request(function))
                .await,
            Err(Boundary::DescriptorEffect(
                DescriptorEffect::FunctionLikeBinding
            ))
        );
        assert_eq!(
            router
                .descriptor_demand_from(Key::Consumer, descriptor_request(ordinary_value))
                .await,
            Err(Boundary::DescriptorEffect(DescriptorEffect::ClassMember))
        );
    })
    .map_err(|boundary| anyhow::anyhow!("unexpected root boundary: {boundary:?}"))?;
    assert_eq!(result.consumer, Some(()));
    assert!(result.graph.pending.is_empty());
    assert!(
        router
            .descriptors
            .borrow()
            .values()
            .all(|entry| { matches!(entry.answer, Some(Err(Boundary::DescriptorEffect(_)))) })
    );
    Ok(())
}

#[test]
fn invocation_tickets_allocate_once_per_call_and_keep_completion_after_consumption() {
    let db = setup_db();
    let env = db.program_environment();
    let owner = descriptor_request(Type::unknown());
    let other_owner = DescriptorRequest {
        instance: None,
        ..owner
    };
    let request = DescriptorInvocationRequest {
        callable: Type::Callable(CallableType::single(&db, Signature::unknown())),
        arguments: [owner.ty, Type::int_literal(1), owner.owner],
    };
    let router = Router::default();
    let mut first = Box::pin(router.invocation_demand(Key::Consumer, owner, request));
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..4 {
        assert!(first.as_mut().poll(&mut cx).is_pending());
    }
    assert_eq!(router.invocation_sequences.borrow().get(&owner), Some(&1));
    let first_reply = {
        let effects = router.effects.borrow();
        assert_eq!(effects.len(), 1);
        let Effect::DemandInvocation { ticket, .. } = &effects[0] else {
            panic!("one invocation declaration");
        };
        assert_eq!(ticket.key.sequence, 0);
        Rc::clone(&ticket.reply)
    };
    let result = run_with(&db, &env, &router, 1000, false, false, |router| async {
        assert!(matches!(first.await, Err(Boundary::InvocationPreparation)));
        assert!(first_reply.borrow().is_none());
        assert_eq!(
            router.invocations.borrow()[&InvocationKey {
                owner,
                sequence: 0,
                request,
            }]
                .answer,
            Some(())
        );
        for next_owner in [owner, other_owner, owner] {
            assert!(matches!(
                router
                    .invocation_demand(Key::Consumer, next_owner, request)
                    .await,
                Err(Boundary::InvocationPreparation)
            ));
        }
    });
    let Ok(result) = result else {
        panic!("fresh root router");
    };
    assert_eq!(result.consumer, Some(()));
    let expected =
        [(owner, 0), (owner, 1), (other_owner, 0), (owner, 2)].map(|(owner, sequence)| {
            InvocationKey {
                owner,
                sequence,
                request,
            }
        });
    assert_eq!(result.invocation_polls.len(), expected.len());
    assert_eq!(router.invocations.borrow().len(), expected.len());
    for key in expected {
        assert_eq!(result.invocation_polls.get(&key), Some(&1));
        assert_eq!(router.invocations.borrow()[&key].answer, Some(()));
    }
    assert_eq!(router.invocation_sequences.borrow().get(&owner), Some(&3));
    assert_eq!(
        router.invocation_sequences.borrow().get(&other_owner),
        Some(&1)
    );
    assert!(first_reply.borrow().is_none());
}

struct DropCounter<'a>(&'a Cell<usize>);

impl Drop for DropCounter<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn task_evidence_and_cancellation_are_stable_across_budgets_and_scheduling() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let receiver = KnownClass::Object.to_class_literal(&db, &env);
    let class = receiver
        .to_class_type(&db)
        .ok_or_else(|| anyhow::anyhow!("expected object class"))?;
    let constructor = ConstructorCallableRequest { class, receiver };
    let slot = descriptor_request(Type::SlotDescriptor(SlotDescriptorType::new(
        &db,
        Type::int_literal(3),
    )));
    let unsupported = descriptor_request(Type::int_literal(4));
    let callable = Type::Callable(CallableType::single(&db, Signature::unknown()));
    let conversion = request(wrap(&db, callable));
    let invocation = DescriptorInvocationRequest {
        callable,
        arguments: [slot.ty, Type::int_literal(1), slot.owner],
    };
    let sample = |budget, reverse_execution, reverse_merge| {
        let router = Router::default();
        let drops = Cell::new(0);
        let resumes = Cell::new(0);
        let guard = DropCounter(&drops);
        let result = run_with(
            &db,
            &env,
            &router,
            budget,
            reverse_execution,
            reverse_merge,
            |router| async {
                let _guard = guard;
                router.declare_descriptor(slot);
                router.declare_descriptor(unsupported);
                router.declare(conversion);
                assert_eq!(
                    router.constructor_demand(Key::Consumer, constructor).await,
                    Err(Boundary::ConstructorEffect(
                        ConstructorEffect::InstanceApproximation
                    ))
                );
                resumes.set(resumes.get() + 1);
                assert!(matches!(
                    router.descriptor_demand_from(Key::Consumer, slot).await,
                    Ok(Ok(Some(_)))
                ));
                resumes.set(resumes.get() + 1);
                assert!(matches!(
                    router
                        .invocation_demand(Key::Consumer, slot, invocation)
                        .await,
                    Err(Boundary::InvocationPreparation)
                ));
                resumes.set(resumes.get() + 1);
            },
        );
        let Ok(result) = result else {
            panic!("fresh root router");
        };
        assert!(matches!(
            run_with(&db, &env, &router, 1000, false, false, |_| async {}),
            Err(Boundary::RootReuse)
        ));
        let constructors: FxHashMap<_, _> = router
            .constructors
            .borrow()
            .iter()
            .map(|(key, entry)| (*key, entry.answer.clone()))
            .collect();
        let descriptors: FxHashMap<_, _> = router
            .descriptors
            .borrow()
            .iter()
            .map(|(key, entry)| (*key, entry.answer))
            .collect();
        let invocations: FxHashMap<_, _> = router
            .invocations
            .borrow()
            .iter()
            .map(|(key, entry)| (*key, entry.answer))
            .collect();
        (
            result.consumer,
            result.consumer_polls,
            result.graph,
            result.constructor_polls,
            result.descriptor_polls,
            result.invocation_polls,
            constructors,
            descriptors,
            invocations,
            drops.get(),
            resumes.get(),
        )
    };
    let full = sample(1000, false, false);
    assert_eq!(full.0, Some(()));
    assert_eq!(full.9, 1);
    assert_eq!(full.10, 3);
    assert_eq!(full.4.len(), 2);
    assert_eq!(full.4.get(&slot), Some(&1));
    assert_eq!(
        full.7.get(&unsupported),
        Some(&Some(Err(Boundary::DescriptorEffect(
            DescriptorEffect::ClassMember
        ))))
    );
    assert!(matches!(full.2.values.get(&conversion), Some(Ok(Some(_)))));
    for budget in 0..=full.2.work + 1 {
        let baseline = sample(budget, false, false);
        assert_eq!(baseline.9, 1, "budget {budget}");
        assert!(baseline.2.work <= budget);
        for reverse_execution in [false, true] {
            for reverse_merge in [false, true] {
                assert_eq!(
                    baseline,
                    sample(budget, reverse_execution, reverse_merge),
                    "budget {budget}, execution {reverse_execution}, merge {reverse_merge}"
                );
            }
        }
    }
    Ok(())
}
