//! Copied observations for the two-query transferred-claim experiment.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::ThreadId;
use std::time::Duration;

use super::{AttemptSupport, LOCAL, MemoReuse};
use crate::cycle::IterationStamp;
use crate::function::SyncOwner;
use crate::key::DatabaseKeyIndex;
use crate::runtime::WaitResult;
use crate::{Durability, Revision};

const CAPACITY: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SupportSnapshot {
    pub owner: usize,
    pub database: usize,
    pub revision: Revision,
    pub cancellation: u8,
    pub state: u8,
    pub explicitly_incomplete: bool,
}

pub(crate) fn support_snapshot(support: &AttemptSupport) -> SupportSnapshot {
    SupportSnapshot {
        owner: Arc::as_ptr(&support.owner).addr(),
        database: support.owner.database,
        revision: support.owner.revision,
        cancellation: support.owner.cancellation,
        state: support.owner.state.load(Ordering::Acquire),
        explicitly_incomplete: support.explicitly_incomplete,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionSnapshot {
    pub support: SupportSnapshot,
    pub remaining: usize,
    pub scope: Option<usize>,
}

pub(crate) fn session_snapshot() -> Option<SessionSnapshot> {
    LOCAL.with(|local| {
        let Ok(local) = local.try_borrow() else {
            BROKEN.set(true);
            return None;
        };
        local.session.as_ref().map(|session| SessionSnapshot {
            support: support_snapshot(&session.support),
            remaining: session.remaining,
            scope: session.current_scope.map(|scope| scope.get()),
        })
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Slots<T: Copy> {
    pub entries: [Option<T>; 2],
    pub overflow: bool,
}

impl<T: Copy> Default for Slots<T> {
    fn default() -> Self {
        Self {
            entries: [None; 2],
            overflow: false,
        }
    }
}

impl<T: Copy> Slots<T> {
    pub(crate) fn push(&mut self, value: T) {
        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(value);
        } else {
            self.overflow = true;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SyncSnapshot {
    pub owner: SyncOwner,
    pub anyone_waiting: bool,
    pub is_transfer_target: bool,
    pub claimed_twice: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GraphSnapshot {
    pub edges: Slots<(ThreadId, ThreadId)>,
    pub dependents: Slots<(DatabaseKeyIndex, Slots<ThreadId>)>,
    pub pending: Slots<(ThreadId, WaitResult)>,
    pub transferred: Slots<(DatabaseKeyIndex, ThreadId, DatabaseKeyIndex)>,
    pub reverse: Slots<(DatabaseKeyIndex, Slots<DatabaseKeyIndex>)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MemoSnapshot {
    pub identity: usize,
    pub has_value: bool,
    pub verified_at: Revision,
    pub execution_revision: Option<Revision>,
    pub iteration: IterationStamp,
    pub heads: Slots<(DatabaseKeyIndex, IterationStamp)>,
    pub converged: bool,
    pub final_: bool,
    pub changed_at: Revision,
    pub durability: Durability,
    pub support: Option<SupportSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Claim,
    Mode,
    Terminal,
    TransferBegin,
    TransferEnd,
    Restore,
    Edge,
    WaitConsumed,
    Unblock,
    Mapping,
    MappingRemoved,
    Undo,
    TransferWaitBegin,
    TransferWaitEnd,
    EdgeRemap,
    Reuse,
    SeedAllowed,
    PrepareSupplied,
    PrepareRetained,
    Iteration,
    Previous,
    SeedActive,
    Verification,
    ColdSelected,
    ColdInitial,
    InitialInserted,
    Verified,
    Executed,
    Target,
    TargetCurrent,
    CommitCurrent,
    RootPublished,
    Poisoned,
    TargetPublished,
    Refetch,
    SupportIncoming,
    SupportAccepted,
    PreDebit,
    DebitAccepted,
    DebitRefused,
    Admission,
    BodyValue,
    InitialValue,
    RecoveryValue,
    Authority,
    Gate,
    RootResult,
    MarkerRead,
    ChildRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Body,
    Initial,
    Recovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Drop,
    Abort,
    Panic,
    Absent,
    Iteration,
    Finalize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Default,
    SelfOnly,
    TransferTo(DatabaseKeyIndex),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Event {
    pub kind: Kind,
    pub key: Option<DatabaseKeyIndex>,
    pub other_key: Option<DatabaseKeyIndex>,
    pub serial: Option<usize>,
    pub mode: Option<Mode>,
    pub sync: Option<SyncSnapshot>,
    pub memo: Option<MemoSnapshot>,
    pub other_memo: Option<MemoSnapshot>,
    pub session: Option<SessionSnapshot>,
    pub support: Option<SupportSnapshot>,
    pub previous_support: Option<SupportSnapshot>,
    pub peer: Option<ThreadId>,
    pub from: Option<ThreadId>,
    pub wait: Option<WaitResult>,
    pub reuse: Option<MemoReuse>,
    pub action: Option<Action>,
    pub step: Option<Step>,
    pub decision: bool,
    pub units: usize,
    pub identity: usize,
    pub value: u32,
    pub iteration: Option<IterationStamp>,
    pub phase: Option<&'static str>,
}

impl Event {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            key: None,
            other_key: None,
            serial: None,
            mode: None,
            sync: None,
            memo: None,
            other_memo: None,
            session: session_snapshot(),
            support: None,
            previous_support: None,
            peer: None,
            from: None,
            wait: None,
            reuse: None,
            action: None,
            step: None,
            decision: false,
            units: 0,
            identity: 0,
            value: 0,
            iteration: None,
            phase: None,
        }
    }
    pub(crate) fn key(mut self, key: DatabaseKeyIndex) -> Self {
        self.key = Some(key);
        self
    }
    pub(crate) fn serial(mut self, serial: usize) -> Self {
        self.serial = Some(serial);
        self
    }
    pub(crate) fn memo(mut self, memo: Option<MemoSnapshot>) -> Self {
        self.memo = memo;
        self
    }
    pub(crate) fn decision(mut self, decision: bool) -> Self {
        self.decision = decision;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub ordinal: usize,
    pub worker: usize,
    pub thread: ThreadId,
    pub event: Event,
}

pub(crate) struct TraceConfig {
    pub ordinal: Arc<AtomicUsize>,
    pub worker: usize,
}

#[derive(Debug)]
pub(crate) struct TransferTrace {
    pub records: Vec<Record>,
    pub broken: bool,
}

struct Collector {
    config: TraceConfig,
    thread: ThreadId,
    records: Vec<Record>,
}

thread_local! {
    static COLLECTOR: RefCell<Option<Collector>> = const { RefCell::new(None) };
    static BROKEN: Cell<bool> = const { Cell::new(false) };
    static FINALITY_PAUSE: RefCell<Option<FinalityPause>> = const { RefCell::new(None) };
    static FINALITY_PAUSE_DIRECT_INSTALLED: Cell<bool> = const { Cell::new(false) };
    static FINALITY_PAUSE_INBOX: RefCell<Option<Receiver<FinalityPause>>> = const { RefCell::new(None) };
}

pub(crate) struct FinalityPause {
    key: DatabaseKeyIndex,
    identity: usize,
    head: DatabaseKeyIndex,
    head_identity: usize,
    selected: Sender<()>,
    resume: Receiver<()>,
}

pub(crate) struct FinalityPauseControl {
    selected: Receiver<()>,
    resume: Sender<()>,
}

impl FinalityPauseControl {
    pub(crate) fn wait_for_selection(&self) {
        self.selected
            .recv_timeout(Duration::from_secs(5))
            .expect("the participant selected its exact head");
    }
}

impl Drop for FinalityPauseControl {
    fn drop(&mut self) {
        // Release the proof on success and on a failed producer assertion.
        let _ = self.resume.send(());
    }
}

pub(crate) fn finality_pause(
    key: DatabaseKeyIndex,
    identity: usize,
    head: DatabaseKeyIndex,
    head_identity: usize,
) -> (FinalityPause, FinalityPauseControl) {
    let (selected, selected_rx) = mpsc::channel();
    let (resume_tx, resume) = mpsc::channel();
    (
        FinalityPause {
            key,
            identity,
            head,
            head_identity,
            selected,
            resume,
        },
        FinalityPauseControl {
            selected: selected_rx,
            resume: resume_tx,
        },
    )
}

pub(crate) struct FinalityPauseInstalled;

impl Drop for FinalityPauseInstalled {
    fn drop(&mut self) {
        FINALITY_PAUSE.with_borrow_mut(|slot| *slot = None);
        FINALITY_PAUSE_DIRECT_INSTALLED.set(false);
    }
}

pub(crate) fn install_finality_pause(pause: FinalityPause) -> FinalityPauseInstalled {
    assert!(!FINALITY_PAUSE_DIRECT_INSTALLED.get());
    FINALITY_PAUSE_INBOX.with_borrow(|slot| assert!(slot.is_none()));
    FINALITY_PAUSE.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(pause);
    });
    FINALITY_PAUSE_DIRECT_INSTALLED.set(true);
    FinalityPauseInstalled
}

pub(crate) struct FinalityPauseInboxInstalled;

impl Drop for FinalityPauseInboxInstalled {
    fn drop(&mut self) {
        FINALITY_PAUSE_INBOX.with_borrow_mut(|slot| *slot = None);
        FINALITY_PAUSE.with_borrow_mut(|slot| *slot = None);
    }
}

pub(crate) fn install_finality_pause_inbox(
    receiver: Receiver<FinalityPause>,
) -> FinalityPauseInboxInstalled {
    assert!(!FINALITY_PAUSE_DIRECT_INSTALLED.get());
    FINALITY_PAUSE.with_borrow(|slot| assert!(slot.is_none()));
    FINALITY_PAUSE_INBOX.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(receiver);
    });
    FinalityPauseInboxInstalled
}

// This dedicated hook runs outside graph locks, after the canonical claim and head
// selection but before acceptance is acquired. The trace recorder must never wait.
pub(crate) fn finality_head_selected(
    key: DatabaseKeyIndex,
    identity: usize,
    head: DatabaseKeyIndex,
    head_identity: usize,
) {
    let incoming = FINALITY_PAUSE_INBOX.with_borrow(|slot| {
        slot.as_ref().and_then(|receiver| receiver.try_recv().ok())
    });
    if let Some(pause) = incoming {
        FINALITY_PAUSE.with_borrow_mut(|slot| {
            assert!(slot.is_none());
            *slot = Some(pause);
        });
    }
    let pause = FINALITY_PAUSE.with_borrow_mut(|slot| {
        if slot.as_ref().is_some_and(|pause| {
            (pause.key, pause.identity, pause.head, pause.head_identity)
                == (key, identity, head, head_identity)
        }) {
            slot.take()
        } else {
            None
        }
    });
    if let Some(pause) = pause {
        pause
            .selected
            .send(())
            .expect("proof controller remains live");
        pause
            .resume
            .recv_timeout(Duration::from_secs(5))
            .expect("proof controller released selection");
    }
}

// Transition hooks may run under the claim or graph lock. The capacity check keeps
// their only write local and nonallocating; diagnostics cannot wake a query.
pub(crate) fn record(event: Event) {
    COLLECTOR.with(|collector| {
        let Ok(mut collector) = collector.try_borrow_mut() else {
            BROKEN.set(true);
            return;
        };
        if let Some(collector) = collector.as_mut() {
            if collector.records.len() == CAPACITY {
                BROKEN.set(true);
                return;
            }
            let ordinal = collector.config.ordinal.fetch_add(1, Ordering::Relaxed);
            collector.records.push(Record {
                ordinal,
                worker: collector.config.worker,
                thread: collector.thread,
                event,
            });
        }
    });
}

pub(crate) fn collect<T>(config: TraceConfig, body: impl FnOnce() -> T) -> (T, TransferTrace) {
    let collector = Collector {
        config,
        thread: std::thread::current().id(),
        records: Vec::with_capacity(CAPACITY),
    };
    COLLECTOR.with_borrow_mut(|slot| assert!(slot.replace(collector).is_none()));
    BROKEN.set(false);
    let result = body();
    let collector = COLLECTOR.with_borrow_mut(|slot| slot.take().expect("installed collector"));
    (
        result,
        TransferTrace {
            records: collector.records,
            broken: BROKEN.get(),
        },
    )
}
