use super::super::validation_trace::{self as trace, TraceEvent};
use super::*;
#[cfg(feature = "accumulator")]
use crate::Accumulator;
use crate::attempt_probe::QueryPolicy;
use crate::cycle::CycleRecoveryStrategy;
use crate::function::maybe_changed_after::validation::{ClaimedMemo, VerificationAction};
use crate::zalsa_local::{QueryEdgeKind, QueryOriginRef};

struct TracingAdmission<'a> {
    db: &'a dyn Database,
    inner: &'a dyn ExecutionAdmission,
}

impl ExecutionAdmission for TracingAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let owner = self.db.zalsa_local().active_query().map(|(key, _)| key);
        let depths = attempt_probe::stack_depths();
        trace::record(TraceEvent::Admission {
            work,
            outcome: None,
            owner,
            depths,
        });
        let unwind = AdmissionUnwind;
        let result = self.inner.admit(work);
        std::mem::forget(unwind);
        trace::record(TraceEvent::Admission {
            work,
            outcome: Some(result),
            owner,
            depths,
        });
        result
    }
}

struct AdmissionUnwind;

impl Drop for AdmissionUnwind {
    fn drop(&mut self) {
        trace::record(TraceEvent::Result {
            outcome: "admission.unwind",
            unchanged: None,
        });
    }
}

#[derive(Clone, Copy)]
enum Scenario {
    Backdated,
    ChangedFirst,
    Nested,
}

fn scenario(scenario: Scenario, stop: Option<usize>) -> (Vec<TraceEvent>, usize) {
    let (mut db, root, leaf, revision) = fixture(match scenario {
        Scenario::Backdated => 3,
        Scenario::ChangedFirst => 2,
        Scenario::Nested => 1,
    });
    let mut script = Script::default();
    let mode = if matches!(scenario, Scenario::ChangedFirst) {
        root.set_next(&mut db).to(None);
        RouteMode::MissingWrapped
    } else {
        RouteMode::Both
    };
    let nested = if matches!(scenario, Scenario::Nested) {
        let child = Node::new(&db, None, 4);
        assert_eq!(scalar(&db, child), 0);
        child.set_value(&mut db).to(6);
        script.nested = Some((
            root.as_id(),
            scalar::fn_ingredient_(&db, db.zalsa()).database_key_index(child.as_id()),
            revision,
        ));
        Some(child)
    } else {
        None
    };
    let admission = Admission {
        work: Cell::new(0),
        stop,
        panic: false,
    };
    let (_, events) = trace::collect(|| {
        let result = try_with_attempt(&db, 10_000, || {
            validate(
                &db,
                root,
                revision,
                &script,
                &TracingAdmission {
                    db: &db,
                    inner: &admission,
                },
                mode,
            )
        });
        match result {
            Ok(AttemptOutcome::Complete(Ok(result))) if stop.is_none() => {
                assert_eq!(
                    result.is_unchanged(),
                    !matches!(scenario, Scenario::ChangedFirst)
                );
                trace::record(TraceEvent::Result {
                    outcome: "complete",
                    unchanged: Some(true),
                });
            }
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)) if stop.is_some() => {
                trace::record(TraceEvent::Result {
                    outcome: "allowance",
                    unchanged: None,
                });
            }
            other => panic!("unexpected validation trace outcome: {other:?}"),
        }
    });
    assert_idle(&db, root);
    assert_idle(&db, leaf);
    if let Some(child) = nested {
        assert_idle(&db, child);
    }
    if stop.is_some() {
        assert!(matches!(
            try_with_attempt(&db, 10_000, || validate(
                &db,
                root,
                revision,
                &Script::default(),
                &Admission::unrestricted(),
                RouteMode::Both
            )),
            Ok(AttemptOutcome::Complete(Ok(_)))
        ));
    }
    (events, admission.work.get())
}

#[test]
fn ordered_validation_traces_preserve_identity_and_handoffs() {
    for (name, case) in [
        ("backdated", Scenario::Backdated),
        ("changed-first", Scenario::ChangedFirst),
        ("nested", Scenario::Nested),
    ] {
        let (first, _) = scenario(case, None);
        let (second, _) = scenario(case, None);
        assert_eq!(
            trace::normalize(&first),
            trace::normalize(&second),
            "{name}"
        );
        assert!(first.iter().any(|event| matches!(
            event,
            TraceEvent::Outer {
                phase: "verification.complete",
                ..
            }
        )));
        assert!(first.iter().any(|event| matches!(
            event,
            TraceEvent::Outer {
                phase: "validation.complete",
                ..
            }
        )));
        trace::emit(name, &first);
    }
}

#[test]
fn every_admitted_validation_boundary_has_a_frozen_refusal_trace() {
    let (baseline, count) = scenario(Scenario::Backdated, None);
    for phase in [
        "dependency.demand",
        "finish",
        "commit",
        "verification.complete",
        "validation.complete",
    ] {
        assert!(
            baseline.iter().any(
                |event| matches!(event, TraceEvent::Outer { phase: found, .. } if *found == phase)
            ),
            "{phase}"
        );
    }
    for ordinal in 0..count {
        let (first, work) = scenario(Scenario::Backdated, Some(ordinal));
        let (second, _) = scenario(Scenario::Backdated, Some(ordinal));
        assert_eq!(work, ordinal + 1);
        assert_eq!(
            trace::normalize(&first),
            trace::normalize(&second),
            "work {ordinal}"
        );
        let refused = first
            .iter()
            .position(|event| {
                matches!(
                    event,
                    TraceEvent::Admission {
                        outcome: Some(Err(RunError::Refused(Incomplete::Allowance))),
                        ..
                    }
                )
            })
            .unwrap();
        assert!(
            !first[refused + 1..].iter().any(|event| matches!(
                event,
                TraceEvent::Verifier { .. } | TraceEvent::Body { .. }
            ))
        );
        trace::emit(&format!("refuse-{ordinal}"), &first);
    }
}

#[test]
fn admission_panic_preserves_ordered_frame_free_cleanup() {
    let (db, root, leaf, revision) = fixture(3);
    let admission = Admission {
        work: Cell::new(0),
        stop: Some(8),
        panic: true,
    };
    let (_, events) = trace::collect(|| {
        let panic = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 10_000, || {
                validate(
                    &db,
                    root,
                    revision,
                    &Script::default(),
                    &TracingAdmission {
                        db: &db,
                        inner: &admission,
                    },
                    RouteMode::Both,
                )
            })
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"validation admission panic")
        );
        trace::record(TraceEvent::Result {
            outcome: "admission.panic",
            unchanged: None,
        });
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TraceEvent::Outer {
            phase: "panic.drop",
            ..
        }
    )));
    assert_idle(&db, root);
    assert_idle(&db, leaf);
    trace::emit("admission-panic", &events);
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn cancellation_traces_retain_the_original_payload_and_cleanup_order() {
    for during_body in [false, true] {
        let (db, root, leaf, revision) = fixture(3);
        let script = Script {
            cancel: during_body.then_some(leaf.as_id()),
            ..Script::default()
        };
        let cancelling = CancelValidation {
            db: &db,
            cancelled: Cell::new(false),
        };
        let unrestricted = Admission::unrestricted();
        let admission: &dyn ExecutionAdmission = if during_body {
            &unrestricted
        } else {
            &cancelling
        };
        let (_, events) = trace::collect(|| {
            let result = crate::Cancelled::catch(AssertUnwindSafe(|| {
                try_with_attempt(&db, 10_000, || {
                    validate(
                        &db,
                        root,
                        revision,
                        &script,
                        &TracingAdmission {
                            db: &db,
                            inner: admission,
                        },
                        RouteMode::Both,
                    )
                })
            }));
            db.zalsa().runtime().reset_cancellation_flag();
            assert!(matches!(result, Err(crate::Cancelled::PendingWrite)));
            trace::record(TraceEvent::Result {
                outcome: "cancelled.pending-write",
                unchanged: None,
            });
        });
        assert_idle(&db, root);
        assert_idle(&db, leaf);
        trace::emit(
            if during_body {
                "cancel-body"
            } else {
                "cancel-metadata"
            },
            &events,
        );
    }
}

#[crate::input]
struct ProducerInput {
    early: u32,
    late: u32,
}

#[crate::tracked]
struct Entity<'db> {
    key: u32,
}

#[crate::tracked(returns(copy), specify)]
fn specified<'db>(_db: &'db dyn Database, _entity: Entity<'db>) -> u32 {
    0
}

#[crate::tracked(returns(copy))]
fn output_producer(db: &dyn Database, input: ProducerInput) -> u32 {
    trace::record(TraceEvent::Body {
        key: output_producer::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    });
    let entity = Entity::new(db, *input.early(db));
    specified::specify(db, entity, 17);
    *input.late(db)
}

fn stored<'db, C: Configuration>(
    db: &'db DatabaseImpl,
    ingredient: &IngredientImpl<C>,
    id: Id,
) -> &'db Memo<C> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .expect("ordinary execution warmed this memo")
}

#[derive(Clone, Copy)]
enum KernelStop {
    Never,
    Dependency(DatabaseKeyIndex),
    Commit,
}

/// Exercise real producer memos without registering a producer as a return-only route.
fn kernel<C: Configuration>(
    db: &DatabaseImpl,
    ingredient: &IngredientImpl<C>,
    id: Id,
    stop: KernelStop,
) -> Option<crate::function::maybe_changed_after::validation::MemoValidity> {
    let _operation = attempt_probe::enter(
        db.zalsa(),
        QueryPolicy::Unclassified,
        "producer validation trace",
    );
    let claim =
        match ingredient
            .sync_table
            .try_claim(db.zalsa(), db.zalsa_local(), id, Reentrancy::Deny)
        {
            ClaimResult::Claimed(claim) => claim,
            _ => panic!("ordinary producer fixture must have an unclaimed memo"),
        };
    let memo = ingredient
        .memo_slot(
            db.zalsa(),
            id,
            ingredient.memo_ingredient_index(db.zalsa(), id),
        )
        .get_erased()
        .unwrap();
    let mut verification = ClaimedMemo { claim, memo }.verify(C::CYCLE_STRATEGY);
    loop {
        let refuse = match (verification.action(), stop) {
            (VerificationAction::Work(work), KernelStop::Commit) => work.phase_name() == "commit",
            (VerificationAction::Dependency { request, .. }, KernelStop::Dependency(key)) => {
                request.key == key
            }
            _ => false,
        };
        let work = ExecutionWork::Work { units: 1 };
        let owner = db.zalsa_local().active_query().map(|(key, _)| key);
        let depths = attempt_probe::stack_depths();
        trace::record(TraceEvent::Admission {
            work,
            outcome: None,
            owner,
            depths,
        });
        let result = if refuse {
            Err(RunError::Refused(Incomplete::Allowance))
        } else {
            Ok(())
        };
        trace::record(TraceEvent::Admission {
            work,
            outcome: Some(result),
            owner,
            depths,
        });
        if refuse {
            // This is an ordinary kernel fixture, without an attempt or ancestor query frame.
            verification.into_claim().abort();
            trace::record(TraceEvent::Result {
                outcome: "kernel.refused",
                unchanged: None,
            });
            return None;
        }
        match verification.action() {
            VerificationAction::Work(work) => work.advance(),
            VerificationAction::Event(event) => {
                event.event();
                event.finish();
            }
            VerificationAction::Dependency { request, reply } => {
                let result = request.key.maybe_changed_after(
                    (db as &dyn Database).into(),
                    db.zalsa(),
                    request.changed_after,
                );
                reply.resume(result);
            }
            VerificationAction::Complete => {
                let verified = match verification.complete() {
                    Ok(verified) => verified,
                    Err(verification) => {
                        verification.into_claim().abort();
                        panic!("complete action must transfer its claimed memo");
                    }
                };
                trace::record(TraceEvent::Result {
                    outcome: "kernel.complete",
                    unchanged: Some(verified.result.is_unchanged()),
                });
                return Some(verified.result);
            }
        }
        if let Some(event) = verification.pending_event() {
            event.event();
            event.finish();
        }
    }
}

#[test]
fn pending_reply_and_checked_completion_retain_the_original_claim() {
    let (db, root, _, _) = fixture(3);
    let ingredient = scalar::fn_ingredient_(&db, db.zalsa());
    let _operation =
        attempt_probe::enter(db.zalsa(), QueryPolicy::ReturnOnly, "verifier API control");
    let claim = match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        root.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim,
        _ => panic!("fixture memo must be unclaimed"),
    };
    let serial = claim.test_serial();
    let memo = ingredient
        .memo_slot(
            db.zalsa(),
            root.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), root.as_id()),
        )
        .get_erased()
        .unwrap();
    let address = std::ptr::from_ref(memo.header());
    let mut verification = ClaimedMemo { claim, memo }.verify(CycleRecoveryStrategy::Panic);
    let pending = loop {
        match verification.action() {
            VerificationAction::Work(work) => work.advance(),
            VerificationAction::Event(event) => {
                event.event();
                event.finish();
            }
            VerificationAction::Dependency { request, .. } => break request,
            VerificationAction::Complete => panic!("edited fixture must inspect an input"),
        }
    };
    verification = match verification.complete() {
        Err(verification) => verification,
        Ok(_) => panic!("a pending dependency cannot complete verification"),
    };
    let VerificationAction::Dependency { request, reply } = verification.action() else {
        panic!("dropping the reply handle must leave the same input pending");
    };
    assert_eq!(request.key, pending.key);
    assert_eq!(request.changed_after, pending.changed_after);
    let result = request.key.maybe_changed_after(
        (&db as &dyn Database).into(),
        db.zalsa(),
        request.changed_after,
    );
    reply.resume(result);
    loop {
        match verification.action() {
            VerificationAction::Work(work) => work.advance(),
            VerificationAction::Event(event) => {
                event.event();
                event.finish();
            }
            VerificationAction::Dependency { request, reply } => {
                let result = request.key.maybe_changed_after(
                    (&db as &dyn Database).into(),
                    db.zalsa(),
                    request.changed_after,
                );
                reply.resume(result);
            }
            VerificationAction::Complete => break,
        }
    }
    let verified = match verification.complete() {
        Ok(verified) => verified,
        Err(verification) => {
            verification.into_claim().abort();
            panic!("terminal verification must transfer its claim");
        }
    };
    assert!(verified.result.is_unchanged());
    assert_eq!(verified.claimed.claim.test_serial(), serial);
    assert!(std::ptr::eq(verified.claimed.memo.header(), address));
    drop(verified);
    let claim = match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        root.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim,
        _ => panic!("completed verification must release its claim"),
    };
    assert_ne!(claim.test_serial(), serial);
    claim.abort();
}

#[test]
fn specified_output_validation_survives_a_later_change_or_refusal() {
    for refuse in [false, true] {
        let mut db = DatabaseImpl::default();
        let input = ProducerInput::new(&db, 2, 4);
        assert_eq!(output_producer(&db, input), 4);
        let ingredient = output_producer::fn_ingredient_(&db, db.zalsa());
        let old = stored(&db, ingredient, input.as_id());
        let QueryOriginRef::Derived(edges) = old.header.origin() else {
            panic!("producer must have ordinary derived edges")
        };
        let specified_index = specified::fn_ingredient_(&db, db.zalsa()).index;
        let output = edges
            .iter()
            .find(|edge| {
                edge.kind() == QueryEdgeKind::Output
                    && edge.key().ingredient_index() == specified_index
            })
            .unwrap()
            .key();
        let late = edges
            .iter()
            .rfind(|edge| edge.kind() == QueryEdgeKind::Input)
            .unwrap()
            .key();
        let old_revision = old.header.verified_at.load();
        input.set_late(&mut db).to(9);
        let ingredient = output_producer::fn_ingredient_(&db, db.zalsa());
        let assigned = stored(
            &db,
            specified::fn_ingredient_(&db, db.zalsa()),
            output.key_index(),
        );
        assert_eq!(assigned.header.verified_at.load(), old_revision);
        let (result, events) = trace::collect(|| {
            kernel(
                &db,
                ingredient,
                input.as_id(),
                if refuse {
                    KernelStop::Dependency(late)
                } else {
                    KernelStop::Never
                },
            )
        });
        assert_eq!(
            result.map(|value| value.is_unchanged()),
            if refuse { None } else { Some(false) }
        );
        assert_eq!(
            assigned.header.verified_at.load(),
            db.zalsa().current_revision()
        );
        assert_eq!(
            stored(&db, ingredient, input.as_id())
                .header
                .verified_at
                .load(),
            old_revision
        );
        let published = events
            .iter()
            .position(|event| matches!(event, TraceEvent::Output { key, .. } if *key == output))
            .unwrap();
        let later = events
            .iter()
            .position(|event| matches!(event, TraceEvent::Request { key, .. } if *key == late))
            .unwrap();
        assert!(published < later);
        if !refuse {
            assert!(events.iter().any(|event| matches!(
                event,
                TraceEvent::Verifier {
                    phase: "commit.changed",
                    ..
                }
            )));
        }
        trace::emit(
            if refuse {
                "output-refused"
            } else {
                "output-changed"
            },
            &events,
        );
        assert_eq!(output_producer(&db, input), 9);
    }
}

#[cfg(feature = "accumulator")]
#[crate::accumulator]
struct Warning(u32);

#[cfg(feature = "accumulator")]
#[crate::tracked(returns(copy))]
fn accumulating_leaf(db: &dyn Database, input: ProducerInput) -> u32 {
    trace::record(TraceEvent::Body {
        key: accumulating_leaf::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    });
    if *input.late(db) != 0 {
        Warning(7).accumulate(db);
    }
    0
}

#[cfg(feature = "accumulator")]
#[crate::tracked(returns(copy))]
fn accumulating_parent(db: &dyn Database, input: ProducerInput) -> u32 {
    trace::record(TraceEvent::Body {
        key: accumulating_parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    });
    accumulating_leaf(db, input)
}

#[cfg(feature = "accumulator")]
#[test]
fn accumulated_inputs_publish_before_a_refused_commit() {
    let mut db = DatabaseImpl::default();
    let input = ProducerInput::new(&db, 0, 0);
    assert_eq!(accumulating_parent(&db, input), 0);
    let ingredient = accumulating_parent::fn_ingredient_(&db, db.zalsa());
    let old = stored(&db, ingredient, input.as_id());
    let old_revision = old.header.verified_at.load();
    assert!(old.header.revisions.accumulated_inputs.load().is_empty());
    input.set_late(&mut db).to(1);
    let ingredient = accumulating_parent::fn_ingredient_(&db, db.zalsa());
    let (result, events) =
        trace::collect(|| kernel(&db, ingredient, input.as_id(), KernelStop::Commit));
    assert!(result.is_none());
    let memo = stored(&db, ingredient, input.as_id());
    assert!(memo.header.revisions.accumulated_inputs.load().is_any());
    assert_eq!(memo.header.verified_at.load(), old_revision);
    let key = ingredient.database_key_index(input.as_id());
    assert!(events.iter().any(|event| matches!(event, TraceEvent::Verifier { phase: "accumulated.published", key: owner, verified_at, accumulated: Some(true), .. } if *owner == key && *verified_at == old_revision)));
    assert!(!events.iter().any(|event| matches!(event, TraceEvent::Verifier { phase: "commit.published", key: owner, .. } if *owner == key)));
    trace::emit("accumulated-refused-commit", &events);
    assert_eq!(
        accumulating_parent::accumulated::<Warning>(&db, input)
            .iter()
            .map(|warning| warning.0)
            .collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(
        stored(&db, ingredient, input.as_id())
            .header
            .verified_at
            .load(),
        db.zalsa().current_revision()
    );
}
