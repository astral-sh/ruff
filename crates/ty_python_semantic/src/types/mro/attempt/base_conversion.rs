//! Stored base conversions and dependency refusals under an installed attempt.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use salsa::prepared_source_probe::Stamp;

use super::{AttemptMroEffects, UnsupportedMroOperation};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mro::construction::{StaticMroWork, SynchronousStaticMroEffects};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DynamicType, InternedType, KnownInstanceType,
    RecursivelyDefined, SpecialFormType, StaticClassLiteral, Type, TypingModule, UnionType,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file(
            "/src/bases.py",
            "from typing import Generic, Protocol, TypeVar\nT = TypeVar('T')\nclass GenericBase(Generic[T]): ...\nclass ProtocolBase(Protocol[T]): ...\nclass Specialized(GenericBase[int]): ...\n",
        )
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = system_path_to_file(db, "/src/bases.py")?;
    global_symbol(db, db.program_file(file), name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}

fn assert_no_dependency_work(db: &TestDb) {
    for event in db.clone().take_salsa_events() {
        assert!(
            !matches!(
                event.kind,
                salsa::EventKind::WillExecute { .. } | salsa::EventKind::DidInternValue { .. }
            ),
            "{event:?}"
        );
    }
}

#[test]
fn stored_base_conversion_needs_no_queries_or_interning() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let owner = class(&db, "GenericBase")?;
    let generic = owner.explicit_bases(&db)[0];
    let protocol = class(&db, "ProtocolBase")?.explicit_bases(&db)[0];
    let specialized = class(&db, "Specialized")?.explicit_bases(&db)[0];
    assert!(matches!(
        generic,
        Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_))
    ));
    assert!(matches!(
        protocol,
        Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_))
    ));
    let Type::GenericAlias(alias) = specialized else {
        anyhow::bail!("expected a specialized base")
    };
    for (ty, expected) in [
        (generic, Some(ClassBase::Generic)),
        (protocol, Some(ClassBase::Protocol)),
        (
            specialized,
            Some(ClassBase::Class(ClassType::Generic(alias))),
        ),
        (Type::any(), Some(ClassBase::Dynamic(DynamicType::Any))),
        (Type::Never, Some(ClassBase::unknown())),
        (
            Type::SpecialForm(SpecialFormType::Any),
            Some(ClassBase::Any),
        ),
        (
            Type::SpecialForm(SpecialFormType::Protocol),
            Some(ClassBase::Protocol),
        ),
        (
            Type::SpecialForm(SpecialFormType::Generic),
            Some(ClassBase::Generic),
        ),
        (
            Type::SpecialForm(SpecialFormType::TypedDict(TypingModule::Typing)),
            Some(ClassBase::TypedDict(TypingModule::Typing)),
        ),
        (Type::SpecialForm(SpecialFormType::Union), None),
        (Type::int_literal(1), None),
        (Type::AlwaysTruthy, None),
    ] {
        db.clone().clear_salsa_events();
        let (outcome, _) = expansion_probe::run_mro(&db, 1, || {
            let effects = AttemptMroEffects::new(&db);
            effects.checkpoint(StaticMroWork::ConvertBase { index: 0 })?;
            effects.converted_explicit_base(&env, owner, 0, ty)
        });
        assert_eq!(outcome, Ok(Ok(expected)));
        assert_no_dependency_work(&db);
        assert_eq!(
            ClassBase::try_from_explicit_base(&db, &env, ty, Some(owner.into())),
            expected
        );
        assert_no_dependency_work(&db);
    }
    Ok(())
}

#[test]
fn conversion_dependencies_refuse_before_execution_and_do_not_become_invalid_bases()
-> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let owner = class(&db, "GenericBase")?;
    let inputs = [
        Type::Union(UnionType::new(
            &db,
            [Type::SpecialForm(SpecialFormType::Any), Type::any()].as_slice(),
            RecursivelyDefined::No,
        )),
        Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
            &db,
            Type::any(),
        ))),
        Type::SpecialForm(SpecialFormType::Tuple),
    ];
    let stamp = Stamp::current(&db);
    for ty in inputs {
        db.clone().clear_salsa_events();
        for _ in 0..3 {
            let consumer = Cell::new(false);
            let (outcome, _) = expansion_probe::run_mro(&db, 1, || {
                let effects = AttemptMroEffects::new(&db);
                effects.checkpoint(StaticMroWork::ConvertBase { index: 0 })?;
                let base = effects.converted_explicit_base(&env, owner, 0, ty)?;
                consumer.set(true);
                Ok::<_, Incomplete>(base)
            });
            assert_eq!(
                outcome,
                Err(Incomplete::UnsupportedMroOperation(
                    UnsupportedMroOperation::BaseConversion
                ))
            );
            assert!(!consumer.get());
            assert_no_dependency_work(&db);
            assert_eq!(Stamp::current(&db), stamp);
        }
    }
    assert_eq!(
        ClassBase::try_from_explicit_base(&db, &env, inputs[0], None),
        Some(ClassBase::Dynamic(DynamicType::Any))
    );
    assert_eq!(
        ClassBase::try_from_explicit_base(&db, &env, inputs[1], None),
        Some(ClassBase::Dynamic(DynamicType::Any))
    );
    Ok(())
}

#[test]
fn stopped_conversion_cannot_publish_a_stored_result() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let owner = class(&db, "GenericBase")?;
    for prespend in [0, 7] {
        let consumer = Cell::new(false);
        let (outcome, _) = expansion_probe::run_mro(&db, prespend, || {
            expansion_probe::charge_work(&db, prespend)?;
            let effects = AttemptMroEffects::new(&db);
            effects.checkpoint(StaticMroWork::ConvertBase { index: 0 })?;
            let value = effects.converted_explicit_base(
                &env,
                owner,
                0,
                Type::SpecialForm(SpecialFormType::Protocol),
            )?;
            consumer.set(true);
            Ok::<_, Incomplete>(value)
        });
        assert_eq!(outcome, Err(Incomplete::Allowance));
        assert!(!consumer.get());
    }
    let (outcome, _) = expansion_probe::run_mro(&db, 10, || {
        expansion_probe::refuse(&db, Incomplete::Interrupted);
        let result = AttemptMroEffects::new(&db).converted_explicit_base(
            &env,
            owner,
            0,
            Type::SpecialForm(SpecialFormType::Protocol),
        );
        assert_eq!(result, Err(Incomplete::Interrupted));
    });
    assert_eq!(outcome, Err(Incomplete::Interrupted));
    Ok(())
}
