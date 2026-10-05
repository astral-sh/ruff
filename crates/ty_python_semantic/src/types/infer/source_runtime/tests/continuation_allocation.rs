use std::cell::RefCell;
use std::future::{Future, poll_fn, ready};
use std::pin::{Pin, pin};
use std::task::{Context, Poll};

use salsa::attempt_probe::remaining_allowance_for_diagnostics;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::call::bind::origin::OriginEffects;
use crate::types::call::bind::source_check::SourceCheckerAccess;

#[derive(Default)]
struct AllocationEvents {
    work_before: Cell<Option<usize>>,
    constructed: Cell<usize>,
    polled: Cell<usize>,
    dropped: Cell<usize>,
}

struct ObservedFuture<'a, const N: usize> {
    events: &'a AllocationEvents,
    padding: [u8; N],
}

impl<const N: usize> Future for ObservedFuture<'_, N> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        std::hint::black_box(&self.padding);
        self.events.polled.set(self.events.polled.get() + 1);
        Poll::Ready(())
    }
}

impl<const N: usize> Drop for ObservedFuture<'_, N> {
    fn drop(&mut self) {
        self.events.dropped.set(self.events.dropped.get() + 1);
    }
}

struct Allocate<'a, const N: usize> {
    events: &'a AllocationEvents,
    count: usize,
    inline: bool,
}

impl<'db, const N: usize> MemberOperation<'db> for Allocate<'_, N> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        self.events
            .work_before
            .set(remaining_allowance_for_diagnostics(access.db()));
        for _ in 0..self.count {
            let before = remaining_allowance_for_diagnostics(access.db()).unwrap();
            let make = || {
                self.events
                    .constructed
                    .set(self.events.constructed.get() + 1);
                ObservedFuture {
                    events: self.events,
                    padding: [0; N],
                }
            };
            if self.inline {
                effects.initialize_value(make).await?.await;
            } else {
                effects.allocate_future(make).await?.await;
            }
            let after = remaining_allowance_for_diagnostics(access.db()).unwrap();
            assert_eq!(before - after, 1);
        }
        Ok(())
    }
}

fn allocate<const N: usize>(
    inline: bool,
    policy: AnalysisPolicy,
    count: usize,
) -> (AnalysisOutcome<()>, AllocationEvents) {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let events = AllocationEvents::default();
    let result = controlled_member_operation(
        &prepared,
        Allocate::<N> {
            events: &events,
            count,
            inline,
        },
        &policy,
    )
    .unwrap();
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    (result, events)
}

/// Boxed continuations and inline values charge one work unit independently of size, while bytes
/// remain cumulative after polling and destruction. Refused constructions never run their factory.
#[test]
fn fixed_value_work_and_bytes_are_independent_and_not_refunded() {
    for inline in [false, true] {
        check_work_and_bytes(inline);
    }
}

fn check_work_and_bytes(inline: bool) {
    for (result, events) in [
        allocate::<8>(inline, funded(), 2),
        allocate::<4096>(inline, funded(), 2),
    ] {
        assert_eq!(result, AnalysisOutcome::Complete(()));
        assert_eq!(events.constructed.get(), 2);
        assert_eq!(events.polled.get(), 2);
        assert_eq!(events.dropped.get(), 2);
    }

    let (_, events) = allocate::<4096>(inline, funded(), 1);
    let work_before = events.work_before.get().unwrap();
    let (result, events) = allocate::<4096>(
        inline,
        AnalysisPolicy {
            semantic_work_limit: funded().semantic_work_limit - work_before,
            ..funded()
        },
        1,
    );
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }
    );
    assert_eq!(events.work_before.get(), Some(0));
    assert_eq!(events.constructed.get(), 0);
    assert_eq!(events.polled.get(), 0);
    assert_eq!(events.dropped.get(), 0);

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let (_, events) = allocate::<4096>(
            inline,
            AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            1,
        );
        if events.constructed.get() == 0 {
            lower = middle + 1;
        } else {
            upper = middle;
        }
    }
    let policy = AnalysisPolicy {
        requested_bytes_limit: lower,
        ..funded()
    };
    let (result, events) = allocate::<4096>(inline, policy, 1);
    assert_eq!(result, AnalysisOutcome::Complete(()));
    assert_eq!(events.dropped.get(), 1);

    let (result, events) = allocate::<4096>(inline, policy, 2);
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        }
    );
    assert!(events.work_before.get().is_some());
    assert_eq!(events.constructed.get(), 1);
    assert_eq!(events.polled.get(), 1);
    assert_eq!(events.dropped.get(), 1);

    let two_futures = lower + size_of::<ObservedFuture<'_, 4096>>();
    let (result, events) = allocate::<4096>(
        inline,
        AnalysisPolicy {
            requested_bytes_limit: two_futures,
            ..funded()
        },
        2,
    );
    assert_eq!(result, AnalysisOutcome::Complete(()));
    assert_eq!(events.constructed.get(), 2);
    assert_eq!(events.polled.get(), 2);
    assert_eq!(events.dropped.get(), 2);

    let (result, events) = allocate::<4096>(
        inline,
        AnalysisPolicy {
            requested_bytes_limit: two_futures - 1,
            ..funded()
        },
        2,
    );
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        }
    );
    assert_eq!(events.constructed.get(), 1);
    assert_eq!(events.polled.get(), 1);
    assert_eq!(events.dropped.get(), 1);

    let (result, events) = allocate::<4096>(
        inline,
        AnalysisPolicy {
            requested_bytes_limit: lower - 1,
            ..funded()
        },
        1,
    );
    assert_eq!(
        result,
        AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        }
    );
    assert!(events.work_before.get().is_some());
    assert_eq!(events.constructed.get(), 0);
    assert_eq!(events.polled.get(), 0);
    assert_eq!(events.dropped.get(), 0);
}

struct RecordedDrop<'a> {
    events: &'a RefCell<Vec<&'static str>>,
    name: &'static str,
}

impl Drop for RecordedDrop<'_> {
    fn drop(&mut self) {
        self.events.borrow_mut().push(self.name);
    }
}

#[derive(Clone, Copy, Debug)]
enum LocalAdapter {
    Origin,
    Checker,
    FixedTransfers,
    BoxedFuture,
}

struct RetainFactory<'a> {
    events: &'a RefCell<Vec<&'static str>>,
    adapter: LocalAdapter,
    work: Option<usize>,
    bytes: Option<usize>,
}

impl<'db> MemberOperation<'db> for RetainFactory<'_> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let events = self.events;
        let _owner = RecordedDrop {
            events,
            name: "owner",
        };
        let capture = RecordedDrop {
            events,
            name: "capture",
        };
        let factory = move || {
            events.borrow_mut().push("invoked");
            capture
        };
        let mut helper = pin!(async {
            match self.adapter {
                LocalAdapter::Origin => {
                    effects.origin_local(self.work, self.bytes, factory).await
                }
                LocalAdapter::Checker => {
                    effects.checker_local(self.work, self.bytes, factory).await
                }
                LocalAdapter::FixedTransfers => {
                    let quote = self.work.zip(self.bytes).ok_or(RunError::Contract(
                        "fixed transfer input quotation overflow",
                    ));
                    effects.local_quoted_with_fixed_transfers(quote, factory).await
                }
                LocalAdapter::BoxedFuture => {
                    let quote = self.work.zip(self.bytes).ok_or(RunError::Contract(
                        "boxed future input quotation overflow",
                    ));
                    Ok(effects
                        .boxed_future_with_fixed_transfers(quote, || ready(factory()))
                        .await?
                        .await)
                }
            }
        });
        let capture = poll_fn(|cx| {
            let result = helper.as_mut().poll(cx);
            if result.is_pending() {
                events.borrow_mut().push("suspended");
            }
            result
        })
        .await?;
        drop(capture);
        events.borrow_mut().push("resumed");
        Ok(())
    }
}

/// Refused origin, checker, fixed-transfer, and boxed-future factories keep captures alive when the helper suspends, then
/// drop them before the enclosing operation's owner during drainage. Work, byte, and overflow
/// refusals, including the checker's `isize` bound and overflow while adding the fixed helper's
/// work or bytes, do not invoke the factory. For the boxed-future adapter, this also prevents
/// child construction. A fresh factory with admissible charges runs successfully in the same revision.
#[test]
fn local_factories_retain_captures_through_refusal_and_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    for adapter in [LocalAdapter::Origin, LocalAdapter::Checker, LocalAdapter::FixedTransfers, LocalAdapter::BoxedFuture] {
        let (work_error, storage_error) = match adapter {
            LocalAdapter::Origin => (
                "source work quotation overflow",
                "source work quotation overflow",
            ),
            LocalAdapter::Checker => (
                "checker work quotation overflow",
                "checker storage quotation overflow",
            ),
            LocalAdapter::FixedTransfers => (
                "fixed transfer input quotation overflow",
                "fixed transfer input quotation overflow",
            ),
            LocalAdapter::BoxedFuture => (
                "boxed future input quotation overflow",
                "boxed future input quotation overflow",
            ),
        };
        let work_overflow = AnalysisFailure::Execution(RunError::Contract(work_error));
        let storage_overflow = AnalysisFailure::Execution(RunError::Contract(storage_error));
        let extra_cases = match adapter {
            LocalAdapter::Origin => [None, None],
            LocalAdapter::Checker => [
                Some((
                    Some(1),
                    Some(isize::MAX as usize + 1),
                    Err(storage_overflow),
                )),
                None,
            ],
            LocalAdapter::FixedTransfers => [
                // The helper's checked addition of 64 work units overflows for this payload.
                Some((
                    Some(usize::MAX - 63),
                    Some(0),
                    Err(AnalysisFailure::Execution(RunError::Contract(
                        "local transfer work quotation overflow",
                    ))),
                )),
                // Adding its nonzero fixed carrier bytes to this payload also overflows.
                Some((
                    Some(1),
                    Some(usize::MAX),
                    Err(AnalysisFailure::Execution(RunError::Contract(
                        "local transfer byte quotation overflow",
                    ))),
                )),
            ],
            LocalAdapter::BoxedFuture => [
                Some((
                    Some(usize::MAX - 31),
                    Some(0),
                    Err(AnalysisFailure::Execution(RunError::Contract(
                        "boxed future work quotation overflow",
                    ))),
                )),
                Some((
                    Some(1),
                    Some(usize::MAX),
                    Err(AnalysisFailure::Execution(RunError::Contract(
                        "boxed future byte quotation overflow",
                    ))),
                )),
            ],
        };
        let cases = [
            (
                Some(funded().semantic_work_limit + 1),
                Some(0),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            (
                Some(1),
                Some(funded().requested_bytes_limit + 1),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    completed: (),
                }),
            ),
            // A missing work or byte quotation supplies an already-refused input quote.
            (None, Some(0), Err(work_overflow)),
            (Some(1), None, Err(storage_overflow)),
        ]
        .into_iter()
        .chain(extra_cases.into_iter().flatten());
        for (work, bytes, expected) in cases {
            let events = RefCell::new(Vec::new());
            let result = controlled_member_operation(
                &prepared,
                RetainFactory {
                    events: &events,
                    adapter,
                    work,
                    bytes,
                },
                &funded(),
            );
            assert_eq!(result, expected, "{adapter:?}, work={work:?}, bytes={bytes:?}");
            assert_eq!(*events.borrow(), ["suspended", "capture", "owner"]);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);

            events.borrow_mut().clear();
            let retry = controlled_member_operation(
                &prepared,
                RetainFactory {
                    events: &events,
                    adapter,
                    work: Some(1),
                    bytes: Some(0),
                },
                &funded(),
            );
            assert_eq!(retry, Ok(AnalysisOutcome::Complete(())));
            assert_eq!(*events.borrow(), ["invoked", "capture", "resumed", "owner"]);
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}
