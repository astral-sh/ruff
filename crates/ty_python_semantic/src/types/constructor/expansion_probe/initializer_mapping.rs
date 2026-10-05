//! Source controls for eager substitution of an initializer's owned `Self`.

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, system_path_to_file};
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::{AsId, FromId};
use salsa::prepared_source_probe::Stamp;

use super::{Incomplete, Observation, Statistics, run_mro_observed};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::call::bindings::InstanceBindingsWork;
use crate::types::{BoundTypeVarInstance, Specialization, StaticClassLiteral, Type};

const ALLOWANCE: usize = 100_000;

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
enum Arguments {
    Valid,
    Missing,
    Wrong,
}

impl Arguments {
    fn diagnostic(self) -> Option<&'static str> {
        match self {
            Self::Valid => None,
            Self::Missing => Some("missing-argument"),
            Self::Wrong => Some("invalid-argument-type"),
        }
    }
}

fn source(target: &str, entry: Entry, arguments: Arguments) -> String {
    let arguments = match arguments {
        Arguments::Valid => "1",
        Arguments::Missing => "",
        Arguments::Wrong => "'wrong'",
    };
    let demand = match entry {
        Entry::Direct => format!("result = {target}({arguments})"),
        Entry::Callable => {
            format!("factory: Callable[[int], {target}] = {target}\nresult = factory({arguments})")
        }
    };
    format!(
        "from typing import Callable, Self\n\nclass Owner:\n    __init__: Self\n    def __call__(self, value: int) -> None: ...\n\nclass Receiver(Owner): ...\n\n{demand}\n"
    )
}

fn database(source: &str) -> anyhow::Result<(TestDb, File)> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/initializer.py", source)
        .build()?;
    let file = system_path_to_file(&db, "/src/initializer.py")?;
    db.clear_salsa_events();
    Ok((db, file))
}

fn observations(statistics: &Statistics) -> &[Observation] {
    statistics.observations.as_deref().unwrap_or_default()
}

fn executions(db: &mut TestDb) -> Vec<String> {
    db.take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
struct DiagnosticIdentity {
    rule: String,
    message: String,
    range: Option<(u32, u32)>,
}

fn diagnostics(diagnostics: &[Diagnostic]) -> Vec<DiagnosticIdentity> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticIdentity {
            rule: diagnostic.id().as_str().to_owned(),
            message: diagnostic.headline_message().to_owned(),
            range: diagnostic
                .primary_span()
                .and_then(|span| span.range())
                .map(|range| (range.start().into(), range.end().into())),
        })
        .collect()
}

fn result_class(db: &TestDb, file: File) -> anyhow::Result<StaticClassLiteral<'_>> {
    let result = global_symbol(db, db.program_file(file), "result")
        .place
        .expect_type();
    let env = db.program_environment();
    result
        .as_nominal_instance()
        .and_then(|instance| instance.class(db, &env).static_class_literal(db))
        .map(|(class, _)| class)
        .ok_or_else(|| anyhow::anyhow!("expected a nominal result, got {result:?}"))
}

fn assert_released(db: &TestDb, stamp: Stamp, statistics: &Statistics) {
    super::search_observation::assert_released();
    assert_eq!(statistics.active_drivers, 0, "{statistics:?}");
    assert!(!super::active());
    assert_eq!(Stamp::current(db), stamp);
    let mut scopes = Vec::new();
    for event in observations(statistics) {
        match event {
            Observation::SearchStarted { scope, parent, .. } => {
                assert_eq!(*parent, scopes.last().copied());
                scopes.push(*scope);
            }
            Observation::SearchFinished(search) => assert_eq!(scopes.pop(), Some(search.scope)),
            Observation::InitializerMappingFinished {
                frames,
                dropped_frames,
                ..
            } => assert_eq!(frames, dropped_frames),
            _ => {}
        }
    }
    assert!(scopes.is_empty());
}

#[derive(Clone, Copy, Debug)]
enum MappingEpisode {
    Started(usize),
    Finished(usize),
    Resumed(usize),
}

fn assert_mapped(
    db: &TestDb,
    file: File,
    target: &str,
    entry: Entry,
    statistics: &Statistics,
) -> anyhow::Result<()> {
    let events = observations(statistics);
    let env = db.program_environment();
    let receiver = result_class(db, file)?;
    assert_eq!(receiver.name(db), target);
    let mut mapped = 0;
    let mut completed = 0;
    let mut completed_requested_entry = false;
    // Callable assignment and the later call to its narrowed factory are separate demands.
    // Each mapping must reach its own continuation before the next demand starts mapping.
    let mut episode = None;
    let mut instance_bindings_published = false;
    for (index, event) in events.iter().enumerate() {
        match *event {
            Observation::InitializerMappingStarted {
                variable,
                receiver_class,
            } => {
                assert!(
                    episode.is_none(),
                    "unfinished mapping episode {episode:?}: {events:#?}"
                );
                assert!(
                    matches!(
                        index.checked_sub(1).and_then(|index| events.get(index)),
                        Some(Observation::SearchFinished(_))
                    ),
                    "mapping starts after its initializer search has ended: {events:#?}"
                );
                let variable = BoundTypeVarInstance::from_id(
                    variable.ok_or_else(|| anyhow::anyhow!("initializer is not an owned Self"))?,
                );
                assert!(variable.typevar(db).is_self(db));
                let owner = variable
                    .typevar(db)
                    .upper_bound(db, &env)
                    .and_then(Type::as_nominal_instance)
                    .and_then(|instance| instance.class(db, &env).static_class_literal(db))
                    .map(|(class, _)| class)
                    .ok_or_else(|| anyhow::anyhow!("Self has no nominal owner"))?;
                assert_eq!(owner.name(db), "Owner");
                assert_eq!(receiver_class, Some(receiver.as_id()));
                mapped += 1;
                episode = Some(MappingEpisode::Started(index));
                instance_bindings_published = false;
            }
            Observation::InitializerMappingFinished {
                complete,
                matches_receiver,
                frames,
                dropped_frames,
            } => {
                assert!(complete && matches_receiver, "{event:?}");
                assert!(frames > 0);
                assert_eq!(frames, dropped_frames);
                let Some(MappingEpisode::Started(started)) = episode else {
                    anyhow::bail!("mapping finished without its start: {episode:?}\n{events:#?}");
                };
                assert!(started < index);
                episode = Some(MappingEpisode::Finished(index));
            }
            Observation::InitializerBindingResumed => {
                let Some(MappingEpisode::Finished(finished)) = episode else {
                    anyhow::bail!(
                        "initializer resumed without its completed mapping: {episode:?}\n{events:#?}"
                    );
                };
                assert!(finished < index);
                episode = Some(MappingEpisode::Resumed(index));
            }
            Observation::InstanceBindingsWork { work, accepted } => {
                assert!(accepted, "{event:?}");
                if work == InstanceBindingsWork::Publish {
                    instance_bindings_published = true;
                }
            }
            _ => {}
        }
        let is_direct_completion = Entry::Direct.completed_class(*event) == Some(receiver.as_id());
        let is_callable_completion =
            Entry::Callable.completed_class(*event) == Some(receiver.as_id());
        if (is_direct_completion || is_callable_completion)
            && let Some(pending) = episode
        {
            let preceding = match pending {
                MappingEpisode::Finished(finished) if is_direct_completion => {
                    assert!(instance_bindings_published, "{events:#?}");
                    finished
                }
                MappingEpisode::Resumed(resumed) if is_callable_completion => resumed,
                _ => anyhow::bail!(
                    "constructor completion does not match its mapping episode: {pending:?}, {event:?}\n{events:#?}"
                ),
            };
            assert!(preceding < index);
            completed += 1;
            completed_requested_entry |= entry.completed_class(*event) == Some(receiver.as_id());
            episode = None;
        }
    }
    assert!(mapped > 0, "{statistics:#?}");
    assert!(
        episode.is_none(),
        "unfinished mapping episode {episode:?}: {events:#?}"
    );
    assert_eq!(completed, mapped, "{statistics:#?}");
    assert_eq!(
        mapped,
        events
            .iter()
            .filter(|event| matches!(event, Observation::InitializerMappingFinished { .. }))
            .count()
    );
    assert!(completed_requested_entry, "{entry:?}: {statistics:#?}");
    Ok(())
}

fn normalization_requests(db: &TestDb, statistics: &Statistics) -> Vec<String> {
    let env = db.program_environment();
    statistics
        .observations()
        .iter()
        .filter_map(|event| {
            let Observation::TupleNormalization(id) = *event else {
                return None;
            };
            let specialization = Specialization::from_id(id);
            let variables = specialization
                .generic_context(db)
                .variables(db)
                .map(|variable| Type::TypeVar(variable).display(db, &env).to_string())
                .collect::<Vec<_>>();
            let types = specialization
                .types(db)
                .iter()
                .map(|ty| ty.display(db, &env).to_string())
                .collect::<Vec<_>>();
            Some(format!(
                "variables={variables:?}, arguments={types:?}, has_tuple={}, materialization={:?}",
                specialization.tuple(db).is_some(),
                specialization.materialization_kind(db)
            ))
        })
        .collect()
}

#[test]
fn eager_source_initializer_matches_ordinary_arguments_result_and_read_order() -> anyhow::Result<()>
{
    let mut failures = Vec::new();
    for target in ["Owner", "Receiver"] {
        for entry in [Entry::Direct, Entry::Callable] {
            for arguments in [Arguments::Valid, Arguments::Missing, Arguments::Wrong] {
                let result = (|| -> anyhow::Result<()> {
                    let source = source(target, entry, arguments);
                    let (mut ordinary_db, ordinary_file) = database(&source)?;
                    let ordinary = ordinary_db.check_file(ordinary_file);
                    let ordinary_reads = executions(&mut ordinary_db);
                    let expected = diagnostics(&ordinary);
                    assert_eq!(
                        expected
                            .iter()
                            .map(|diagnostic| diagnostic.rule.as_str())
                            .collect::<Vec<_>>(),
                        arguments.diagnostic().into_iter().collect::<Vec<_>>(),
                        "{source}\n{ordinary:#?}"
                    );
                    assert_eq!(
                        result_class(&ordinary_db, ordinary_file)?.name(&ordinary_db),
                        target
                    );

                    // No semantic lookup or instance construction precedes the installed root.
                    let (mut db, file) = database(&source)?;
                    let stamp = Stamp::current(&db);
                    let (outcome, statistics) =
                        run_mro_observed(&db, ALLOWANCE, || db.check_file(file));
                    assert_released(&db, stamp, &statistics);
                    let actual = outcome.map_err(|error| {
                        anyhow::anyhow!(
                            "{target}, {entry:?}, {arguments:?}: {error:?}\n{statistics:#?}"
                        )
                    })?;
                    let actual_reads = executions(&mut db);
                    assert_eq!(diagnostics(&actual), expected, "{source}");
                    assert_eq!(actual_reads, ordinary_reads, "{source}");
                    assert_mapped(&db, file, target, entry, &statistics)?;
                    executions(&mut db);
                    executions(&mut ordinary_db);
                    for _ in 0..2 {
                        let expected = diagnostics(&ordinary_db.check_file(ordinary_file));
                        let expected_reads = executions(&mut ordinary_db);
                        let (outcome, statistics) =
                            run_mro_observed(&db, ALLOWANCE, || db.check_file(file));
                        assert_released(&db, stamp, &statistics);
                        let actual = outcome.map_err(|error| anyhow::anyhow!("{error:?}"))?;
                        assert_eq!(diagnostics(&actual), expected);
                        assert_eq!(executions(&mut db), expected_reads);
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    failures.push(format!("{target} {entry:?} {arguments:?}: {error:#}"));
                }
            }
        }
    }
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

fn minimum_allowance(source: &str, reached: impl Fn(&Statistics) -> bool) -> anyhow::Result<usize> {
    let (db, file) = database(source)?;
    let (outcome, statistics) = run_mro_observed(&db, ALLOWANCE, || db.check_file(file));
    outcome.map_err(|error| {
        anyhow::anyhow!("calibration did not complete: {error:?}\n{statistics:#?}")
    })?;
    assert!(reached(&statistics), "{statistics:#?}");
    let mut low = 0;
    let mut high = ALLOWANCE;
    while low < high {
        let middle = low + (high - low) / 2;
        let (db, file) = database(source)?;
        let stamp = Stamp::current(&db);
        let (outcome, statistics) = run_mro_observed(&db, middle, || db.check_file(file));
        assert_released(&db, stamp, &statistics);
        assert!(
            outcome.is_ok() || matches!(outcome, Err(Incomplete::Allowance)),
            "{outcome:#?}"
        );
        if reached(&statistics) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(low)
}

#[test]
fn eager_initializer_refuses_before_and_inside_mapping_then_retries_without_edit()
-> anyhow::Result<()> {
    for entry in [Entry::Direct, Entry::Callable] {
        let source = source("Receiver", entry, Arguments::Valid);
        let started = minimum_allowance(&source, |statistics| {
            observations(statistics)
                .iter()
                .any(|event| matches!(event, Observation::InitializerMappingStarted { .. }))
        })?;
        let frame_created = minimum_allowance(&source, |statistics| {
            observations(statistics).iter().any(|event| matches!(event, Observation::InitializerMappingFinished { frames, .. } if *frames > 0))
        })?;
        assert!(started > 0);
        for (allowance, inside_mapping) in [(started - 1, false), (frame_created, true)] {
            let (db, file) = database(&source)?;
            let stamp = Stamp::current(&db);
            let (outcome, statistics) = run_mro_observed(&db, allowance, || db.check_file(file));
            assert!(
                matches!(outcome, Err(Incomplete::Allowance)),
                "{outcome:#?}\n{statistics:#?}"
            );
            assert_released(&db, stamp, &statistics);
            let events = observations(&statistics);
            assert_eq!(
                events
                    .iter()
                    .any(|event| matches!(event, Observation::InitializerMappingStarted { .. })),
                inside_mapping,
                "{statistics:#?}"
            );
            if inside_mapping {
                assert!(events.iter().any(|event| matches!(event, Observation::InitializerMappingFinished { complete: false, frames, .. } if *frames > 0)), "{statistics:#?}");
            }
            assert!(
                !events.iter().any(|event| matches!(
                    event,
                    Observation::InitializerMappingFinished { complete: true, .. }
                        | Observation::InitializerBindingResumed
                )),
                "{statistics:#?}"
            );
            assert!(
                !events.iter().any(|event| {
                    let class = match event {
                        Observation::ConstructorCompleted { class }
                        | Observation::ConstructorCallableCompleted { class } => *class,
                        _ => return false,
                    };
                    StaticClassLiteral::from_id(class).name(&db) == "Receiver"
                }),
                "{statistics:#?}"
            );

            for retry in 0..2 {
                let (outcome, statistics) =
                    run_mro_observed(&db, ALLOWANCE, || db.check_file(file));
                assert_released(&db, stamp, &statistics);
                let actual = outcome.map_err(|error| {
                    anyhow::anyhow!("{entry:?}, retry {retry}: {error:?}\n{statistics:#?}")
                })?;
                assert!(actual.is_empty(), "{actual:#?}");
                assert_eq!(result_class(&db, file)?.name(&db), "Receiver");
                if retry == 0 {
                    assert_mapped(&db, file, "Receiver", entry, &statistics)?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn cold_generic_source_initializer_retains_controlled_continuations() -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for entry in [Entry::Direct, Entry::Callable] {
        let result = (|| -> anyhow::Result<()> {
            let demand = match entry {
                Entry::Direct => "result = Receiver(1)",
                Entry::Callable => {
                    "factory: Callable[[int], Receiver] = Receiver\nresult = factory(1)"
                }
            };
            let source = format!(
                "from typing import Callable, Self\n\nclass Owner[T]:\n    __init__: Self\n    def __call__(self, value: int) -> None: ...\n\nclass Receiver(Owner[int]): ...\n\n{demand}\n"
            );
            let (mut ordinary_db, ordinary_file) = database(&source)?;
            let expected = diagnostics(&ordinary_db.check_file(ordinary_file));
            assert!(expected.is_empty(), "{source}\n{expected:?}");
            let expected_reads = executions(&mut ordinary_db);
            let (mut db, file) = database(&source)?;
            let stamp = Stamp::current(&db);
            let (outcome, statistics) = run_mro_observed(&db, ALLOWANCE, || db.check_file(file));
            assert_released(&db, stamp, &statistics);
            let actual = outcome.map_err(|error| {
                anyhow::anyhow!(
                    "{entry:?}: {error:?}\nnormalization={:?}\n{statistics:#?}",
                    normalization_requests(&db, &statistics)
                )
            })?;
            let actual_reads = executions(&mut db);
            assert!(
                actual_reads
                    .iter()
                    .any(|query| query.contains("pep695_generic_context_inner")),
                "{actual_reads:?}"
            );
            assert_eq!(actual_reads, expected_reads, "{entry:?}");
            assert_eq!(diagnostics(&actual), expected, "{entry:?}");
            assert_mapped(&db, file, "Receiver", entry, &statistics)?;
            Ok(())
        })();
        if let Err(error) = result {
            failures.push(format!("{entry:?}: {error:#}"));
        }
    }
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
