//! Named declaration reads retain attempt support across controlled constructor callbacks.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::Stamp;

use super::AttemptMroEffects;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete, Observation, Statistics};
use crate::types::mro::root::{
    MroTailRequest, SynchronousMroRootEffects, apply_optional_class_specialization_sync,
    mro_tail_request_sync,
};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{
    ClassLiteral, ClassType, GenericContext, StaticClassLiteral, Type, TypeContext, TypeMapping,
};

const HEADERS: &str = "class Header[T]: ...\nclass Params[**P]: ...\nclass Tuple[*Ts]: ...\nclass Defaulted[T = int]: ...\n";
const CALLBACK: &str = "class Base: ...\nclass Factory:\n    def __new__(cls) -> type[Base]: ...\nclass Owner(Factory()): ...\n";
const ALLOWANCE: usize = 100_000;

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/context.pyi", source)
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = system_path_to_file(db, "/src/context.pyi")?;
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

fn read_context<'db>(
    db: &'db TestDb,
    class: StaticClassLiteral<'db>,
) -> Result<Option<GenericContext<'db>>, Incomplete> {
    SynchronousMroRootEffects::generic_context(&AttemptMroEffects::new(db), class)
}

fn complete<T>(result: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    result
        .and_then(|value| value)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

#[test]
fn cold_declared_headers_use_canonical_source_queries() -> anyhow::Result<()> {
    for (name, variable) in [
        ("Header", "T"),
        ("Params", "P"),
        ("Tuple", "Ts"),
        ("Defaulted", "T"),
    ] {
        let db = database(HEADERS)?;
        let class = class(&db, name)?;
        let setup_reads = executions(&db);
        assert!(
            setup_reads
                .iter()
                .all(|query| !query.contains("pep695_generic_context_inner")),
            "{setup_reads:?}"
        );
        let stamp = Stamp::current(&db);
        let (outcome, _) =
            expansion_probe::run_mro_observed(&db, ALLOWANCE, || read_context(&db, class));
        let Some(context) = complete(outcome)? else {
            anyhow::bail!("missing {name} context")
        };
        let reads = executions(&db);
        assert!(
            reads
                .iter()
                .any(|query| query.contains("pep695_generic_context_inner")),
            "{reads:?}"
        );
        assert!(
            reads
                .iter()
                .all(|query| !query.contains("bound_typevar_default_type")),
            "defaults must remain cold: {reads:?}"
        );
        assert_eq!(
            context
                .variables(&db)
                .map(|v| v.name(&db).as_str())
                .collect::<Vec<_>>(),
            [variable]
        );
        assert_eq!(Some(context), class.generic_context(&db));
        assert_eq!(Stamp::current(&db), stamp);
        for _ in 0..2 {
            let (outcome, _) =
                expansion_probe::run_mro_observed(&db, ALLOWANCE, || read_context(&db, class));
            assert_eq!(complete(outcome)?, Some(context));
            assert!(executions(&db).is_empty());
        }
    }
    Ok(())
}

struct StopRead<'db> {
    db: &'db TestDb,
    checks: Cell<usize>,
    stop_at: usize,
}
impl SourceReadControl for StopRead<'_> {
    type Error = Incomplete;
    fn check(&self) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)?;
        let check = self.checks.get() + 1;
        self.checks.set(check);
        if check == self.stop_at {
            Err(expansion_probe::refuse(self.db, Incomplete::Interrupted))
        } else {
            Ok(())
        }
    }
}

#[test]
fn refused_context_reads_do_not_publish_absence_and_reuse_completed_headers() -> anyhow::Result<()>
{
    for stop_at in [1, 2] {
        let db = database(HEADERS)?;
        let class = class(&db, "Header")?;
        executions(&db);
        let stamp = Stamp::current(&db);
        let consumer_reached = Cell::new(false);
        let (outcome, _) = expansion_probe::run_mro_observed(&db, ALLOWANCE, || {
            let control = StopRead {
                db: &db,
                checks: Cell::new(0),
                stop_at,
            };
            let context = read_source(&control, || read_context(&db, class))??;
            consumer_reached.set(true);
            Ok::<_, Incomplete>(context)
        });
        assert_eq!(outcome, Err(Incomplete::Interrupted));
        assert!(!consumer_reached.get());
        let reads = executions(&db);
        assert_eq!(
            reads
                .iter()
                .any(|query| query.contains("pep695_generic_context_inner")),
            stop_at == 2,
            "{reads:?}"
        );
        for retry in 0..2 {
            let (outcome, _) =
                expansion_probe::run_mro_observed(&db, ALLOWANCE, || read_context(&db, class));
            let context = complete(outcome)?;
            let reads = executions(&db);
            assert!(context.is_some());
            assert_eq!(
                reads
                    .iter()
                    .any(|query| query.contains("pep695_generic_context_inner")),
                stop_at == 1 && retry == 0,
                "completed child must remain reusable: {reads:?}"
            );
            assert_eq!(context, class.generic_context(&db));
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
    }
    Ok(())
}

#[test]
fn warm_source_context_does_not_exempt_supplied_specialization() -> anyhow::Result<()> {
    let db = database(HEADERS)?;
    let header = class(&db, "Header")?;
    let Some(header_context) = header.generic_context(&db) else {
        anyhow::bail!("missing header context")
    };
    let Some(variable) = header_context.variables(&db).next() else {
        anyhow::bail!("missing header variable")
    };
    let fresh = Type::TypeVar(variable).apply_type_mapping(
        &db,
        &db.program_environment(),
        &TypeMapping::FreshenBoundTypeVars {
            generic_context: header_context,
            delta: 1,
        },
        TypeContext::default(),
    );
    let Some(fresh_variable) = fresh.as_typevar() else {
        anyhow::bail!("freshening lost the type variable")
    };
    assert_ne!(fresh_variable, variable);
    assert_eq!(fresh_variable.typevar(&db), variable.typevar(&db));
    let class = class(&db, "Defaulted")?;
    let Some(context) = class.generic_context(&db) else {
        anyhow::bail!("missing context")
    };
    let supplied = context.specialize(&db, vec![fresh]);
    executions(&db);
    let stamp = Stamp::current(&db);
    for allowance in [0, 1] {
        let (outcome, _) = expansion_probe::run_mro_observed(&db, allowance, || {
            apply_optional_class_specialization_sync(
                &db,
                class,
                Some(supplied),
                &AttemptMroEffects::new(&db),
            )
        });
        assert_eq!(outcome, Err(Incomplete::Allowance));
        assert!(executions(&db).is_empty());
    }
    let (outcome, _) = expansion_probe::run_mro_observed(&db, ALLOWANCE, || {
        apply_optional_class_specialization_sync(
            &db,
            class,
            Some(supplied),
            &AttemptMroEffects::new(&db),
        )
    });
    let ClassType::Generic(alias) = complete(outcome)? else {
        anyhow::bail!("missing supplied specialization")
    };
    assert_eq!(alias.specialization(&db), supplied);
    assert_eq!(alias.specialization(&db).types(&db), [fresh]);
    assert!(executions(&db).is_empty());
    for allowance in [0, 1] {
        let (outcome, _) = expansion_probe::run_mro_observed(&db, allowance, || {
            mro_tail_request_sync(
                &db,
                class.into(),
                Some(supplied),
                &AttemptMroEffects::new(&db),
            )
        });
        assert!(matches!(outcome, Err(Incomplete::Allowance)));
    }
    let (outcome, _) = expansion_probe::run_mro_observed(&db, ALLOWANCE, || {
        mro_tail_request_sync(
            &db,
            class.into(),
            Some(supplied),
            &AttemptMroEffects::new(&db),
        )
    });
    let MroTailRequest::Static(actual_class, Some(actual)) = complete(outcome)? else {
        anyhow::bail!("missing tail specialization")
    };
    assert_eq!(actual_class, class);
    assert_eq!(actual, supplied);
    assert_eq!(Stamp::current(&db), stamp);
    Ok(())
}

fn callback_inside_context(statistics: &Statistics, owner: salsa::Id, factory: salsa::Id) -> bool {
    let mut contexts = Vec::new();
    let mut reached = false;
    for event in statistics.observations() {
        match *event {
            Observation::ClassContextEntered(class) => contexts.push(class),
            Observation::ClassContextExited(class) => assert_eq!(contexts.pop(), Some(class)),
            Observation::Constructor(class) if class == factory && contexts.contains(&owner) => {
                reached = true;
            }
            _ => {}
        }
    }
    assert!(contexts.is_empty());
    reached
}

#[test]
fn declaration_context_constructor_callback_joins_the_installed_attempt() -> anyhow::Result<()> {
    let ordinary = database(CALLBACK)?;
    let ordinary_owner = class(&ordinary, "Owner")?;
    class(&ordinary, "Factory")?;
    executions(&ordinary);
    let expected = ordinary_owner.generic_context(&ordinary);
    assert!(expected.is_none());
    let expected_reads = executions(&ordinary);
    let db = database(CALLBACK)?;
    let owner = class(&db, "Owner")?;
    let factory = class(&db, "Factory")?;
    let setup = executions(&db);
    assert!(
        setup
            .iter()
            .all(|query| !query.contains("infer_deferred_types")
                && !query.contains("explicit_bases_inner")),
        "{setup:?}"
    );
    let stamp = Stamp::current(&db);
    let (outcome, statistics) =
        expansion_probe::run_mro_observed(&db, ALLOWANCE, || read_context(&db, owner));
    let actual = complete(outcome).map_err(|error| anyhow::anyhow!("{error}\n{statistics:#?}"))?;
    let reads = executions(&db);
    assert_eq!(reads, expected_reads);
    assert!(
        reads
            .iter()
            .any(|query| query.contains("infer_deferred_types")),
        "{reads:?}"
    );
    assert!(
        callback_inside_context(&statistics, owner.as_id(), factory.as_id()),
        "{statistics:#?}"
    );
    assert!(statistics.observations().iter().any(|event| matches!(event, Observation::ConstructorCompleted { class } if *class == factory.as_id())), "{statistics:#?}");
    assert!(actual.is_none());
    assert_eq!(actual, owner.generic_context(&db));
    assert_eq!(Stamp::current(&db), stamp);
    assert!(!expansion_probe::active());
    Ok(())
}

#[test]
fn source_callback_refusal_uses_the_parent_allowance_and_retries() -> anyhow::Result<()> {
    let mut witness = false;
    for allowance in 0..128 {
        let db = database(CALLBACK)?;
        let owner = class(&db, "Owner")?;
        let factory = class(&db, "Factory")?;
        executions(&db);
        let stamp = Stamp::current(&db);
        let reached_consumer = Cell::new(false);
        let (outcome, statistics) = expansion_probe::run_mro_observed(&db, allowance, || {
            let value = read_context(&db, owner);
            if let Err(reason) = value {
                assert_eq!(read_context(&db, owner), Err(reason));
            }
            let value = value?;
            reached_consumer.set(true);
            Ok::<_, Incomplete>(value)
        });
        let refused_entry = statistics.observations().windows(2).any(|pair| {
            matches!(pair, [Observation::Constructor(class), Observation::Debit { accepted: false }] if *class == factory.as_id())
        });
        if !refused_entry {
            continue;
        }
        witness = true;
        assert_eq!(outcome, Err(Incomplete::Allowance));
        assert!(!reached_consumer.get());
        assert!(callback_inside_context(
            &statistics,
            owner.as_id(),
            factory.as_id()
        ));
        assert!(!statistics.observations().iter().any(|event| matches!(event, Observation::ConstructorCompleted { class } if *class == factory.as_id())));
        let mut stopped = false;
        for event in statistics.observations() {
            match event {
                Observation::Refusal => stopped = true,
                Observation::Debit { accepted: true } => assert!(!stopped),
                _ => {}
            }
        }
        assert!(stopped);
        executions(&db);
        for retry in 0..2 {
            let (retried, statistics) =
                expansion_probe::run_mro_observed(&db, ALLOWANCE, || read_context(&db, owner));
            let actual = complete(retried)?;
            let reads = executions(&db);
            assert_eq!(
                reads
                    .iter()
                    .any(|query| query.contains("infer_deferred_types")),
                retry == 0,
                "{reads:?}"
            );
            if retry == 0 {
                assert!(callback_inside_context(
                    &statistics,
                    owner.as_id(),
                    factory.as_id()
                ));
            }
            assert_eq!(actual, owner.generic_context(&db));
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
        // Spending the allowance before the same cold source read must refuse the same entry.
        let db = database(CALLBACK)?;
        let owner = class(&db, "Owner")?;
        let factory = class(&db, "Factory")?;
        executions(&db);
        let (shifted, statistics) = expansion_probe::run_mro_observed(&db, allowance + 7, || {
            expansion_probe::charge_work(&db, 7)?;
            read_context(&db, owner)
        });
        assert_eq!(shifted, Err(Incomplete::Allowance));
        assert!(statistics.observations().windows(2).any(|pair| {
            matches!(pair, [Observation::Constructor(class), Observation::Debit { accepted: false }] if *class == factory.as_id())
        }));
        break;
    }
    assert!(witness, "no refusal at the actual source callback entrance");
    Ok(())
}
