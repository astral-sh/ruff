//! Query-level controls for separating declaration MROs from supplied specializations.

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem as _;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::AsId as _;

use super::{DuplicateBaseError, Mro, StaticMroErrorKind};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::generic_alias_try_mro_ingredient;
use crate::types::class_base::ClassBase;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral, Type};

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/declaration_mro.pyi", source)
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = system_path_to_file(db, "/src/declaration_mro.pyi")?;
    global_symbol(db, db.program_file(file), name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn assert_fallback(db: &TestDb, owner: StaticClassLiteral<'_>, mro: &Mro<'_>) {
    assert_eq!(
        &mro[..],
        [
            ClassBase::Class(ClassType::NonGeneric(owner.into())),
            ClassBase::unknown(),
            ClassBase::object(db, &db.program_environment()),
        ],
    );
}

#[test]
fn tracked_declarations_preserve_errors_and_successful_fallbacks() -> anyhow::Result<()> {
    let db = database(
        "from missing import unknown\n\
         from typing import Generic, TypeVar\n\
         K = TypeVar('K')\nV = TypeVar('V')\n\
         class A: ...\n\
         class Invalid(42, A, False): ...\n\
         class Duplicate(A, A): ...\n\
         class DuplicateDynamic(unknown, unknown): ...\n\
         class Reorder(Generic[K, V], dict): ...\n\
         class Conflict(object, int): ...\n",
    )?;
    let invalid = class(&db, "Invalid")?;
    let result = invalid.try_mro(&db, None);
    let Err(error) = result else {
        anyhow::bail!("invalid bases produced a successful MRO");
    };
    let raw = invalid.explicit_bases(&db);
    assert_eq!(
        error.reason(),
        &StaticMroErrorKind::InvalidBases(Box::from([(0, raw[0]), (2, raw[2])])),
    );
    assert_fallback(&db, invalid, error.fallback_mro());

    let duplicate = class(&db, "Duplicate")?;
    let Err(error) = duplicate.try_mro(&db, None) else {
        anyhow::bail!("concrete duplicate bases produced a successful MRO");
    };
    let StaticMroErrorKind::DuplicateBases(errors) = error.reason() else {
        anyhow::bail!("wrong duplicate error: {error:?}");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].first_index, 0);
    assert_eq!(&*errors[0].later_indices, [1]);
    assert_eq!(
        errors[0].duplicate_base,
        ClassBase::Class(ClassType::NonGeneric(class(&db, "A")?.into())),
    );
    assert_fallback(&db, duplicate, error.fallback_mro());

    let dynamic = class(&db, "DuplicateDynamic")?;
    let result = dynamic
        .try_mro(&db, None)
        .map_err(|error| anyhow::anyhow!("dynamic duplicate became an error: {error:?}"))?;
    assert_fallback(&db, dynamic, result);

    for (name, index) in [("Reorder", Some(0)), ("Conflict", None)] {
        let owner = class(&db, name)?;
        let Err(error) = owner.try_mro(&db, None) else {
            anyhow::bail!("{name} produced a successful MRO");
        };
        let StaticMroErrorKind::UnresolvableMro {
            bases_list,
            generic_index,
        } = error.reason()
        else {
            anyhow::bail!("wrong conflict error: {error:?}");
        };
        assert_eq!(&**bases_list, owner.explicit_bases(&db));
        assert_eq!(*generic_index, index);
    }
    Ok(())
}

#[test]
fn generic_errors_preserve_declaration_details_and_requested_fallback_roots() -> anyhow::Result<()>
{
    let db = database(
        "class Base[T]: ...\n\
         class Invalid[T](42): ...\n\
         class Duplicate[T](Base[T], Base[T]): ...\n\
         class X: ...\nclass Y: ...\n\
         class Left[T](X, Y): ...\nclass Right[T](Y, X): ...\n\
         class Conflict[T](Left[T], Right[T]): ...\n\
         invalid = Invalid[int]\nduplicate = Duplicate[int]\nconflict = Conflict[int]\n",
    )?;
    let file = system_path_to_file(&db, "/src/declaration_mro.pyi")?;
    let alias = |name| -> anyhow::Result<GenericAlias<'_>> {
        let Type::GenericAlias(alias) = global_symbol(&db, db.program_file(file), name)
            .place
            .expect_type()
        else {
            anyhow::bail!("missing alias {name}");
        };
        Ok(alias)
    };
    let invalid = alias("invalid")?;
    let duplicate = alias("duplicate")?;
    let conflict = alias("conflict")?;
    let Type::GenericAlias(duplicate_base) = duplicate.origin(&db).explicit_bases(&db)[0] else {
        anyhow::bail!("missing declared duplicate base");
    };
    let cases = [
        (
            invalid,
            StaticMroErrorKind::InvalidBases(Box::from([(0, Type::int_literal(42))])),
        ),
        (
            duplicate,
            StaticMroErrorKind::DuplicateBases(Box::from([DuplicateBaseError {
                duplicate_base: ClassBase::Class(ClassType::Generic(duplicate_base)),
                first_index: 0,
                later_indices: Box::from([1]),
            }])),
        ),
        (
            conflict,
            StaticMroErrorKind::UnresolvableMro {
                bases_list: conflict.origin(&db).explicit_bases(&db).into(),
                generic_index: None,
            },
        ),
    ];
    for (alias, expected) in cases {
        let owner = alias.origin(&db);
        // Error details describe the declared bases; the fallback retains the requested root.
        for (specialization, root) in [
            (None, owner.default_specialization(&db)),
            (Some(alias.specialization(&db)), ClassType::Generic(alias)),
        ] {
            let Err(error) = owner.try_mro(&db, specialization) else {
                anyhow::bail!("{} produced a successful MRO", owner.name(&db));
            };
            assert_eq!(error.reason(), &expected);
            assert_eq!(
                &error.fallback_mro()[..],
                [
                    ClassBase::Class(root),
                    ClassBase::unknown(),
                    ClassBase::object(&db, &db.program_environment()),
                ],
            );
        }
    }
    Ok(())
}

#[test]
fn tracked_declaration_cycle_refines_its_error_seed() -> anyhow::Result<()> {
    let db = database("class E[T](E.a): ...\n")?;
    let owner = class(&db, "E")?;
    db.clone().clear_salsa_events();
    let result = owner.try_mro(&db, None).map_err(|error| {
        anyhow::anyhow!("productive declaration retained error seed: {error:?}")
    })?;
    let events = db.clone().take_salsa_events();
    let root = ClassBase::Class(owner.apply_optional_specialization(&db, None));
    assert_eq!(
        &result[..],
        [
            root,
            ClassBase::unknown(),
            ClassBase::Generic,
            ClassBase::object(&db, &db.program_environment())
        ],
    );
    let iterations: Vec<_> = events
        .iter()
        .filter_map(|event| match event.kind {
            salsa::EventKind::WillIterateCycle { database_key, .. } => Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            ),
            _ => None,
        })
        .collect();
    assert!(
        iterations
            .iter()
            .any(|name| name.contains("explicit_bases_inner")
                || name.contains("try_mro_unspecialized")),
        "no source cycle refinement: {iterations:?}",
    );
    let seed = Mro::static_cycle(&db, owner, None);
    let Err(seed) = seed else {
        anyhow::bail!("cycle seed unexpectedly succeeded");
    };
    assert!(seed.is_cycle());
    assert_eq!(
        &seed.fallback_mro()[..],
        [
            root,
            ClassBase::unknown(),
            ClassBase::object(&db, &db.program_environment())
        ]
    );
    Ok(())
}

#[test]
fn specialized_cycles_preserve_the_requested_root() -> anyhow::Result<()> {
    let db = database("class A[T](A[T]): ...\ninteger = A[int]\ntext = A[str]\n")?;
    let owner = class(&db, "A")?;
    let file = system_path_to_file(&db, "/src/declaration_mro.pyi")?;
    for name in ["integer", "text"] {
        let Type::GenericAlias(alias) = global_symbol(&db, db.program_file(file), name)
            .place
            .expect_type()
        else {
            anyhow::bail!("missing alias {name}");
        };
        let Err(error) = owner.try_mro(&db, Some(alias.specialization(&db))) else {
            anyhow::bail!("specialized cycle produced a successful MRO");
        };
        assert!(error.is_cycle());
        assert_eq!(
            &error.fallback_mro()[..],
            [
                ClassBase::Class(ClassType::Generic(alias)),
                ClassBase::unknown(),
                ClassBase::object(&db, &db.program_environment())
            ]
        );
    }
    Ok(())
}

#[test]
fn starred_declaration_cycle_expands_the_seed_base_list() -> anyhow::Result<()> {
    for (bases_first, installed) in [(false, false), (true, false), (false, true), (true, true)] {
        let db = database("class Plain: ...\nclass Expands(*(Expands.a, Plain)): ...\n")?;
        db.clone().clear_salsa_events();
        let owner = class(&db, "Expands")?;
        let evaluate = || {
            if bases_first {
                owner.explicit_bases(&db);
            }
            owner.try_mro(&db, None)
        };
        let result = if installed {
            expansion_probe::run_mro(&db, 100_000, evaluate)
                .0
                .map_err(|error| anyhow::anyhow!("installed starred cycle: {error:?}"))?
        } else {
            evaluate()
        };
        let result = result.map_err(|error| {
            anyhow::anyhow!("expanded declaration retained an error: {error:?}")
        })?;
        let events = db.clone().take_salsa_events();
        let bases = owner.explicit_bases(&db);
        assert_eq!(bases.len(), 2);
        let recursive_base = if bases_first {
            ClassBase::Divergent(
                bases[0]
                    .as_divergent()
                    .ok_or_else(|| anyhow::anyhow!("missing divergent base"))?,
            )
        } else {
            ClassBase::unknown()
        };
        assert_eq!(
            &result[..],
            [
                ClassBase::Class(ClassType::NonGeneric(owner.into())),
                recursive_base,
                ClassBase::Class(ClassType::NonGeneric(class(&db, "Plain")?.into())),
                ClassBase::object(&db, &db.program_environment()),
            ],
        );
        let iterations: Vec<_> = events
            .iter()
            .filter_map(|event| match event.kind {
                salsa::EventKind::WillIterateCycle { database_key, .. } => Some(
                    db.ingredient_debug_name(database_key.ingredient_index())
                        .into_owned(),
                ),
                _ => None,
            })
            .collect();
        let expected = if bases_first {
            "explicit_bases_inner"
        } else {
            "try_mro_unspecialized"
        };
        assert!(
            iterations.iter().any(|name| name.contains(expected)),
            "no {expected} cycle refinement: {iterations:?}"
        );
    }
    Ok(())
}

#[test]
fn legacy_generic_cycles_preserve_specialized_dependencies() -> anyhow::Result<()> {
    for (source, installed) in [
        "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass A(Generic[T], A[T]): ...\n",
        "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass A(Generic[T], B[T]): ...\nclass B(Generic[T], A[T]): ...\n",
        "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass A(Generic[T], A[list[T]]): ...\n",
    ].into_iter().flat_map(|source| [false, true].map(|installed| (source, installed))) {
        let db = database(source)?;
        let owner = class(&db, "A")?;
        let result = if installed {
            expansion_probe::run_mro(&db, 100_000, || owner.try_mro(&db, None))
                .0
                .map_err(|error| anyhow::anyhow!("installed legacy cycle: {error:?}"))?
        } else {
            owner.try_mro(&db, None)
        };
        let Err(error) = result else {
            anyhow::bail!("legacy cycle produced a successful MRO: {source}");
        };
        assert!(error.is_cycle());
        assert_eq!(
            &error.fallback_mro()[..],
            [
                ClassBase::Class(owner.apply_optional_specialization(&db, None)),
                ClassBase::unknown(),
                ClassBase::object(&db, &db.program_environment()),
            ],
        );
    }
    Ok(())
}

#[test]
fn exact_source_aliases_are_shared_and_revalidated_after_edits() -> anyhow::Result<()> {
    const SOURCE: &str = "class Extra: ...\nclass Base[T]: ...\nclass First(Base[int]): ...\nclass Second(Base[int]): ...\n";
    let mut db = database(SOURCE)?;
    let first = class(&db, "First")?;
    let second = class(&db, "Second")?;
    db.clone().clear_salsa_events();
    let first_mro = first
        .try_mro(&db, None)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(first_mro.len(), 4);
    assert_eq!(take_alias_executions(&db), 1);

    let second_mro = second
        .try_mro(&db, None)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(&first_mro[1..], &second_mro[1..]);
    assert_eq!(take_alias_executions(&db), 0);
    assert_eq!(first.try_mro(&db, None).ok(), Some(first_mro));
    assert_eq!(take_alias_executions(&db), 0);

    db.write_file(
        "/src/declaration_mro.pyi",
        SOURCE.replace("Base[T]:", "Base[T](Extra):"),
    )?;
    let first = class(&db, "First")?;
    let result = first
        .try_mro(&db, None)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(result.len(), 5);
    assert_eq!(
        result[2],
        ClassBase::Class(ClassType::NonGeneric(class(&db, "Extra")?.into()))
    );
    assert!(take_alias_executions(&db) > 0);
    Ok(())
}

fn take_alias_executions(db: &TestDb) -> usize {
    db.clone()
        .take_salsa_events()
        .iter()
        .filter(|event| match event.kind {
            salsa::EventKind::WillExecute { database_key } => db
                .ingredient_debug_name(database_key.ingredient_index())
                .contains("source_alias_mro"),
            _ => false,
        })
        .count()
}

#[test]
fn source_inherited_context_keeps_nested_callable_variables() -> anyhow::Result<()> {
    let source = "from typing import Callable, Generic, TypeVar\n\
                  T = TypeVar('T')\n\
                  class Base(Generic[T]): ...\n\
                  class Owner(Base[Callable[[T], T]]): ...\n";
    for installed in [false, true] {
        let db = database(source)?;
        let owner = class(&db, "Owner")?;
        let context = if installed {
            expansion_probe::run_mro(&db, 100_000, || owner.generic_context(&db))
                .0
                .map_err(|error| anyhow::anyhow!("installed declaration: {error:?}"))?
        } else {
            owner.generic_context(&db)
        };
        let context = context.ok_or_else(|| anyhow::anyhow!("missing inherited context"))?;
        assert_eq!(
            context
                .variables(&db)
                .map(|variable| variable.name(&db).as_str())
                .collect::<Vec<_>>(),
            ["T"],
        );
    }
    Ok(())
}

#[test]
fn warm_source_alias_does_not_exempt_a_generated_request() -> anyhow::Result<()> {
    let db = database("class Base[T]: ...\nclass First(Base[int]): ...\n")?;
    let first = class(&db, "First")?;
    let mro = first
        .try_mro(&db, None)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let ClassBase::Class(ClassType::Generic(alias)) = mro[1] else {
        anyhow::bail!("missing specialized base");
    };
    assert_eq!(take_alias_executions(&db), 1);

    let (result, _) = expansion_probe::run_mro(&db, 0, || alias.try_mro(&db));
    assert!(matches!(result, Err(Incomplete::Allowance)), "{result:?}");
    let events = db.clone().take_salsa_events();
    let key = generic_alias_try_mro_ingredient(&db).database_key_index(alias.as_id());
    assert!(events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    }));

    for _ in 0..2 {
        let (result, _) = expansion_probe::run_mro(&db, 10_000, || alias.try_mro(&db));
        let result = result
            .map_err(|error| anyhow::anyhow!("incomplete retry: {error:?}"))?
            .map_err(|error| anyhow::anyhow!("semantic error: {error:?}"))?;
        assert_eq!(&result[..], &mro[1..]);
        assert_eq!(take_alias_executions(&db), 0);
    }
    Ok(())
}
