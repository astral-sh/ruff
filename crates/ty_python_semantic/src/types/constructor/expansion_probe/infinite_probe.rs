//! Scratch controls for incomplete constructor expansion and finite alternatives.

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashSet;
use salsa::Database;
use salsa::plumbing::FromId;
use salsa::prepared_source_probe::Stamp;

use super::super::effects::{ConstructorError, checked_source};
use super::{Incomplete, Observation, Statistics, active, observe, run_inner, stopped};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::StaticClassLiteral;
use crate::types::visitor::SearchWork;

const ALLOWANCES: [usize; 3] = [128, 256, 512];
const FINITE_ALLOWANCE: usize = 4_096;

#[derive(Clone, Copy, Debug)]
enum Syntax {
    Pep695,
    Legacy,
}

#[derive(Clone, Copy, Debug)]
enum Entry {
    Direct,
    Callable,
}

impl Entry {
    fn completed_class(self, event: Observation) -> Option<salsa::Id> {
        match (self, event) {
            (Self::Direct, Observation::ConstructorCompleted { class })
            | (Self::Callable, Observation::ConstructorCallableCompleted { class }) => Some(class),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    New,
    Init,
    FiniteFirst,
    GrowingFirst,
}

impl Scenario {
    fn has_alternatives(self) -> bool {
        matches!(self, Self::FiniteFirst | Self::GrowingFirst)
    }
}

#[derive(Clone, Copy, Debug)]
enum Arguments {
    Valid,
    Missing,
    Wrong,
}

fn source(
    syntax: Syntax,
    entry: Entry,
    scenario: Scenario,
    arguments: Arguments,
    finite: bool,
) -> String {
    let (imports, declaration) = match syntax {
        Syntax::Pep695 => ("from typing import Callable\n", "class Chain[T]:"),
        Syntax::Legacy => (
            "from typing import Callable, Generic, TypeVar\n\nT = TypeVar(\"T\")\n",
            "class Chain(Generic[T]):",
        ),
    };
    let member = if matches!(scenario, Scenario::New) {
        "__new__"
    } else {
        "__init__"
    };
    // A class-valued __new__ receives the outer constructor's implicit cls argument.
    let finite_parameters = if matches!(scenario, Scenario::New) {
        "self, constructor: object, value: int"
    } else {
        "self, value: int"
    };
    let next = if finite { "Finite" } else { "Chain[list[T]]" };
    let mut source = format!(
        "from __future__ import annotations\n{imports}\nclass Finite:\n    def __init__({finite_parameters}) -> None: ...\n\n{declaration}\n    {member}: type[{next}]\n"
    );
    let target = match scenario {
        Scenario::New | Scenario::Init => "Chain[int]",
        Scenario::FiniteFirst | Scenario::GrowingFirst => {
            let alternatives = if matches!(scenario, Scenario::FiniteFirst) {
                "type[Finite] | type[Chain[int]]"
            } else {
                "type[Chain[int]] | type[Finite]"
            };
            source.push_str(&format!("\nclass Root:\n    __init__: {alternatives}\n"));
            "Root"
        }
    };
    let demand = match entry {
        Entry::Direct => {
            let (argument, diagnostic) = match arguments {
                Arguments::Valid => ("1", ""),
                Arguments::Missing => ("", "  # error: [missing-argument]"),
                Arguments::Wrong => ("\"wrong\"", "  # error: [invalid-argument-type]"),
            };
            let diagnostic = if scenario.has_alternatives() {
                diagnostic.repeat(2)
            } else {
                diagnostic.to_owned()
            };
            format!("    {target}({argument}){diagnostic}\n")
        }
        Entry::Callable => {
            let result = if matches!(scenario, Scenario::New) {
                if finite { "Finite" } else { "object" }
            } else {
                target
            };
            let (parameters, diagnostic) = match arguments {
                Arguments::Valid => ("int", ""),
                Arguments::Missing => ("", "  # error: [invalid-assignment]"),
                Arguments::Wrong => ("str", "  # error: [invalid-assignment]"),
            };
            format!("    callback: Callable[[{parameters}], {result}] = {target}{diagnostic}\n")
        }
    };
    source.push_str("\ndef demand() -> None:\n");
    source.push_str(&demand);
    source
}

fn database(source: &str) -> anyhow::Result<(TestDb, File)> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/constructor.py", source)?;
    let file = system_path_to_file(&db, "/src/constructor.py")?;
    Ok((db, file))
}

fn observations(statistics: &Statistics) -> &[Observation] {
    statistics.observations.as_deref().unwrap_or_default()
}

fn accepted_debits(events: &[Observation]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Observation::Debit { accepted: true }))
        .count()
}

fn assert_released(db: &TestDb, stamp: Stamp, statistics: &Statistics) {
    super::search_observation::assert_released();
    let mut scopes = Vec::new();
    for event in observations(statistics) {
        match event {
            Observation::SearchStarted { scope, parent, .. } => {
                assert_eq!(*parent, scopes.last().copied());
                scopes.push(*scope);
            }
            Observation::SearchFinished(search) => assert_eq!(scopes.pop(), Some(search.scope)),
            _ => {}
        }
    }
    assert!(scopes.is_empty());
    assert_eq!(statistics.active_drivers, 0, "{statistics:?}");
    assert!(!active());
    assert_eq!(Stamp::current(db), stamp);
    assert_eq!(
        salsa::attempt_probe::try_with_attempt(db, 0, || ()),
        Ok(salsa::attempt_probe::AttemptOutcome::Complete(()))
    );
    assert_eq!(Stamp::current(db), stamp);
}

#[test]
fn generated_initializer_searches_between_expansion_debits() -> anyhow::Result<()> {
    for syntax in [Syntax::Pep695, Syntax::Legacy] {
        for entry in [Entry::Direct, Entry::Callable] {
            let source = source(syntax, entry, Scenario::Init, Arguments::Valid, false);
            let mut previous = (0, 0);
            for allowance in ALLOWANCES {
                let (db, file) = database(&source)?;
                let stamp = Stamp::current(&db);
                for attempt in 1..=2 {
                    let (outcome, statistics) = run_inner(&db, allowance, true, || {
                        let _ = db.check_file(file);
                        assert!(stopped(&db));
                        observe(Observation::RepeatedDemand);
                        let _ = db.check_file(file);
                    });
                    assert_eq!(outcome, Err(Incomplete::Allowance));
                    assert_released(&db, stamp, &statistics);
                    let events = observations(&statistics);
                    let refusal = events
                        .iter()
                        .position(|event| matches!(event, Observation::Refusal))
                        .ok_or_else(|| anyhow::anyhow!("missing refusal"))?;
                    assert_eq!(accepted_debits(&events[refusal..]), 0);
                    let mut total_visits = 0;
                    let mut peak_depth = 0;
                    let mut stack_span = 0;
                    let mut scopes = 0;
                    for event in events {
                        let Observation::SearchFinished(search) = event else {
                            continue;
                        };
                        assert!(search.debits_before <= search.debits_after);
                        assert_eq!(search.expansions_before, search.expansions_after);
                        total_visits += search.visits;
                        peak_depth = peak_depth.max(search.peak_depth);
                        stack_span = stack_span.max(search.stack_span);
                        scopes += 1;
                    }
                    assert!(scopes > 0);
                    assert!(statistics.search_work > 0);
                    assert!(statistics.search_peak_pending <= statistics.search_work);
                    assert!(total_visits <= statistics.search_predicates + 1);
                    assert!(statistics.search_predicates <= allowance);
                    if attempt == 1 {
                        assert!(total_visits > previous.0);
                        assert_eq!(peak_depth, 1);
                        previous = (total_visits, peak_depth);
                    }
                    eprintln!(
                        "SEARCH {syntax:?} {entry:?} allowance={allowance} attempt={attempt} scopes={scopes} visits={total_visits} peak_depth={peak_depth} stack_span={stack_span} expansion_debits={} all_debits={} search_work={} peak_pending={}",
                        statistics.expansions,
                        statistics.debits,
                        statistics.search_work,
                        statistics.search_peak_pending,
                    );
                }
            }
        }
    }
    Ok(())
}

fn assert_incomplete_attempts(
    source: &str,
    label: &str,
    allowance: usize,
    entry: Entry,
) -> anyhow::Result<bool> {
    let (db, file) = database(source)?;
    let stamp = Stamp::current(&db);
    let mut completed_finite_alternative = false;
    for attempt in 1..=2 {
        let (outcome, statistics) = run_inner(&db, allowance, true, || {
            let _ = db.check_file(file);
            assert!(
                stopped(&db),
                "{label}: initial demand completed unexpectedly"
            );
            observe(Observation::RepeatedDemand);
            let _ = db.check_file(file);
        });
        assert_eq!(
            outcome,
            Err(Incomplete::Allowance),
            "{label}; {statistics:?}"
        );
        assert_released(&db, stamp, &statistics);
        let events = observations(&statistics);
        let accepted = accepted_debits(events);
        assert!(accepted <= allowance, "{label}: {accepted} > {allowance}");
        assert!(statistics.expansions > 0, "{label}; {statistics:?}");
        match entry {
            Entry::Direct => {
                assert!(statistics.direct > 0, "{label}; {statistics:?}");
                assert!(statistics.task_polls > 0, "{label}; {statistics:?}");
                assert!(statistics.max_drivers > 0, "{label}; {statistics:?}");
            }
            Entry::Callable => assert!(statistics.conversions > 0, "{label}; {statistics:?}"),
        }
        let refusal = events
            .iter()
            .position(|event| matches!(event, Observation::Refusal))
            .ok_or_else(|| anyhow::anyhow!("{label}: no refusal observation"))?;
        assert_eq!(accepted_debits(&events[refusal..]), 0, "{label}");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Observation::Debit { accepted: false })),
            "{label}: no refused debit"
        );
        let repeated = events
            .iter()
            .position(|event| matches!(event, Observation::RepeatedDemand))
            .ok_or_else(|| anyhow::anyhow!("{label}: repeated demand was not observed"))?;
        assert!(refusal < repeated, "{label}");
        assert_eq!(accepted_debits(&events[repeated..]), 0, "{label}");

        // The finite class is requested only through the union in an infinite fixture.
        // Record completed bindings or signatures; a growing first branch may prevent this event.
        let finite_completed = events[..refusal].iter().any(|event| {
            let Some(class) = entry.completed_class(*event) else {
                return false;
            };
            let class = StaticClassLiteral::from_id(class);
            class.name(&db) == "Finite" && class.definition(&db).file(&db) == file
        });
        completed_finite_alternative |= finite_completed;
        eprintln!(
            "{label}: allowance={allowance} attempt={attempt} accepted={accepted} finite_completed={finite_completed} polls={} expansions={} max_drivers={} stack_span={}",
            statistics.task_polls,
            statistics.expansions,
            statistics.max_drivers,
            statistics.stack_span
        );
    }
    Ok(completed_finite_alternative)
}

fn assert_expected(source: &str, diagnostics: &[Diagnostic]) {
    let mut expected: Vec<_> = source
        .lines()
        .enumerate()
        .flat_map(|(line, text)| {
            text.match_indices("# error: [")
                .map(move |(offset, marker)| {
                    let comment = &text[offset + marker.len()..];
                    (line + 1, comment.split(']').next().unwrap_or(comment))
                })
        })
        .collect();
    let mut actual: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| {
            let start = diagnostic
                .primary_span()
                .and_then(|span| span.range())
                .map_or(0, |range| usize::from(range.start()));
            let line = source[..start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            (line, diagnostic.id().as_str())
        })
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected, "{source}\n{diagnostics:#?}");
}

#[test]
fn constructor_source_recovery_does_not_reach_continuations() -> anyhow::Result<()> {
    let (db, _) = database("class Product: ...\n")?;
    let stamp = Stamp::current(&db);
    let reason = Incomplete::Interrupted;
    let (outcome, statistics) = run_inner(&db, FINITE_ALLOWANCE, true, || {
        let result = checked_source(&db, || {
            assert_eq!(super::refuse(&db, reason), reason);
            Ok(true)
        });
        assert!(matches!(result, Err(ConstructorError::Incomplete(actual)) if actual == reason));

        let mut invoked = false;
        let result = checked_source(&db, || {
            invoked = true;
            Ok(true)
        });
        assert!(!invoked);
        assert!(matches!(result, Err(ConstructorError::Incomplete(actual)) if actual == reason));
    });
    assert_eq!(outcome, Err(reason));
    assert_released(&db, stamp, &statistics);
    Ok(())
}

#[test]
fn initializer_search_refusal_retries_without_caching_partial_bindings() -> anyhow::Result<()> {
    for syntax in [Syntax::Pep695, Syntax::Legacy] {
        for entry in [Entry::Direct, Entry::Callable] {
            let source = source(syntax, entry, Scenario::Init, Arguments::Wrong, true);
            let (control_db, control_file) = database(&source)?;
            let (control, control_statistics) =
                run_inner(&control_db, FINITE_ALLOWANCE, true, || {
                    control_db.check_file(control_file)
                });
            assert_expected(
                &source,
                &control.map_err(|reason| anyhow::anyhow!("control: {reason:?}"))?,
            );

            // Refuse the second predicate of Chain's initializer search. The first predicate
            // has already visited its root, so this exercises abandonment of pending work.
            let mut owners = Vec::new();
            let mut predicates = 0;
            let mut allowance = None;
            let control_events = observations(&control_statistics);
            for (index, event) in control_events.iter().enumerate() {
                match event {
                    Observation::SearchStarted { owner, .. } => owners.push(*owner),
                    Observation::SearchFinished(_) => {
                        owners.pop();
                    }
                    Observation::SearchWork {
                        work: SearchWork::Predicate,
                        accepted: true,
                    } if owners.last().copied().flatten().is_some_and(|owner| {
                        StaticClassLiteral::from_id(owner).name(&control_db) == "Chain"
                    }) =>
                    {
                        predicates += 1;
                        if predicates == 2 {
                            allowance = accepted_debits(&control_events[..index]).checked_sub(1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let allowance = allowance.ok_or_else(|| {
                anyhow::anyhow!("{syntax:?} {entry:?}: no second initializer predicate")
            })?;

            let (mut db, file) = database(&source)?;
            let stamp = Stamp::current(&db);
            db.clear_salsa_events();
            let (limited, statistics) = run_inner(&db, allowance, true, || {
                let _ = db.check_file(file);
                assert!(stopped(&db));
                observe(Observation::RepeatedDemand);
                let _ = db.check_file(file);
            });
            assert_eq!(limited, Err(Incomplete::Allowance));
            assert_released(&db, stamp, &statistics);
            let events = observations(&statistics);
            let refusal = events
                .iter()
                .position(|event| matches!(event, Observation::Refusal))
                .ok_or_else(|| anyhow::anyhow!("no refusal"))?;
            let mut owners = Vec::new();
            let mut refused_predicate = false;
            for event in events {
                match event {
                    Observation::SearchStarted { owner, .. } => owners.push(*owner),
                    Observation::SearchFinished(_) => {
                        owners.pop();
                    }
                    Observation::SearchWork {
                        work: SearchWork::Predicate,
                        accepted: false,
                    } => {
                        assert!(owners.last().copied().flatten().is_some_and(|owner| {
                            StaticClassLiteral::from_id(owner).name(&db) == "Chain"
                        }));
                        refused_predicate = true;
                    }
                    _ => {}
                }
            }
            assert!(refused_predicate, "{syntax:?} {entry:?}: {events:#?}");
            assert_eq!(accepted_debits(events), allowance);
            assert_eq!(accepted_debits(&events[refusal..]), 0);
            for event in &events[refusal..] {
                assert!(
                    !matches!(
                        event,
                        Observation::SearchWork { accepted: true, .. }
                            | Observation::InitializerBindingResumed
                            | Observation::ConstructorCompleted { .. }
                            | Observation::ConstructorCallableCompleted { .. }
                    ),
                    "constructor work continued after refusal: {event:?}"
                );
            }

            let completed_source_keys: FxHashSet<_> = db
                .take_salsa_events()
                .into_iter()
                .filter_map(|event| {
                    let salsa::EventKind::WillExecute { database_key } = event.kind else {
                        return None;
                    };
                    matches!(
                        db.ingredient_debug_name(database_key.ingredient_index())
                            .as_ref(),
                        "source_text" | "parsed_module" | "semantic_index"
                    )
                    .then_some(database_key)
                })
                .collect();
            for name in ["source_text", "parsed_module", "semantic_index"] {
                assert!(
                    completed_source_keys
                        .iter()
                        .any(|key| { db.ingredient_debug_name(key.ingredient_index()) == name })
                );
            }
            let mut previous = None;
            for retry in 1..=2 {
                let (outcome, statistics) =
                    run_inner(&db, FINITE_ALLOWANCE, true, || db.check_file(file));
                assert_expected(
                    &source,
                    &outcome.map_err(|reason| {
                        anyhow::anyhow!("{syntax:?} {entry:?} retry {retry}: {reason:?}")
                    })?,
                );
                assert_released(&db, stamp, &statistics);
                for event in db.take_salsa_events() {
                    if let salsa::EventKind::WillExecute { database_key } = event.kind {
                        assert!(
                            !completed_source_keys.contains(&database_key),
                            "completed source child reexecuted: {database_key:?}"
                        );
                    }
                }
                if let Some((expansions, work)) = previous {
                    assert!(statistics.expansions < expansions);
                    assert!(statistics.search_work < work);
                } else {
                    assert!(statistics.search_work > 0);
                    assert!(statistics.expansions > 0);
                }
                previous = Some((statistics.expansions, statistics.search_work));
                eprintln!(
                    "search retry {syntax:?} {entry:?}: refused allowance={allowance}, retry={retry}, expansions={}, search_work={}",
                    statistics.expansions, statistics.search_work
                );
            }
        }
    }
    Ok(())
}

#[test]
fn growing_new_and_init_remain_incomplete_for_separate_entry_points() -> anyhow::Result<()> {
    for syntax in [Syntax::Pep695, Syntax::Legacy] {
        for scenario in [Scenario::New, Scenario::Init] {
            for entry in [Entry::Direct, Entry::Callable] {
                let source = source(syntax, entry, scenario, Arguments::Valid, false);
                let label = format!("{syntax:?} {scenario:?} {entry:?}");
                for allowance in ALLOWANCES {
                    assert_incomplete_attempts(&source, &label, allowance, entry)?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn finite_initializer_alternative_does_not_hide_growing_work() -> anyhow::Result<()> {
    for syntax in [Syntax::Pep695, Syntax::Legacy] {
        for entry in [Entry::Direct, Entry::Callable] {
            for arguments in [Arguments::Valid, Arguments::Wrong] {
                let mut completed_finite_alternative = false;
                for scenario in [Scenario::FiniteFirst, Scenario::GrowingFirst] {
                    let source = source(syntax, entry, scenario, arguments, false);
                    let label = format!("{syntax:?} {scenario:?} {entry:?} {arguments:?}");
                    for allowance in ALLOWANCES {
                        completed_finite_alternative |=
                            assert_incomplete_attempts(&source, &label, allowance, entry)?;
                    }
                }
                assert!(
                    completed_finite_alternative,
                    "{syntax:?} {entry:?} {arguments:?}: neither union order completed the finite constructor alternative before refusal"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn finite_counterparts_preserve_exact_integer_argument_requirements() -> anyhow::Result<()> {
    for syntax in [Syntax::Pep695, Syntax::Legacy] {
        for scenario in [
            Scenario::New,
            Scenario::Init,
            Scenario::FiniteFirst,
            Scenario::GrowingFirst,
        ] {
            for entry in [Entry::Direct, Entry::Callable] {
                for arguments in [Arguments::Valid, Arguments::Missing, Arguments::Wrong] {
                    let source = source(syntax, entry, scenario, arguments, true);
                    let label = format!("finite {syntax:?} {scenario:?} {entry:?} {arguments:?}");
                    let (db, file) = database(&source)?;
                    let stamp = Stamp::current(&db);
                    let (outcome, statistics) =
                        run_inner(&db, FINITE_ALLOWANCE, true, || db.check_file(file));
                    let diagnostics = outcome
                        .map_err(|reason| anyhow::anyhow!("{label}: {reason:?}; {statistics:?}"))?;
                    assert_expected(&source, &diagnostics);
                    assert_released(&db, stamp, &statistics);
                    assert!(statistics.expansions > 0, "{label}; {statistics:?}");
                    assert!(
                        !observations(&statistics)
                            .iter()
                            .any(|event| matches!(event, Observation::Refusal)),
                        "{label}; {statistics:?}"
                    );
                    if scenario.has_alternatives() {
                        assert!(
                            observations(&statistics).iter().any(|event| {
                                let Some(class) = entry.completed_class(*event) else {
                                    return false;
                                };
                                let class = StaticClassLiteral::from_id(class);
                                class.name(&db) == "Finite"
                                    && class.definition(&db).file(&db) == file
                            }),
                            "{label}: finite constructor alternative did not complete"
                        );
                    }
                    eprintln!(
                        "{label}: polls={} expansions={} max_drivers={} stack_span={}",
                        statistics.task_polls,
                        statistics.expansions,
                        statistics.max_drivers,
                        statistics.stack_span
                    );
                }
            }
        }
    }
    Ok(())
}
