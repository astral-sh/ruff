//! Passive frame-lifetime observations, kept outside the charged futures.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::task::Poll;

use ruff_db::system::{OsSystem, SystemPath, WritableSystem};

use crate::types::constructor::expansion_probe::Incomplete;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Outcome {
    Pending,
    Complete,
    Error(Incomplete),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    DriverEntered {
        driver: usize,
        parent: Option<usize>,
    },
    DriverExited {
        driver: usize,
    },
    Allocated {
        driver: usize,
        frame: usize,
        bytes: usize,
    },
    Polled {
        driver: usize,
        frame: usize,
        outcome: Outcome,
    },
    Dropped {
        driver: usize,
        frame: usize,
    },
}

struct ActiveDriver {
    id: usize,
    frames: Vec<usize>,
}

#[derive(Default)]
struct State {
    next_driver: usize,
    next_frame: usize,
    drivers: Vec<ActiveDriver>,
    events: Vec<Event>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct Installed;

impl Drop for Installed {
    fn drop(&mut self) {
        STATE.with(|state| state.borrow_mut().take());
    }
}

pub(in crate::types) fn capture<R>(body: impl FnOnce() -> R) -> (R, Vec<Event>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.is_none());
        *state = Some(State::default());
    });
    let installed = Installed;
    let result = body();
    let events = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else {
            return Vec::new();
        };
        assert!(state.drivers.is_empty(), "mapping driver escaped capture");
        std::mem::take(&mut state.events)
    });
    drop(installed);
    (result, events)
}

/// A driver owns its observer stack throughout synchronous callbacks; a nested driver
/// installs a separate stack and restores its parent before the parent's poll returns.
pub(super) struct DriverScope(Option<usize>);

impl DriverScope {
    pub(super) fn enter() -> Self {
        Self(STATE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.as_mut()?;
            let driver = state.next_driver;
            state.next_driver += 1;
            let parent = state.drivers.last().map(|driver| driver.id);
            state.drivers.push(ActiveDriver {
                id: driver,
                frames: Vec::new(),
            });
            state.events.push(Event::DriverEntered { driver, parent });
            Some(driver)
        }))
    }
}

impl Drop for DriverScope {
    fn drop(&mut self) {
        let Some(driver) = self.0 else { return };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(state) = state.as_mut() else { return };
            let Some(active) = state.drivers.pop() else {
                panic!("observed driver has no stack");
            };
            assert_eq!(active.id, driver);
            assert!(
                active.frames.is_empty(),
                "driver exited before its frames dropped"
            );
            state.events.push(Event::DriverExited { driver });
        });
    }
}

pub(super) fn allocated(bytes: usize) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else { return };
        let Some(driver) = state.drivers.last_mut() else {
            panic!("frame allocated outside observed driver");
        };
        let frame = state.next_frame;
        state.next_frame += 1;
        driver.frames.push(frame);
        state.events.push(Event::Allocated {
            driver: driver.id,
            frame,
            bytes,
        });
    });
}

pub(super) fn polled<T>(result: &Poll<Result<T, Incomplete>>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else { return };
        let Some((driver, frame)) = state
            .drivers
            .last()
            .and_then(|driver| Some((driver.id, *driver.frames.last()?)))
        else {
            panic!("polled frame has no observer entry");
        };
        let outcome = match result {
            Poll::Pending => Outcome::Pending,
            Poll::Ready(Ok(_)) => Outcome::Complete,
            Poll::Ready(Err(reason)) => Outcome::Error(*reason),
        };
        state.events.push(Event::Polled {
            driver,
            frame,
            outcome,
        });
    });
}

pub(super) fn dropped() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else { return };
        let Some((driver, frame)) = state
            .drivers
            .last_mut()
            .and_then(|driver| Some((driver.id, driver.frames.pop()?)))
        else {
            panic!("dropped frame has no observer entry");
        };
        state.events.push(Event::Dropped { driver, frame });
    });
}

#[derive(Default)]
struct Frame {
    driver: usize,
    bytes: usize,
    polls: usize,
    first: Option<Outcome>,
    last: Option<Outcome>,
    dropped: bool,
}

pub(in crate::types) fn write_trace(name: &str, events: &[Event]) -> anyhow::Result<()> {
    let directory = "/tmp/ty-mapping-frame-readiness";
    OsSystem::new("/tmp").create_directory_all(SystemPath::new(directory))?;
    let mut trace =
        std::io::BufWriter::new(std::fs::File::create(format!("{directory}/{name}.trace"))?);
    let mut frames: BTreeMap<usize, Frame> = BTreeMap::new();
    let mut drivers = Vec::new();
    let mut live_bytes = 0;
    let mut live_frames = 0;
    let mut peak_bytes = 0;
    let mut peak_frames = 0;
    let mut max_drivers = 0;
    for (sequence, event) in events.iter().enumerate() {
        writeln!(trace, "{sequence}: {event:?}")?;
        match *event {
            Event::DriverEntered { driver, parent } => {
                anyhow::ensure!(parent == drivers.last().copied());
                drivers.push(driver);
                max_drivers = max_drivers.max(drivers.len());
            }
            Event::DriverExited { driver } => {
                anyhow::ensure!(drivers.pop() == Some(driver));
            }
            Event::Allocated {
                driver,
                frame,
                bytes,
            } => {
                anyhow::ensure!(drivers.last() == Some(&driver));
                anyhow::ensure!(bytes > 0, "mapping frame has no stored state");
                anyhow::ensure!(
                    frames
                        .insert(
                            frame,
                            Frame {
                                driver,
                                bytes,
                                ..Frame::default()
                            }
                        )
                        .is_none()
                );
                live_bytes += bytes;
                live_frames += 1;
                peak_bytes = peak_bytes.max(live_bytes);
                peak_frames = peak_frames.max(live_frames);
            }
            Event::Polled {
                driver,
                frame,
                outcome,
            } => {
                let frame = frames
                    .get_mut(&frame)
                    .ok_or_else(|| anyhow::anyhow!("poll before allocation"))?;
                anyhow::ensure!(
                    drivers.last() == Some(&driver) && frame.driver == driver && !frame.dropped
                );
                anyhow::ensure!(frame.last.is_none_or(|outcome| outcome == Outcome::Pending));
                frame.first.get_or_insert(outcome);
                frame.last = Some(outcome);
                frame.polls += 1;
            }
            Event::Dropped { driver, frame } => {
                let frame = frames
                    .get_mut(&frame)
                    .ok_or_else(|| anyhow::anyhow!("drop before allocation"))?;
                anyhow::ensure!(
                    drivers.last() == Some(&driver) && frame.driver == driver && !frame.dropped
                );
                frame.dropped = true;
                live_bytes -= frame.bytes;
                live_frames -= 1;
            }
        }
    }
    anyhow::ensure!(drivers.is_empty() && live_bytes == 0 && live_frames == 0);
    anyhow::ensure!(frames.values().all(|frame| frame.dropped));
    let mut output =
        std::io::BufWriter::new(std::fs::File::create(format!("{directory}/{name}.tsv"))?);
    writeln!(output, "frame\tdriver\tbytes\tpolls\tfirst\tlast")?;
    for (id, frame) in &frames {
        writeln!(
            output,
            "{id}\t{}\t{}\t{}\t{:?}\t{:?}",
            frame.driver, frame.bytes, frame.polls, frame.first, frame.last
        )?;
    }
    let total_bytes: usize = frames.values().map(|frame| frame.bytes).sum();
    let completed_immediately = frames
        .values()
        .filter(|frame| frame.first == Some(Outcome::Complete))
        .count();
    let suspended = frames
        .values()
        .filter(|frame| frame.first == Some(Outcome::Pending))
        .count();
    let mut report =
        std::io::BufWriter::new(std::fs::File::create(format!("{directory}/{name}.md"))?);
    writeln!(
        report,
        "# {name}\n\nBox allocations: {}. Cumulative frame bytes: {total_bytes}. Peak live frames: {peak_frames}. Peak live frame bytes: {peak_bytes}. Maximum nested drivers: {max_drivers}. First-poll completed: {completed_immediately}. First-poll pending: {suspended}. Other: {}.\n\nEvery allocated frame was dropped. These counts exclude task-vector buffers, allocator rounding, observer storage, and all other checker allocations.",
        frames.len(),
        frames.len() - completed_immediately - suspended
    )?;
    trace.flush()?;
    output.flush()?;
    report.flush()?;
    Ok(())
}
