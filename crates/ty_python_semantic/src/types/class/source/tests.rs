use std::cell::Cell;

use bitflags::bitflags;
use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::{AsId, FromId};
use salsa::prepared_source_probe::Stamp;

use super::{SourceClassEffects, default_class_specialization_with};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete, Observation, Statistics};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, GenericAlias, KnownClass, StaticClassLiteral,
    Type,
};

const ALLOWANCE: usize = 100_000;
const SOURCES: [&str; 2] = [
    r#"
from typing import Annotated, Self

class Callback:
    __init__: Self
    def __call__(self, value: int) -> None: ...

class Owner[T = Annotated[int, Callback(1)]]: ...
"#,
    r#"
from typing import Annotated, Generic, Self, TypeVar

class Callback:
    __init__: Self
    def __call__(self, value: int) -> None: ...

T = TypeVar("T", default=Annotated[int, Callback(1)])
class Owner(Generic[T]): ...
"#,
];

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/default_callback.pyi", source)
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = system_path_to_file(db, "/src/default_callback.pyi")?;
    global_symbol(db, db.program_file(file), name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
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

struct Fixture<'db> {
    owner: StaticClassLiteral<'db>,
    callback: StaticClassLiteral<'db>,
    variable: BoundTypeVarInstance<'db>,
}

impl<'db> Fixture<'db> {
    fn new(db: &'db TestDb) -> anyhow::Result<Self> {
        let owner = class(db, "Owner")?;
        let callback = class(db, "Callback")?;
        let variable = owner
            .generic_context(db)
            .and_then(|context| context.variables(db).next())
            .ok_or_else(|| anyhow::anyhow!("missing owner type variable"))?;
        executions(db);
        Ok(Self {
            owner,
            callback,
            variable,
        })
    }

    fn read(&self, db: &'db TestDb) -> Result<ClassType<'db>, Incomplete> {
        default_class_specialization_with(db, self.owner, &SourceClassEffects::new(db))
    }

    fn assert_result(&self, db: &'db TestDb, actual: ClassType<'db>) -> anyhow::Result<()> {
        let context = self
            .owner
            .generic_context(db)
            .ok_or_else(|| anyhow::anyhow!("missing owner context"))?;
        let expected = context.specialize(
            db,
            vec![KnownClass::Int.to_instance(db, &db.program_environment())],
        );
        assert_eq!(
            actual,
            ClassType::Generic(GenericAlias::new(db, self.owner, expected))
        );
        Ok(())
    }
}

fn complete<T>(outcome: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    outcome
        .and_then(|value| value)
        .map_err(|reason| anyhow::anyhow!("{reason:?}"))
}

bitflags! {
    #[derive(Default, Debug)]
    struct CallbackEvents: u8 {
        const BOUND_DEFAULT = 1;
        const LAZY_DEFAULT = 1 << 1;
        const CONSTRUCTOR = 1 << 2;
        const ENTRY_REFUSED = 1 << 3;
        const MAPPING_STARTED = 1 << 4;
        const MAPPING_COMPLETE = 1 << 5;
        const CONSTRUCTOR_COMPLETE = 1 << 6;
    }
}

#[derive(Default, Debug)]
struct CallbackEpisode {
    events: CallbackEvents,
    mapping_frames: usize,
}

fn episode(db: &TestDb, fixture: &Fixture<'_>, statistics: &Statistics) -> CallbackEpisode {
    let events = statistics.observations();
    let mut scopes = Vec::new();
    let mut mapping = false;
    let mut episode = CallbackEpisode::default();
    for (index, event) in events.iter().enumerate() {
        match *event {
            Observation::ClassDefaultReadEntered(variable) => scopes.push(variable),
            Observation::ClassDefaultReadExited(variable) => {
                assert_eq!(scopes.pop(), Some(variable));
            }
            Observation::Execute(key) if scopes.contains(&fixture.variable.as_id()) => {
                let name = db.ingredient_debug_name(key.ingredient_index());
                if name.contains("bound_typevar_default_type") {
                    episode.events.insert(CallbackEvents::BOUND_DEFAULT);
                }
                if name.contains("lazy_default_unchecked") {
                    episode.events.insert(CallbackEvents::LAZY_DEFAULT);
                }
            }
            Observation::Constructor(class) if class == fixture.callback.as_id() => {
                assert!(scopes.contains(&fixture.variable.as_id()), "{events:#?}");
                assert!(
                    episode.events.contains(CallbackEvents::BOUND_DEFAULT)
                        && episode.events.contains(CallbackEvents::LAZY_DEFAULT),
                    "{events:#?}"
                );
                episode.events.insert(CallbackEvents::CONSTRUCTOR);
                if matches!(
                    events.get(index + 1),
                    Some(Observation::Debit { accepted: false })
                ) {
                    episode.events.insert(CallbackEvents::ENTRY_REFUSED);
                }
            }
            Observation::InitializerMappingStarted {
                variable,
                receiver_class,
            } if receiver_class == Some(fixture.callback.as_id()) => {
                assert!(scopes.contains(&fixture.variable.as_id()), "{events:#?}");
                assert!(episode.events.contains(CallbackEvents::CONSTRUCTOR));
                let variable = variable.map(BoundTypeVarInstance::from_id);
                assert!(variable.is_some_and(|variable| variable.typevar(db).is_self(db)));
                let env = db.program_environment();
                let owner = variable
                    .and_then(|variable| variable.typevar(db).upper_bound(db, &env))
                    .and_then(Type::as_nominal_instance)
                    .and_then(|instance| instance.class(db, &env).static_class_literal(db))
                    .map(|(class, _)| class);
                assert_eq!(owner, Some(fixture.callback));
                assert!(!mapping);
                mapping = true;
                episode.events.insert(CallbackEvents::MAPPING_STARTED);
            }
            Observation::InitializerMappingFinished {
                complete,
                matches_receiver,
                frames,
                dropped_frames,
            } if mapping => {
                assert!(scopes.contains(&fixture.variable.as_id()), "{events:#?}");
                assert_eq!(frames, dropped_frames);
                assert!(!complete || matches_receiver);
                episode.mapping_frames += frames;
                if complete {
                    episode.events.insert(CallbackEvents::MAPPING_COMPLETE);
                }
                mapping = false;
            }
            Observation::ConstructorCompleted { class } if class == fixture.callback.as_id() => {
                assert!(scopes.contains(&fixture.variable.as_id()), "{events:#?}");
                assert!(episode.events.contains(CallbackEvents::MAPPING_COMPLETE));
                episode.events.insert(CallbackEvents::CONSTRUCTOR_COMPLETE);
            }
            _ => {}
        }
    }
    assert!(scopes.is_empty());
    assert!(!mapping);
    assert!(!expansion_probe::active());
    episode
}

#[test]
fn cold_defaults_keep_constructor_callbacks_and_generated_mapping_controlled() -> anyhow::Result<()>
{
    for source in SOURCES {
        let ordinary = database(source)?;
        let fixture = Fixture::new(&ordinary)?;
        let expected = fixture.owner.default_specialization(&ordinary);
        let expected_reads = executions(&ordinary);
        fixture.assert_result(&ordinary, expected)?;

        let db = database(source)?;
        let fixture = Fixture::new(&db)?;
        let stamp = Stamp::current(&db);
        let (outcome, statistics) =
            expansion_probe::run_mro_observed(&db, ALLOWANCE, || fixture.read(&db));
        let actual = complete(outcome)?;
        assert_eq!(executions(&db), expected_reads);
        let episode = episode(&db, &fixture, &statistics);
        assert!(
            episode.events.contains(CallbackEvents::BOUND_DEFAULT)
                && episode.events.contains(CallbackEvents::LAZY_DEFAULT),
            "{episode:?}"
        );
        assert!(
            episode.events.contains(CallbackEvents::MAPPING_STARTED) && episode.mapping_frames > 0,
            "{episode:?}"
        );
        assert!(
            episode.events.contains(CallbackEvents::MAPPING_COMPLETE)
                && episode
                    .events
                    .contains(CallbackEvents::CONSTRUCTOR_COMPLETE),
            "{episode:?}"
        );
        fixture.assert_result(&db, actual)?;
        assert_eq!(Stamp::current(&db), stamp);
    }
    Ok(())
}

fn minimum_allowance(
    source: &str,
    reached: impl Fn(&CallbackEpisode) -> bool,
) -> anyhow::Result<usize> {
    let db = database(source)?;
    let fixture = Fixture::new(&db)?;
    let (outcome, statistics) =
        expansion_probe::run_mro_observed(&db, ALLOWANCE, || fixture.read(&db));
    complete(outcome)?;
    assert!(
        reached(&episode(&db, &fixture, &statistics)),
        "{statistics:#?}"
    );
    let mut low = 0;
    let mut high = ALLOWANCE;
    while low < high {
        let middle = low + (high - low) / 2;
        let db = database(source)?;
        let fixture = Fixture::new(&db)?;
        let (outcome, statistics) =
            expansion_probe::run_mro_observed(&db, middle, || fixture.read(&db));
        assert!(
            matches!(outcome, Ok(Ok(_)) | Err(Incomplete::Allowance)),
            "{outcome:?}\n{statistics:#?}"
        );
        if reached(&episode(&db, &fixture, &statistics)) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(low)
}

fn assert_refused(episode: &CallbackEpisode, inside_mapping: bool) {
    assert!(
        episode.events.contains(CallbackEvents::BOUND_DEFAULT)
            && episode.events.contains(CallbackEvents::LAZY_DEFAULT),
        "{episode:?}"
    );
    assert!(
        episode.events.contains(CallbackEvents::CONSTRUCTOR),
        "{episode:?}"
    );
    assert!(
        !episode.events.contains(CallbackEvents::MAPPING_COMPLETE)
            && !episode
                .events
                .contains(CallbackEvents::CONSTRUCTOR_COMPLETE),
        "{episode:?}"
    );
    if inside_mapping {
        assert!(
            episode.events.contains(CallbackEvents::MAPPING_STARTED) && episode.mapping_frames > 0,
            "{episode:?}"
        );
    } else {
        assert!(
            episode.events.contains(CallbackEvents::ENTRY_REFUSED),
            "{episode:?}"
        );
        assert!(
            !episode.events.contains(CallbackEvents::MAPPING_STARTED),
            "{episode:?}"
        );
    }
}

#[test]
fn default_callbacks_share_allowance_and_retry_without_an_edit() -> anyhow::Result<()> {
    for source in SOURCES {
        let entry = minimum_allowance(source, |episode| {
            episode.events.contains(CallbackEvents::CONSTRUCTOR)
        })?;
        let mapping = minimum_allowance(source, |episode| episode.mapping_frames > 0)?;
        for (allowance, inside_mapping) in [(entry, false), (mapping, true)] {
            let db = database(source)?;
            let fixture = Fixture::new(&db)?;
            let stamp = Stamp::current(&db);
            let consumed = Cell::new(false);
            let (outcome, statistics) = expansion_probe::run_mro_observed(&db, allowance, || {
                let value = fixture.read(&db);
                if let Err(reason) = value {
                    assert_eq!(fixture.read(&db), Err(reason));
                }
                let value = value?;
                consumed.set(true);
                Ok::<_, Incomplete>(value)
            });
            assert_eq!(outcome, Err(Incomplete::Allowance));
            assert!(!consumed.get());
            assert_refused(&episode(&db, &fixture, &statistics), inside_mapping);
            let mut stopped = false;
            for event in statistics.observations() {
                match event {
                    Observation::Refusal => stopped = true,
                    Observation::Debit { accepted: true } => assert!(!stopped),
                    _ => {}
                }
            }
            assert!(stopped);
            assert_eq!(Stamp::current(&db), stamp);
            executions(&db);

            for retry in 0..2 {
                let (outcome, statistics) =
                    expansion_probe::run_mro_observed(&db, ALLOWANCE, || fixture.read(&db));
                let actual = complete(outcome)?;
                let episode = episode(&db, &fixture, &statistics);
                assert_eq!(
                    episode.events.contains(CallbackEvents::BOUND_DEFAULT),
                    retry == 0,
                    "{episode:?}"
                );
                assert_eq!(
                    episode.events.contains(CallbackEvents::LAZY_DEFAULT),
                    retry == 0,
                    "{episode:?}"
                );
                assert_eq!(
                    episode
                        .events
                        .contains(CallbackEvents::CONSTRUCTOR_COMPLETE),
                    retry == 0,
                    "{episode:?}"
                );
                assert_eq!(episode.mapping_frames > 0, retry == 0, "{episode:?}");
                executions(&db);
                fixture.assert_result(&db, actual)?;
                assert_eq!(Stamp::current(&db), stamp);
            }

            let shifted_db = database(source)?;
            let shifted_fixture = Fixture::new(&shifted_db)?;
            let stamp = Stamp::current(&shifted_db);
            let (outcome, statistics) =
                expansion_probe::run_mro_observed(&shifted_db, allowance + 7, || {
                    expansion_probe::charge_work(&shifted_db, 7)?;
                    shifted_fixture.read(&shifted_db)
                });
            assert_eq!(outcome, Err(Incomplete::Allowance));
            assert_refused(
                &episode(&shifted_db, &shifted_fixture, &statistics),
                inside_mapping,
            );
            assert_eq!(Stamp::current(&shifted_db), stamp);
        }
    }
    Ok(())
}

#[test]
fn source_mro_stops_before_later_defaults_after_a_constructor_refusal() -> anyhow::Result<()> {
    for (source, replacement) in SOURCES.into_iter().zip([
        "class Owner[T = Annotated[int, Callback(1)], U = int]: ...",
        "U = TypeVar(\"U\", default=int)\nclass Owner(Generic[T, U]): ...",
    ]) {
        let Some((prefix, _)) = source.rsplit_once("class Owner") else {
            anyhow::bail!("missing owner declaration")
        };
        let source = format!("{prefix}{replacement}\n");
        let allowance = minimum_allowance(&source, |episode| episode.mapping_frames > 0)?;
        let db = database(&source)?;
        let fixture = Fixture::new(&db)?;
        let Some(later) = fixture
            .owner
            .generic_context(&db)
            .and_then(|context| context.variables(&db).nth(1))
        else {
            anyhow::bail!("missing later default")
        };
        executions(&db);
        let (outcome, statistics) = expansion_probe::run_mro_observed(&db, allowance, || {
            read_source(&SourceClassEffects::new(&db), || {
                fixture.owner.try_mro(&db, None)
            })
        });
        assert!(matches!(outcome, Err(Incomplete::Allowance)), "{outcome:?}");
        assert_refused(&episode(&db, &fixture, &statistics), true);
        assert!(!statistics.observations().iter().any(|event| matches!(
            event,
            Observation::ClassDefaultReadEntered(variable) if *variable == later.as_id()
        )));
        let stamp = Stamp::current(&db);
        for retry in 0..2 {
            let (outcome, statistics) = expansion_probe::run_mro_observed(&db, ALLOWANCE, || {
                read_source(&SourceClassEffects::new(&db), || {
                    fixture.owner.try_mro(&db, None)
                })
            });
            let mro = complete(outcome)?.map_err(|error| anyhow::anyhow!("{error:?}"))?;
            let Some(crate::types::ClassBase::Class(ClassType::Generic(alias))) = mro.first()
            else {
                anyhow::bail!("missing specialized source MRO root")
            };
            let int = KnownClass::Int.to_instance(&db, &db.program_environment());
            assert_eq!(alias.specialization(&db).types(&db), [int, int]);
            assert_eq!(
                episode(&db, &fixture, &statistics)
                    .events
                    .contains(CallbackEvents::CONSTRUCTOR_COMPLETE),
                retry == 0
            );
            assert_eq!(Stamp::current(&db), stamp);
        }
    }
    Ok(())
}

struct RefuseAt {
    checks: Cell<usize>,
    at: Option<usize>,
}

impl SourceReadControl for RefuseAt {
    type Error = usize;

    fn check(&self) -> Result<(), usize> {
        let index = self.checks.get();
        self.checks.set(index + 1);
        if self.at == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}

#[test]
fn cold_and_warm_defaults_stop_at_each_consumption_and_publication_check() -> anyhow::Result<()> {
    for warm in [false, true] {
        let build = || database("class Owner[T = int, U = list[T]]: ...\n");
        let db = build()?;
        let owner = class(&db, "Owner")?;
        if warm {
            owner.default_specialization(&db);
        }
        let control = RefuseAt {
            checks: Cell::new(0),
            at: None,
        };
        let expected = default_class_specialization_with(&db, owner, &control)
            .map_err(|index| anyhow::anyhow!("unexpected refusal at {index}"))?;
        assert_eq!(expected, owner.default_specialization(&db));
        let checks = control.checks.get();
        assert!(checks > 2);
        for index in 0..checks {
            let db = build()?;
            let owner = class(&db, "Owner")?;
            if warm {
                owner.default_specialization(&db);
            }
            let control = RefuseAt {
                checks: Cell::new(0),
                at: Some(index),
            };
            assert_eq!(
                default_class_specialization_with(&db, owner, &control),
                Err(index)
            );
            assert_eq!(control.checks.get(), index + 1);
            for _ in 0..2 {
                let control = RefuseAt {
                    checks: Cell::new(0),
                    at: None,
                };
                let actual = default_class_specialization_with(&db, owner, &control)
                    .map_err(|index| anyhow::anyhow!("unexpected retry refusal at {index}"))?;
                assert_eq!(actual, owner.default_specialization(&db));
            }
        }
    }
    Ok(())
}
