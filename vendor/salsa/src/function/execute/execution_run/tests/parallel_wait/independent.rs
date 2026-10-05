use super::*;

const TEST: &str = "function::execute::execution_run::tests::parallel_wait::independent::independent_public_evaluations";
const CHILD: &str = "SALSA_INDEPENDENT_EVALUATIONS_CHILD";

struct HeldBody {
    entered: mpsc::Sender<Entry>,
    release: mpsc::Receiver<()>,
}

struct ReleaseBody(Option<mpsc::Sender<()>>);

impl Drop for ReleaseBody {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

struct ResetBody;

impl Drop for ResetBody {
    fn drop(&mut self) {
        HELD_BODY.with_borrow_mut(|slot| *slot = None);
    }
}

thread_local! {
    static HELD_BODY: RefCell<Option<HeldBody>> = const { RefCell::new(None) };
}

struct Entry {
    thread: ThreadId,
    support: Option<AttemptSupport>,
    remaining: Option<usize>,
}

fn entry(db: &dyn Database) -> Entry {
    Entry {
        thread: thread::current().id(),
        support: attempt_probe::current(),
        remaining: attempt_probe::remaining_allowance_for_diagnostics(db),
    }
}

pub(super) fn before_value_body(db: &dyn TestDatabase, input: Input) {
    let Some(held) = HELD_BODY.with_borrow_mut(Option::take) else {
        return;
    };
    let key = value::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    assert_eq!(
        db.zalsa_local().active_query().map(|(key, _)| key),
        Some(key)
    );
    assert_eq!(
        attempt_probe::try_with_attempt(db, 1, || panic!("active query admitted a root")),
        Err::<AttemptOutcome<()>, _>(StartError::ActiveQuery)
    );
    let own = attempt_probe::current();
    let receipt = own.as_ref().and_then(|own| own.local_ownership(db.zalsa()));
    let remaining = attempt_probe::remaining_allowance_for_diagnostics(db);
    held.entered.send(entry(db)).unwrap();
    held.release
        .recv_timeout(STAGE_TIMEOUT)
        .expect("held body released");
    match (&own, attempt_probe::current()) {
        (Some(own), Some(current)) => {
            assert!(own.same_owner(&current));
            assert_eq!(own.local_ownership(db.zalsa()), receipt);
        }
        (None, None) => assert!(!attempt_probe::is_incomplete(db)),
        _ => panic!("another evaluation changed the paused worker's Session"),
    }
    assert_eq!(
        attempt_probe::remaining_allowance_for_diagnostics(db),
        remaining
    );
    assert_eq!(
        db.zalsa_local().active_query().map(|(key, _)| key),
        Some(key)
    );
}

fn held_body() -> (HeldBody, mpsc::Receiver<Entry>, ReleaseBody) {
    let (entered, receive) = mpsc::channel();
    let (release, released) = mpsc::channel();
    (
        HeldBody {
            entered,
            release: released,
        },
        receive,
        ReleaseBody(Some(release)),
    )
}

fn schedule() -> Schedule {
    let (outbound, _) = mpsc::channel();
    Schedule {
        owner: false,
        case: Case::Cold,
        outbound,
        inbound: RefCell::new(None),
        body_entered: Cell::new(false),
        identity: Arc::new(()),
        installed_wait: None,
    }
}

fn registered(db: &Db, input: Input) -> RunResult<u32> {
    run_queries(
        db,
        value::fn_ingredient_(db, db.zalsa()),
        consumer::fn_ingredient_(db, db.zalsa()),
        input,
        &schedule(),
    )
}

#[derive(Clone, Copy, Debug)]
enum EntryMode {
    Controlled,
    OrdinaryQuery,
    OrdinaryScope,
}

struct Worker {
    support: Option<AttemptSupport>,
    exit: Option<SessionSnapshot>,
    trace: TransferTrace,
    value: u32,
}

fn worker(
    db: Db,
    input: Input,
    mode: EntryMode,
    held: Option<HeldBody>,
    entered: Option<mpsc::Sender<Entry>>,
    waiting: Option<mpsc::Sender<Gate>>,
    config: TraceConfig,
) -> Worker {
    let _body = ResetBody;
    HELD_BODY.with_borrow_mut(|slot| assert!(std::mem::replace(slot, held).is_none()));
    let _event = EventInstalled;
    if let Some(release_owner) = waiting {
        WAIT_EVENT.with_borrow_mut(|slot| {
            assert!(
                slot.replace(WaitEvent {
                    key: value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
                    release_owner,
                    observed: Arc::new(AtomicUsize::new(0)),
                    panic: None,
                })
                .is_none()
            );
        });
    }
    let support = RefCell::new(None);
    let exit = Cell::new(None);
    let (value, trace) = transfer_trace::collect(config, || {
        let notify = || {
            if let Some(entered) = &entered {
                entered.send(entry(&db)).unwrap();
            }
        };
        match mode {
            EntryMode::Controlled => {
                let result = attempt_probe::try_with_attempt(&db, WAIT_ALLOWANCE, || {
                    *support.borrow_mut() = attempt_probe::current();
                    let _exit = ExitSession(&exit);
                    notify();
                    registered(&db, input)
                });
                let Ok(AttemptOutcome::Complete(Ok(value))) = result else {
                    panic!("independent registered worker failed: {result:?}");
                };
                value
            }
            EntryMode::OrdinaryQuery | EntryMode::OrdinaryScope => {
                assert!(attempt_probe::current().is_none());
                notify();
                let value = match mode {
                    EntryMode::OrdinaryScope => {
                        attempt_probe::try_with_operation(&db, || value(&db, input)).unwrap()
                    }
                    _ => value(&db, input),
                };
                assert!(!attempt_probe::is_incomplete(&db));
                value
            }
        }
    });
    assert_worker_clean(&db);
    assert!(!db.cancellation_token().is_cancelled());
    assert!(!trace.broken);
    for record in &trace.records {
        eprintln!("INDEPENDENT {mode:?} {record:?}");
    }
    Worker {
        support: support.into_inner(),
        exit: exit.get(),
        trace,
        value,
    }
}

fn claim_owner(db: &Db, input: Input, owner: ThreadId, waiting: bool) {
    let state = value::fn_ingredient_(db, db.zalsa())
        .sync_table
        .test_transfer_state(input.as_id())
        .unwrap();
    assert!(matches!(state.owner, SyncOwner::Thread(actual) if actual == owner));
    assert_eq!(state.anyone_waiting, waiting);
    assert!(!state.claimed_twice && !state.is_transfer_target);
}

fn installed_edges(db: &Db, input: Input, owner: ThreadId, waiters: &[ThreadId]) {
    // A WillBlockOn signal precedes registration. Acquiring the graph mutex here waits
    // for its actual condvar handoff; the owner stays paused throughout this inspection.
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    let key = value::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    assert!(!graph.edges.overflow && !graph.dependents.overflow);
    let edges = graph
        .edges
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), waiters.len());
    for waiter in waiters {
        assert!(edges.contains(&(*waiter, owner)));
    }
    let dependents = graph
        .dependents
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(dependents.len(), 1);
    assert_eq!(dependents[0].0, key);
    assert!(!dependents[0].1.overflow);
    let actual = dependents[0]
        .1
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), waiters.len());
    assert!(waiters.iter().all(|waiter| actual.contains(waiter)));
    assert!(!graph.pending.overflow && graph.pending.is_empty());
    assert!(!graph.transferred.overflow && graph.transferred.is_empty());
    assert!(!graph.reverse.overflow && graph.reverse.is_empty());
    claim_owner(db, input, owner, true);
}

fn check_worker(worker: &Worker, expected: u32, waited: bool) {
    assert_eq!(worker.value, expected);
    match (&worker.support, worker.exit) {
        (Some(support), Some(exit)) => {
            let work = if waited { 1 } else { INPUT_CONVERSION_WORK + 1 } + ROOT_READ_WORK;
            assert_eq!(exit.remaining, WAIT_ALLOWANCE - work);
            assert_eq!(
                exit.support.owner,
                transfer_trace::support_snapshot(support).owner
            );
            assert_eq!(transfer_trace::support_snapshot(support).state, 2);
        }
        (None, None) => {}
        _ => panic!("worker lost its Session observation"),
    }
    let consumed = worker
        .trace
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::WaitConsumed)
        .collect::<Vec<_>>();
    assert_eq!(consumed.len(), usize::from(waited));
    for record in consumed {
        assert!(matches!(record.event.wait, Some(WaitResult::Completed)));
    }
    for claim in worker
        .trace
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::Claim)
    {
        let terminal = worker
            .trace
            .records
            .iter()
            .filter(|record| {
                record.event.kind == Kind::Terminal && record.event.serial == claim.event.serial
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal.len(), 1);
        assert!(matches!(
            terminal[0].event.wait,
            Some(WaitResult::Completed)
        ));
    }
}

fn shared_key(owner_mode: EntryMode, waiter_modes: &[EntryMode]) {
    let db = Db::default();
    let input = Input::new(&db, 4);
    let other = Input::new(&db, 9);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let (held, entered, release) = held_body();
    let reports = thread::scope(|scope| {
        // Declare the release guard after the scope starts so unwinding releases the
        // owner before the scope's implicit joins.
        let release = release;
        let owner_db = db.clone();
        let config = TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        };
        let owner = scope
            .spawn(move || worker(owner_db, input, owner_mode, Some(held), None, None, config));
        let owner_entry = entered.recv_timeout(STAGE_TIMEOUT).unwrap();
        claim_owner(&db, input, owner_entry.thread, false);
        let mut waiters = Vec::new();
        let mut waiter_ids = Vec::new();
        let mut supports = Vec::new();
        if let Some(support) = owner_entry.support {
            assert_eq!(
                owner_entry.remaining,
                Some(WAIT_ALLOWANCE - INPUT_CONVERSION_WORK)
            );
            supports.push(support);
        }
        for (index, mode) in waiter_modes.iter().copied().enumerate() {
            let waiter_db = db.clone();
            let (entry_tx, entry_rx) = mpsc::channel();
            let (wait_tx, wait_rx) = mpsc::channel();
            let config = TraceConfig {
                worker: index + 1,
                ordinal: ordinal.clone(),
            };
            waiters.push(scope.spawn(move || {
                worker(
                    waiter_db,
                    input,
                    mode,
                    None,
                    Some(entry_tx),
                    Some(wait_tx),
                    config,
                )
            }));
            let waiter = entry_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            if let Some(support) = waiter.support {
                assert_eq!(waiter.remaining, Some(WAIT_ALLOWANCE));
                assert!(
                    supports
                        .iter()
                        .all(|previous| !support.same_owner(previous))
                );
                supports.push(support);
            }
            assert!(matches!(
                wait_rx.recv_timeout(STAGE_TIMEOUT),
                Ok(Gate::Ready)
            ));
            waiter_ids.push(waiter.thread);
            installed_edges(&db, input, owner_entry.thread, &waiter_ids);
        }
        let other_db = db.clone();
        let config = TraceConfig {
            worker: 3,
            ordinal: ordinal.clone(),
        };
        let unrelated = scope
            .spawn(move || {
                worker(
                    other_db,
                    other,
                    EntryMode::Controlled,
                    None,
                    None,
                    None,
                    config,
                )
            })
            .join()
            .unwrap();
        check_worker(&unrelated, 9, false);
        installed_edges(&db, input, owner_entry.thread, &waiter_ids);
        drop(release);
        let mut reports = vec![owner.join().unwrap()];
        reports.extend(waiters.into_iter().map(|waiter| waiter.join().unwrap()));
        reports
    });
    for (index, report) in reports.iter().enumerate() {
        check_worker(report, 4, index != 0);
    }
    assert_eq!(db.counts.snapshot(), [2, 0]);
    assert_installed_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    assert_claim_released(&db, value::fn_ingredient_(&db, db.zalsa()), input);
    assert_exclusion_released(&db);
    let memo = installed_value_memo(&db, input);
    let baseline = Db::default();
    let baseline_input = Input::new(&baseline, 4);
    assert_eq!(value(&baseline, baseline_input), 4);
    assert_eq!(
        dependencies(&db, value::fn_ingredient_(&db, db.zalsa()), input),
        dependencies(
            &baseline,
            value::fn_ingredient_(&baseline, baseline.zalsa()),
            baseline_input
        )
    );
    assert_eq!(value(&db, input), 4);
    assert_eq!(installed_value_memo(&db, input).identity, memo.identity);
    assert_eq!(db.counts.snapshot(), [2, 0]);
}

#[derive(Clone, Copy, Debug)]
enum Finish {
    Complete,
    Allowance,
    Local,
    Panic,
}

fn staggered(finish: Finish) {
    let db = Db::default();
    let input = Input::new(&db, 4);
    let other = Input::new(&db, 9);
    let (held, entered, release) = held_body();
    let (start, started) = mpsc::channel();
    let a_support = RefCell::new(None);
    let a_exit = Cell::new(None);
    let b_entry = RefCell::new(None);
    let identity = Arc::new(());
    thread::scope(|scope| {
        let release = release;
        let b_db = db.clone();
        let b = scope.spawn(move || {
            started.recv_timeout(STAGE_TIMEOUT).unwrap();
            worker(
                b_db,
                input,
                EntryMode::Controlled,
                Some(held),
                None,
                None,
                TraceConfig {
                    worker: 1,
                    ordinal: Arc::new(AtomicUsize::new(0)),
                },
            )
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            attempt_probe::try_with_attempt(&db, 17, || {
                let _exit = ExitSession(&a_exit);
                *a_support.borrow_mut() = attempt_probe::current();
                assert_eq!(empty_run(&db, 3), Ok(()));
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(&db),
                    Some(14)
                );
                start.send(()).unwrap();
                *b_entry.borrow_mut() = Some(entered.recv_timeout(STAGE_TIMEOUT).unwrap());
                match finish {
                    Finish::Complete => Ok(()),
                    Finish::Allowance => {
                        let refused = empty_run(&db, 15);
                        assert_eq!(refused, Err(RunError::Refused(Incomplete::Allowance)));
                        let ordinary_db = db.clone();
                        thread::spawn(move || {
                            assert!(attempt_probe::current().is_none());
                            assert_eq!(
                                attempt_probe::try_with_operation(&ordinary_db, || {
                                    assert!(!attempt_probe::is_incomplete(&ordinary_db));
                                    assert_eq!(
                                        attempt_probe::remaining_allowance_for_diagnostics(
                                            &ordinary_db
                                        ),
                                        None
                                    );
                                }),
                                Ok(())
                            );
                            assert_worker_clean(&ordinary_db);
                        })
                        .join()
                        .unwrap();
                        refused
                    }
                    Finish::Local | Finish::Panic => {
                        let db = &db;
                        let identity = &identity;
                        RegistryBuilder::new(db, &ADMISSION)?
                            .seal()?
                            .run(|endpoint| async move {
                                endpoint
                                    .local_call(|| {
                                        if matches!(finish, Finish::Local) {
                                            db.cancellation_token().cancel();
                                            Ok(())
                                        } else {
                                            panic_any(OwnerPanic(identity.clone()));
                                        }
                                    })
                                    .await;
                                Ok(())
                            })
                    }
                }
            })
        }));
        match finish {
            Finish::Complete => assert_eq!(result.unwrap(), Ok(AttemptOutcome::Complete(Ok(())))),
            Finish::Allowance => assert_eq!(
                result.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            ),
            Finish::Local => assert!(matches!(
                result.unwrap_err().downcast_ref::<Cancelled>(),
                Some(Cancelled::Local)
            )),
            Finish::Panic => assert!(Arc::ptr_eq(
                &result.unwrap_err().downcast_ref::<OwnerPanic>().unwrap().0,
                &identity
            )),
        }
        assert_worker_clean(&db);
        assert!(!db.cancellation_token().is_cancelled());
        assert!(!attempt_probe::is_incomplete(&db));
        let a = a_support.borrow().as_ref().unwrap().clone();
        let exit = a_exit.get().unwrap();
        assert_eq!(exit.remaining, 14);
        assert_eq!(
            exit.support.owner,
            transfer_trace::support_snapshot(&a).owner
        );
        assert_eq!(
            transfer_trace::support_snapshot(&a).state,
            match finish {
                Finish::Complete => 2,
                Finish::Allowance => 1,
                _ => 3,
            }
        );
        let b_entry = b_entry.borrow();
        let b_entry = b_entry.as_ref().unwrap();
        let b_support = b_entry.support.as_ref().unwrap();
        assert!(!a.same_owner(b_support));
        assert!(!b_support.owns_current_session(db.zalsa()));
        assert!(b_support.local_ownership(db.zalsa()).is_none());
        assert_eq!(
            b_entry.remaining,
            Some(WAIT_ALLOWANCE - INPUT_CONVERSION_WORK)
        );
        assert_eq!(transfer_trace::support_snapshot(b_support).state, 0);
        claim_owner(&db, input, b_entry.thread, false);
        let c_db = db.clone();
        let c = scope
            .spawn(move || {
                worker(
                    c_db,
                    other,
                    EntryMode::Controlled,
                    None,
                    None,
                    None,
                    TraceConfig {
                        worker: 2,
                        ordinal: Arc::new(AtomicUsize::new(0)),
                    },
                )
            })
            .join()
            .unwrap();
        check_worker(&c, 9, false);
        let c_support = c.support.as_ref().unwrap();
        assert!(!c_support.same_owner(&a) && !c_support.same_owner(b_support));
        claim_owner(&db, input, b_entry.thread, false);
        assert_eq!(
            attempt_probe::try_with_attempt(&db, 5, || {
                let fresh = attempt_probe::current().unwrap();
                assert!(!fresh.same_owner(&a) && !fresh.same_owner(b_support));
                empty_run(&db, 1)?;
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(&db),
                    Some(4)
                );
                Ok(())
            }),
            Ok(AttemptOutcome::Complete(Ok::<_, RunError>(())))
        );
        drop(release);
        check_worker(&b.join().unwrap(), 4, false);
    });
    assert_installed_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    assert_claim_released(&db, value::fn_ingredient_(&db, db.zalsa()), input);
    assert_exclusion_released(&db);
    assert_eq!(db.counts.snapshot(), [2, 0]);
}

fn stale_registration() {
    let db = Db::default();
    let stale =
        attempt_probe::try_with_attempt(&db, 11, || RegistryBuilder::new(&db, &ADMISSION)?.seal())
            .unwrap();
    let AttemptOutcome::Complete(Ok(stale)) = stale else {
        panic!("old registration was not created");
    };
    for attached in [false, true] {
        let check = || {
            attempt_probe::try_with_attempt(&db, 5, || {
                let own = attempt_probe::current().unwrap();
                assert_eq!(
                    attempt_probe::try_with_attempt(&db, 1, || ()),
                    Err(StartError::NestedAttempt)
                );
                assert_eq!(
                    attempt_probe::try_with_operation(&db, || empty_run(&db, 1)),
                    Ok(Ok(()))
                );
                assert!(attempt_probe::current().unwrap().same_owner(&own));
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(&db),
                    Some(4)
                );
            })
        };
        let result = if attached {
            crate::attach(&db, check)
        } else {
            check()
        };
        assert_eq!(result, Ok(AttemptOutcome::Complete(())));
        assert_worker_clean(&db);
    }
    assert_eq!(
        attempt_probe::try_with_attempt(&db, 5, || {
            let own = attempt_probe::current().unwrap();
            let result = stale.run(|_| async { Ok(()) });
            assert_eq!(
                result,
                Err(RunError::Contract("execution run has a foreign attempt"))
            );
            assert!(attempt_probe::current().unwrap().same_owner(&own));
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(5)
            );
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_exclusion_released(&db);
}

fn enclosing_attachment_owns_token_reset() {
    let db = Db::default();
    let token = db.cancellation_token();
    crate::attach(&db, || {
        let db = &db;
        let token = &token;
        let result = catch_unwind(AssertUnwindSafe(|| {
            attempt_probe::try_with_attempt(db, 5, || {
                RegistryBuilder::new(db, &ADMISSION)?
                    .seal()?
                    .run(|endpoint| async move {
                        endpoint
                            .local_call(|| {
                                token.cancel();
                                Ok(())
                            })
                            .await;
                        Ok(())
                    })
            })
        }));
        assert!(matches!(
            result.unwrap_err().downcast_ref::<Cancelled>(),
            Some(Cancelled::Local)
        ));
        assert_worker_clean(db);
        assert!(
            token.is_cancelled(),
            "the enclosing attachment still owns reset"
        );
    });
    assert!(!token.is_cancelled());
    assert_eq!(
        attempt_probe::try_with_attempt(&db, 5, || empty_run(&db, 1)),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
    assert_exclusion_released(&db);
}

#[test]
fn independent_public_evaluations() {
    if std::env::var(CHILD).as_deref() != Ok(TEST) {
        watchdog(TEST, CHILD);
        return;
    }
    shared_key(
        EntryMode::Controlled,
        &[EntryMode::Controlled, EntryMode::Controlled],
    );
    shared_key(EntryMode::OrdinaryQuery, &[EntryMode::Controlled]);
    shared_key(EntryMode::Controlled, &[EntryMode::OrdinaryQuery]);
    shared_key(EntryMode::Controlled, &[EntryMode::OrdinaryScope]);
    for finish in [
        Finish::Complete,
        Finish::Allowance,
        Finish::Local,
        Finish::Panic,
    ] {
        staggered(finish);
    }
    stale_registration();
    enclosing_attachment_owns_token_reset();
}
