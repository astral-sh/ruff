//! Deterministic setup for two workers that enter through the public evaluation API.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{AttemptOutcome, LOCAL, StartError};
use crate::Database;
use crate::prepared_source_probe::Stamp;

const SETUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairInstallError {
    Start(StartError),
    SetupAborted,
}

enum SetupCommand {
    Start,
    Abort,
}

enum SetupEvent {
    Ready(usize),
    Finished,
}

struct PairState {
    stamp: Stamp,
    events: Sender<SetupEvent>,
}

struct SetupController {
    commands: [Sender<SetupCommand>; 2],
    events: Receiver<SetupEvent>,
}

impl SetupController {
    fn release(&self) {
        let deadline = Instant::now() + SETUP_TIMEOUT;
        let mut ready = [false; 2];
        while !ready.iter().all(|ready| *ready) {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(SetupEvent::Ready(worker)) => ready[worker] = true,
                Ok(SetupEvent::Finished) | Err(_) => return,
            }
        }
        for command in &self.commands {
            if command.send(SetupCommand::Start).is_err() {
                return;
            }
        }
    }
}

impl Drop for SetupController {
    fn drop(&mut self) {
        for command in &self.commands {
            let _ = command.send(SetupCommand::Abort);
        }
    }
}

struct WorkerFinished<'a>(&'a Sender<SetupEvent>);

impl Drop for WorkerFinished<'_> {
    fn drop(&mut self) {
        let _ = self.0.send(SetupEvent::Finished);
    }
}

pub(crate) struct Participant<'pair> {
    state: &'pair PairState,
    worker: usize,
    command: Receiver<SetupCommand>,
}

impl Participant<'_> {
    fn await_start(&self) -> Result<(), PairInstallError> {
        if self
            .state
            .events
            .send(SetupEvent::Ready(self.worker))
            .is_err()
            || !matches!(
                self.command.recv_timeout(SETUP_TIMEOUT),
                Ok(SetupCommand::Start)
            )
        {
            return Err(PairInstallError::SetupAborted);
        }
        Ok(())
    }

    fn check_entry(&self, db: &dyn Database) -> Result<(), PairInstallError> {
        assert!(
            self.state.stamp.belongs_to(db),
            "foreign paired participant database or stamp"
        );
        check_local_entry(db).map_err(PairInstallError::Start)
    }

    pub(crate) fn run_ordinary<T>(
        self,
        db: &dyn Database,
        body: impl FnOnce() -> T,
    ) -> Result<T, PairInstallError> {
        self.check_entry(db)?;
        self.await_start()?;
        let value = super::try_with_operation(db, body).map_err(PairInstallError::Start)?;
        assert!(
            self.state.stamp.belongs_to(db),
            "database changed during paired ordinary work"
        );
        Ok(value)
    }

    pub(crate) fn run<T>(
        self,
        db: &dyn Database,
        allowance: usize,
        body: impl FnOnce() -> T,
    ) -> Result<AttemptOutcome<T>, PairInstallError> {
        self.check_entry(db)?;
        // Setup failure has no evaluation to finish or abandon. Actual entry occurs only
        // after the schedule is ready, through the same API as an independent caller.
        self.await_start()?;
        super::try_with_attempt(db, allowance, body).map_err(PairInstallError::Start)
    }
}

fn check_local_entry(db: &dyn Database) -> Result<(), StartError> {
    if db.zalsa_local().active_query().is_some() {
        return Err(StartError::ActiveQuery);
    }
    LOCAL.with_borrow(|local| {
        if local.session.is_some() {
            return Err(StartError::NestedAttempt);
        }
        if !local.operations.is_empty() || !local.scopes.is_empty() {
            return Err(StartError::ActiveOperation);
        }
        Ok(())
    })
}

pub(crate) fn run_pair<D, L, R, FL, FR>(
    db: &D,
    left: FL,
    right: FR,
) -> Result<(thread::Result<L>, thread::Result<R>), StartError>
where
    D: Database + Clone + Send,
    FL: for<'pair> FnOnce(D, Participant<'pair>) -> L + Send,
    FR: for<'pair> FnOnce(D, Participant<'pair>) -> R + Send,
    L: Send,
    R: Send,
{
    let left_db = db.clone();
    let right_db = db.clone();
    let (events_tx, events_rx) = mpsc::channel();
    let (left_tx, left_rx) = mpsc::channel();
    let (right_tx, right_rx) = mpsc::channel();
    check_local_entry(db)?;
    let state = PairState {
        stamp: Stamp::current(db),
        events: events_tx,
    };
    let outcomes = thread::scope(|scope| {
        // Drop sends Abort before scope's implicit joins if spawning or coordination unwinds.
        let controller = SetupController {
            commands: [left_tx, right_tx],
            events: events_rx,
        };
        let state = &state;
        let left = thread::Builder::new()
            .name("paired-attempt-left".into())
            .spawn_scoped(scope, move || {
                let _finished = WorkerFinished(&state.events);
                left(
                    left_db,
                    Participant {
                        state,
                        worker: 0,
                        command: left_rx,
                    },
                )
            })
            .unwrap_or_else(|error| panic!("cannot spawn left paired worker: {error}"));
        let right = match thread::Builder::new()
            .name("paired-attempt-right".into())
            .spawn_scoped(scope, move || {
                let _finished = WorkerFinished(&state.events);
                right(
                    right_db,
                    Participant {
                        state,
                        worker: 1,
                        command: right_rx,
                    },
                )
            }) {
            Ok(right) => right,
            Err(error) => {
                drop(controller);
                let outcome = left.join();
                if let Err(payload) = outcome {
                    std::panic::resume_unwind(payload);
                }
                panic!("cannot spawn right paired worker: {error}");
            }
        };
        controller.release();
        drop(controller);
        (left.join(), right.join())
    });
    Ok(outcomes)
}
