use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    RefuseWork,
    RefuseStorage,
    AddRead,
    QueueChild,
    QueueChildAndRefuse,
    PendingWrite,
}

struct Control {
    action: Action,
    extra: Number,
    fired: bool,
    busy: bool,
    admissions: Vec<ExecutionWork>,
}

thread_local! {
    static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) };
}

struct ClearControl;

impl Drop for ClearControl {
    fn drop(&mut self) {
        CONTROL.with_borrow_mut(|control| *control = None);
    }
}

struct AdmissionCallback;

impl Drop for AdmissionCallback {
    fn drop(&mut self) {
        CONTROL.with_borrow_mut(|control| control.as_mut().unwrap().busy = false);
    }
}

struct ReadAdmission;

impl ExecutionAdmission for ReadAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !CONTROL.with_borrow(|control| control.as_ref().is_some_and(|control| !control.busy))
            || !selected_boundary()
        {
            return Ok(());
        }
        let (db, key, endpoint, policy_calls) = SCRIPT.with_borrow(|slot| {
            let script = slot.as_ref().unwrap();
            (
                script.db,
                script.key,
                script.endpoint.clone().unwrap(),
                script.policy_calls,
            )
        });
        if policy_calls != 1 || inputs(db).contains(&key) {
            return Ok(());
        }
        let stage = match work {
            ExecutionWork::Work { .. } => "read.work",
            ExecutionWork::Resource { .. } => "read.storage",
            ExecutionWork::Task { .. } | ExecutionWork::Poll => return Ok(()),
        };
        note(stage);
        let action = CONTROL.with_borrow_mut(|control| {
            let control = control.as_mut().unwrap();
            control.admissions.push(work);
            if control.fired
                || matches!(work, ExecutionWork::Work { .. })
                    != (control.action == Action::RefuseWork)
            {
                return None;
            }
            control.fired = true;
            control.busy = true;
            Some((control.action, control.extra))
        });
        let Some((action, extra)) = action else {
            return Ok(());
        };
        let _callback = AdmissionCallback;
        if matches!(
            action,
            Action::QueueChild | Action::QueueChildAndRefuse | Action::PendingWrite
        ) {
            let child = Marker("child.drop");
            let _reply = endpoint.demand(move || {
                poll_fn(move |_| -> Poll<RunResult<()>> {
                    let _child = &child;
                    panic!("a child queued by rejected read admission must not execute")
                })
            })?;
        }
        match action {
            Action::RefuseWork => Err(RunError::Refused(Incomplete::Allowance)),
            Action::RefuseStorage | Action::QueueChildAndRefuse => {
                Err(RunError::Refused(Incomplete::RequestedAllocation))
            }
            Action::AddRead => {
                assert_eq!(extra.value(db), 29);
                note("read.added");
                Ok(())
            }
            Action::QueueChild => Ok(()),
            Action::PendingWrite => {
                db.zalsa().runtime().set_cancellation_flag();
                Ok(())
            }
        }
    }
}

static READ_ADMISSION: ReadAdmission = ReadAdmission;

fn run(fixture: &Fixture) -> RunResult<u32> {
    let db: &'static dyn Database = fixture.db;
    let mut registry = RegistryBuilder::new(db, &READ_ADMISSION)?;
    let leaf = registry.reserve(db, eviction_leaf::fn_ingredient_(db, db.zalsa()))?;
    let caller = registry.reserve(db, read_caller::fn_ingredient_(db, db.zalsa()))?;
    let leaf_binding = registry.provider(&LEAF_PROVIDER)?;
    registry.bind_executable(&leaf, &leaf_binding)?;
    let provider = Box::leak(Box::new(CallerProvider {
        leaf,
        binding: leaf_binding,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&caller, &binding)?;
    let call = fixture.calls[0];
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&caller, call.as_id())?
            .await?)
    })
}

fn install(fixture: &Fixture, action: Action) -> (Clear, ClearControl) {
    let extra = Number::new(fixture.db, 29);
    let clear = fixture.install(0, Point::Eviction, Fault::Quiet, false);
    CONTROL.with_borrow_mut(|control| {
        assert!(control.is_none());
        *control = Some(Control {
            action,
            extra,
            fired: false,
            busy: false,
            admissions: Vec::new(),
        });
    });
    (clear, ClearControl)
}

fn assert_unpublished(fixture: &Fixture) {
    let ingredient = read_caller::fn_ingredient_(fixture.db, fixture.db.zalsa());
    let id = fixture.calls[0].as_id();
    assert!(
        ingredient
            .get_memo_from_table_for(
                fixture.db.zalsa(),
                id,
                ingredient.memo_ingredient_index(fixture.db.zalsa(), id),
            )
            .is_none(),
        "a parent whose dependency read was rejected must remain unpublished"
    );
}

fn reject(action: Action) {
    let fixture = Fixture::new();
    let (clear, control) = install(&fixture, action);
    let mut returned = None;
    let ((captured, _trace), events) = completion::collect(|| {
        validation_trace::collect(|| {
            prepared_source_probe::capture(fixture.db, || {
                catch_unwind(AssertUnwindSafe(|| {
                    try_with_attempt(fixture.db, 100_000, || {
                        let result = run(&fixture);
                        returned = Some(result);
                        result
                    })
                }))
            })
            .unwrap()
        })
    });
    if action == Action::PendingWrite {
        fixture.db.zalsa().runtime().reset_cancellation_flag();
    }

    // An implementation without read admission reaches delivery and records the target edge.
    assert!(
        !captured.reads.iter().any(|read| read.key == fixture.key),
        "refused dependency storage recorded a canonical read"
    );
    assert_eq!(
        target_events(&fixture, &events)
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>(),
        [Stage::Eviction]
    );
    let reason = match action {
        Action::RefuseWork => Incomplete::Allowance,
        Action::RefuseStorage | Action::QueueChildAndRefuse => {
            Incomplete::RequestedAllocation
        }
        Action::QueueChild | Action::PendingWrite => Incomplete::Interrupted,
        Action::AddRead => panic!("successful reentry is not a rejection"),
    };
    CONTROL.with_borrow(|control| assert!(control.as_ref().unwrap().fired));
    SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        assert!(!script.notes.iter().any(|(stage, _)| *stage == "delivered"));
        let locate = |name| {
            script
                .notes
                .iter()
                .position(|(stage, _)| *stage == name)
                .unwrap()
        };
        let admission = locate(if action == Action::RefuseWork {
            "read.work"
        } else {
            "read.storage"
        });
        let caller = locate("caller.drop");
        let original = &script.notes[admission].1;
        let before = &script.notes[locate("eviction")].1;
        assert!(original.read_state.is_some());
        assert_eq!(original.read_state, before.read_state);
        assert_eq!(script.notes[caller].1.inputs, original.inputs);
        assert_eq!(script.notes[caller].1.read_state, original.read_state);
        assert!(!original.inputs.contains(&fixture.key));
        if matches!(
            action,
            Action::QueueChild | Action::QueueChildAndRefuse | Action::PendingWrite
        ) {
            let child = locate("child.drop");
            assert!(admission < child && child < caller);
            let child = &script.notes[child].1;
            assert_eq!(child.operations, original.operations);
            assert_eq!(child.caller, original.caller);
            assert_eq!(child.depths, original.depths);
            assert_eq!(child.inputs, original.inputs);
            assert_eq!(child.read_state, original.read_state);
            assert_eq!((child.memo, child.value), fixture.selected);
            assert_eq!(
                (child.verified, child.changed),
                (original.verified, original.changed)
            );
            assert!(child.caller_claimed);
            assert!(!child.target_claimed);
            assert_eq!(child.reason, Some(reason));
            assert!(!child.panicking);
        }
    });
    if action != Action::PendingWrite {
        assert_eq!(
            captured.value.unwrap(),
            Ok(AttemptOutcome::Incomplete(reason))
        );
        assert_eq!(
            returned,
            Some(Err(if action == Action::QueueChild {
                RunError::Contract("completed task retained a child")
            } else {
                RunError::Refused(reason)
            }))
        );
    } else {
        assert!(returned.is_none());
        assert!(matches!(
            captured
                .value
                .unwrap_err()
                .downcast_ref::<crate::Cancelled>(),
            Some(crate::Cancelled::PendingWrite)
        ));
    }
    assert_unpublished(&fixture);
    fixture.assert_cached_and_idle(0);
    assert!(attempt_probe::current().is_none());
    drop(control);
    drop(clear);
    quiet(&fixture, 0, false);
}

#[test]
fn dependency_admission_refuses_before_recording_and_retries_the_same_parent() {
    for action in [Action::RefuseWork, Action::RefuseStorage] {
        reject(action);
    }
}

#[test]
fn dependency_admission_keeps_queued_children_inside_the_selected_owner() {
    for action in [Action::QueueChild, Action::QueueChildAndRefuse] {
        reject(action);
    }
}

#[test]
#[cfg(not(feature = "shuttle"))]
fn dependency_admission_preserves_pending_write_and_child_cleanup() {
    reject(Action::PendingWrite);
}

#[test]
fn dependency_admission_requotes_after_an_observer_adds_a_read() {
    let fixture = Fixture::new();
    let (_clear, _control) = install(&fixture, Action::AddRead);
    let ((captured, _trace), events) = completion::collect(|| {
        validation_trace::collect(|| {
            prepared_source_probe::capture(fixture.db, || {
                try_with_attempt(fixture.db, 100_000, || run(&fixture))
            })
            .unwrap()
        })
    });
    assert_eq!(captured.value, Ok(AttemptOutcome::Complete(Ok(8))));
    assert_read(&fixture, 0, &events, &captured.reads);
    CONTROL.with_borrow(|control| {
        let control = control.as_ref().unwrap();
        assert!(control.fired);
        assert!(matches!(
            control.admissions.as_slice(),
            [
                ExecutionWork::Work { .. },
                ExecutionWork::Resource { .. },
                ExecutionWork::Work { .. },
                ExecutionWork::Resource { .. }
            ]
        ));
    });
    SCRIPT.with_borrow(|slot| {
        let script = slot.as_ref().unwrap();
        let state = |name| {
            &script
                .notes
                .iter()
                .find(|(stage, _)| *stage == name)
                .unwrap()
                .1
        };
        let before = state("read.storage");
        let added = state("read.added");
        assert_eq!(added.caller, before.caller);
        assert_eq!(added.operations, before.operations);
        assert_eq!(added.depths, before.depths);
        assert_eq!(added.inputs.len(), before.inputs.len() + 1);
        assert_eq!(&added.inputs[..before.inputs.len()], before.inputs);
        let mut expected = added.inputs.clone();
        expected.push(fixture.key);
        assert_eq!(state("delivered").inputs, expected);
    });
    fixture.assert_cached_and_idle(0);
}
