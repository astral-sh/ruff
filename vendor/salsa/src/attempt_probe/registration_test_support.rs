use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EntryKind {
    Scope,
    Query,
}

struct Pause {
    kind: EntryKind,
    entered: Sender<()>,
    release: Receiver<()>,
}

thread_local! {
    static PAUSE: RefCell<Option<Pause>> = const { RefCell::new(None) };
}

/// Pauses test-only registration bookkeeping with no query or dependency-graph locks held.
pub(super) fn after_increment(kind: EntryKind) {
    let pause = PAUSE.with_borrow_mut(Option::take);
    if let Some(pause) = pause {
        assert_eq!(pause.kind, kind);
        pause.entered.send(()).expect("entry controller is alive");
        pause
            .release
            .recv_timeout(TIMEOUT)
            .expect("entry controller released the worker");
    }
}

pub(crate) struct EntryControl {
    start: RefCell<Option<Sender<()>>>,
    entered: Receiver<()>,
    release: RefCell<Option<Sender<()>>>,
    finished: Receiver<()>,
}

impl EntryControl {
    pub(crate) fn start_and_wait(&self) {
        self.start
            .borrow_mut()
            .take()
            .expect("worker starts only once")
            .send(())
            .expect("worker is awaiting entry");
        self.entered
            .recv_timeout(TIMEOUT)
            .expect("worker reached the real registration increment");
    }

    pub(crate) fn release_and_wait(&self) {
        self.release();
        self.finished
            .recv_timeout(TIMEOUT)
            .expect("worker completed its registered operation");
    }

    fn release(&self) {
        if let Some(release) = self.release.borrow_mut().take() {
            let _ = release.send(());
        }
    }
}

impl Drop for EntryControl {
    fn drop(&mut self) {
        // Release a paused worker before scoped joining, including when an assertion panics.
        self.release();
    }
}

pub(crate) struct EntryHook {
    start: Receiver<()>,
    pause: Pause,
    finished: Sender<()>,
}

impl EntryHook {
    pub(crate) fn run<T>(self, body: impl FnOnce() -> T) -> Option<T> {
        match self.start.recv_timeout(TIMEOUT) {
            Ok(()) => {}
            // The controlling test unwound before asking this worker to enter.
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => panic!("entry controller did not start the worker"),
        }
        PAUSE.with_borrow_mut(|pause| assert!(pause.replace(self.pause).is_none()));
        let _reset = Reset;
        let value = body();
        let _ = self.finished.send(());
        Some(value)
    }
}

struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        PAUSE.with_borrow_mut(|pause| *pause = None);
    }
}

pub(crate) fn paused_entry(kind: EntryKind) -> (EntryControl, EntryHook) {
    let (start_tx, start_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    (
        EntryControl {
            start: RefCell::new(Some(start_tx)),
            entered: entered_rx,
            release: RefCell::new(Some(release_tx)),
            finished: finished_rx,
        },
        EntryHook {
            start: start_rx,
            pause: Pause {
                kind,
                entered: entered_tx,
                release: release_rx,
            },
            finished: finished_tx,
        },
    )
}
