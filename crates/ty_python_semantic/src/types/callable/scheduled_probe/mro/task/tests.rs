//! Scheduler contracts using fixed MRO payloads and prepared classes as request identities.
//! These controlled tasks do not establish support for constructing source-defined MROs.

use std::cell::RefCell;
use std::fmt::{Debug, Write};
use std::future::{Future, pending, poll_fn};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::task::{Context, Waker};
use std::time::Instant;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::callable::scheduled_probe::mapping::tests::RawMroOwnerProbe;
use crate::types::callable::scheduled_probe::source::PreparedDeclarations;
use crate::types::callable::scheduled_probe::source::declarations::MissingDeclaration;
use crate::types::callable::scheduled_probe::{ConsumerSnapshot, run_with};
use crate::types::mro::StaticMroErrorKind;
use crate::types::{ClassLiteral, ClassType, Type};

const BUDGET: usize = 100_000_000;
const ORDERS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

#[derive(Clone)]
pub(super) enum Script<'db> {
    Complete,
    Await {
        child: StaticClassLiteral<'db>,
        copies: usize,
    },
    RejectSecond {
        first: StaticClassLiteral<'db>,
        second: StaticClassLiteral<'db>,
        rejected: Rc<Cell<bool>>,
    },
    RetainOwner {
        owner: Rc<Cell<Option<(StaticMroRequest<'db>, MroNodeId)>>>,
        exhaust: bool,
    },
    WaitForRelease {
        owner: Rc<Cell<Option<(StaticMroRequest<'db>, MroNodeId)>>>,
        release: Rc<Cell<bool>>,
    },
    StoredOutcome(Rc<RefCell<Option<StaticMroOutcome<'db>>>>),
    Park,
}

pub(super) async fn controlled_task<'db>(
    db: &'db dyn Db,
    router: &Router<'db, '_>,
    request: StaticMroRequest<'db>,
    node: MroNodeId,
    script: Script<'db>,
) -> StaticMroOutcome<'db> {
    let sequence = Cell::new(0);
    let work = PreparedMroWork::task(router, request, node, &sequence);
    match script {
        Script::Complete => {}
        Script::Await { child, copies } => {
            let demands = (0..copies)
                .map(|_| Box::pin(router.demand_static_mro(db, &work, child, None)))
                .collect();
            for answer in together(demands, true).await {
                let answer = answer?;
                router.static_mro_is_cycle(&work, answer).await?;
            }
        }
        Script::RejectSecond {
            first,
            second,
            rejected,
        } => {
            let mut first = Box::pin(router.demand_static_mro(db, &work, first, None));
            register_pending(first.as_mut(), &work).await?;
            work.checkpoint(1).await?;
            let second = router.demand_static_mro(db, &work, second, None).await;
            assert!(matches!(
                second,
                Err(LookupFailure::Boundary(Boundary::MroDomain))
            ));
            rejected.set(true);
            drop(first);
        }
        Script::RetainOwner { owner, exhaust } => {
            owner.set(Some((request, node)));
            if exhaust {
                loop {
                    work.checkpoint(64).await?;
                }
            }
        }
        Script::WaitForRelease { owner, release } => {
            owner.set(Some((request, node)));
            while !release.get() {
                work.checkpoint(1).await?;
            }
        }
        Script::StoredOutcome(outcome) => {
            return outcome
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Err(Boundary::MroDomain.into()));
        }
        Script::Park => return pending().await,
    }
    Ok(Ok(Mro::from(vec![ClassBase::Any])))
}

async fn together<F: Future>(mut futures: Vec<Pin<Box<F>>>, repoll: bool) -> Vec<F::Output> {
    let mut answers = (0..futures.len()).map(|_| None).collect::<Vec<_>>();
    poll_fn(|cx| {
        for (future, answer) in futures.iter_mut().zip(&mut answers) {
            if answer.is_some() {
                continue;
            }
            if let Poll::Ready(value) = future.as_mut().poll(cx) {
                *answer = Some(value);
            } else if repoll {
                assert!(future.as_mut().poll(cx).is_pending());
            }
        }
        if answers.iter().all(Option::is_some) {
            Poll::Ready(answers.iter_mut().filter_map(Option::take).collect())
        } else {
            Poll::Pending
        }
    })
    .await
}

async fn register_pending<F: Future>(
    mut future: Pin<&mut F>,
    work: &PreparedMroWork<'_, '_, '_>,
) -> Result<(), Boundary> {
    loop {
        let before = work.router.mro_effect_count.get();
        poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        if work.router.mro_effect_count.get() != before {
            return Ok(());
        }
        work.checkpoint(1).await?;
    }
}

fn checked<T>(value: Result<T, impl Debug>) -> anyhow::Result<T> {
    value.map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn fixture(count: usize) -> anyhow::Result<TestDb> {
    let mut source = String::new();
    for index in 0..count {
        writeln!(source, "class Node{index}[T]: ...")?;
    }
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/mro_tasks.py", &source)
        .build()
}

fn prepared(
    db: &TestDb,
    count: usize,
) -> anyhow::Result<(Rc<PreparedDeclarations<'_>>, Vec<StaticClassLiteral<'_>>)> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mro_tasks.py")?,
        env.program(db),
    );
    let prepared = Rc::new(checked(PreparedDeclarations::prepare(db, file))?);
    let classes = (0..count)
        .map(|index| {
            let Type::ClassLiteral(ClassLiteral::Static(class)) =
                global_symbol(db, file, &format!("Node{index}"))
                    .place
                    .expect_type()
            else {
                anyhow::bail!("expected a static fixture class");
            };
            Ok(class)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok((prepared, classes))
}

fn request<'db>(
    router: &Router<'db, '_>,
    class: StaticClassLiteral<'db>,
) -> anyhow::Result<StaticMroRequest<'db>> {
    Ok(StaticMroRequest {
        evaluation: router
            .evaluation_domain
            .0
            .ok_or_else(|| anyhow::anyhow!("missing evaluation domain"))?,
        class,
        specialization: None,
    })
}

fn install<'db>(
    router: &Router<'db, '_>,
    class: StaticClassLiteral<'db>,
    script: Script<'db>,
) -> anyhow::Result<StaticMroRequest<'db>> {
    let request = request(router, class)?;
    router
        .static_mro
        .borrow_mut()
        .controlled
        .insert(request, script);
    Ok(request)
}

fn assert_complete<R>(snapshot: &ConsumerSnapshot<'_, R>) {
    assert!(!snapshot.graph.exhausted);
    assert!(snapshot.consumer.is_some());
    assert!(snapshot.graph.static_mro_pending.is_empty());
    assert!(
        snapshot
            .static_mro_starts
            .values()
            .all(|starts| *starts == 1)
    );
}

#[test]
fn raw_mapping_owner_does_not_transfer_to_other_tasks_or_recycled_slots() -> anyhow::Result<()> {
    let db = fixture(3)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 3)?;
    let context = ClassLiteral::Static(classes[0])
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("fixture should be generic"))?;
    let specialization = context.specialize(&db, &[Type::int_literal(1)]);
    let other_specialization = context.specialize(&db, &[Type::int_literal(2)]);
    for (execution, merge) in ORDERS {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        let owners = std::array::from_fn::<_, 3, _>(|_| Rc::new(Cell::new(None)));
        let releases = std::array::from_fn::<_, 3, _>(|_| Rc::new(Cell::new(false)));
        for index in 0..3 {
            install(
                &router,
                classes[index],
                Script::WaitForRelease {
                    owner: Rc::clone(&owners[index]),
                    release: Rc::clone(&releases[index]),
                },
            )?;
        }
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            BUDGET,
            execution,
            merge,
            |router| async {
                let consumer = PreparedMroWork::consumer(router);
                let mut a = Box::pin(router.demand_static_mro(&db, &consumer, classes[0], None));
                let mut b = Box::pin(router.demand_static_mro(&db, &consumer, classes[1], None));
                checked(register_pending(a.as_mut(), &consumer).await)?;
                checked(register_pending(b.as_mut(), &consumer).await)?;
                let (a_request, a_node, b_request, b_node) = loop {
                    if let (Some((a_request, a_node)), Some((b_request, b_node))) =
                        (owners[0].get(), owners[1].get())
                    {
                        break (a_request, a_node, b_request, b_node);
                    }
                    checked(consumer.checkpoint(1).await)?;
                };
                let a_sequence = Cell::new(0);
                let b_sequence = Cell::new(0);
                let a_work = PreparedMroWork::task(router, a_request, a_node, &a_sequence);
                let b_work = PreparedMroWork::task(router, b_request, b_node, &b_sequence);
                let probe = checked(RawMroOwnerProbe::new(
                    &db,
                    &a_work,
                    specialization,
                    other_specialization,
                ))?;
                probe.assert_wrong_owner(&b_work);
                probe.assert_wrong_owner(&consumer);

                releases[0].set(true);
                checked(a.await)?;
                assert_eq!(
                    router.validate_static_mro_owner(a_request, a_node),
                    Err(Boundary::MroDomain)
                );
                let mut c = Box::pin(router.demand_static_mro(&db, &consumer, classes[2], None));
                checked(register_pending(c.as_mut(), &consumer).await)?;
                let (c_request, c_node) = loop {
                    if let Some(owner) = owners[2].get() {
                        break owner;
                    }
                    checked(consumer.checkpoint(1).await)?;
                };
                assert_eq!(c_node.slot, a_node.slot);
                assert_ne!(c_node.generation, a_node.generation);
                assert_eq!(router.validate_static_mro_owner(b_request, b_node), Ok(()));
                assert_eq!(router.validate_static_mro_owner(c_request, c_node), Ok(()));
                probe.assert_retired(&db, &a_work);
                assert_eq!(a_sequence.get(), 0);
                assert_eq!(b_sequence.get(), 0);

                releases[1].set(true);
                releases[2].set(true);
                checked(b.await)?;
                checked(c.await)?;
                Ok::<_, anyhow::Error>(())
            },
        ))?;
        assert_complete(&snapshot);
        checked(
            snapshot
                .consumer
                .ok_or_else(|| anyhow::anyhow!("missing consumer"))?,
        )?;
        assert_eq!(snapshot.static_mro_starts.len(), 3);
        assert_eq!(snapshot.static_mro_counts.result_publications, 3);
        assert_eq!(snapshot.graph.mapping_pending.len(), 2);
    }
    Ok(())
}

#[test]
fn stored_semantic_errors_preserve_classification_and_complete_fallback() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/mro_tasks.py", "class Node0[T]: ...\n")
        .with_file(
            "/src/mro_error.py",
            "class Base: ...\nclass Invalid(Base, Base): ...\n",
        )
        .build()?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    let error_file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro_error.py")?,
        env.program(&db),
    );
    let Type::ClassLiteral(ClassLiteral::Static(invalid)) =
        global_symbol(&db, error_file, "Invalid")
            .place
            .expect_type()
    else {
        anyhow::bail!("expected a static duplicate-base fixture class");
    };

    for (execution, merge) in ORDERS {
        let cycle = StaticMroError::cycle(
            &db,
            &env,
            ClassType::NonGeneric(ClassLiteral::Static(classes[0])),
        );
        let Err(duplicate) = Mro::of_static_class(&db, invalid, None) else {
            anyhow::bail!("expected duplicate-base MRO construction to fail");
        };
        assert!(matches!(
            duplicate.reason(),
            StaticMroErrorKind::DuplicateBases(_)
        ));
        for (error, is_cycle) in [(cycle, true), (duplicate, false)] {
            let fallback = error.fallback_mro().iter().copied().collect::<Vec<_>>();
            assert_eq!(fallback.len(), 3);
            let outcome = Rc::new(RefCell::new(Some(Ok(Err(error)))));
            let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
            install(
                &router,
                classes[0],
                Script::StoredOutcome(Rc::clone(&outcome)),
            )?;
            let snapshot = checked(run_with(
                &db,
                &env,
                &router,
                BUDGET,
                execution,
                merge,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let first = router
                        .demand_static_mro(&db, &work, classes[0], None)
                        .await?;
                    let cached = router
                        .demand_static_mro(&db, &work, classes[0], None)
                        .await?;
                    assert_eq!(first, cached);
                    assert_eq!(router.static_mro_is_cycle(&work, first).await, Ok(is_cycle));
                    assert_eq!(
                        router.static_mro_len(&work, first).await,
                        Ok(fallback.len())
                    );
                    for (index, entry) in fallback.iter().copied().enumerate() {
                        assert_eq!(
                            router.static_mro_entry(&work, first, index).await,
                            Ok(Some(entry))
                        );
                    }
                    assert_eq!(
                        router.static_mro_entry(&work, first, fallback.len()).await,
                        Ok(None)
                    );
                    Ok::<_, LookupFailure<'_>>(())
                },
            ))?;
            assert_complete(&snapshot);
            assert_eq!(snapshot.consumer, Some(Ok(())));
            assert_eq!(snapshot.static_mro_starts.len(), 1);
            assert_eq!(snapshot.static_mro_counts.result_publications, 1);
            assert!(outcome.borrow().is_none());
            assert_eq!(router.static_mro.borrow().outcomes.len(), 1);
        }
    }
    Ok(())
}

#[test]
fn stored_outer_failures_propagate_through_every_view_without_fallback() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    for (execution, merge) in ORDERS {
        for failure in [
            LookupFailure::Missing(MissingDeclaration(DeclarationKey::ExplicitBases(
                classes[0],
            ))),
            LookupFailure::Unsupported(LookupOperation::StaticMroErrorConstruction),
        ] {
            let outcome = Rc::new(RefCell::new(Some(Err(failure.clone()))));
            let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
            install(
                &router,
                classes[0],
                Script::StoredOutcome(Rc::clone(&outcome)),
            )?;
            let snapshot = checked(run_with(
                &db,
                &env,
                &router,
                BUDGET,
                execution,
                merge,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let first = router
                        .demand_static_mro(&db, &work, classes[0], None)
                        .await?;
                    let cached = router
                        .demand_static_mro(&db, &work, classes[0], None)
                        .await?;
                    assert_eq!(first, cached);
                    assert_eq!(
                        router.static_mro_is_cycle(&work, first).await,
                        Err(failure.clone())
                    );
                    assert_eq!(
                        router.static_mro_len(&work, first).await,
                        Err(failure.clone())
                    );
                    for index in [0, usize::MAX] {
                        assert_eq!(
                            router.static_mro_entry(&work, first, index).await,
                            Err(failure.clone())
                        );
                    }
                    Ok::<_, LookupFailure<'_>>(())
                },
            ))?;
            assert_complete(&snapshot);
            assert_eq!(snapshot.consumer, Some(Ok(())));
            assert_eq!(snapshot.static_mro_starts.len(), 1);
            assert_eq!(snapshot.static_mro_counts.result_publications, 1);
            assert!(outcome.borrow().is_none());
            assert_eq!(router.static_mro.borrow().outcomes.len(), 1);
        }
    }
    Ok(())
}

#[test]
fn same_child_instances_and_completed_hits_have_independent_replies() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    for (execution, merge) in ORDERS {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        let request = install(&router, classes[0], Script::Complete)?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            BUDGET,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                let first = together(
                    (0..2)
                        .map(|_| Box::pin(router.demand_static_mro(&db, &work, classes[0], None)))
                        .collect(),
                    true,
                )
                .await;
                let second = together(
                    (0..2)
                        .map(|_| Box::pin(router.demand_static_mro(&db, &work, classes[0], None)))
                        .collect(),
                    true,
                )
                .await;
                let answers = first
                    .into_iter()
                    .chain(second)
                    .collect::<Result<Vec<_>, _>>()?;
                assert!(answers.iter().all(|answer| *answer == answers[0]));
                Ok::<_, LookupFailure<'_>>(answers[0])
            },
        ))?;
        assert_complete(&snapshot);
        assert!(matches!(snapshot.consumer, Some(Ok(_))));
        assert_eq!(snapshot.static_mro_starts.len(), 1);
        assert_eq!(snapshot.static_mro_polls.get(&request), Some(&1));
        assert_eq!(snapshot.static_mro_counts.reply_allocations, 4);
        assert_eq!(snapshot.static_mro_counts.reply_deliveries, 4);
        assert_eq!(snapshot.static_mro_counts.reply_reads, 4);
        assert_eq!(snapshot.static_mro_counts.request_probes, 8);
        assert_eq!(snapshot.static_mro_counts.graph_passes, 0);
        assert_eq!(snapshot.static_mro_counts.sweeps, 0);
    }
    Ok(())
}

#[test]
fn consumers_can_demand_distinct_children_concurrently() -> anyhow::Result<()> {
    let db = fixture(2)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 2)?;
    for (execution, merge) in ORDERS {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        for class in &classes {
            install(&router, *class, Script::Complete)?;
        }
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            BUDGET,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                together(
                    classes
                        .iter()
                        .map(|class| Box::pin(router.demand_static_mro(&db, &work, *class, None)))
                        .collect(),
                    true,
                )
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
            },
        ))?;
        assert_complete(&snapshot);
        let answers = checked(
            snapshot
                .consumer
                .ok_or_else(|| anyhow::anyhow!("missing consumer"))?,
        )?;
        assert_ne!(answers[0], answers[1]);
        assert_eq!(snapshot.static_mro_starts.len(), 2);
        assert_eq!(snapshot.static_mro_counts.reply_deliveries, 2);
        assert_eq!(snapshot.static_mro_counts.reply_reads, 2);
        assert_eq!(snapshot.static_mro_counts.graph_passes, 0);
        assert_eq!(snapshot.static_mro_counts.sweeps, 0);
    }
    Ok(())
}

#[test]
fn request_identity_preserves_none_identity_and_distinct_specializations() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    let context = ClassLiteral::Static(classes[0])
        .generic_context(&db)
        .ok_or_else(|| anyhow::anyhow!("fixture should be generic"))?;
    let specializations = [
        None,
        Some(context.identity_specialization(&db)),
        Some(context.specialize(&db, &[Type::int_literal(1)])),
        Some(context.specialize(&db, &[Type::int_literal(2)])),
    ];
    let router = checked(Router::with_declarations(&db, &env, prepared))?;
    let root = request(&router, classes[0])?;
    for specialization in specializations {
        router.static_mro.borrow_mut().controlled.insert(
            StaticMroRequest {
                specialization,
                ..root
            },
            Script::Complete,
        );
    }
    let snapshot = checked(run_with(
        &db,
        &env,
        &router,
        BUDGET,
        false,
        false,
        |router| async {
            let work = PreparedMroWork::consumer(router);
            let mut answers = Vec::new();
            for specialization in specializations {
                let first = router
                    .demand_static_mro(&db, &work, classes[0], specialization)
                    .await?;
                let cached = router
                    .demand_static_mro(&db, &work, classes[0], specialization)
                    .await?;
                assert_eq!(first, cached);
                answers.push(first);
            }
            Ok::<_, LookupFailure<'_>>(answers)
        },
    ))?;
    assert_complete(&snapshot);
    let answers = checked(
        snapshot
            .consumer
            .ok_or_else(|| anyhow::anyhow!("missing consumer"))?,
    )?;
    assert_eq!(
        answers.into_iter().collect::<FxHashSet<_>>().len(),
        specializations.len()
    );
    assert_eq!(snapshot.static_mro_starts.len(), specializations.len());
    assert_eq!(
        snapshot.static_mro_counts.reply_allocations,
        2 * specializations.len()
    );
    assert_eq!(
        snapshot.static_mro_counts.result_publications,
        specializations.len()
    );
    Ok(())
}

#[test]
fn rejected_second_task_child_wakes_its_owner_while_first_is_parked() -> anyhow::Result<()> {
    let db = fixture(3)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 3)?;
    for (execution, merge) in ORDERS {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        let rejected = Rc::new(Cell::new(false));
        let root = install(
            &router,
            classes[0],
            Script::RejectSecond {
                first: classes[1],
                second: classes[2],
                rejected: Rc::clone(&rejected),
            },
        )?;
        let parked = install(&router, classes[1], Script::Park)?;
        let other = install(&router, classes[2], Script::Complete)?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            BUDGET,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                router.demand_static_mro(&db, &work, classes[0], None).await
            },
        ))?;
        assert!(rejected.get());
        assert!(matches!(snapshot.consumer, Some(Ok(_))));
        assert!(!snapshot.graph.exhausted);
        assert_eq!(snapshot.static_mro_starts.get(&root), Some(&1));
        assert_eq!(snapshot.static_mro_starts.get(&parked), Some(&1));
        assert!(!snapshot.static_mro_starts.contains_key(&other));
        assert_eq!(snapshot.graph.static_mro_pending.len(), 1);
        assert!(snapshot.graph.static_mro_pending.contains(&parked));
        assert_eq!(snapshot.static_mro_counts.reply_deliveries, 2);
        assert_eq!(snapshot.static_mro_counts.graph_passes, 0);
        assert_eq!(router.static_mro.borrow().edges, 0);
        assert!(router.static_mro.borrow().pending.is_empty());
    }
    Ok(())
}

#[test]
fn exact_self_and_multi_node_cycles_finish_under_every_order() -> anyhow::Result<()> {
    let db = fixture(4)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 4)?;
    for size in [1, 3] {
        for (execution, merge) in ORDERS {
            let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
            for index in 0..size {
                install(
                    &router,
                    classes[index],
                    Script::Await {
                        child: classes[(index + 1) % size],
                        copies: 2,
                    },
                )?;
            }
            install(
                &router,
                classes[3],
                Script::Await {
                    child: classes[0],
                    copies: 1,
                },
            )?;
            let snapshot = checked(run_with(
                &db,
                &env,
                &router,
                BUDGET,
                execution,
                merge,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let result = router
                        .demand_static_mro(&db, &work, classes[3], None)
                        .await?;
                    router.static_mro_is_cycle(&work, result).await
                },
            ))?;
            assert_complete(&snapshot);
            assert!(matches!(
                snapshot.consumer,
                Some(Err(LookupFailure::Unsupported(
                    LookupOperation::MroCycleRecovery
                )))
            ));
            assert_eq!(snapshot.static_mro_starts.len(), size + 1);
            assert_eq!(snapshot.static_mro_counts.graph_passes, 1);
            assert_eq!(snapshot.static_mro_counts.graph_nodes, size + 1);
            let state = router.static_mro.borrow();
            assert!(state.outcomes.iter().all(|outcome| matches!(
                outcome,
                Err(LookupFailure::Unsupported(
                    LookupOperation::MroCycleRecovery
                ))
            )));
            assert!(state.active.is_empty());
            assert!(state.pending.is_empty());
            assert_eq!(state.edges, 0);
        }
    }
    Ok(())
}

#[test]
fn fixed_payload_chains_and_shared_children_do_no_graph_or_cleanup_scans() -> anyhow::Result<()> {
    let db = fixture(33)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 33)?;
    for width in [false, true] {
        let mut previous_work = 0;
        let mut previous_probes = 0;
        for size in [8, 16, 32] {
            let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
            for index in 0..size {
                install(
                    &router,
                    classes[index],
                    Script::Await {
                        child: classes[if width { size } else { index + 1 }],
                        copies: 1,
                    },
                )?;
            }
            let leaf = install(&router, classes[size], Script::Complete)?;
            let started = Instant::now();
            let snapshot = checked(run_with(
                &db,
                &env,
                &router,
                BUDGET,
                false,
                false,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let roots = if width { size } else { 1 };
                    together(
                        classes[..roots]
                            .iter()
                            .map(|class| {
                                Box::pin(router.demand_static_mro(&db, &work, *class, None))
                            })
                            .collect(),
                        true,
                    )
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                },
            ))?;
            let elapsed = started.elapsed();
            assert_complete(&snapshot);
            assert!(matches!(snapshot.consumer, Some(Ok(_))));
            assert_eq!(snapshot.static_mro_starts.len(), size + 1);
            assert_eq!(snapshot.static_mro_polls.get(&leaf), Some(&1));
            assert_eq!(snapshot.static_mro_counts.graph_passes, 0);
            assert_eq!(snapshot.static_mro_counts.graph_nodes, 0);
            assert_eq!(snapshot.static_mro_counts.sweeps, 0);
            assert_eq!(snapshot.static_mro_counts.swept_replies, 0);
            let instances = if width { 2 * size } else { size + 1 };
            assert_eq!(snapshot.static_mro_counts.reply_allocations, instances);
            assert_eq!(snapshot.static_mro_counts.reply_deliveries, instances);
            assert_eq!(snapshot.static_mro_counts.reply_reads, instances);
            assert_eq!(snapshot.static_mro_counts.result_publications, size + 1);
            assert!(snapshot.static_mro_counts.peak_nodes <= size + 1);
            assert!(snapshot.static_mro_counts.peak_replies <= instances);
            assert!(snapshot.work() > previous_work);
            assert!(snapshot.static_mro_counts.request_probes > previous_probes);
            if previous_probes != 0 {
                assert!(snapshot.static_mro_counts.request_probes <= 2 * previous_probes + 2);
            }
            previous_work = snapshot.work();
            previous_probes = snapshot.static_mro_counts.request_probes;
            let state = router.static_mro.borrow();
            assert_eq!(state.outcomes.len(), size + 1);
            assert!(
                state
                    .outcomes
                    .iter()
                    .all(|outcome| matches!(outcome, Ok(Ok(mro)) if mro.len() == 1))
            );
            assert!(state.active.is_empty());
            assert!(state.pending.is_empty());
            eprintln!(
                "MRO scheduler width={width} size={size}: elapsed={elapsed:?}, units={}, probes={}, outcomes={}, slots={}",
                snapshot.work(),
                snapshot.static_mro_counts.request_probes,
                state.outcomes.len(),
                state.slots.len()
            );
        }
    }
    Ok(())
}

fn lease<'db>(
    state: &mut State<'db>,
    owner: ReplyOwner<'db>,
) -> Result<MroDemandLease<'db>, Boundary> {
    let dirty = Rc::clone(
        state
            .cleanup_dirty
            .get_or_insert_with(|| Rc::new(Cell::new(false))),
    );
    let poll = match owner {
        ReplyOwner::Consumer => Rc::clone(
            state
                .consumer_poll
                .get_or_insert_with(|| Rc::new(ReplyPollState::default())),
        ),
        ReplyOwner::Task { node, .. } => Rc::clone(&state.node(node)?.poll),
    };
    poll.outstanding.set(
        poll.outstanding
            .get()
            .checked_add(1)
            .ok_or(Boundary::CostOverflow)?,
    );
    Ok(MroDemandLease {
        reply: Rc::new(MroReply {
            answer: Cell::new(None),
            owner,
            poll,
            future_live: Cell::new(true),
            global_index: Cell::new(None),
            child_index: Cell::new(None),
            child: Cell::new(None),
            cleanup_dirty: dirty,
        }),
        released: false,
    })
}

fn demand<'db>(child: StaticMroRequest<'db>, lease: &MroDemandLease<'db>) -> Demand<'db> {
    Demand {
        child,
        reply: Rc::clone(&lease.reply),
    }
}

fn commit<'db>(
    state: &mut State<'db>,
    round: Round<'db>,
    evaluation: usize,
) -> anyhow::Result<Commit<'db>> {
    let mut work = 0;
    let commit = checked(state.commit_round(
        round,
        evaluation,
        true,
        &mut Budget {
            work: &mut work,
            limit: BUDGET,
        },
    ))?
    .ok_or_else(|| anyhow::anyhow!("unexpected budget exhaustion"))?;
    for (_, node) in &commit.spawned {
        checked(state.activate(*node))?;
    }
    Ok(commit)
}

fn completion(request: StaticMroRequest<'_>, node: MroNodeId) -> Completion<'_> {
    Completion {
        request,
        node,
        outcome: Ok(Ok(Mro::from(vec![ClassBase::Any]))),
    }
}

#[test]
fn completion_round_subscribers_share_one_committed_payload_and_one_wakeup() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let (_, classes) = prepared(&db, 1)?;
    let request = StaticMroRequest {
        evaluation: 7,
        class: classes[0],
        specialization: None,
    };
    let mut state = State::default();
    let first = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let second = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let registered = commit(
        &mut state,
        Round {
            demands: vec![demand(request, &first), demand(request, &second)],
            completed: vec![],
        },
        7,
    )?;
    assert_eq!(registered.spawned.len(), 1);
    let node = registered.spawned[0].1;
    assert_eq!(checked(state.fanout(node))?, 1);
    assert_eq!(state.pending.len(), 2);
    assert_eq!(first.reply.poll.pending.get(), 2);
    let third = checked(lease(&mut state, ReplyOwner::Consumer))?;
    checked(state.retire_owner(Some(node)))?;
    let completed = commit(
        &mut state,
        Round {
            demands: vec![demand(request, &third)],
            completed: vec![completion(request, node)],
        },
        7,
    )?;
    assert!(completed.spawned.is_empty());
    assert_eq!(completed.wakeups.len(), 1);
    assert!(completed.wakeups.contains(&Key::Consumer));
    assert_eq!(completed.deliveries.len(), 3);
    assert_eq!(state.outcomes.len(), 1);
    assert!(state.pending.is_empty());
    assert!(state.active.is_empty());
    assert_eq!(first.reply.poll.pending.get(), 0);
    assert_eq!(checked(state.poll_units(None))?, 3 * 32);
    for pending in [&first, &second, &third] {
        assert!(pending.reply.answer.get().is_none());
    }
    state.deliver(completed, true);
    let answer = first
        .reply
        .answer
        .get()
        .ok_or_else(|| anyhow::anyhow!("missing reply"))?;
    assert_eq!(second.reply.answer.get(), Some(answer));
    assert_eq!(third.reply.answer.get(), Some(answer));
    drop((first, second, third));
    assert_eq!(checked(state.poll_units(None))?, 0);
    assert!(
        !state
            .cleanup_dirty
            .as_ref()
            .is_some_and(|dirty| dirty.get())
    );
    assert_eq!(state.counters.sweeps, 0);
    Ok(())
}

#[test]
fn dropping_instances_removes_only_their_registered_edge_counts() -> anyhow::Result<()> {
    let db = fixture(3)?;
    let (_, classes) = prepared(&db, 3)?;
    let requests = classes
        .iter()
        .map(|class| StaticMroRequest {
            evaluation: 9,
            class: *class,
            specialization: None,
        })
        .collect::<Vec<_>>();
    let mut state = State::default();
    let root_reply = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let root = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[0], &root_reply)],
            completed: vec![],
        },
        9,
    )?
    .spawned[0]
        .1;
    let owner = ReplyOwner::Task {
        request: requests[0],
        node: root,
    };
    let first = checked(lease(&mut state, owner))?;
    let second = checked(lease(&mut state, owner))?;
    let child = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[1], &first), demand(requests[1], &second)],
            completed: vec![],
        },
        9,
    )?
    .spawned[0]
        .1;
    assert_eq!(state.edges, 1);
    assert_eq!(
        checked(state.node(root))?
            .waiting_on
            .map(|edge| edge.instances),
        Some(2)
    );
    assert_eq!(checked(state.node(child))?.incoming.len(), 1);
    assert_eq!(checked(state.fanout(child))?, 1);
    drop(first);
    let mut units = 0;
    assert!(checked(state.cleanup(
        true,
        &mut Budget {
            work: &mut units,
            limit: BUDGET
        }
    ))?);
    assert_eq!(state.edges, 1);
    assert_eq!(
        checked(state.node(root))?
            .waiting_on
            .map(|edge| edge.instances),
        Some(1)
    );
    assert_eq!(second.reply.poll.pending.get(), 1);
    drop(second);
    let replacement = checked(lease(&mut state, owner))?;
    let registered = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[2], &replacement)],
            completed: vec![],
        },
        9,
    )?;
    assert_eq!(registered.spawned.len(), 1);
    assert!(registered.deliveries.is_empty());
    assert_eq!(
        checked(state.node(root))?.waiting_on.map(|edge| edge.child),
        Some(registered.spawned[0].1)
    );
    assert!(checked(state.node(child))?.incoming.is_empty());
    assert_eq!(state.edges, 1);
    assert_eq!(state.counters.sweeps, 2);
    assert_eq!(state.counters.swept_replies, 5);
    Ok(())
}

#[test]
fn cancellation_before_commit_allocates_no_subscription_or_task() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let (_, classes) = prepared(&db, 1)?;
    let request = StaticMroRequest {
        evaluation: 11,
        class: classes[0],
        specialization: None,
    };
    let mut state = State::default();
    let instance = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let registration = demand(request, &instance);
    drop(instance);
    let committed = commit(
        &mut state,
        Round {
            demands: vec![registration],
            completed: vec![],
        },
        11,
    )?;
    assert!(committed.spawned.is_empty());
    assert!(committed.deliveries.is_empty());
    assert!(state.pending.is_empty());
    assert!(state.entries.is_empty());
    assert_eq!(state.counters.request_probes, 0);
    assert_eq!(state.counters.sweeps, 0);
    assert_eq!(checked(state.poll_units(None))?, 0);
    Ok(())
}

#[test]
fn retained_leases_cannot_keep_retired_or_recycled_owners_alive() -> anyhow::Result<()> {
    let db = fixture(3)?;
    let (_, classes) = prepared(&db, 3)?;
    let requests = classes
        .iter()
        .map(|class| StaticMroRequest {
            evaluation: 13,
            class: *class,
            specialization: None,
        })
        .collect::<Vec<_>>();
    let mut state = State::default();
    let root_reply = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let root = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[0], &root_reply)],
            completed: vec![],
        },
        13,
    )?
    .spawned[0]
        .1;
    let owner = ReplyOwner::Task {
        request: requests[0],
        node: root,
    };
    let retained = checked(lease(&mut state, owner))?;
    let child = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[1], &retained)],
            completed: vec![],
        },
        13,
    )?
    .spawned[0]
        .1;
    let old_poll = Rc::clone(&retained.reply.poll);
    checked(state.retire_owner(Some(root)))?;
    let completed = commit(
        &mut state,
        Round {
            demands: vec![],
            completed: vec![completion(requests[0], root)],
        },
        13,
    )?;
    state.deliver(completed, true);
    assert_eq!(old_poll.pending.get(), 0);
    assert_eq!(old_poll.outstanding.get(), 1);
    assert!(!state.owner_live(owner, true));
    assert!(retained.reply.answer.get().is_none());
    let replacement = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let fresh = commit(
        &mut state,
        Round {
            demands: vec![demand(requests[2], &replacement)],
            completed: vec![],
        },
        13,
    )?
    .spawned[0]
        .1;
    assert_eq!(fresh.slot, root.slot);
    assert_ne!(fresh.generation, root.generation);
    assert!(!Rc::ptr_eq(&old_poll, &checked(state.node(fresh))?.poll));
    drop(retained);
    assert_eq!(old_poll.outstanding.get(), 0);
    assert_eq!(checked(state.node(fresh))?.poll.outstanding.get(), 0);
    checked(state.retire_owner(Some(child)))?;
    let completed = commit(
        &mut state,
        Round {
            demands: vec![],
            completed: vec![completion(requests[1], child)],
        },
        13,
    )?;
    assert!(completed.deliveries.is_empty());
    assert_eq!(state.edges, 0);
    Ok(())
}

type PendingPair<'db> = (
    State<'db>,
    Vec<MroDemandLease<'db>>,
    Vec<(StaticMroRequest<'db>, MroNodeId)>,
);

fn pending_pair<'db>(classes: &[StaticClassLiteral<'db>]) -> anyhow::Result<PendingPair<'db>> {
    let mut state = State::default();
    let mut leases = Vec::new();
    let mut demands = Vec::new();
    for class in &classes[..2] {
        let instance = checked(lease(&mut state, ReplyOwner::Consumer))?;
        demands.push(demand(
            StaticMroRequest {
                evaluation: 17,
                class: *class,
                specialization: None,
            },
            &instance,
        ));
        leases.push(instance);
    }
    let registered = commit(
        &mut state,
        Round {
            demands,
            completed: vec![],
        },
        17,
    )?;
    for (_, node) in &registered.spawned {
        checked(state.retire_owner(Some(*node)))?;
    }
    Ok((state, leases, registered.spawned))
}

#[test]
fn publication_and_delivery_are_atomic_at_the_budget_boundary() -> anyhow::Result<()> {
    let db = fixture(2)?;
    let (_, classes) = prepared(&db, 2)?;
    let (mut full, _leases, nodes) = pending_pair(&classes)?;
    let mut required = 0;
    let completed = nodes
        .iter()
        .map(|(request, node)| completion(*request, *node))
        .collect();
    assert!(
        checked(full.commit_round(
            Round {
                demands: vec![],
                completed
            },
            17,
            true,
            &mut Budget {
                work: &mut required,
                limit: BUDGET
            }
        ))?
        .is_some()
    );
    assert!(required > 0);
    for limit in [0, required - 1] {
        let (mut state, leases, nodes) = pending_pair(&classes)?;
        let mut work = 0;
        let completed = nodes
            .iter()
            .map(|(request, node)| completion(*request, *node))
            .collect();
        assert!(
            checked(state.commit_round(
                Round {
                    demands: vec![],
                    completed
                },
                17,
                true,
                &mut Budget {
                    work: &mut work,
                    limit
                }
            ))?
            .is_none()
        );
        assert!(state.outcomes.is_empty());
        assert!(state.entries.values().all(|entry| entry.answer.is_none()));
        assert_eq!(state.pending.len(), 2);
        assert!(
            leases
                .iter()
                .all(|lease| lease.reply.answer.get().is_none())
        );
        assert_eq!(state.counters.reply_deliveries, 0);
        let completed = nodes
            .iter()
            .map(|(request, node)| completion(*request, *node))
            .collect();
        let committed = checked(state.commit_round(
            Round {
                demands: vec![],
                completed,
            },
            17,
            true,
            &mut Budget {
                work: &mut work,
                limit: required,
            },
        ))?
        .ok_or_else(|| anyhow::anyhow!("publication retry should fit exactly"))?;
        assert_eq!(work, required);
        assert_eq!(state.outcomes.len(), 2);
        assert_eq!(committed.deliveries.len(), 2);
        assert!(
            leases
                .iter()
                .all(|lease| lease.reply.answer.get().is_none())
        );
        state.deliver(committed, true);
        assert!(
            leases
                .iter()
                .all(|lease| lease.reply.answer.get().is_some())
        );
        assert_eq!(state.counters.reply_deliveries, 2);
    }
    Ok(())
}

#[test]
fn graph_and_reply_reservations_reject_checked_overflow() -> anyhow::Result<()> {
    assert_eq!(graph_pass_units(usize::MAX, 0), Err(Boundary::CostOverflow));
    assert_eq!(graph_pass_units(0, usize::MAX), Err(Boundary::CostOverflow));
    assert_eq!(table_probe_units(usize::MAX), Err(Boundary::CostOverflow));
    let mut work = usize::MAX;
    assert_eq!(
        Budget {
            work: &mut work,
            limit: usize::MAX
        }
        .reserve(1),
        Err(Boundary::CostOverflow)
    );
    let mut state = State::default();
    let instance = checked(lease(&mut state, ReplyOwner::Consumer))?;
    instance.reply.poll.outstanding.set(usize::MAX);
    assert_eq!(state.poll_units(None), Err(Boundary::CostOverflow));
    instance.reply.poll.outstanding.set(1);
    instance.reply.poll.epoch.set(usize::MAX);
    state.consumer_epoch = usize::MAX;
    assert_eq!(state.begin_poll(None), Err(Boundary::CostOverflow));

    let db = fixture(1)?;
    let (_, classes) = prepared(&db, 1)?;
    let mut state = State::default();
    state.slots.push(Slot {
        generation: usize::MAX,
        node: None,
    });
    state.free_slots.push(0);
    let instance = checked(lease(&mut state, ReplyOwner::Consumer))?;
    let child = StaticMroRequest {
        evaluation: 17,
        class: classes[0],
        specialization: None,
    };
    let mut work = 0;
    assert!(matches!(
        state.commit_round(
            Round {
                demands: vec![demand(child, &instance)],
                completed: vec![],
            },
            17,
            true,
            &mut Budget {
                work: &mut work,
                limit: BUDGET
            },
        ),
        Err(Boundary::CostOverflow),
    ));
    assert_eq!(state.free_slots, [0]);
    assert!(state.entries.is_empty());
    assert!(state.outcomes.is_empty());
    assert!(state.pending.is_empty());
    assert!(instance.reply.answer.get().is_none());
    Ok(())
}

#[test]
fn completed_and_exhausted_drivers_reject_old_owner_operations() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    for exhaust in [false, true] {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        let owner = Rc::new(Cell::new(None));
        install(
            &router,
            classes[0],
            Script::RetainOwner {
                owner: Rc::clone(&owner),
                exhaust,
            },
        )?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            100_000,
            false,
            false,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                router.demand_static_mro(&db, &work, classes[0], None).await
            },
        ))?;
        assert_eq!(snapshot.graph.exhausted, exhaust);
        let (request, node) = owner
            .get()
            .ok_or_else(|| anyhow::anyhow!("task never started"))?;
        assert!(!router.driver_is_live());
        assert!(!router.consumer_active.get());
        assert_eq!(
            router.validate_static_mro_owner(request, node),
            Err(Boundary::MroDomain)
        );
        let sequence = Cell::new(0);
        let task_work = PreparedMroWork::task(&router, request, node, &sequence);
        let consumer_work = PreparedMroWork::consumer(&router);
        let id = snapshot
            .graph
            .static_mro_values
            .get(&request)
            .copied()
            .unwrap_or(StaticMroResultId {
                evaluation: request.evaluation,
                slot: 0,
            });
        for work in [&task_work, &consumer_work] {
            let effects = router.effects.borrow().len();
            let mut cx = Context::from_waker(Waker::noop());
            assert_eq!(
                Box::pin(work.checkpoint(1)).as_mut().poll(&mut cx),
                Poll::Ready(Err(Boundary::MroDomain))
            );
            assert_eq!(
                Box::pin(router.demand_static_mro(&db, work, classes[0], None))
                    .as_mut()
                    .poll(&mut cx),
                Poll::Ready(Err(LookupFailure::Boundary(Boundary::MroDomain)))
            );
            assert_eq!(
                Box::pin(router.static_mro_is_cycle(work, id))
                    .as_mut()
                    .poll(&mut cx),
                Poll::Ready(Err(LookupFailure::Boundary(Boundary::MroDomain)))
            );
            assert_eq!(router.effects.borrow().len(), effects);
        }
        let fresh = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        assert_ne!(fresh.evaluation_domain.0, router.evaluation_domain.0);
        let request = install(&fresh, classes[0], Script::Complete)?;
        let result = checked(run_with(
            &db,
            &env,
            &fresh,
            BUDGET,
            false,
            false,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                let valid = router
                    .demand_static_mro(&db, &work, classes[0], None)
                    .await?;
                assert!(matches!(
                    router.static_mro_is_cycle(&work, id).await,
                    Err(LookupFailure::Boundary(Boundary::MroDomain))
                ));
                assert_ne!(valid.evaluation, id.evaluation);
                router.static_mro_is_cycle(&work, valid).await
            },
        ))?;
        assert_complete(&result);
        assert_eq!(result.consumer, Some(Ok(false)));
        assert_eq!(result.static_mro_starts.get(&request), Some(&1));
    }
    Ok(())
}

#[test]
fn cancellation_at_driver_reservations_leaves_no_partial_publication() -> anyhow::Result<()> {
    let db = fixture(1)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 1)?;
    let sample = |budget| -> anyhow::Result<_> {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        install(&router, classes[0], Script::Complete)?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            budget,
            false,
            false,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                together(
                    (0..4)
                        .map(|_| Box::pin(router.demand_static_mro(&db, &work, classes[0], None)))
                        .collect(),
                    true,
                )
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
            },
        ))?;
        assert!(snapshot.work() <= budget);
        assert_eq!(
            snapshot.graph.static_mro_values.len(),
            snapshot.static_mro_counts.result_publications
        );
        assert!(
            snapshot.static_mro_counts.reply_deliveries == 0
                || snapshot.static_mro_counts.reply_deliveries == 4
        );
        assert!(router.static_mro.borrow().pending.is_empty());
        assert!(router.static_mro.borrow().active.is_empty());
        assert!(!router.driver_is_live());
        let work = PreparedMroWork::consumer(&router);
        let effects = router.effects.borrow().len();
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(
            Box::pin(router.demand_static_mro(&db, &work, classes[0], None))
                .as_mut()
                .poll(&mut cx),
            Poll::Ready(Err(LookupFailure::Boundary(Boundary::MroDomain)))
        );
        assert_eq!(router.effects.borrow().len(), effects);
        Ok(snapshot)
    };
    let full = sample(BUDGET)?;
    assert_complete(&full);
    let mut budgets = vec![0, full.work().saturating_sub(1), full.work()];
    for boundary in &full.graph.boundaries {
        budgets.extend([boundary.saturating_sub(1), *boundary, boundary + 1]);
    }
    budgets.sort_unstable();
    budgets.dedup();
    let mut before_registration = false;
    let mut before_publication = false;
    let mut before_consumption = false;
    for budget in budgets {
        let partial = sample(budget)?;
        before_registration |= partial.static_mro_counts.reply_allocations != 0
            && partial.static_mro_starts.is_empty();
        before_publication |= !partial.static_mro_starts.is_empty()
            && partial.static_mro_counts.result_publications == 0;
        before_consumption |=
            partial.static_mro_counts.reply_deliveries != 0 && partial.consumer.is_none();
    }
    assert!(before_registration);
    assert!(before_publication);
    assert!(before_consumption);
    assert_complete(&sample(full.work())?);
    Ok(())
}

#[test]
fn cycle_analysis_and_publication_require_separate_complete_reservations() -> anyhow::Result<()> {
    let db = fixture(2)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 2)?;
    for (execution, merge) in ORDERS {
        let sample = |budget| -> anyhow::Result<_> {
            let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
            for index in 0..2 {
                install(
                    &router,
                    classes[index],
                    Script::Await {
                        child: classes[1 - index],
                        copies: 1,
                    },
                )?;
            }
            let snapshot = checked(run_with(
                &db,
                &env,
                &router,
                budget,
                execution,
                merge,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let answer = router
                        .demand_static_mro(&db, &work, classes[0], None)
                        .await?;
                    router.static_mro_is_cycle(&work, answer).await
                },
            ))?;
            assert!(snapshot.work() <= budget);
            assert!(matches!(
                snapshot.static_mro_counts.result_publications,
                0 | 2
            ));
            assert_eq!(
                snapshot.graph.static_mro_values.len(),
                snapshot.static_mro_counts.result_publications
            );
            if snapshot.static_mro_counts.result_publications == 0 {
                assert_eq!(snapshot.static_mro_counts.reply_deliveries, 0);
                assert!(snapshot.consumer.is_none());
                assert!(router.static_mro.borrow().outcomes.is_empty());
            } else {
                assert!(
                    router
                        .static_mro
                        .borrow()
                        .outcomes
                        .iter()
                        .all(|outcome| matches!(
                            outcome,
                            Err(LookupFailure::Unsupported(
                                LookupOperation::MroCycleRecovery
                            ))
                        ))
                );
            }
            Ok(snapshot)
        };
        let full = sample(BUDGET)?;
        assert_complete(&full);
        assert!(matches!(
            full.consumer,
            Some(Err(LookupFailure::Unsupported(
                LookupOperation::MroCycleRecovery
            )))
        ));
        let graph_units = checked(graph_pass_units(2, 2))?;
        let mut budgets = Vec::new();
        for boundary in &full.graph.boundaries {
            let after_graph = boundary + graph_units;
            budgets.extend([
                boundary.saturating_sub(1),
                *boundary,
                after_graph - 1,
                after_graph,
                after_graph + 1,
            ]);
        }
        budgets.sort_unstable();
        budgets.dedup();
        let mut before_graph = false;
        let mut before_publication = false;
        for budget in budgets {
            let partial = sample(budget)?;
            before_graph |= partial.static_mro_counts.peak_replies == 3
                && partial.static_mro_counts.graph_passes == 0;
            before_publication |= partial.static_mro_counts.graph_passes == 1
                && partial.static_mro_counts.result_publications == 0;
        }
        assert!(before_graph, "execution={execution}, merge={merge}");
        assert!(before_publication, "execution={execution}, merge={merge}");
        let retry = sample(BUDGET)?;
        assert_complete(&retry);
        assert_eq!(retry.static_mro_counts.graph_passes, 1);
        assert_eq!(retry.static_mro_counts.result_publications, 2);
    }
    Ok(())
}

#[test]
fn independent_nonquiescent_work_defers_cycle_analysis_until_budget_exhaustion()
-> anyhow::Result<()> {
    let db = fixture(2)?;
    let env = db.program_environment();
    let (prepared, classes) = prepared(&db, 2)?;
    for (execution, merge) in ORDERS {
        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        let cycle = install(
            &router,
            classes[0],
            Script::Await {
                child: classes[0],
                copies: 1,
            },
        )?;
        let spinning = install(
            &router,
            classes[1],
            Script::RetainOwner {
                owner: Rc::new(Cell::new(None)),
                exhaust: true,
            },
        )?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &router,
            100_000,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                together(
                    classes
                        .iter()
                        .map(|class| Box::pin(router.demand_static_mro(&db, &work, *class, None)))
                        .collect(),
                    true,
                )
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
            },
        ))?;
        assert!(snapshot.graph.exhausted);
        assert!(snapshot.consumer.is_none());
        assert_eq!(snapshot.static_mro_starts.get(&cycle), Some(&1));
        assert_eq!(snapshot.static_mro_starts.get(&spinning), Some(&1));
        assert_eq!(snapshot.static_mro_counts.graph_passes, 0);
        assert_eq!(snapshot.static_mro_counts.graph_nodes, 0);
        assert_eq!(snapshot.static_mro_counts.result_publications, 0);
        assert!(snapshot.graph.static_mro_values.is_empty());
        assert_eq!(snapshot.graph.static_mro_pending.len(), 2);
        assert!(snapshot.graph.static_mro_pending.contains(&cycle));
        assert!(snapshot.graph.static_mro_pending.contains(&spinning));

        let router = checked(Router::with_declarations(&db, &env, Rc::clone(&prepared)))?;
        install(
            &router,
            classes[0],
            Script::Await {
                child: classes[0],
                copies: 1,
            },
        )?;
        let retry = checked(run_with(
            &db,
            &env,
            &router,
            BUDGET,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                let answer = router
                    .demand_static_mro(&db, &work, classes[0], None)
                    .await?;
                router.static_mro_is_cycle(&work, answer).await
            },
        ))?;
        assert_complete(&retry);
        assert!(matches!(
            retry.consumer,
            Some(Err(LookupFailure::Unsupported(
                LookupOperation::MroCycleRecovery
            )))
        ));
        assert_eq!(retry.static_mro_counts.graph_passes, 1);
    }
    Ok(())
}

#[test]
fn salsa_unwind_releases_tasks_replies_and_execution_capabilities() -> anyhow::Result<()> {
    let db = fixture(2)?;
    let env = db.program_environment();
    let (declarations, classes) = prepared(&db, 2)?;
    for (execution, merge) in ORDERS {
        let baseline = checked(Router::with_declarations(
            &db,
            &env,
            Rc::clone(&declarations),
        ))?;
        install(
            &baseline,
            classes[0],
            Script::Await {
                child: classes[1],
                copies: 2,
            },
        )?;
        install(
            &baseline,
            classes[1],
            Script::RetainOwner {
                owner: Rc::new(Cell::new(None)),
                exhaust: true,
            },
        )?;
        let snapshot = checked(run_with(
            &db,
            &env,
            &baseline,
            50_000,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                router.demand_static_mro(&db, &work, classes[0], None).await
            },
        ))?;
        assert!(snapshot.graph.exhausted);
        assert_eq!(snapshot.static_mro_counts.peak_nodes, 2);
        assert_eq!(snapshot.static_mro_counts.peak_replies, 3);
        let cut = *snapshot
            .graph
            .boundaries
            .last()
            .ok_or_else(|| anyhow::anyhow!("missing cancellation boundary"))?;

        // Local cancellation remains set after the unwind; source preparation belongs to a fresh database.
        let cancelled_db = fixture(2)?;
        let cancelled_env = cancelled_db.program_environment();
        let (prepared, cancelled_classes) = prepared(&cancelled_db, 2)?;
        let router = checked(Router::with_declarations(
            &cancelled_db,
            &cancelled_env,
            prepared,
        ))?;
        let root = install(
            &router,
            cancelled_classes[0],
            Script::Await {
                child: cancelled_classes[1],
                copies: 2,
            },
        )?;
        let owner = Rc::new(Cell::new(None));
        let spinning = install(
            &router,
            cancelled_classes[1],
            Script::RetainOwner {
                owner: Rc::clone(&owner),
                exhaust: true,
            },
        )?;
        router.cancel_at(cut, cancelled_db.cancellation_token());
        let published = Cell::new(false);
        let lifetime = Rc::new(());
        let retained = Rc::clone(&lifetime);
        let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            run_with(
                &cancelled_db,
                &cancelled_env,
                &router,
                BUDGET,
                execution,
                merge,
                |router| async {
                    let work = PreparedMroWork::consumer(router);
                    let answer = router
                        .demand_static_mro(&cancelled_db, &work, cancelled_classes[0], None)
                        .await;
                    drop(retained);
                    published.set(true);
                    answer
                },
            )
        }));
        assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
        assert!(!published.get());
        assert_eq!(Rc::strong_count(&lifetime), 1);
        assert!(!router.driver_is_live());
        assert!(!router.consumer_active.get());
        {
            let state = router.static_mro.borrow();
            assert_eq!(state.counters.peak_nodes, 2);
            assert_eq!(state.counters.peak_replies, 3);
            assert!(state.pending.is_empty());
            assert!(state.active.is_empty());
            assert!(state.slots.iter().all(|slot| slot.node.is_none()));
            assert_eq!(state.edges, 0);
            assert_eq!(state.notifications, 0);
            assert!(state.outcomes.is_empty());
            assert!(state.entries[&root].answer.is_none());
            assert!(state.entries[&spinning].answer.is_none());
            let consumer = state
                .consumer_poll
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing consumer poll state"))?;
            assert_eq!(consumer.outstanding.get(), 0);
            assert_eq!(consumer.pending.get(), 0);
        }
        let (request, node) = owner
            .get()
            .ok_or_else(|| anyhow::anyhow!("spinning owner never started"))?;
        let sequence = Cell::new(0);
        let task_work = PreparedMroWork::task(&router, request, node, &sequence);
        let consumer_work = PreparedMroWork::consumer(&router);
        let id = StaticMroResultId {
            evaluation: request.evaluation,
            slot: 0,
        };
        for work in [&task_work, &consumer_work] {
            let effects = router.effects.borrow().len();
            let mut cx = Context::from_waker(Waker::noop());
            assert_eq!(
                Box::pin(work.checkpoint(1)).as_mut().poll(&mut cx),
                Poll::Ready(Err(Boundary::MroDomain))
            );
            assert_eq!(
                Box::pin(router.demand_static_mro(&cancelled_db, work, cancelled_classes[0], None))
                    .as_mut()
                    .poll(&mut cx),
                Poll::Ready(Err(LookupFailure::Boundary(Boundary::MroDomain)))
            );
            assert_eq!(
                Box::pin(router.static_mro_is_cycle(work, id))
                    .as_mut()
                    .poll(&mut cx),
                Poll::Ready(Err(LookupFailure::Boundary(Boundary::MroDomain)))
            );
            assert_eq!(router.effects.borrow().len(), effects);
        }

        let fresh = checked(Router::with_declarations(
            &db,
            &env,
            Rc::clone(&declarations),
        ))?;
        install(
            &fresh,
            classes[0],
            Script::Await {
                child: classes[1],
                copies: 2,
            },
        )?;
        install(&fresh, classes[1], Script::Complete)?;
        let retry = checked(run_with(
            &db,
            &env,
            &fresh,
            BUDGET,
            execution,
            merge,
            |router| async {
                let work = PreparedMroWork::consumer(router);
                let answer = router
                    .demand_static_mro(&db, &work, classes[0], None)
                    .await?;
                router.static_mro_is_cycle(&work, answer).await
            },
        ))?;
        assert_complete(&retry);
        assert_eq!(retry.consumer, Some(Ok(false)));
    }
    Ok(())
}
