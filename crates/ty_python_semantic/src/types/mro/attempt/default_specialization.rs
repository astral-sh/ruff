use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::prepared_source_probe::Stamp;

use super::{AttemptMroEffects, UnsupportedMroOperation};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::defaults::{default_specialization_with, specialize_partial_with};
use crate::types::source_read::read_source;
use crate::types::{ClassLiteral, GenericContext, KnownClass, Specialization, Type};

const SOURCE: &str = r#"
from typing import Generic, ParamSpec, TypeVar, TypeVarTuple

T = TypeVar("T")
P = ParamSpec("P")
Ts = TypeVarTuple("Ts")
D = TypeVar("D", default=int)

class Legacy(Generic[T]): ...
class LegacyParams(Generic[P]): ...
class LegacyTuple(Generic[*Ts]): ...
class Header[T]: ...
class HeaderParams[**P]: ...
class HeaderTuple[*Ts]: ...
class Defaulted[T = int]: ...
class LegacyDefaulted(Generic[D]): ...
"#;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/defaults.py", SOURCE)
        .build()
}

fn context<'db>(
    db: &'db TestDb,
    name: &str,
) -> anyhow::Result<(GenericContext<'db>, Option<KnownClass>)> {
    let env = db.program_environment();
    let class = if name == "tuple" {
        KnownClass::Tuple.try_to_class_literal(db, &env)
    } else {
        let file = db.program_file(system_path_to_file(db, "/src/defaults.py")?);
        global_symbol(db, file, name)
            .place
            .ignore_possibly_undefined()
            .and_then(Type::as_class_literal)
            .and_then(ClassLiteral::as_static)
    }
    .ok_or_else(|| anyhow::anyhow!("missing class {name}"))?;
    let context = if expansion_probe::active() {
        read_source(&AttemptMroEffects::new(db), || class.generic_context(db))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
    } else {
        class.generic_context(db)
    }
    .ok_or_else(|| anyhow::anyhow!("missing context for {name}"))?;
    Ok((context, class.known(db)))
}

fn request<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Specialization<'db>> {
    let (context, known) = context(db, name)?;
    if expansion_probe::active() {
        default_specialization_with(db, context, known, &AttemptMroEffects::new(db))
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    } else {
        Ok(context.default_specialization(db, known))
    }
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

#[test]
fn cold_missing_defaults_preserve_canonical_queries_and_raw_specializations() -> anyhow::Result<()>
{
    for name in ["Legacy", "LegacyParams", "LegacyTuple", "tuple"] {
        let mut ordinary = database()?;
        ordinary.clear_salsa_events();
        let expected = request(&ordinary, name)?;
        let expected_reads = executions(&ordinary);
        let descriptions = expected
            .types(&ordinary)
            .iter()
            .map(|ty| {
                ty.display(&ordinary, &ordinary.program_environment())
                    .to_string()
            })
            .collect::<Vec<_>>();

        let mut db = database()?;
        db.clear_salsa_events();
        let (result, _) = expansion_probe::run_mro_observed(&db, 100_000, || request(&db, name));
        let actual_reads = executions(&db);
        let actual = result.map_err(|error| anyhow::anyhow!("{name}: {error:?}"))??;
        assert_eq!(actual_reads, expected_reads, "{name}");
        assert!(
            actual_reads
                .iter()
                .any(|query| query.contains("bound_typevar_default_type"))
        );
        assert_eq!(actual, request(&db, name)?);
        assert_eq!(
            actual
                .types(&db)
                .iter()
                .map(|ty| ty.display(&db, &db.program_environment()).to_string())
                .collect::<Vec<_>>(),
            descriptions
        );
        assert_eq!(actual.materialization_kind(&db), None);
        assert_eq!(actual.tuple(&db).is_some(), name == "tuple");
    }
    Ok(())
}

#[test]
fn cold_default_construction_refuses_and_retries_without_an_edit() -> anyhow::Result<()> {
    for name in ["Legacy", "LegacyParams", "LegacyTuple", "tuple"] {
        let db = database()?;
        let stamp = Stamp::current(&db);
        let (limited, _) = expansion_probe::run_mro_observed(&db, 1, || request(&db, name));
        assert!(matches!(limited, Err(Incomplete::Allowance)), "{limited:?}");
        for _ in 0..2 {
            let (result, _) =
                expansion_probe::run_mro_observed(&db, 100_000, || request(&db, name));
            let actual = result.map_err(|error| anyhow::anyhow!("{name}: {error:?}"))??;
            assert_eq!(actual, request(&db, name)?);
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
    }
    Ok(())
}

#[test]
fn prepared_pep695_headers_keep_default_queries_cold() -> anyhow::Result<()> {
    for name in ["Header", "HeaderParams", "HeaderTuple"] {
        let ordinary = database()?;
        let (original_context, original_known) = context(&ordinary, name)?;
        executions(&ordinary);
        let expected = original_context.default_specialization(&ordinary, original_known);
        let original_reads = executions(&ordinary);
        assert!(
            original_reads
                .iter()
                .any(|query| query.contains("bound_typevar_default_type"))
        );

        let db = database()?;
        let stamp = Stamp::current(&db);
        // Reading the source header leaves default queries cold. Refusing the subsequent
        // construction must not evaluate those defaults.
        let (context, known) = context(&db, name)?;
        executions(&db);
        let build =
            || default_specialization_with(&db, context, known, &AttemptMroEffects::new(&db));
        let (limited, _) = expansion_probe::run_mro_observed(&db, 1, build);
        assert_eq!(limited, Err(Incomplete::Allowance));
        assert!(executions(&db).is_empty());
        let (result, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
        let reads = executions(&db);
        let actual = result
            .and_then(|result| result)
            .map_err(|error| anyhow::anyhow!("{name}: {error:?}"))?;
        assert_eq!(reads, original_reads, "{name}");
        assert_eq!(actual, context.default_specialization(&db, known));
        assert_eq!(actual.types(&db).len(), expected.types(&ordinary).len());
        for _ in 0..2 {
            let (retry, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
            assert_eq!(retry, Ok(Ok(actual)));
        }
        assert_eq!(Stamp::current(&db), stamp);
        assert!(!expansion_probe::active());
    }
    Ok(())
}

#[test]
fn interruption_after_a_default_query_retries_without_an_edit() -> anyhow::Result<()> {
    for name in ["Legacy", "LegacyParams", "LegacyTuple", "tuple"] {
        let mut reached = false;
        for allowance in 0..256 {
            let db = database()?;
            let (context, known) = context(&db, name)?;
            executions(&db);
            let stamp = Stamp::current(&db);
            let build =
                || default_specialization_with(&db, context, known, &AttemptMroEffects::new(&db));
            let (limited, _) = expansion_probe::run_mro_observed(&db, allowance, build);
            let reads = executions(&db);
            if !matches!(limited, Err(Incomplete::Allowance))
                || !reads
                    .iter()
                    .any(|query| query.contains("bound_typevar_default_type"))
            {
                continue;
            }
            reached = true;
            for _ in 0..2 {
                let (retried, _) = expansion_probe::run_mro_observed(&db, 100_000, build);
                let actual = retried
                    .and_then(|result| result)
                    .map_err(|error| anyhow::anyhow!("{name}: {error:?}"))?;
                assert_eq!(actual, context.default_specialization(&db, known));
                assert_eq!(Stamp::current(&db), stamp);
                assert!(!expansion_probe::active());
            }
            break;
        }
        assert!(
            reached,
            "never interrupted after the default query for {name}"
        );
    }
    Ok(())
}

#[test]
fn present_defaults_refuse_but_supplied_arguments_skip_them() -> anyhow::Result<()> {
    for name in ["Defaulted", "LegacyDefaulted"] {
        let db = database()?;
        let (context, known) = context(&db, name)?;
        let stamp = Stamp::current(&db);
        executions(&db);
        let (limited, _) = expansion_probe::run_mro_observed(&db, 100_000, || {
            default_specialization_with(&db, context, known, &AttemptMroEffects::new(&db))
        });
        assert_eq!(
            limited,
            Err(Incomplete::UnsupportedMroOperation(
                UnsupportedMroOperation::TypeVarDefault
            ))
        );
        assert!(
            executions(&db)
                .iter()
                .all(|query| !query.contains("bound_typevar_default_type"))
        );
        for _ in 0..2 {
            let (result, _) = expansion_probe::run_mro_observed(&db, 100_000, || {
                specialize_partial_with(
                    &db,
                    context,
                    [Some(Type::Never)],
                    &AttemptMroEffects::new(&db),
                )
            });
            let actual = result
                .and_then(|result| result)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            assert_eq!(actual.types(&db), [Type::Never]);
            assert!(
                executions(&db)
                    .iter()
                    .all(|query| !query.contains("bound_typevar_default_type"))
            );
            assert_eq!(Stamp::current(&db), stamp);
            assert!(!expansion_probe::active());
        }
    }
    Ok(())
}
