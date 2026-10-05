use std::cell::RefCell;

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::AttemptMroEffects;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mro::Mro;
use crate::types::source_read::read_source;
use crate::types::{ClassLiteral, KnownClass, StaticClassLiteral, class_mro_literals};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/mro_attempt.py",
            "class Owner: ...\nclass Receiver(Owner): ...\nclass Other(Owner): ...\nclass Multiple(Receiver, Other): ...\nclass Invalid(1): ...\n",
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mro_attempt.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))
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

fn complete<T>(result: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    result
        .and_then(|result| result)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

#[test]
fn cold_mro_uses_ordinary_query_owners() -> anyhow::Result<()> {
    for (name, expected_names) in [
        ("Owner", &["Owner"][..]),
        ("Receiver", &["Receiver", "Owner"][..]),
        ("Multiple", &["Multiple", "Receiver", "Other", "Owner"][..]),
        ("Invalid", &["Invalid"][..]),
    ] {
        let db = database()?;
        let owner = class(&db, name)?;
        executions(&db);
        let (result, _) = expansion_probe::run_mro(&db, 10_000, || {
            read_source(&AttemptMroEffects::new(&db), || {
                class_mro_literals(&db, owner.into())
            })
        });
        let actual = complete(result)?;
        let events = executions(&db);
        assert!(
            events
                .iter()
                .any(|event| event.contains("class_mro_literals")),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.contains("try_mro_unspecialized")),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.contains("known_class_to_class_literal")),
            "cold object lookup missing: {events:?}"
        );
        let mut expected = expected_names
            .iter()
            .map(|name| class(&db, name).map(ClassLiteral::Static))
            .collect::<anyhow::Result<Vec<_>>>()?;
        expected.push(ClassLiteral::object(&db, &db.program_environment()));
        assert_eq!(actual.as_ref(), expected, "{name}");
        if name == "Invalid" {
            assert!(owner.try_mro(&db, None).is_err());
        }
        assert!(
            actual
                .last()
                .and_then(|class| class.as_static())
                .is_some_and(|class| class.is_known(&db, KnownClass::Object))
        );
    }
    Ok(())
}

#[test]
fn stopped_mro_owners_and_seed_do_not_resolve_fallback_dependencies() -> anyhow::Result<()> {
    let db = database()?;
    let owner = class(&db, "Owner")?;
    executions(&db);
    let retained = RefCell::new(None);
    let outcome = salsa::attempt_probe::try_with_attempt(&db, 10_000, || {
        salsa::attempt_probe::report_incomplete(&db, salsa::attempt_probe::Incomplete::Interrupted);
        let recovered = owner
            .try_mro(&db, None)
            .expect("internal recovery is not an inheritance error");
        assert!(recovered.is_empty());
        *retained.borrow_mut() = Some(recovered);
        assert!(class_mro_literals(&db, owner.into()).is_empty());
        assert!(
            Mro::static_cycle(&db, owner, None)
                .expect("stopped seed")
                .is_empty()
        );
    });
    assert!(matches!(
        outcome,
        Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    ));
    let events = executions(&db);
    assert_eq!(
        events.len(),
        2,
        "stopped owners entered a semantic child: {events:?}"
    );
    for _ in 0..2 {
        let (result, _) = expansion_probe::run_mro(&db, 10_000, || {
            read_source(&AttemptMroEffects::new(&db), || {
                class_mro_literals(&db, owner.into())
            })
        });
        assert_eq!(complete(result)?.len(), 2);
        assert!(retained.borrow().expect("retained recovery").is_empty());
    }
    Ok(())
}

#[test]
fn refusal_during_mro_collection_retries_at_the_same_revision() -> anyhow::Result<()> {
    let mut refusals = 0;
    let mut completions = 0;
    for allowance in 0..160 {
        let db = database()?;
        let owner = class(&db, "Receiver")?;
        let reached_consumer = RefCell::new(false);
        let (result, _) = expansion_probe::run_mro(&db, allowance, || {
            let literals = read_source(&AttemptMroEffects::new(&db), || {
                class_mro_literals(&db, owner.into())
            })?;
            *reached_consumer.borrow_mut() = true;
            Ok::<_, Incomplete>(literals.len())
        });
        match result {
            Err(Incomplete::Allowance) => {
                refusals += 1;
                assert!(!*reached_consumer.borrow());
            }
            Ok(Ok(3)) => completions += 1,
            other => anyhow::bail!("allowance {allowance}: {other:?}"),
        }
        for _ in 0..2 {
            let (result, _) = expansion_probe::run_mro(&db, 10_000, || {
                read_source(&AttemptMroEffects::new(&db), || {
                    class_mro_literals(&db, owner.into())
                })
            });
            assert_eq!(complete(result)?.len(), 3);
        }
    }
    assert!(
        refusals > 10 && completions > 0,
        "{refusals} refusals, {completions} completions"
    );
    Ok(())
}
