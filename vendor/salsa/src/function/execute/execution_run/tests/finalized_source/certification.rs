use super::*;
use crate::attempt_probe::Incomplete;
use crate::function::execute::execution_run::registration::FinalSourceError;

#[crate::tracked(returns(copy))]
fn unclassified(db: &dyn Db, input: Number) -> u32 {
    input.value(db)
}

#[crate::tracked]
struct Product<'db> {
    value: u32,
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn creator(db: &dyn Db, input: Number) -> u32 {
    let value = input.value(db);
    Product::new(db, value);
    value
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn certify_in_query(db: &dyn Db, input: Number) -> bool {
    #[cfg(not(feature = "shuttle"))]
    concurrent::pause();
    assert_eq!(
        FinalSourceMemo::certify(db, source::fn_ingredient_(db, db.zalsa()), input.as_id())
            .unwrap_err(),
        if attempt_probe::current().is_some() {
            FinalSourceError::ActiveAttempt
        } else {
            FinalSourceError::ActiveQuery
        }
    );
    true
}

#[cfg(not(feature = "shuttle"))]
mod concurrent {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::function::SyncOwner;

    const TIMEOUT: Duration = Duration::from_secs(5);
    const TEST: &str = "function::execute::execution_run::tests::finalized_source::certification::concurrent::certification_overlaps_an_independent_evaluation";
    const CHILD: &str = "SALSA_CONCURRENT_SOURCE_CERTIFICATION_CHILD";

    thread_local! {
        static PAUSE: RefCell<Option<(mpsc::Sender<thread::ThreadId>, mpsc::Receiver<()>)>> = const { RefCell::new(None) };
    }

    pub(super) fn pause() {
        if let Some((entered, release)) = PAUSE.with_borrow_mut(Option::take) {
            entered.send(thread::current().id()).unwrap();
            release
                .recv_timeout(TIMEOUT)
                .expect("certification peer released");
        }
    }

    struct Release(Option<mpsc::Sender<()>>);

    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(release) = self.0.take() {
                let _ = release.send(());
            }
        }
    }

    fn overlap(refuse: bool) {
        let db = TestDb::default();
        let input = Number::new(&db, 7);
        let held = Number::new(&db, 13);
        assert_eq!(source(&db, input), 7);
        let ingredient = source::fn_ingredient_(&db, db.zalsa());
        let identity = std::ptr::from_ref(stored(&db, ingredient, input.as_id()));
        let (entered, receive) = mpsc::channel();
        let (release, released) = mpsc::channel();
        thread::scope(|scope| {
            let release = Release(Some(release));
            let worker_db = db.clone();
            let worker = scope.spawn(move || {
                PAUSE.with_borrow_mut(|slot| assert!(slot.replace((entered, released)).is_none()));
                let outcome = try_with_attempt(&worker_db, 1, || {
                    assert!(certify_in_query(&worker_db, held));
                    if refuse {
                        assert_eq!(
                            attempt_probe::charge(&worker_db, 2),
                            Err(Incomplete::Allowance)
                        );
                    }
                });
                assert!(attempt_probe::current().is_none());
                assert_idle(&worker_db);
                outcome
            });
            let owner = receive.recv_timeout(TIMEOUT).unwrap();
            let claim = certify_in_query::fn_ingredient_(&db, db.zalsa())
                .sync_table
                .test_transfer_state(held.as_id())
                .unwrap();
            assert!(matches!(claim.owner, SyncOwner::Thread(thread) if thread == owner));
            assert!(attempt_probe::current().is_none());
            assert!(!attempt_probe::is_incomplete(&db));
            let certificate =
                FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
            let admission = Admission::default();
            assert_eq!(
                try_with_attempt(&db, 100_000, || run_consumer(
                    &db,
                    ingredient,
                    std::slice::from_ref(&certificate),
                    input,
                    &admission
                )),
                Ok(AttemptOutcome::Complete(Ok(8)))
            );
            assert_eq!(
                try_with_attempt(&db, 100_000, || {
                    let mut registry = RegistryBuilder::new(&db, &admission)?;
                    let route = registry.register_final_source(
                        &db as &dyn Db,
                        ingredient,
                        &[certificate],
                    )?;
                    let revision = db.zalsa().current_revision();
                    let key = ingredient.database_key_index(input.as_id());
                    registry.seal()?.run(move |endpoint| async move {
                        assert!(matches!(
                            endpoint.validate(key, revision)?.await?,
                            VerifyResult::Unchanged { .. }
                        ));
                        assert_eq!(*endpoint.read_final_source(&route, input.as_id()).await, 7);
                        Ok(())
                    })
                }),
                Ok(AttemptOutcome::Complete(Ok(())))
            );
            let claim = certify_in_query::fn_ingredient_(&db, db.zalsa())
                .sync_table
                .test_transfer_state(held.as_id())
                .unwrap();
            assert!(matches!(claim.owner, SyncOwner::Thread(thread) if thread == owner));
            assert_eq!(
                std::ptr::from_ref(stored(&db, ingredient, input.as_id())),
                identity
            );
            assert_eq!(db.counts.bodies(), (1, 1));
            assert_eq!(
                stored(
                    &db,
                    consumer::fn_ingredient_(&db, db.zalsa()),
                    input.as_id()
                )
                .header
                .origin()
                .inputs()
                .collect::<Vec<_>>(),
                [ingredient.database_key_index(input.as_id())]
            );
            drop(release);
            assert_eq!(
                worker.join().unwrap(),
                if refuse {
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                } else {
                    Ok(AttemptOutcome::Complete(()))
                }
            );
        });
        assert_idle(&db);
        assert!(attempt_probe::current().is_none());
        assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
        assert!(
            certify_in_query::fn_ingredient_(&db, db.zalsa())
                .sync_table
                .test_transfer_state(held.as_id())
                .is_none()
        );
        assert_eq!(source(&db, input), 7);
        assert_eq!(consumer(&db, input), 8);
        assert_eq!(db.counts.bodies(), (1, 1));
        assert_eq!(
            std::ptr::from_ref(stored(&db, ingredient, input.as_id())),
            identity
        );
    }

    #[test]
    fn certification_overlaps_an_independent_evaluation() {
        if std::env::var(CHILD).as_deref() != Ok(TEST) {
            crate::function::execute::execution_run::tests::parallel_wait::watchdog(TEST, CHILD);
            return;
        }
        overlap(false);
        overlap(true);
    }
}

#[test]
fn certification_rejects_missing_stale_unclassified_and_output_memos() {
    let mut db = TestDb::default();
    let input = Number::new(&db, 7);
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap_err(),
        FinalSourceError::MissingMemo
    );
    assert_eq!(db.counts.bodies(), (0, 0));
    assert_eq!(source(&db, input), 7);
    assert!(certify_in_query(&db, input));
    assert_eq!(
        try_with_attempt(&db, 100, || {
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap_err()
        }),
        Ok(AttemptOutcome::Complete(FinalSourceError::ActiveAttempt))
    );
    let foreign = TestDb::default();
    let foreign_input = Number::new(&foreign, 3);
    assert_eq!(source(&foreign, foreign_input), 3);
    assert_eq!(
        FinalSourceMemo::certify(&foreign as &dyn Db, ingredient, foreign_input.as_id())
            .unwrap_err(),
        FinalSourceError::ForeignIngredient
    );
    assert_eq!(unclassified(&db, input), 7);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            unclassified::fn_ingredient_(&db, db.zalsa()),
            input.as_id()
        )
        .unwrap_err(),
        FinalSourceError::UnsupportedPolicy
    );
    assert_eq!(creator(&db, input), 7);
    let creator_ingredient = creator::fn_ingredient_(&db, db.zalsa());
    assert!(
        !stored(&db, creator_ingredient, input.as_id())
            .header
            .outputs_are_empty()
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, creator_ingredient, input.as_id()).unwrap_err(),
        FinalSourceError::OutputBearingMemo
    );
    assert!(
        consumer::fn_ingredient_(&db, db.zalsa())
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                consumer::fn_ingredient_(&db, db.zalsa())
                    .memo_ingredient_index(db.zalsa(), input.as_id())
            )
            .is_none()
    );
    input.set_value(&mut db).to(8);
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap_err(),
        FinalSourceError::UnverifiedMemo
    );
    assert_eq!(db.counts.bodies(), (1, 0));
    assert_eq!(source(&db, input), 8);
    prepared_consumer(&db, input, 9, (2, 1));
}

#[test]
fn source_registration_is_atomic_and_lookups_charge_the_three_key_bound() {
    let db = TestDb::default();
    let mut inputs = [
        Number::new(&db, 7),
        Number::new(&db, 8),
        Number::new(&db, 9),
    ];
    inputs.sort_by_key(|input| input.as_id());
    let missing = Number::new(&db, 10);
    let ingredient = source::fn_ingredient_(&db, db.zalsa());
    for input in inputs {
        source(&db, input);
    }
    let certificates = inputs
        .map(|input| FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap());
    for order in [vec![], vec![0, 0], vec![1, 0]] {
        let malformed: Vec<_> = order
            .iter()
            .map(|index| {
                FinalSourceMemo::certify(&db as &dyn Db, ingredient, inputs[*index].as_id())
                    .unwrap()
            })
            .collect();
        let admission = Admission::default();
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let expected = if malformed.is_empty() {
                "final source registration has no keys"
            } else {
                "final source keys are not sorted and unique"
            };
            assert!(
                matches!(registry.register_final_source(&db as &dyn Db, ingredient, &malformed), Err(RunError::Contract(actual)) if actual == expected)
            );
            let route =
                registry.register_final_source(&db as &dyn Db, ingredient, &certificates)?;
            assert_eq!(route.prepared_capacity(), 3);
            registry.seal()?.run(move |endpoint| async move {
                Ok(*endpoint.read_final_source(&route, inputs[0].as_id()).await)
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(7))));
    }
    for (validate, absent) in [(false, false), (false, true), (true, false), (true, true)] {
        let admission = Admission::default();
        let requested = if absent { missing } else { inputs[1] };
        let cancellations_before = db.counts.cancellations.load(Ordering::Relaxed);
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let before = admission.0.borrow().len();
            let route =
                registry.register_final_source(&db as &dyn Db, ingredient, &certificates)?;
            let registration = admission.0.borrow()[before..].to_vec();
            assert_eq!(&registration[..3], &[ExecutionWork::Work { units: 16 }; 3]);
            assert_eq!(registration.len(), 7);
            assert_eq!(
                registration[3],
                ExecutionWork::Resource {
                    requested_bytes: 3 * std::mem::size_of_val(&certificates[0])
                }
            );
            assert!(registration[4..].iter().all(|work| matches!(work, ExecutionWork::Resource { requested_bytes } if *requested_bytes > 0)));
            assert_eq!(route.prepared_capacity(), 3);
            let registry = registry.seal()?;
            let capacities = registry.registration_capacities();
            assert!(capacities.0 >= 1 && capacities.1 >= 1);
            if std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some() {
                eprintln!(
                    "FINAL_SOURCE_REGISTRATION requests={registration:?} vector_capacity={} registry_capacities={capacities:?}",
                    route.prepared_capacity()
                );
            }
            let revision = db.zalsa().current_revision();
            let key = ingredient.database_key_index(requested.as_id());
            let result = registry.run(move |endpoint| async move {
                if validate {
                    assert!(matches!(
                        endpoint.validate(key, revision)?.await?,
                        VerifyResult::Unchanged { .. }
                    ));
                } else {
                    assert_eq!(
                        *endpoint.read_final_source(&route, requested.as_id()).await,
                        8
                    );
                }
                Ok(())
            });
            assert_eq!(
                result,
                if absent {
                    Err(RunError::Contract("final source key is not registered"))
                } else {
                    Ok(())
                }
            );
            result
        });
        assert_eq!(
            outcome,
            if absent {
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            } else {
                Ok(AttemptOutcome::Complete(Ok(())))
            }
        );
        let work = admission.0.borrow();
        if std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some() {
            eprintln!(
                "FINAL_SOURCE_OPERATION validation={validate} absent={absent} attempt_cancellation_checks={} work={work:?}",
                db.counts.cancellations.load(Ordering::Relaxed) - cancellations_before,
            );
        }
        assert_eq!(
            work.iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            if validate { 2 } else { 1 }
        );
        assert!(work.contains(&ExecutionWork::Work {
            units: if validate { 35 } else { 67 }
        }));
        assert_eq!(db.counts.bodies(), (3, 0));
        assert_idle(&db);
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn return_source(db: &dyn Db, input: Number) -> u32 {
    input.value(db)
}

struct Leaf;
impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Leaf
where
    C: Configuration<DbView = dyn Db, Input<'db> = Number, Output<'db> = u32>,
{
    // Number conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        _: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Db,
        input: Number,
    ) -> RunResult<u32> {
        Ok(input.value(db))
    }
    async fn initial(
        &'run self,
        _: ProviderContext<'run, 'db, Self>,
        _: &'db dyn Db,
        _: Id,
        _: Number,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }
    async fn recover<'call>(
        &'run self,
        _: ProviderContext<'run, 'db, Self>,
        _: &'db dyn Db,
        _: &'call Cycle<'call>,
        _: &'call u32,
        _: u32,
        _: Number,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

#[test]
fn finalized_source_routes_are_mode_exclusive_and_registry_specific() {
    let db = TestDb::default();
    let input = Number::new(&db, 7);
    assert_eq!(return_source(&db, input), 7);
    let ingredient = return_source::fn_ingredient_(&db, db.zalsa());
    let certificate = FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.as_id()).unwrap();
    for source_first in [false, true] {
        let admission = Admission::default();
        let outcome = try_with_attempt(&db, 100_000, || {
            let provider = Leaf;
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            if source_first {
                registry.register_final_source(
                    &db as &dyn Db,
                    ingredient,
                    std::slice::from_ref(&certificate),
                )?;
                assert!(matches!(
                    registry.reserve(&db as &dyn Db, ingredient),
                    Err(RunError::Contract("query route already reserved"))
                ));
            } else {
                let route = registry.reserve(&db as &dyn Db, ingredient)?;
                let binding = registry.provider(&provider)?;
                registry.bind_executable(&route, &binding)?;
                assert!(matches!(
                    registry.register_final_source(
                        &db as &dyn Db,
                        ingredient,
                        std::slice::from_ref(&certificate)
                    ),
                    Err(RunError::Contract("query route already reserved"))
                ));
            }
            registry.seal()?.run(|_| async { Ok(()) })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    }
    let admission = Admission::default();
    let route = match try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let route = registry.register_final_source(&db as &dyn Db, ingredient, &[certificate])?;
        registry.seal()?.run(move |_| async move { Ok(route) })
    })
    .unwrap()
    {
        AttemptOutcome::Complete(Ok(route)) => route,
        _ => panic!("the source-only registry returns its actual route"),
    };
    let foreign = TestDb::default();
    for current in [&db as &dyn Database, &foreign as &dyn Database] {
        let route = &route;
        let outcome = try_with_attempt(current, 100_000, || {
            let result: RunResult<()> = RegistryBuilder::new(current, &admission)?.seal()?.run(
                move |endpoint| async move {
                    endpoint.read_final_source(route, input.as_id()).await;
                    Ok(())
                },
            );
            assert_eq!(
                result,
                Err(RunError::Contract("foreign final source route"))
            );
            result
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_idle(current);
    }
}
