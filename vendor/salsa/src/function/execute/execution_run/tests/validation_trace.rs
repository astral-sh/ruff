use std::cell::RefCell;

use super::super::{ExecutionWork, RunError};
use super::observation::Event;
use crate::function::maybe_changed_after::VerifyResult;
use crate::{DatabaseKeyIndex, Revision};

#[derive(Clone, Copy, Debug)]
pub(in crate::function) enum TraceEvent {
    Admission {
        work: ExecutionWork,
        outcome: Option<Result<(), RunError>>,
        owner: Option<DatabaseKeyIndex>,
        depths: (usize, usize),
    },
    Outer {
        phase: &'static str,
        key: Option<DatabaseKeyIndex>,
        operation: usize,
        owner: Option<DatabaseKeyIndex>,
        query_depth: usize,
        operation_depth: usize,
    },
    Verifier {
        phase: &'static str,
        key: DatabaseKeyIndex,
        claim: usize,
        memo: usize,
        verified_at: Revision,
        current_revision: Revision,
        accumulated: Option<bool>,
        owner: Option<DatabaseKeyIndex>,
        depths: (usize, usize),
    },
    Request {
        owner: DatabaseKeyIndex,
        key: DatabaseKeyIndex,
        changed_after: Revision,
    },
    Reply {
        owner: DatabaseKeyIndex,
        key: DatabaseKeyIndex,
        result: VerifyResult,
    },
    Output {
        owner: DatabaseKeyIndex,
        key: DatabaseKeyIndex,
    },
    Ownership(Event),
    Body {
        key: DatabaseKeyIndex,
    },
    Result {
        outcome: &'static str,
        unchanged: Option<bool>,
    },
}

thread_local! {
    static TRACE: RefCell<Option<Vec<TraceEvent>>> = const { RefCell::new(None) };
}

pub(in crate::function) fn record(event: TraceEvent) {
    TRACE.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            trace.push(event);
        }
    });
}

pub(in crate::function::execute::execution_run) fn collect<T>(
    body: impl FnOnce() -> T,
) -> (T, Vec<TraceEvent>) {
    TRACE.with_borrow_mut(|trace| assert!(trace.replace(Vec::new()).is_none()));
    let reset = Reset;
    let result = body();
    let trace = TRACE.with_borrow_mut(|trace| trace.take().unwrap());
    drop(reset);
    (result, trace)
}

pub(in crate::function::execute::execution_run) fn hot_selected(
    operation: usize,
) -> Option<(DatabaseKeyIndex, TraceEvent)> {
    TRACE.with_borrow(|trace| {
        let mut events = trace.as_ref()?.iter().rev().copied().filter(|event| {
            matches!(event, TraceEvent::Outer { operation: observed, .. } if *observed == operation)
        });
        let selected @ TraceEvent::Outer {
            phase: "fetch.selected",
            key: None,
            ..
        } = events.next()?
        else {
            return None;
        };
        let TraceEvent::Outer {
            phase: "fetch.probe",
            key: Some(key),
            ..
        } = events.next()?
        else {
            return None;
        };
        Some((key, selected))
    })
}

pub(super) fn latest_boundary() -> Option<TraceEvent> {
    TRACE.with_borrow(|trace| {
        trace.as_ref()?.iter().rev().copied().find(|event| {
            matches!(
                event,
                TraceEvent::Outer { .. }
                    | TraceEvent::Verifier { .. }
                    | TraceEvent::Request { .. }
                    | TraceEvent::Reply { .. }
            )
        })
    })
}

pub(super) fn dependency_resumes(owner: DatabaseKeyIndex) -> usize {
    TRACE.with_borrow(|trace| {
        trace.as_ref().map_or(0, |trace| {
            trace.iter().filter(|event| {
            matches!(event, TraceEvent::Outer { phase: "dependency.resume", key: Some(key), .. }
                if *key == owner)
        }).count()
        })
    })
}

pub(super) fn pending_dependency(owner: DatabaseKeyIndex) -> Option<DatabaseKeyIndex> {
    TRACE.with_borrow(|trace| {
        for event in trace.as_ref()?.iter().rev() {
            match event {
                TraceEvent::Request {
                    owner: pending,
                    key,
                    ..
                } if *pending == owner => {
                    return Some(*key);
                }
                TraceEvent::Reply { owner: resumed, .. } if *resumed == owner => return None,
                _ => {}
            }
        }
        None
    })
}

struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        TRACE.with_borrow_mut(|trace| *trace = None);
    }
}

/// Assign identities in first-observation order without losing aliasing relationships.
/// Byte charges, phase order, revisions and all outcomes remain in the saved trace.
#[derive(Default)]
struct Identities {
    keys: Vec<DatabaseKeyIndex>,
    claims: Vec<usize>,
    memos: Vec<usize>,
    operations: Vec<usize>,
}

fn identity<T: PartialEq>(values: &mut Vec<T>, value: T) -> usize {
    if let Some(index) = values.iter().position(|known| *known == value) {
        index
    } else {
        let index = values.len();
        values.push(value);
        index
    }
}

impl Identities {
    fn key(&mut self, value: DatabaseKeyIndex) -> usize {
        identity(&mut self.keys, value)
    }

    fn owner(&mut self, value: Option<DatabaseKeyIndex>) -> Option<usize> {
        value.map(|key| self.key(key))
    }

    fn event(&mut self, event: TraceEvent) -> String {
        match event {
            TraceEvent::Admission {
                work,
                outcome,
                owner,
                depths,
            } => {
                let owner = self.owner(owner);
                format!("admission {work:?} {outcome:?} owner={owner:?} depths={depths:?}")
            }
            TraceEvent::Outer {
                phase,
                key,
                operation,
                owner,
                query_depth,
                operation_depth,
            } => {
                let key = self.owner(key);
                let operation = identity(&mut self.operations, operation);
                let owner = self.owner(owner);
                format!(
                    "outer {phase} key={key:?} operation={operation} owner={owner:?} query_depth={query_depth} operation_depth={operation_depth}"
                )
            }
            TraceEvent::Verifier {
                phase,
                key,
                claim,
                memo,
                verified_at,
                current_revision,
                accumulated,
                owner,
                depths,
            } => {
                let key = self.key(key);
                let claim = identity(&mut self.claims, claim);
                let memo = identity(&mut self.memos, memo);
                let owner = self.owner(owner);
                format!(
                    "verifier {phase} key={key} claim={claim} memo={memo} verified={verified_at:?} current={current_revision:?} accumulated={accumulated:?} owner={owner:?} depths={depths:?}"
                )
            }
            TraceEvent::Request {
                owner,
                key,
                changed_after,
            } => {
                let owner = self.key(owner);
                let key = self.key(key);
                format!("request owner={owner} key={key} changed_after={changed_after:?}")
            }
            TraceEvent::Reply { owner, key, result } => {
                let owner = self.key(owner);
                let key = self.key(key);
                format!("reply owner={owner} key={key} result={result:?}")
            }
            TraceEvent::Output { owner, key } => {
                let owner = self.key(owner);
                let key = self.key(key);
                format!("output owner={owner} key={key}")
            }
            TraceEvent::Ownership(Event::Claim {
                key,
                serial,
                operation,
            }) => {
                let key = self.key(key);
                let claim = identity(&mut self.claims, serial);
                let operation = identity(&mut self.operations, operation);
                format!("claim key={key} claim={claim} operation={operation}")
            }
            TraceEvent::Ownership(Event::Execute {
                key,
                serial,
                operation,
                old_memo,
            }) => {
                let key = self.key(key);
                let claim = identity(&mut self.claims, serial);
                let operation = identity(&mut self.operations, operation);
                let memo = old_memo.map(|memo| identity(&mut self.memos, memo));
                format!("execute key={key} claim={claim} operation={operation} old_memo={memo:?}")
            }
            TraceEvent::Body { key } => format!("body key={}", self.key(key)),
            TraceEvent::Result { outcome, unchanged } => {
                format!("result {outcome} unchanged={unchanged:?}")
            }
        }
    }
}

pub(super) fn normalize(events: &[TraceEvent]) -> Vec<String> {
    let mut identities = Identities::default();
    events
        .iter()
        .map(|event| identities.event(*event))
        .collect()
}

pub(super) fn emit(case: &str, events: &[TraceEvent]) {
    for (ordinal, event) in normalize(events).iter().enumerate() {
        println!("VALIDATION_TRACE\t{case}\t{ordinal}\t{event}");
    }
}
