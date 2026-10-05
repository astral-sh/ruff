//! Passive, test-only accounting for the installed attempt's existing admission policy.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Write;
use std::panic::Location;
use std::rc::Rc;

use ruff_db::system::{OsSystem, SystemPath, WritableSystem};

use super::Incomplete;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Channel {
    Continuation,
    Weighted,
    Checkpoint,
}

thread_local! {
    static LEDGER: RefCell<Option<Ledger>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::types) struct Work {
    pub kind: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::types) enum Event {
    Charge {
        channel: Channel,
        site: &'static Location<'static>,
        work: Option<Rc<Work>>,
        units: usize,
        outcome: Result<(), salsa::attempt_probe::Incomplete>,
    },
    Refusal {
        site: &'static Location<'static>,
        reason: Incomplete,
        work: Option<Rc<Work>>,
    },
    Overflow {
        site: &'static Location<'static>,
        width: usize,
        multiplier: usize,
    },
}

struct Ledger {
    detailed: bool,
    work: Option<Rc<Work>>,
    events: Vec<Event>,
}

struct Installed;

impl Drop for Installed {
    fn drop(&mut self) {
        LEDGER.with(|ledger| ledger.borrow_mut().take());
    }
}

pub(in crate::types) fn capture<R>(detailed: bool, body: impl FnOnce() -> R) -> (R, Vec<Event>) {
    LEDGER.with(|ledger| {
        let mut ledger = ledger.borrow_mut();
        assert!(ledger.is_none());
        *ledger = Some(Ledger {
            detailed,
            work: None,
            events: Vec::new(),
        });
    });
    let installed = Installed;
    let result = body();
    let events = LEDGER.with(|ledger| {
        ledger
            .borrow_mut()
            .as_mut()
            .map(|ledger| std::mem::take(&mut ledger.events))
            .unwrap_or_default()
    });
    drop(installed);
    (result, events)
}

/// Attach a work enum only while its synchronous checkpoint runs. This scope must not
/// survive an await or a poll: another task could otherwise inherit the suspended label.
pub(in crate::types) fn scope<T: Debug>(work: &T) -> Scope {
    LEDGER.with(|ledger| {
        let mut ledger = ledger.borrow_mut();
        let Some(ledger) = ledger.as_mut().filter(|ledger| ledger.detailed) else {
            return Scope::Inactive;
        };
        let previous = ledger.work.replace(Rc::new(Work {
            kind: std::any::type_name::<T>(),
            value: format!("{work:?}"),
        }));
        Scope::Active { previous }
    })
}

pub(in crate::types) enum Scope {
    Inactive,
    Active { previous: Option<Rc<Work>> },
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Self::Active { previous } = self {
            LEDGER.with(|ledger| {
                if let Some(ledger) = ledger.borrow_mut().as_mut() {
                    ledger.work = previous.take();
                }
            });
        }
    }
}

#[track_caller]
pub(in crate::types) fn record(
    channel: Channel,
    units: usize,
    outcome: Result<(), salsa::attempt_probe::Incomplete>,
) {
    let site = Location::caller();
    LEDGER.with(|ledger| {
        if let Some(ledger) = ledger.borrow_mut().as_mut() {
            super::descriptor_observation::event(
                "charge",
                (channel, site, &ledger.work, units, outcome),
            );
            ledger.events.push(Event::Charge {
                channel,
                site,
                work: ledger.work.clone(),
                units,
                outcome,
            });
        }
    });
}

#[track_caller]
pub(in crate::types) fn refusal(reason: Incomplete) {
    let site = Location::caller();
    LEDGER.with(|ledger| {
        if let Some(ledger) = ledger.borrow_mut().as_mut() {
            super::descriptor_observation::event("refusal", (site, reason, &ledger.work));
            ledger.events.push(Event::Refusal {
                site,
                reason,
                work: ledger.work.clone(),
            });
        }
    });
}

#[track_caller]
pub(in crate::types) fn overflow(width: usize, multiplier: usize) {
    let site = Location::caller();
    LEDGER.with(|ledger| {
        if let Some(ledger) = ledger.borrow_mut().as_mut() {
            ledger.events.push(Event::Overflow {
                site,
                width,
                multiplier,
            });
        }
    });
}

fn family(site: &Location<'_>, work: Option<&Work>) -> &'static str {
    if let Some(work) = work {
        if work.value == "\"MappingFrame\"" {
            return "MappingFrame";
        }
        if work.kind.ends_with("TypeTransformationWork") {
            return "Transformer";
        }
        if work.kind.ends_with("SearchWork") {
            return "Search";
        }
    }
    let file = site.file();
    if file.ends_with("types/constructor/expansion_probe.rs") {
        "Constructor"
    } else if file.ends_with("types/mro/attempt.rs") {
        "Mro"
    } else if file.ends_with("types/mapping/attempt.rs") {
        "Mapping"
    } else if file.ends_with("types/instance/attempt.rs") {
        "Instance"
    } else if file.ends_with("types/mapping/self_binding/attempt.rs") {
        "SelfOwnership"
    } else if file.ends_with("types/class/instance_flags.rs") {
        "InstanceFlags"
    } else {
        "Unclassified"
    }
}

/// Validate the ledger using the unchanged atomic debit policy and retain the entire sequence.
pub(super) fn write_trace(
    name: &str,
    allowance: usize,
    events: &[Event],
    incomplete: Option<Incomplete>,
) -> anyhow::Result<()> {
    let directory = "/tmp/ty-forwarding-charge-ledger";
    OsSystem::new("/tmp").create_directory_all(SystemPath::new(directory))?;
    let mut trace =
        std::io::BufWriter::new(std::fs::File::create(format!("{directory}/{name}.tsv"))?);
    writeln!(
        trace,
        "sequence\tchannel\tfamily\tsite\tunits\toutcome\tkind\twork"
    )?;
    let mut accepted = 0usize;
    let mut first_refusal = None;
    let mut rows: BTreeMap<_, (usize, usize, usize)> = BTreeMap::new();
    let mut zero_continuations = 0;
    let mut zero_work = 0;
    let mut frames = 0;
    for (sequence, event) in events.iter().enumerate() {
        match event {
            Event::Charge {
                channel,
                site,
                work,
                units,
                outcome,
            } => {
                let family = family(site, work.as_deref());
                let kind = work.as_ref().map_or("", |work| work.kind);
                let value = work.as_ref().map_or("", |work| work.value.as_str());
                writeln!(
                    trace,
                    "{sequence}\t{channel:?}\t{family}\t{site}\t{units}\t{outcome:?}\t{kind}\t{value}"
                )?;
                if outcome.is_ok() {
                    anyhow::ensure!(first_refusal.is_none(), "accepted work after refusal");
                    accepted = accepted
                        .checked_add(*units)
                        .ok_or_else(|| anyhow::anyhow!("accepted sum overflow"))?;
                    anyhow::ensure!(accepted <= allowance);
                    if *units == 0 {
                        if *channel == Channel::Continuation {
                            zero_continuations += 1;
                        } else {
                            zero_work += 1;
                        }
                        continue;
                    }
                    anyhow::ensure!(
                        family != "Unclassified",
                        "unclassified positive site: {site}"
                    );
                    if family == "MappingFrame" {
                        // Captured before the observer was installed, using the same debug profile.
                        anyhow::ensure!(*units == 1824, "charged future layout changed: {units}");
                        frames += 1;
                    }
                    let variant = value.split(" {").next().unwrap_or(value);
                    let row = rows
                        .entry((family, format!("{site}"), kind, variant))
                        .or_default();
                    row.0 += 1;
                    row.1 += units;
                    row.2 = row.2.max(*units);
                } else if first_refusal.is_none() {
                    anyhow::ensure!(
                        *outcome == Err(salsa::attempt_probe::Incomplete::Allowance),
                        "unexpected first refusal: {event:?}"
                    );
                    anyhow::ensure!(
                        *units > allowance - accepted,
                        "first refusal fits remaining allowance"
                    );
                    first_refusal = Some(format!(
                        "{family} at {site}: {units} requested, {} remaining, work={value}",
                        allowance - accepted
                    ));
                }
            }
            Event::Refusal { .. } | Event::Overflow { .. } => {
                writeln!(trace, "{sequence}\t{event:?}")?;
                anyhow::bail!("unexpected non-debit refusal in forwarding fixture: {event:?}");
            }
        }
    }
    anyhow::ensure!(first_refusal.is_some() == incomplete.is_some());
    let mut report =
        std::io::BufWriter::new(std::fs::File::create(format!("{directory}/{name}.md"))?);
    writeln!(
        report,
        "# {name}\n\nAllowance: {allowance}. Accepted units: {accepted}. Outcome: {incomplete:?}. Accepted frames: {frames}. Zero continuation calls: {zero_continuations}. Zero work charges: {zero_work}.\n\nFirst refusal: {first_refusal:?}.\n\n| Family | Site | Work | Accepted calls | Accepted units | Largest debit |\n| --- | --- | --- | ---: | ---: | ---: |"
    )?;
    for ((family, site, kind, variant), (calls, units, largest)) in rows {
        let kind = kind.rsplit("::").next().unwrap_or(kind);
        writeln!(
            report,
            "| {family} | {site} | {kind}::{variant} | {calls} | {units} | {largest} |"
        )?;
    }
    trace.flush()?;
    report.flush()?;
    Ok(())
}

pub(super) fn without_work(events: &[Event]) -> Vec<Event> {
    events
        .iter()
        .cloned()
        .map(|mut event| {
            match &mut event {
                Event::Charge { work, .. } | Event::Refusal { work, .. } => *work = None,
                Event::Overflow { .. } => {}
            }
            event
        })
        .collect()
}
