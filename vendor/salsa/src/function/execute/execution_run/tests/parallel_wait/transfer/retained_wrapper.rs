use super::super::super::super::registration::ProviderBinding;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Registered,
    Native,
}

fn wrapper_key(db: &dyn TransferDatabase, input: Input) -> DatabaseKeyIndex {
    wrapper::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id())
}
fn wrapper_enter(db: &dyn TransferDatabase, input: Input) {
    let mut event = Observation::new(Kind::Gate).key(wrapper_key(db, input));
    event.phase = Some("wrapper.body.enter");
    trace::record(event);
}
fn wrapper_value(db: &dyn TransferDatabase, input: Input, value: u32) -> u32 {
    let mut event = Observation::new(Kind::BodyValue).key(wrapper_key(db, input));
    event.value = value;
    event.step = Some(Step::Body);
    trace::record(event);
    value
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn wrapper(db: &dyn TransferDatabase, input: Input) -> u32 {
    wrapper_enter(db, input);
    wrapper_value(db, input, b(db, input).0)
}

struct WrapperProvider<'run, 'db, A: Configuration, B: Configuration> {
    b: Route<'db, B>,
    ab: ProviderBinding<'run, Providers<'run, 'db, A, B>>,
}
impl<'run, 'db: 'run, A, B, W> ExecutableRouteProvider<'run, 'db, W>
    for WrapperProvider<'run, 'db, A, B>
where
    A: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = AValue>,
    B: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = BValue>,
    W: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    // Conversion reconstructs one Input handle; equality compares one u32.
    fixture_native_value!(executable, 'run, 'db, W, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TransferDatabase,
        input: Input,
    ) -> RunResult<u32> {
        context
            .endpoint()
            .local_call(|| {
                debit(db, context.endpoint(), wrapper_key(db, input), Step::Body)?;
                wrapper_enter(db, input);
                Ok::<(), RunError>(())
            })
            .await;
        let ab = context.endpoint().provider(self.ab.clone())?;
        Ok(wrapper_value(
            db,
            input,
            ab.fetch_ref(&self.b, input.as_id())?.await?.0,
        ))
    }
    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TransferDatabase,
        _id: Id,
        _input: Input,
    ) -> RunResult<u32> {
        Err(RunError::Contract("ordinary wrapper has no initializer"))
    }
    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TransferDatabase,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Input,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::Contract("ordinary wrapper has no recovery"))
    }
}
fn run_wrapper_queries<'db, A, B, W>(
    db: &'db dyn TransferDatabase,
    a_ingredient: &'db IngredientImpl<A>,
    b_ingredient: &'db IngredientImpl<B>,
    w_ingredient: &'db IngredientImpl<W>,
    input: Input,
    schedule: &Schedule,
) -> RunResult<u32>
where
    A: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = AValue>,
    B: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = BValue>,
    W: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let a = registry.reserve(db, a_ingredient)?;
    let b = registry.reserve(db, b_ingredient)?;
    let w = registry.reserve(db, w_ingredient)?;
    let providers = Providers {
        a: a.clone(),
        b: b.clone(),
        schedule,
    };
    let mut registry = registry;
    let ab = registry.provider(&providers)?;
    registry.bind_executable(&a, &ab)?;
    registry.bind_executable(&b, &ab)?;
    let provider = WrapperProvider { b, ab };
    let mut registry = registry;
    let wrapped = registry.provider(&provider)?;
    registry.bind_executable(&w, &wrapped)?;
    registry.seal()?.run(move |endpoint| async move {
        let context = endpoint.provider(wrapped)?;
        Ok(*context.fetch_ref(&w, input.as_id())?.await?)
    })
}
fn wrapper_root_observation(db: &Db, input: Input, value: u32) {
    let key = wrapper_key(db, input);
    let memo = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .memo(db.zalsa(), input.as_id())
        .unwrap()
        .transfer_test_snapshot();
    let mut event = Observation::new(Kind::RootResult).key(key).memo(Some(memo));
    event.value = value;
    trace::record(event);
    eprintln!("RETAINED_WRAPPER immediate root key={key:?} value={value} memo={memo:?}");
}

fn wrapper_worker(
    db: Db,
    input: Input,
    data: ScheduleData,
    supports: Supports,
    entry: Entry,
    live: mpsc::Sender<NativeLive>,
    ordinal: Arc<AtomicUsize>,
) -> NativeReport {
    assert_eq!(data.worker, 1);
    let token = db.cancellation_token();
    assert!(!token.is_cancelled());
    let installed = NativeObserverInstalled;
    NATIVE_OBSERVER.with_borrow_mut(|slot| {
        assert!(
            slot.replace(NativeObserver {
                gate: None,
                wait: None
            })
            .is_none()
        )
    });
    let exit = Cell::new(None);
    let driver = Cell::new(None);
    let support = RefCell::new(None);
    let (outcome, records) = trace::collect(TraceConfig { worker: 1, ordinal }, || {
        catch_unwind(AssertUnwindSafe(|| {
            crate::attach(&db, || {
                let schedule =
                    ScheduleOwner::install(data, Keys::new(&db, input), supports.clone());
                let result = attempt_probe::try_with_attempt(&db, ALLOWANCE, || {
                    let _exit = super::super::ExitSession(&exit);
                    let own = attempt_probe::current().unwrap();
                    supports[1].set(own.clone()).unwrap();
                    *support.borrow_mut() = Some(own.clone());
                    live.send(NativeLive {
                        thread: thread::current().id(),
                        token: token.clone(),
                        support: Some(own),
                    })
                    .unwrap();
                    native_status(&db, "native.entry", Some(wrapper_key(&db, input)));
                    let result = match entry {
                        Entry::Registered => run_wrapper_queries(
                            &db,
                            a::fn_ingredient_(&db, db.zalsa()),
                            b::fn_ingredient_(&db, db.zalsa()),
                            wrapper::fn_ingredient_(&db, db.zalsa()),
                            input,
                            &schedule.0,
                        ),
                        Entry::Native => Ok(wrapper(&db, input)),
                    };
                    driver.set(Some(result));
                    if let Ok(value) = result {
                        wrapper_root_observation(&db, input, value);
                    }
                    result
                });
                native_status(&db, "native.attachment.exit", Some(wrapper_key(&db, input)));
                result
            })
        }))
    });
    drop(installed);
    emit_native(&format!("wrapper donor entry={entry:?}"), &outcome);
    for record in &records.records {
        eprintln!("NATIVE_TRANSFER worker=1 wrapper={entry:?} {record:?}");
    }
    super::super::assert_worker_clean(&db);
    assert!(crate::with_attached_database(|_| ()).is_none());
    assert!(!token.is_cancelled());
    assert!(!records.broken);
    let support = support.into_inner();
    let own = support.as_ref().unwrap();
    assert!(!own.owns_current_session(db.zalsa()));
    assert_eq!(
        exit.get().unwrap().support.owner,
        trace::support_snapshot(own).owner
    );
    eprintln!(
        "RETAINED_WRAPPER donor cleanup entry={entry:?} exit={:?} support={:?}",
        exit.get(),
        trace::support_snapshot(own)
    );
    NativeReport {
        outcome,
        report: WorkerReport {
            worker: 1,
            trace: records,
            driver: driver.get(),
            support,
        },
        exit: exit.get(),
        allowance: ALLOWANCE,
    }
}

fn replay_wrapper(
    order: RefusalReplayOrder,
    entry: Entry,
    baseline: &Baseline,
    calibration: &Calibration,
) {
    let mut db = Db {
        storage: crate::Storage::new(Some(Box::new(|event| {
            refusal_replay_event(&event);
            wait_event(event);
        }))),
        counts: Arc::default(),
    };
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let w = wrapper_key(&db, input);
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let reports = thread::scope(|scope| {
        let [left, right] = schedules();
        let (paused_tx, paused_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release = Release {
            sender: Some(release_tx),
            value: Some(()),
        };
        let (r_tx, r_rx) = mpsc::channel();
        let (d_tx, d_rx) = mpsc::channel();
        let (pause_tx, pause_rx) = mpsc::channel();
        let (donor_wait_tx, donor_wait_rx) = mpsc::channel();
        let r_db = db.clone();
        let d_db = db.clone();
        let r_supports = supports.clone();
        let d_supports = supports.clone();
        let r_ordinal = ordinal.clone();
        let d_ordinal = ordinal.clone();
        let receiver = scope.spawn(move || {
            native_worker(
                r_db,
                input,
                left,
                r_supports,
                0,
                true,
                calibration.allowance,
                NativeObserver {
                    gate: Some(ReclaimGate {
                        key: keys.0[1],
                        entered: paused_tx,
                        release: release_rx,
                    }),
                    wait: None,
                },
                r_tx,
                r_ordinal,
                false,
            )
        });
        let donor = scope.spawn(move || {
            let (pause_rx, inbox) = match entry {
                Entry::Registered => (pause_rx, None),
                Entry::Native => {
                    let inbox = trace::install_finality_pause_inbox(pause_rx);
                    let (_, unused_pause_rx) = mpsc::channel();
                    (unused_pause_rx, Some(inbox))
                }
            };
            let installed = ReplayDonorInstalled::install(ReplayDonorObserver {
                pause: pause_rx,
                installed: None,
                waits: donor_wait_tx,
            });
            let report = wrapper_worker(d_db, input, right, d_supports, entry, d_tx, d_ordinal);
            if let Some(inbox) = inbox {
                drop(installed);
                drop(inbox);
                assert!(REPLAY_DONOR.with_borrow(|slot| slot.is_none()));
                eprintln!("RETAINED_WRAPPER native inbox hook retired");
            } else {
                installed.finish();
            }
            report
        });
        let receiver_live = r_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let donor_live = d_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let paused = paused_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        assert_eq!(paused.thread, receiver_live.thread);
        installed_reclaim(&db, input, paused, donor_live.thread, None);
        replay_fresh_claim(&db, w, donor_live.thread);
        eprintln!("RETAINED_WRAPPER original W claim remains owned at receiver reclaim: {w:?}");
        let [old_a, old_b] = keys.0.map(|key| {
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .memo(db.zalsa(), key.key_index())
                .unwrap()
                .transfer_test_snapshot()
        });
        assert!(!old_a.final_ && !old_b.final_);
        assert_eq!(old_a.support.unwrap().owner, old_b.support.unwrap().owner);
        assert_eq!(
            old_b.support.unwrap().owner,
            trace::support_snapshot(donor_live.support.as_ref().unwrap()).owner
        );
        let (donor_pause, donor_control) =
            trace::finality_pause(keys.0[1], old_b.identity, keys.0[0], old_a.identity);
        let (third_pause, third_control) =
            trace::finality_pause(keys.0[0], old_a.identity, keys.0[0], old_a.identity);
        pause_tx.send(donor_pause).unwrap();
        receiver_live.token.cancel();
        drop(release);
        donor_control.wait_for_selection();
        let receiver_report = receiver.join().unwrap();
        assert_eq!(
            receiver_report.outcome.as_ref().unwrap(),
            &Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        replay_fresh_claim(&db, keys.0[1], donor_live.thread);
        replay_fresh_claim(&db, w, donor_live.thread);

        let third_supports = Arc::new([OnceLock::new(), OnceLock::new()]);
        third_supports[1]
            .set(receiver_live.support.as_ref().unwrap().clone())
            .unwrap();
        let (third_tx, third_rx) = mpsc::channel();
        let (third_wait_tx, third_wait_rx) = mpsc::channel();
        let third_db = db.clone();
        let third_ordinal = ordinal.clone();
        let third = scope.spawn(move || {
            let installed = trace::install_finality_pause(third_pause);
            let report = native_worker(
                third_db,
                input,
                third_schedule(),
                third_supports,
                2,
                true,
                ALLOWANCE,
                NativeObserver {
                    gate: None,
                    wait: (order == RefusalReplayOrder::WaitBeforeVerification)
                        .then_some((keys.0[1], third_wait_tx)),
                },
                third_tx,
                third_ordinal,
                true,
            );
            drop(installed);
            eprintln!("TRANSFER_REPLAY independent hook retired");
            report
        });
        let third_live = third_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        third_control.wait_for_selection();
        replay_fresh_claim(&db, keys.0[0], third_live.thread);
        assert!(
            !third_live
                .support
                .as_ref()
                .unwrap()
                .same_owner(donor_live.support.as_ref().unwrap())
        );
        eprintln!(
            "RETAINED_WRAPPER {entry:?} {order:?} exact selections A={} B={}",
            old_a.identity, old_b.identity
        );

        match order {
            RefusalReplayOrder::WaitBeforeVerification => {
                drop(third_control);
                assert_eq!(
                    third_wait_rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
                    third_live.thread
                );
                replay_installed_wait(&db, third_live.thread, donor_live.thread);
                drop(donor_control);
            }
            RefusalReplayOrder::VerificationBeforeWait => {
                drop(donor_control);
                assert_eq!(
                    donor_wait_rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
                    (donor_live.thread, keys.0[0])
                );
                replay_installed_wait(&db, donor_live.thread, third_live.thread);
                drop(third_control);
            }
        }
        vec![
            receiver_report,
            donor.join().unwrap(),
            third.join().unwrap(),
        ]
    });
    audit(&db, input);
    let w_state = db
        .zalsa()
        .lookup_ingredient(w.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(input.as_id());
    eprintln!("RETAINED_WRAPPER post-join W state={w_state:?}");
    assert!(w_state.is_none());
    let mut records = reports
        .iter()
        .flat_map(|report| report.report.trace.records.iter().copied())
        .collect::<Vec<_>>();
    records.sort_unstable_by_key(|record| record.ordinal);
    assert!(
        records
            .iter()
            .enumerate()
            .all(|(index, record)| index == record.ordinal)
    );
    assert_native_claims(&records, NativeRow::Refusal);
    let registered_records = records
        .iter()
        .filter(|record| entry == Entry::Registered || record.worker != 1)
        .copied()
        .collect::<Vec<_>>();
    native_registered_retries(&registered_records, &reports);
    native_allowances(&reports, None);
    let handoff = handoff(&records, keys);
    let owners = [
        trace::support_snapshot(reports[0].report.support.as_ref().unwrap()).owner,
        trace::support_snapshot(reports[1].report.support.as_ref().unwrap()).owner,
    ];
    let restart = receiver_restart(&records, keys, &handoff, owners);
    let accounting = records
        .iter()
        .filter(|record| !is_native_check(record))
        .copied()
        .collect::<Vec<_>>();
    let prefix = debit_prefix(
        &db,
        input,
        &accounting,
        restart.debit,
        calibration.allowance,
    );
    assert_eq!(prefix, calibration.prefix);
    assert_eq!(
        calibration.allowance - restart.debit.event.session.unwrap().remaining,
        calibration.allowance
    );
    let returned = native_donor_refetch(&records, keys, &handoff, false);
    let claim = after(
        &records,
        returned,
        "donor newly claims B after receiver refusal",
        |record| query_event(record, 1, Kind::Claim, keys.0[1]),
    );
    assert_eq!(claim.event.mode, Some(Mode::Default));
    assert!(!claim.event.sync.unwrap().claimed_twice);
    let verified = after(&records, claim, "donor verifies its retained B", |record| {
        query_event(record, 1, Kind::Verified, keys.0[1])
            && record.event.serial == claim.event.serial
    });
    assert_eq!(
        verified.event.memo.unwrap().identity,
        handoff.provisional.event.memo.unwrap().identity
    );
    let head = after(
        &records,
        claim,
        "donor selects the retained provisional A head",
        |record| {
            query_event(record, 1, Kind::Verification, keys.0[1])
                && record.event.phase == Some("finality.head")
        },
    );
    assert!(!head.event.decision && head.ordinal < verified.ordinal);
    assert_eq!(
        head.event.other_memo.unwrap().identity,
        handoff.initial.event.memo.unwrap().identity
    );
    let terminal = after(
        &records,
        verified,
        "donor releases its new B claim",
        |record| {
            query_event(record, 1, Kind::Terminal, keys.0[1])
                && record.event.serial == claim.event.serial
        },
    );
    let fresh = records.iter().find(|record| {
        record.ordinal > verified.ordinal
            && (query_event(record, 1, Kind::PreDebit, keys.0[1])
                || (entry == Entry::Native && query_event(record, 1, Kind::BodyValue, keys.0[1])))
            && record.event.step == Some(Step::Body)
    });
    let transfers = records
        .iter()
        .filter(|record| {
            record.ordinal > verified.ordinal && record.event.kind == Kind::TransferBegin
        })
        .count();
    match order {
        RefusalReplayOrder::WaitBeforeVerification => {
            let wait = after(
                &records,
                claim,
                "independent A waits on donor B",
                |record| query_event(record, 2, Kind::Edge, keys.0[1]),
            );
            assert!(wait.ordinal < verified.ordinal);
        }
        RefusalReplayOrder::VerificationBeforeWait => {
            assert!(!verified.event.decision);
            let fresh = fresh.expect("rejected provisional B executes a fresh body");
            let wait = after(
                &records,
                verified,
                "fresh donor B waits on independent A",
                |record| query_event(record, 1, Kind::Edge, keys.0[0]),
            );
            assert!(verified.ordinal < fresh.ordinal && verified.ordinal < wait.ordinal);
            if entry == Entry::Registered {
                assert!(fresh.ordinal < wait.ordinal);
            }
            assert!(
                transfers > 0,
                "the restarted component transfers a participant claim"
            );
        }
    }
    eprintln!(
        "RETAINED_WRAPPER {entry:?} {order:?} fresh_claim={:?} serial={:?} verified={} release={:?} fresh_body={} transfers={transfers}",
        claim.event.mode,
        claim.event.serial,
        verified.event.decision,
        terminal.event.mode,
        fresh.is_some()
    );
    wrapper_refusal(&records, keys, &handoff, &restart, owners);
    wrapper_history(
        &records, w, keys, entry, &handoff, claim, verified, terminal,
    );
    eprintln!(
        "RETAINED_WRAPPER ordered-and-clean entry={entry:?} order={order:?} A/B counts={:?} records={}",
        db.counts.snapshot(),
        records.len()
    );
    for (worker, report) in reports.iter().enumerate() {
        if worker == 0 {
            assert_eq!(
                report.outcome.as_ref().unwrap(),
                &Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
        } else {
            assert_eq!(
                report.outcome.as_ref().unwrap(),
                &Ok(AttemptOutcome::Complete(Ok(3))),
                "{order:?} worker={worker}"
            );
        }
    }
    assert_eq!(wrapper(&db, input), 3);
    native_recovery(
        &mut db,
        input,
        baseline,
        NativeRow::Refusal,
        &records,
        Some(owners),
        None,
    );
    eprintln!("RETAINED_WRAPPER {entry:?} {order:?} passed");
}

fn wrapper_history(
    records: &[Record],
    w: DatabaseKeyIndex,
    keys: Keys,
    entry: Entry,
    handoff: &Handoff<'_>,
    b_claim: &Record,
    verified: &Record,
    b_terminal: &Record,
) {
    let claim = find(records, "donor owns genuine W before B", |record| {
        query_event(record, 1, Kind::Claim, w)
    });
    assert_eq!(claim.event.mode, Some(Mode::Default));
    assert!(claim.ordinal < handoff.donor_claim.ordinal && claim.ordinal < b_claim.ordinal);
    let body = after(records, claim, "wrapper body starts", |record| {
        query_event(record, 1, Kind::Gate, w) && record.event.phase == Some("wrapper.body.enter")
    });
    assert!(body.ordinal < handoff.donor_claim.ordinal);
    let published = after(records, verified, "W publishes its own result", |record| {
        query_event(record, 1, Kind::RootPublished, w)
    });
    let terminal = after(
        records,
        published,
        "W releases its original claim",
        |record| {
            query_event(record, 1, Kind::Terminal, w) && record.event.serial == claim.event.serial
        },
    );
    let result = after(records, terminal, "independent W root returns", |record| {
        query_event(record, 1, Kind::RootResult, w)
    });
    assert_eq!(published.event.serial, claim.event.serial);
    assert!(b_terminal.ordinal < published.ordinal);
    assert!(
        !records
            .iter()
            .any(|record| query_event(record, 1, Kind::Claim, w) && record.ordinal > claim.ordinal)
    );
    assert!(!records.iter().any(|record| record.event.key == Some(w)
        && matches!(
            record.event.kind,
            Kind::InitialValue | Kind::RecoveryValue | Kind::ColdInitial
        )));
    assert_eq!(
        published.event.memo.unwrap().identity,
        result.event.memo.unwrap().identity
    );
    let memo = result.event.memo.unwrap();
    assert!(memo.final_ || memo.support.is_some());
    if let Some(support) = memo.support {
        assert_eq!(
            support.owner,
            handoff
                .provisional
                .event
                .memo
                .unwrap()
                .support
                .unwrap()
                .owner
        );
    }
    assert!(
        !records
            .iter()
            .any(|record| query_event(record, 1, Kind::RootResult, keys.0[1]))
    );
    let body_count = records
        .iter()
        .filter(|record| {
            query_event(record, 1, Kind::Gate, w)
                && record.event.phase == Some("wrapper.body.enter")
        })
        .count();
    eprintln!(
        "RETAINED_WRAPPER witness entry={entry:?} W_key={w:?} original_claim={} serial={:?} fresh_B_claim={} verified={} accepted={} B_release={:?} W_publication={} W_memo={:?} W_release={:?} W_root={} value={} W_bodies={body_count}",
        claim.ordinal,
        claim.event.serial,
        b_claim.ordinal,
        verified.ordinal,
        verified.event.decision,
        b_terminal.event.mode,
        published.ordinal,
        published.event.memo.unwrap(),
        terminal.event.mode,
        result.ordinal,
        result.event.value
    );
}

fn wrapper_refusal(
    records: &[Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    restart: &Restart<'_>,
    owners: [usize; 2],
) {
    assert_eq!(restart.debit.event.session.unwrap().remaining, 0);
    let refused = after(
        records,
        restart.debit,
        "receiver refuses reclaimed B",
        |record| query_event(record, 0, Kind::DebitRefused, keys.0[1]),
    );
    assert_eq!(refused.event.session.unwrap().support.state, 1);
    assert!(!records.iter().any(|record| record.worker == 0
        && record.ordinal > refused.ordinal
        && matches!(record.event.kind, Kind::BodyValue | Kind::DebitAccepted)));
    let b_abort = after(records, refused, "receiver aborts B", |record| {
        query_event(record, 0, Kind::Terminal, keys.0[1])
            && record.event.action == Some(Action::Abort)
    });
    assert!(b_abort.event.sync.unwrap().claimed_twice);
    let undo = after(
        records,
        b_abort,
        "receiver restores transferred ownership",
        |record| query_event(record, 0, Kind::Undo, keys.0[1]),
    );
    let a_abort = after(records, undo, "receiver aborts A", |record| {
        query_event(record, 0, Kind::Terminal, keys.0[0])
            && record.event.action == Some(Action::Abort)
    });
    let returned = native_donor_refetch(records, keys, handoff, false);
    assert!(a_abort.ordinal < returned.ordinal);
    assert!(matches!(returned.event.wait, Some(WaitResult::Cancelled)));
    assert_eq!(returned.event.session.unwrap().support.owner, owners[1]);
    assert_eq!(returned.event.session.unwrap().support.state, 0);
    for record in records
        .iter()
        .filter(|record| record.worker == 1 && record.ordinal > returned.ordinal)
    {
        if record.event.kind == Kind::SupportAccepted {
            assert_ne!(record.event.support.unwrap().owner, owners[0]);
        }
        if matches!(record.event.kind, Kind::SeedAllowed | Kind::SeedActive)
            && record.event.decision
            && let Some(support) = record.event.support
        {
            assert_ne!(support.owner, owners[0]);
        }
    }
}
fn control(order: RefusalReplayOrder, entry: Entry) {
    let baseline = ordinary();
    let reference = paired(&baseline, None);
    let calibration = native_row(NativeRow::Calibration, true, &baseline, None).unwrap();
    assert_eq!(calibration.allowance, reference.allowance);
    assert_eq!(calibration.prefix, reference.prefix);
    eprintln!(
        "RETAINED_WRAPPER calibration entry={entry:?} order={order:?} allowance={} prefix={:?}",
        calibration.allowance, calibration.prefix
    );
    replay_wrapper(order, entry, &baseline, &calibration);
}
#[test]
fn registered_wait_before_verification() {
    control(
        RefusalReplayOrder::WaitBeforeVerification,
        Entry::Registered,
    );
}
#[test]
fn registered_verification_before_wait() {
    control(
        RefusalReplayOrder::VerificationBeforeWait,
        Entry::Registered,
    );
}
#[test]
fn native_wait_before_verification() {
    control(RefusalReplayOrder::WaitBeforeVerification, Entry::Native);
}
#[test]
fn native_verification_before_wait() {
    control(RefusalReplayOrder::VerificationBeforeWait, Entry::Native);
}
