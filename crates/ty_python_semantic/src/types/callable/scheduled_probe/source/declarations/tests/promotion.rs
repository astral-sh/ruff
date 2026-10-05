use super::*;
use crate::types::enums::is_single_member_enum;
use crate::types::{GenericAlias, TypeVarVariance};
use salsa::prepared_source_probe::Status;

#[test]
fn promotion_facts_cover_forwarded_nominal_origins() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        let source = format!(
            "{}\ndef scalar_list(value: list[int]) -> None: ...\n",
            code(markdown, CASES[2])?
        );
        db.write_file("/src/promotion.py", source)?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/promotion.py")?;
        let file = ProgramFile::new(&db, file, env.program(&db));
        let prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let initializer = class_named(&prepared, "Initializer");
        for variable in prepared
            .context(initializer)
            .unwrap()
            .unwrap()
            .variables(&db)
        {
            assert_eq!(
                prepared.promotion_variance(variable).unwrap(),
                variable.variance(&db)
            );
        }

        let function = prepared.globals["scalar_list"]
            .value
            .place
            .expect_type()
            .as_function_literal()
            .unwrap();
        let parameter = prepared.signature(function).unwrap().overloads[0]
            .parameters()
            .get_positional(0)
            .unwrap()
            .annotated_type();
        let Type::NominalInstance(instance) = parameter else {
            panic!("expected a list instance");
        };
        let list = instance.class(&db, &env).into_generic_alias().unwrap();
        let context = list.specialization(&db).generic_context(&db);
        let variable = context.variables(&db).next().unwrap();
        assert_eq!(
            prepared.promotion_variance(variable).unwrap(),
            variable.variance(&db)
        );
        assert_eq!(list.origin(&db).known(&db), Some(KnownClass::List));
        assert!(!prepared.classes.contains_key(&list.origin(&db)));

        let counts = (
            prepared.promotion.variances.len(),
            prepared.promotion.enum_singletons.len(),
            prepared.promotion.scalar_fallbacks.len(),
        );
        let mut receiver = prepared.promotion_scalar_fallback(KnownClass::Str).unwrap();
        for _ in 0..64 {
            receiver = Type::instance(
                &db,
                &env,
                ClassType::Generic(GenericAlias::new(
                    &db,
                    list.origin(&db),
                    context.specialize(&db, [receiver].as_slice()),
                )),
            );
            assert_eq!(
                prepared.promotion_variance(variable).unwrap(),
                TypeVarVariance::Invariant
            );
        }
        assert!(matches!(receiver, Type::NominalInstance(_)));
        assert_eq!(
            counts,
            (
                prepared.promotion.variances.len(),
                prepared.promotion.enum_singletons.len(),
                prepared.promotion.scalar_fallbacks.len(),
            )
        );
    }
    Ok(())
}

#[test]
fn promotion_variance_retains_binding_context_and_owner_support() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/variance.py",
        r#"
from typing import Generic
from typing_extensions import TypeVar

T = TypeVar("T", infer_variance=True)
Cov = TypeVar("Cov", covariant=True)

class Producer(Generic[T]):
    def get(self) -> T: ...

class Consumer(Generic[T]):
    def put(self, value: T) -> None: ...

class Explicit(Generic[Cov]): ...
"#,
    )?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/variance.py")?;
    let file = ProgramFile::new(&db, file, env.program(&db));
    let prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let variable = |name| {
        prepared
            .context(class_named(&prepared, name))
            .unwrap()
            .unwrap()
            .variables(&db)
            .next()
            .unwrap()
    };
    let producer = variable("Producer");
    let consumer = variable("Consumer");
    let explicit = variable("Explicit");
    assert_eq!(producer.typevar(&db), consumer.typevar(&db));
    assert_ne!(producer, consumer);
    assert_ne!(producer.binding_context(&db), consumer.binding_context(&db));
    for variable in [producer, consumer, explicit] {
        assert_eq!(
            prepared.promotion_variance(variable).unwrap(),
            variable.variance(&db)
        );
        let fact = &prepared.promotion.variances[&variable];
        assert!(!fact.support.is_empty());
        assert!(fact.stamp.belongs_to(&db));
        assert!(
            fact.support
                .iter()
                .all(|read| read.parent.is_none() && read.status == Status::Final)
        );
    }
    assert_eq!(
        prepared.promotion_variance(producer).unwrap(),
        TypeVarVariance::Covariant
    );
    assert_eq!(
        prepared.promotion_variance(consumer).unwrap(),
        TypeVarVariance::Contravariant
    );
    let captured = probe::capture(&db, || explicit.variance(&db)).unwrap();
    assert_eq!(captured.value, TypeVarVariance::Covariant);
    assert!(captured.reads.is_empty());

    let warm = PreparedDeclarations::prepare(&db, file).unwrap();
    assert_eq!(
        warm.promotion.variances.len(),
        prepared.promotion.variances.len()
    );
    for variable in [producer, consumer, explicit] {
        assert_eq!(
            warm.promotion_variance(variable).unwrap(),
            prepared.promotion_variance(variable).unwrap()
        );
    }
    Ok(())
}

#[test]
fn promotion_facts_preserve_negative_results_and_exact_missing_keys() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/facts.py",
        r#"
from enum import Enum

class One(Enum):
    MEMBER = 1

class Many(Enum):
    FIRST = 1
    SECOND = 2

class Box[T]:
    value: T
"#,
    )?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/facts.py")?;
    let file = ProgramFile::new(&db, file, env.program(&db));
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let one = ClassLiteral::Static(class_named(&prepared, "One"));
    let many = ClassLiteral::Static(class_named(&prepared, "Many"));
    let variable = prepared
        .context(class_named(&prepared, "Box"))
        .unwrap()
        .unwrap()
        .variables(&db)
        .next()
        .unwrap();
    for (class, expected) in [(one, true), (many, false)] {
        assert_eq!(is_single_member_enum(&db, class), expected);
        assert_eq!(prepared.promotion_enum_singleton(class).unwrap(), expected);
        assert!(
            !prepared.promotion.enum_singletons[&class]
                .support
                .is_empty()
        );
    }
    for known in [
        KnownClass::Str,
        KnownClass::Bool,
        KnownClass::Int,
        KnownClass::Bytes,
    ] {
        assert_eq!(
            prepared.promotion_scalar_fallback(known).unwrap(),
            known.to_instance(&db, &env)
        );
        assert!(
            !prepared.promotion.scalar_fallbacks[&known]
                .support
                .is_empty()
        );
    }
    let variance = prepared.promotion_variance(variable).unwrap();
    let scalar = prepared.promotion_scalar_fallback(KnownClass::Int).unwrap();
    let successful = probe::capture(&db, || {
        assert_eq!(prepared.promotion_variance(variable).unwrap(), variance);
        assert!(prepared.promotion_enum_singleton(one).unwrap());
        assert!(!prepared.promotion_enum_singleton(many).unwrap());
        assert_eq!(
            prepared.promotion_scalar_fallback(KnownClass::Int).unwrap(),
            scalar
        );
    })
    .unwrap();
    assert!(successful.reads.is_empty());

    prepared.promotion.variances.remove(&variable);
    prepared.promotion.enum_singletons.remove(&one);
    prepared.promotion.scalar_fallbacks.remove(&KnownClass::Int);
    for _ in 0..2 {
        let missing = probe::capture(&db, || {
            assert_eq!(
                prepared.promotion_variance(variable).unwrap_err(),
                PromotionFactKey::Variance(variable)
            );
            assert_eq!(
                prepared.promotion_enum_singleton(one).unwrap_err(),
                PromotionFactKey::EnumSingleton(one)
            );
            assert_eq!(
                prepared
                    .promotion_scalar_fallback(KnownClass::Int)
                    .unwrap_err(),
                PromotionFactKey::ScalarFallback(KnownClass::Int)
            );
            assert!(!prepared.promotion_enum_singleton(many).unwrap());
        })
        .unwrap();
        assert!(missing.reads.is_empty());
    }
    Ok(())
}
