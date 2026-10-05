use std::cell::{Cell, RefCell};

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use salsa::plumbing::FromId;
use ty_python_core::ProgramFile;

use super::AttemptMroEffects;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::constructor::expansion_probe::{self, Incomplete, Observation};
use crate::types::mro::{Mro, StaticMroError};
use crate::types::source_read::read_source;
use crate::types::{ClassLiteral, ClassType, StaticClassLiteral};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/mro_cycles.pyi",
            r#"
class Seed: ...
class Direct(Direct): ...
class First(Second): ...
class Second(Third): ...
class Third(First): ...
"#,
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mro_cycles.pyi")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))
}

#[derive(Debug, Default)]
struct Trace {
    executed: Vec<String>,
    iterated: Vec<String>,
    finalized: Vec<String>,
}

impl Trace {
    fn take(db: &TestDb) -> Self {
        let mut trace = Self::default();
        for event in db.clone().take_salsa_events() {
            let (database_key, output) = match event.kind {
                salsa::EventKind::WillExecute { database_key } => {
                    (database_key, &mut trace.executed)
                }
                salsa::EventKind::WillIterateCycle { database_key, .. } => {
                    (database_key, &mut trace.iterated)
                }
                salsa::EventKind::DidFinalizeCycle { database_key, .. } => {
                    (database_key, &mut trace.finalized)
                }
                _ => continue,
            };
            output.push(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            );
        }
        trace
    }

    fn executed(&self, query: &str) -> bool {
        self.executed.iter().any(|name| name.contains(query))
    }

    fn assert_cold_productive_mro_cycle(&self) {
        assert!(self.executed("try_mro_unspecialized"), "{self:?}");
        assert!(self.executed("generic_context"), "{self:?}");
        assert!(
            self.executed("known_class_to_class_literal"),
            "cold object lookup missing: {self:?}"
        );
        // A finalization also witnesses a real fixpoint iteration when its initial value already
        // agrees with the body. Calling the seed directly cannot emit either cycle event.
        assert!(
            self.iterated
                .iter()
                .chain(&self.finalized)
                .any(|name| name.contains("try_mro_unspecialized")),
            "no tracked MRO cycle: {self:?}"
        );
        assert!(
            self.finalized
                .iter()
                .any(|name| name.contains("try_mro_unspecialized")),
            "MRO cycle did not finish: {self:?}"
        );
    }
}

fn complete<T>(result: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    result
        .and_then(|value| value)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn assert_cycle_fallback<'db>(
    db: &'db TestDb,
    owner: StaticClassLiteral<'db>,
    result: Result<&Mro<'db>, &StaticMroError<'db>>,
) -> anyhow::Result<()> {
    let Err(error) = result else {
        anyhow::bail!("inheritance cycle became a successful MRO");
    };
    assert!(error.is_cycle(), "unexpected inheritance error: {error:?}");
    assert_eq!(
        &error.fallback_mro()[..],
        [
            ClassBase::Class(ClassType::NonGeneric(owner.into())),
            ClassBase::unknown(),
            ClassBase::object(db, &db.program_environment()),
        ],
    );
    Ok(())
}

#[test]
fn ordinary_and_installed_inheritance_cycles_keep_productive_fallbacks() -> anyhow::Result<()> {
    for name in ["Direct", "First"] {
        for installed in [false, true] {
            let db = database()?;
            let owner = class(&db, name)?;
            Trace::take(&db);
            let result = if installed {
                let (result, _) = expansion_probe::run_mro(&db, 10_000, || {
                    read_source(&AttemptMroEffects::new(&db), || owner.try_mro(&db, None))
                });
                complete(result)?
            } else {
                owner.try_mro(&db, None)
            };
            let trace = Trace::take(&db);
            trace.assert_cold_productive_mro_cycle();
            assert_cycle_fallback(&db, owner, result)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Storage {
    Empty,
    Nonempty,
    InheritanceCycle,
    OtherError,
}

fn storage(result: Result<&Mro<'_>, &StaticMroError<'_>>) -> Storage {
    match result {
        Ok(mro) if mro.is_empty() => Storage::Empty,
        Ok(_) => Storage::Nonempty,
        Err(error) if error.is_cycle() => Storage::InheritanceCycle,
        Err(_) => Storage::OtherError,
    }
}

#[test]
fn seed_refusal_stops_before_later_dependencies_and_retries_without_an_edit() -> anyhow::Result<()>
{
    let mut refused_before_context = false;
    let mut refused_after_object = false;
    let mut completed = false;
    for allowance in (0..24).chain([10_000]) {
        let db = database()?;
        let owner = class(&db, "Seed")?;
        Trace::take(&db);
        let observed = Cell::new(None);
        let consumed = Cell::new(false);
        let (result, statistics) = expansion_probe::run_mro_observed(&db, allowance, || {
            let seed = read_source(&AttemptMroEffects::new(&db), || {
                let seed = Mro::static_cycle(&db, owner, None);
                observed.set(Some(storage(seed.as_ref().map_err(Box::as_ref))));
                seed
            })?;
            consumed.set(true);
            Ok::<_, Incomplete>(seed)
        });
        let trace = Trace::take(&db);
        match result {
            Err(Incomplete::Allowance) => {
                assert_eq!(
                    observed.get(),
                    Some(Storage::Empty),
                    "allowance {allowance}: {trace:?}"
                );
                assert!(!consumed.get());
                refused_before_context |= !trace.executed("generic_context");
                refused_after_object |= trace.executed("known_class_to_class_literal");
            }
            Ok(Ok(seed)) => {
                completed = true;
                assert_eq!(observed.get(), Some(Storage::InheritanceCycle));
                assert!(consumed.get());
                assert!(
                    trace.executed("known_class_to_class_literal"),
                    "object was not cold: {trace:?}"
                );
                assert_cycle_fallback(&db, owner, seed.as_ref().map_err(Box::as_ref))?;
            }
            other => anyhow::bail!("allowance {allowance}: {other:?}"),
        }
        if allowance <= 2 {
            assert!(
                trace.executed.is_empty(),
                "root refusal entered a source dependency: {allowance}: {trace:?}"
            );
        }
        let mut context_depth = 0;
        let mut stopped = false;
        for observation in statistics.observations() {
            match observation {
                Observation::ClassContextEntered(_) => context_depth += 1,
                Observation::ClassContextExited(_) => context_depth -= 1,
                Observation::Refusal => {
                    stopped = true;
                }
                Observation::Debit { accepted: true } => assert!(!stopped),
                _ => {}
            }
        }
        assert_eq!(context_depth, 0);
        assert!(
            !trace
                .iterated
                .iter()
                .chain(&trace.finalized)
                .any(|name| name.contains("try_mro_unspecialized")),
            "direct seed unexpectedly evaluated a tracked MRO cycle: {trace:?}"
        );

        for _ in 0..2 {
            let (retry, _) = expansion_probe::run_mro(&db, 10_000, || {
                read_source(&AttemptMroEffects::new(&db), || {
                    Mro::static_cycle(&db, owner, None)
                })
            });
            let seed = complete(retry)?;
            assert_cycle_fallback(&db, owner, seed.as_ref().map_err(Box::as_ref))?;
        }
        let (ordinary_owner, _) = expansion_probe::run_mro(&db, 10_000, || {
            read_source(&AttemptMroEffects::new(&db), || owner.try_mro(&db, None))
        });
        let mro = complete(ordinary_owner)?.map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(
            mro.len(),
            2,
            "synthetic seed changed an acyclic owner's result"
        );
    }
    assert!(refused_before_context && refused_after_object && completed);
    Ok(())
}

#[test]
fn interrupted_tracked_cycles_retry_and_keep_borrowed_recovery_separate() -> anyhow::Result<()> {
    const SOURCE: &str = r#"
class Seed: ...
class First(Second): ...
class Second(Third): ...
class Third(First, Factory()): ...
class Factory(First):
    def __new__(cls) -> type[Seed]: ...
"#;
    let build = || {
        TestDbBuilder::new()
            .with_file("/src/mro_cycles.pyi", SOURCE)
            .build()
    };
    let shape = |db: &TestDb, result: Result<&Mro<'_>, &StaticMroError<'_>>| {
        let entries = result.unwrap_or_else(StaticMroError::fallback_mro);
        (
            storage(result),
            entries
                .iter()
                .map(|base| base.display(db, &db.program_environment()).to_string())
                .collect::<Vec<_>>(),
        )
    };
    let ordinary = build()?;
    let ordinary_owner = class(&ordinary, "First")?;
    Trace::take(&ordinary);
    let expected = shape(&ordinary, ordinary_owner.try_mro(&ordinary, None));
    Trace::take(&ordinary).assert_cold_productive_mro_cycle();
    let mut refusals = 0;
    let mut completions = 0;
    let mut constructor_refusals = 0;
    for allowance in [0, 1, 2, 3, 8, 16, 32, 64, 96, 128, 1_000, 100_000] {
        let db = build()?;
        let owner = class(&db, "First")?;
        Trace::take(&db);
        let observed = Cell::new(None);
        let consumed = Cell::new(false);
        let retained = RefCell::new(None);
        let (result, statistics) = expansion_probe::run_mro_observed(&db, allowance, || {
            let result = read_source(&AttemptMroEffects::new(&db), || {
                let result = owner.try_mro(&db, None);
                observed.set(Some(storage(result)));
                if let Ok(mro) = result
                    && mro.is_empty()
                {
                    *retained.borrow_mut() = Some(mro);
                }
                result
            })?;
            consumed.set(true);
            Ok::<_, Incomplete>(result)
        });
        let trace = Trace::take(&db);
        let called_constructor = statistics.observations().iter().any(|observation| matches!(
            observation,
            Observation::Constructor(id) if StaticClassLiteral::from_id(*id).name(&db) == "Factory"
        ));
        match result {
            Err(Incomplete::Allowance) => {
                refusals += 1;
                constructor_refusals += usize::from(called_constructor);
                assert!(!consumed.get());
                assert_eq!(
                    observed.get(),
                    Some(Storage::Empty),
                    "allowance {allowance}: {trace:?}"
                );
            }
            Ok(Ok(result)) => {
                completions += 1;
                assert!(
                    called_constructor,
                    "cold MRO did not infer its constructor base"
                );
                assert!(consumed.get());
                trace.assert_cold_productive_mro_cycle();
                assert_eq!(shape(&db, result), expected);
            }
            other => anyhow::bail!("allowance {allowance}: {other:?}"),
        }
        for retry_index in 0..2 {
            Trace::take(&db);
            let (retry, _) = expansion_probe::run_mro(&db, 100_000, || {
                read_source(&AttemptMroEffects::new(&db), || owner.try_mro(&db, None))
            });
            let result = complete(retry)?;
            let retry_trace = Trace::take(&db);
            assert_eq!(shape(&db, result), expected);
            if let Some(recovery) = *retained.borrow() {
                assert!(
                    recovery.is_empty(),
                    "retry changed borrowed incomplete storage"
                );
            }
            if retry_index == 1 {
                assert!(
                    !retry_trace.executed("try_mro_unspecialized"),
                    "completed retry was not reused: {retry_trace:?}"
                );
            }
        }
    }
    assert!(refusals > 0 && constructor_refusals > 0 && completions > 0);
    Ok(())
}
