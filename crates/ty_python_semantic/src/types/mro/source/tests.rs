use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::Stamp;

use super::{Context, source_dynamic_mro, source_enum_mro, source_named_tuple_mro};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::{DynamicClassAnchor, DynamicClassLiteral, DynamicClassScopeOffset};
use crate::types::class_base::{ClassBase, ClassBaseConversion};
use crate::types::constructor::expansion_probe::{self, Incomplete, Observation};
use crate::types::mro::iteration::{SynchronousMroIterationEffects, full_mro_with};
use crate::types::mro::root::MroTailRequest;
use crate::types::mro::{DynamicMroError, DynamicMroErrorKind};
use crate::types::source_read::UnrestrictedSourceRead;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, ClassType, DynamicType, InternedType, KnownClass,
    KnownInstanceType, MaterializationKind, RecursivelyDefined, SpecialFormType,
    StaticClassLiteral, Type, UnionType,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/dynamic_mro.py",
            "from enum import Enum\n\
             from typing import NamedTuple, TypedDict\n\
             class Base[T]: ...\n\
             IntBase = Base[int]\nStrBase = Base[str]\n\
             Dynamic = type('Dynamic', (IntBase,), {})\n\
             Named = NamedTuple('Named', [('value', int)])\n\
             Typed = TypedDict('Typed', {'value': int})\n\
             Enumeration = Enum('Enumeration', {'VALUE': 1})\n",
        )
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let file = system_path_to_file(db, "/src/dynamic_mro.py")?;
    Ok(global_symbol(db, db.program_file(file), name)
        .place
        .expect_type())
}

fn request<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<MroTailRequest<'db>> {
    match symbol(db, name)?.as_class_literal() {
        Some(ClassLiteral::Dynamic(literal)) => Ok(MroTailRequest::Dynamic(literal)),
        Some(ClassLiteral::DynamicEnum(literal)) => Ok(MroTailRequest::DynamicEnum(literal)),
        Some(ClassLiteral::DynamicNamedTuple(literal)) => {
            Ok(MroTailRequest::DynamicNamedTuple(literal))
        }
        Some(ClassLiteral::DynamicTypedDict(literal)) => {
            Ok(MroTailRequest::DynamicTypedDict(literal))
        }
        actual => anyhow::bail!("{name}: {actual:?}"),
    }
}

#[test]
fn dynamic_source_tails_match_public_queries_and_retry_after_interruption() -> anyhow::Result<()> {
    for name in ["Dynamic", "Named", "Typed", "Enumeration"] {
        let db = database()?;
        let request = request(&db, name)?;
        db.clone().clear_salsa_events();
        let outcome = salsa::attempt_probe::try_with_attempt(&db, 10_000, || {
            salsa::attempt_probe::report_incomplete(
                &db,
                salsa::attempt_probe::Incomplete::Interrupted,
            );
            // Invoke each private query while stopped, then reject consumption through the
            // declaration provider. Recovery values remain internal to this attempt.
            match request {
                MroTailRequest::Dynamic(literal) => assert!(
                    source_dynamic_mro(&db, literal)
                        .as_ref()
                        .is_ok_and(|mro| mro.is_empty())
                ),
                MroTailRequest::DynamicEnum(literal) => assert!(
                    source_enum_mro(&db, literal)
                        .as_ref()
                        .is_ok_and(|mro| mro.is_empty())
                ),
                MroTailRequest::DynamicNamedTuple(literal) => {
                    assert!(source_named_tuple_mro(&db, literal).is_empty());
                }
                MroTailRequest::DynamicTypedDict(literal) => {
                    literal.mro(&db);
                }
                MroTailRequest::Static(..) => anyhow::bail!("unexpected static request"),
            }
            assert!(Context::new(&db).full_mro(request).is_err());
            Ok(())
        });
        assert!(matches!(
            outcome,
            Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(
                salsa::attempt_probe::Incomplete::Interrupted
            ))
        ));
        for _ in 0..2 {
            let outcome = salsa::attempt_probe::try_with_attempt(&db, 10_000, || {
                Context::new(&db).full_mro(request)
            });
            let Ok(salsa::attempt_probe::AttemptOutcome::Complete(Ok(actual))) = outcome else {
                anyhow::bail!("{name}: retry did not complete: {outcome:?}");
            };
            assert_eq!(
                actual,
                super::infallible(full_mro_with(&db, request, &UnrestrictedSourceRead)),
                "{name}"
            );
            assert!(actual.len() >= 2);
        }
    }
    Ok(())
}

#[test]
fn dynamic_fallbacks_preserve_original_payloads_and_error_kinds() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let base = symbol(&db, "Base")?
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing Base"))?;
    let int_base = symbol(&db, "IntBase")?;
    let str_base = symbol(&db, "StrBase")?;
    let object = KnownClass::Object.to_class_literal(&db, &env);
    let int = KnownClass::Int.to_class_literal(&db, &env);
    let object_base = ClassBase::object(&db, &env);
    let int_base_class = ClassBase::try_from_explicit_base(&db, &env, int_base, None)
        .ok_or_else(|| anyhow::anyhow!("missing IntBase"))?;
    let cases = [
        ("empty", vec![]),
        (
            "invalid",
            vec![Type::int_literal(42), int_base, Type::int_literal(13)],
        ),
        ("duplicate", vec![int_base, str_base]),
        ("dynamic", vec![Type::unknown(), Type::unknown()]),
        ("conflict", vec![object, int]),
    ];
    for (name, bases) in cases {
        // All requests share a source location. Their exact base payloads must still give
        // distinct query operands, including the two aliases of the same generic class.
        let literal = DynamicClassLiteral::new(
            &db,
            "Derived",
            DynamicClassAnchor::ScopeOffset {
                scope: base.body_scope(&db),
                offset: DynamicClassScopeOffset::Node(0),
                explicit_bases: bases.clone().into_boxed_slice(),
            },
            Box::default(),
            false,
            None,
        );
        let result = source_dynamic_mro(&db, literal);
        assert_eq!(result, literal.try_mro(&db), "{name}");
        match (name, result) {
            ("empty" | "dynamic", Ok(_)) => {}
            ("invalid", Err(error)) => assert_eq!(
                error.reason(),
                &DynamicMroErrorKind::InvalidBases(Box::from([(0, bases[0]), (2, bases[2])]))
            ),
            ("duplicate", Err(error)) => assert!(matches!(
                error.reason(),
                DynamicMroErrorKind::DuplicateBases(_)
            )),
            ("conflict", Err(error)) => {
                assert_eq!(error.reason(), &DynamicMroErrorKind::UnresolvableMro);
            }
            _ => anyhow::bail!("{name}: unexpected result {result:?}"),
        }
        let mro = result
            .as_ref()
            .unwrap_or_else(DynamicMroError::fallback_mro);
        assert_eq!(
            mro[0],
            ClassBase::Class(ClassType::NonGeneric(literal.into()))
        );
        assert_eq!(mro.last(), Some(&object_base));
        assert_eq!(mro.iter().filter(|base| **base == object_base).count(), 1);
        if name == "duplicate" {
            assert_eq!(mro[1], int_base_class);
            assert_eq!(
                mro.iter()
                    .filter(|entry| entry.mro_identity(&db) == int_base_class.mro_identity(&db))
                    .count(),
                1
            );
        }
    }
    Ok(())
}

fn source_database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/dynamic_mro.py", source)
        .build()
}

fn static_class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    symbol(db, name)?
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing static class {name}"))
}

fn complete<T>(result: Result<Result<T, Incomplete>, Incomplete>) -> anyhow::Result<T> {
    result
        .and_then(|result| result)
        .map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            let name = db.ingredient_debug_name(database_key.ingredient_index());
            Some(
                name.rsplit("::")
                    .next()
                    .unwrap_or(&name)
                    .trim_end_matches('_')
                    .to_owned(),
            )
        })
        .collect()
}

#[test]
fn source_base_defaults_preserve_dependent_ancestor_arguments() -> anyhow::Result<()> {
    for source in [
        r#"
class Ancestor[T, U]: ...
class Base[T = int, U = list[T]](Ancestor[U, T]): ...
class Receiver(Base): ...
Dynamic = type("Dynamic", (Base,), {})
ExpectedBase = Base[int, list[int]]
ExpectedAncestor = Ancestor[list[int], int]
"#,
        r#"
from typing import Generic, TypeVar
T = TypeVar("T", default=int)
U = TypeVar("U", default=list[T])
class Ancestor(Generic[T, U]): ...
class Base(Ancestor[U, T], Generic[T, U]): ...
class Receiver(Base): ...
Dynamic = type("Dynamic", (Base,), {})
ExpectedBase = Base[int, list[int]]
ExpectedAncestor = Ancestor[list[int], int]
"#,
    ] {
        let ordinary_db = source_database(source)?;
        let ordinary = static_class(&ordinary_db, "Receiver")?;
        let expected =
            super::compute(&ordinary_db, ordinary).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let expected = expected
            .iter()
            .map(|base| {
                base.display(&ordinary_db, &ordinary_db.program_environment())
                    .to_string()
            })
            .collect::<Vec<_>>();

        let db = source_database(source)?;
        let receiver = static_class(&db, "Receiver")?;
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            super::compute(&db, receiver).map_err(|error| anyhow::anyhow!("{error:?}"))
        });
        let actual = result.map_err(|error| anyhow::anyhow!("{error:?}"))??;
        assert_eq!(
            actual
                .iter()
                .map(|base| base.display(&db, &db.program_environment()).to_string())
                .collect::<Vec<_>>(),
            expected
        );
        for (index, name) in [(1, "ExpectedBase"), (2, "ExpectedAncestor")] {
            let expected = symbol(&db, name)?;
            assert!(matches!(expected, Type::GenericAlias(_)));
            assert_eq!(actual.get(index).copied().map(Type::from), Some(expected));
        }
        let MroTailRequest::Dynamic(dynamic) = request(&db, "Dynamic")? else {
            anyhow::bail!("expected dynamic class");
        };
        let (result, _) =
            expansion_probe::run_mro(&db, 100_000, || source_dynamic_mro(&db, dynamic));
        let actual = result
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        for (index, name) in [(1, "ExpectedBase"), (2, "ExpectedAncestor")] {
            assert_eq!(
                actual.get(index).copied().map(Type::from),
                Some(symbol(&db, name)?)
            );
        }
    }
    Ok(())
}

fn conversion_input<'db>(
    db: &'db TestDb,
    name: &str,
) -> anyhow::Result<(Type<'db>, Option<ClassLiteral<'db>>)> {
    if let Some(base) = name.strip_suffix("_top") {
        let (ty, subclass) = conversion_input(db, base)?;
        let env = db.program_environment();
        let materialized = ty.materialize(
            db,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::new(&env),
        );
        assert!(
            match materialized {
                Type::Recursive(recursive) => recursive.materialization_kind(db).is_some(),
                Type::TypeAlias(alias) => alias.materialization_kind(db).is_some(),
                _ => false,
            },
            "{name}: materialization did not retain its lazy marker"
        );
        return Ok((materialized, subclass));
    }
    let ty = match name {
        "recursive" => {
            let ty = symbol(db, name)?;
            assert!(matches!(ty, Type::Recursive(_)), "{ty:?}");
            ty
        }
        "Alias" | "Manual" | "RecursiveAlias" | "ManualRecursive" => {
            let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) = symbol(db, name)?
            else {
                anyhow::bail!("missing type alias {name}");
            };
            Type::TypeAlias(alias)
        }
        "Number" => {
            let Type::KnownInstance(KnownInstanceType::NewType(newtype)) = symbol(db, name)? else {
                anyhow::bail!("missing NewType");
            };
            Type::NewTypeInstance(newtype)
        }
        "annotated_instance" | "annotated_any" | "annotated_literal" => {
            let inner = match name {
                "annotated_instance" => symbol(db, "instance")?,
                "annotated_any" => Type::any(),
                _ => symbol(db, "Base")?,
            };
            Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(db, inner)))
        }
        "Tuple" => Type::SpecialForm(SpecialFormType::Tuple),
        "Type" => Type::SpecialForm(SpecialFormType::Type),
        "NamedTuple" => {
            return Ok((
                Type::SpecialForm(SpecialFormType::NamedTuple),
                Some(static_class(db, "Record")?.into()),
            ));
        }
        _ => anyhow::bail!("unknown conversion {name}"),
    };
    Ok((ty, None))
}

#[test]
fn source_conversion_dependencies_match_ordinary_values() -> anyhow::Result<()> {
    let source = r#"
from typing import Any, NamedTuple, NewType, TypeAliasType
class Base[T = int]: ...
instance: Base[str]
Recursive = tuple["Recursive", Any]
recursive: Recursive
type Alias = Base[Any]
Manual = TypeAliasType("Manual", Base[Any])
type RecursiveAlias = tuple[Any, RecursiveAlias]
ManualRecursive = TypeAliasType("ManualRecursive", tuple[Any, "ManualRecursive"])
Number = NewType("Number", int)
class Record(NamedTuple):
    first: int
    second: str
ExpectedRecordBase = tuple[int, str]
ExpectedInstance = Base[str]
"#;
    for name in [
        "recursive",
        "recursive_top",
        "Alias",
        "RecursiveAlias_top",
        "Manual",
        "ManualRecursive_top",
        "Number",
        "annotated_instance",
        "annotated_any",
        "annotated_literal",
        "Tuple",
        "Type",
        "NamedTuple",
    ] {
        let ordinary_db = source_database(source)?;
        let (ty, subclass) = conversion_input(&ordinary_db, name)?;
        let expected = ClassBaseConversion::from_explicit_type(ty).resolve(
            &ordinary_db,
            &ordinary_db.program_environment(),
            subclass,
        );
        let expected = expected.map(|base| {
            base.display(&ordinary_db, &ordinary_db.program_environment())
                .to_string()
        });

        let db = source_database(source)?;
        let (ty, subclass) = conversion_input(&db, name)?;
        let env = db.program_environment();
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            ClassBaseConversion::from_explicit_type(ty).resolve_with(
                &db,
                &env,
                subclass,
                &Context::new(&db),
            )
        });
        let actual = complete(result).map_err(|error| anyhow::anyhow!("{name}: {error}"))?;
        assert_eq!(
            actual.map(|base| base.display(&db, &env).to_string()),
            expected,
            "{name}"
        );
        assert_eq!(
            actual,
            ClassBaseConversion::from_explicit_type(ty).resolve(&db, &env, subclass),
            "{name}"
        );
        match name.strip_suffix("_top").unwrap_or(name) {
            "recursive" | "Alias" | "Manual" | "RecursiveAlias" | "ManualRecursive" | "Number"
            | "annotated_literal" => {
                assert_eq!(actual, None, "{name}");
            }
            "annotated_any" => assert_eq!(actual, Some(ClassBase::Dynamic(DynamicType::Any))),
            "annotated_instance" => assert_eq!(
                actual.map(Type::from),
                Some(symbol(&db, "ExpectedInstance")?)
            ),
            "NamedTuple" => assert_eq!(
                actual.map(Type::from),
                Some(symbol(&db, "ExpectedRecordBase")?)
            ),
            _ => assert!(matches!(actual, Some(ClassBase::Class(_))), "{name}"),
        }
    }
    Ok(())
}

#[test]
fn source_union_children_preserve_defaults_and_explicit_any_provenance() -> anyhow::Result<()> {
    let db = source_database("class Base[T = int]: ...\n")?;
    let base = symbol(&db, "Base")?;
    let env = db.program_environment();
    let explicit_any = Type::SpecialForm(SpecialFormType::Any);
    for (elements, expected) in [
        (
            vec![explicit_any, Type::any()],
            Some(ClassBase::Dynamic(DynamicType::Any)),
        ),
        (
            vec![base, Type::unknown()],
            Some(ClassBase::Dynamic(DynamicType::Unknown)),
        ),
        (vec![Type::int_literal(1), base, Type::any()], None),
    ] {
        let ty = Type::Union(UnionType::new(
            &db,
            elements.as_slice(),
            RecursivelyDefined::No,
        ));
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            ClassBaseConversion::from_explicit_type(ty).resolve_with(
                &db,
                &env,
                None,
                &Context::new(&db),
            )
        });
        assert_eq!(complete(result)?, expected);
        assert_eq!(
            ClassBaseConversion::from_explicit_type(ty).resolve(&db, &env, None),
            expected
        );
    }
    let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
        ClassBaseConversion::from_explicit_type(explicit_any).resolve_with(
            &db,
            &env,
            None,
            &Context::new(&db),
        )
    });
    assert_eq!(complete(result)?, Some(ClassBase::Any));
    Ok(())
}

#[test]
fn newtype_base_constructor_refusal_retries_without_an_edit() -> anyhow::Result<()> {
    let source = r#"
from typing import NewType
class Owner: ...
class Factory:
    def __new__(cls) -> type[Owner]: ...
Number = NewType("Number", Factory())
"#;
    let ordinary_db = source_database(source)?;
    let (ty, subclass) = conversion_input(&ordinary_db, "Number")?;
    let expected = ClassBaseConversion::from_explicit_type(ty).resolve(
        &ordinary_db,
        &ordinary_db.program_environment(),
        subclass,
    );
    assert_eq!(expected, None);

    let db = source_database(source)?;
    let (ty, subclass) = conversion_input(&db, "Number")?;
    let factory = static_class(&db, "Factory")?;
    let env = db.program_environment();
    let stamp = Stamp::current(&db);
    executions(&db);
    let (result, statistics) = expansion_probe::run_mro_observed(&db, 1, || {
        ClassBaseConversion::from_explicit_type(ty).resolve_with(
            &db,
            &env,
            subclass,
            &Context::new(&db),
        )
    });
    assert_eq!(result, Err(Incomplete::Allowance));
    assert!(statistics.observations().iter().any(|observation| matches!(
        observation,
        Observation::Constructor(id) if *id == factory.as_id()
    )));
    let reads = executions(&db);
    assert!(reads.iter().any(|name| name == "lazy_base"), "{reads:?}");
    for retry in 0..2 {
        let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
            ClassBaseConversion::from_explicit_type(ty).resolve_with(
                &db,
                &env,
                subclass,
                &Context::new(&db),
            )
        });
        assert_eq!(complete(result)?, None);
        assert_eq!(Stamp::current(&db), stamp);
        let reads = executions(&db);
        assert_eq!(
            reads.iter().any(|name| name == "lazy_base"),
            retry == 0,
            "{reads:?}"
        );
    }
    Ok(())
}
