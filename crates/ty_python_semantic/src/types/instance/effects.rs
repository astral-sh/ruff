//! Semantic facts required to construct a class's instance type.
//!
//! Classification remains lazy: a `TypedDict` needs no protocol or nominal-inheritance facts.
//! Queued evaluation reports source-dependent facts explicitly until their queries are supervised.

use std::borrow::Cow;
use std::convert::Infallible;
use std::future::{Future, ready};

use crate::types::generics::Specialization;
use crate::types::signatures::effects::sealed;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InstanceWork {
    Dispatch,
    Publish,
}

pub(in crate::types) trait InstanceEffects<'db>: sealed::Sealed {
    type Error;

    async fn checkpoint(&self, _work: InstanceWork) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn class_literal_and_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Self::Error>;

    async fn known_class(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;

    async fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn is_protocol(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn inherits_from_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<TupleType<'db>, Self::Error>;
}

pub(super) struct LegacyInlineEffects;

impl sealed::Sealed for LegacyInlineEffects {}

impl<'db> InstanceEffects<'db> for LegacyInlineEffects {
    type Error = Infallible;

    fn class_literal_and_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Self::Error>>
    {
        ready(Ok(class.class_literal_and_specialization(db)))
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>> {
        ready(Ok(class.known(db)))
    }

    fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(class.is_typed_dict(db)))
    }

    fn is_protocol(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(class.is_protocol(db)))
    }

    fn inherits_from_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(class.inherits_from_explicit_any(db)))
    }

    fn tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> impl Future<Output = Result<TupleType<'db>, Self::Error>> {
        ready(Ok(TupleType::new(
            db,
            env,
            specialization
                .and_then(|spec| Some(Cow::Borrowed(spec.tuple(db)?)))
                .unwrap_or_else(|| Cow::Owned(TupleSpec::homogeneous(Type::unknown())))
                .as_ref(),
        )))
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(in crate::types) enum InstanceEffect {
    TypedDictClassification,
    ProtocolClassification,
    ExplicitAnyInheritance,
    TupleNormalization,
}

#[cfg(test)]
pub(in crate::types) struct QueuedInstanceEffects;

#[cfg(test)]
impl sealed::Sealed for QueuedInstanceEffects {}

#[cfg(test)]
impl<'db> InstanceEffects<'db> for QueuedInstanceEffects {
    type Error = InstanceEffect;

    fn class_literal_and_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Self::Error>>
    {
        ready(Ok(class.class_literal_and_specialization(db)))
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>> {
        ready(Ok(class.known(db)))
    }

    fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(
            class
                .is_typed_dict_without_inference(db)
                .ok_or(InstanceEffect::TypedDictClassification),
        )
    }

    fn is_protocol(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(
            class
                .is_protocol_without_inference(db)
                .ok_or(InstanceEffect::ProtocolClassification),
        )
    }

    fn inherits_from_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(
            class
                .inherits_from_explicit_any_without_inference(db)
                .ok_or(InstanceEffect::ExplicitAnyInheritance),
        )
    }

    fn tuple(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> impl Future<Output = Result<TupleType<'db>, Self::Error>> {
        ready(Err(InstanceEffect::TupleNormalization))
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::types::{ClassType, KnownClass};

    fn fixture() -> anyhow::Result<TestDb> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/instances.py",
            r#"
            from typing import Any, Protocol, TypedDict

            class Plain: ...
            class Generic[T]: ...
            class Interface(Protocol):
                def method(self) -> int: ...
            class Record(TypedDict):
                value: int
            class FromAny(Any): ...

            Specialized = Generic[int]
            SpecializedTuple = tuple[int, ...]
            "#,
        )?;
        Ok(db)
    }

    fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassType<'db>> {
        let env = db.program_environment();
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/instances.py")?,
            env.program(db),
        );
        match global_symbol(db, file, name).place.expect_type() {
            Type::ClassLiteral(class) => Ok(ClassType::NonGeneric(class)),
            Type::GenericAlias(alias) => Ok(ClassType::Generic(alias)),
            other => anyhow::bail!("expected a class for {name}, got {other:?}"),
        }
    }

    #[test]
    fn queued_instance_preserves_stored_class_specialization() -> anyhow::Result<()> {
        let db = fixture()?;
        let env = db.program_environment();
        for name in ["Plain", "Specialized"] {
            let class = class(&db, name)?;
            let result = try_poll_immediate(Type::instance_with(
                &db,
                &env,
                &QueuedInstanceEffects,
                class,
            ));
            let Poll::Ready(Ok(Type::NominalInstance(instance))) = result else {
                anyhow::bail!("expected a completed nominal instance for {name}: {result:?}");
            };
            assert_eq!(instance.class(&db, &env), class);
            assert!(!instance.inherits_from_explicit_any());
            assert_eq!(
                Type::NominalInstance(instance),
                Type::instance(&db, &env, class)
            );
        }

        let known_protocol = KnownClass::SupportsIndex
            .try_to_class_literal(&db, &env)
            .ok_or_else(|| anyhow::anyhow!("missing SupportsIndex"))?;
        let class = ClassType::NonGeneric(known_protocol.into());
        assert_eq!(
            try_poll_immediate(Type::instance_with(
                &db,
                &env,
                &QueuedInstanceEffects,
                class
            )),
            Poll::Ready(Ok(Type::instance(&db, &env, class))),
        );
        Ok(())
    }

    #[test]
    fn queued_instance_boundaries_do_not_depend_on_warm_queries() -> anyhow::Result<()> {
        let db = fixture()?;
        let env = db.program_environment();
        for name in ["Interface", "Record", "FromAny"] {
            let class = class(&db, name)?;
            let ClassLiteral::Static(literal) = class.class_literal(&db) else {
                anyhow::bail!("expected a static class for {name}");
            };
            for warm in [false, true] {
                if warm {
                    Type::instance(&db, &env, class);
                }
                assert_eq!(
                    try_poll_immediate(Type::instance_with(
                        &db,
                        &env,
                        &QueuedInstanceEffects,
                        class,
                    )),
                    Poll::Ready(Err(InstanceEffect::TypedDictClassification)),
                    "{name}, warm={warm}",
                );
                assert_eq!(
                    try_poll_immediate(QueuedInstanceEffects.is_protocol(&db, literal)),
                    Poll::Ready(Err(InstanceEffect::ProtocolClassification)),
                );
                assert_eq!(
                    try_poll_immediate(
                        QueuedInstanceEffects.inherits_from_explicit_any(&db, literal.into()),
                    ),
                    Poll::Ready(Err(InstanceEffect::ExplicitAnyInheritance)),
                );
            }
        }

        let tuple = KnownClass::Tuple
            .try_to_class_literal(&db, &env)
            .ok_or_else(|| anyhow::anyhow!("missing tuple"))?;
        for class in [
            ClassType::NonGeneric(tuple.into()),
            class(&db, "SpecializedTuple")?,
        ] {
            assert_eq!(
                try_poll_immediate(Type::instance_with(
                    &db,
                    &env,
                    &QueuedInstanceEffects,
                    class
                )),
                Poll::Ready(Err(InstanceEffect::TupleNormalization)),
            );
        }
        Ok(())
    }

    struct CompletedClassFacts {
        typed_dict: bool,
        protocol: Result<bool, InstanceEffect>,
        inherits_explicit_any: Result<bool, InstanceEffect>,
    }

    impl sealed::Sealed for CompletedClassFacts {}

    impl<'db> InstanceEffects<'db> for CompletedClassFacts {
        type Error = InstanceEffect;

        fn class_literal_and_specialization(
            &self,
            db: &'db dyn Db,
            class: ClassType<'db>,
        ) -> impl Future<Output = Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Self::Error>>
        {
            ready(Ok(class.class_literal_and_specialization(db)))
        }

        fn known_class(
            &self,
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>> {
            ready(Ok(class.known(db)))
        }

        fn is_typed_dict(
            &self,
            _db: &'db dyn Db,
            _class: StaticClassLiteral<'db>,
        ) -> impl Future<Output = Result<bool, Self::Error>> {
            ready(Ok(self.typed_dict))
        }

        fn is_protocol(
            &self,
            _db: &'db dyn Db,
            _class: StaticClassLiteral<'db>,
        ) -> impl Future<Output = Result<bool, Self::Error>> {
            ready(self.protocol)
        }

        fn inherits_from_explicit_any(
            &self,
            _db: &'db dyn Db,
            _class: ClassLiteral<'db>,
        ) -> impl Future<Output = Result<bool, Self::Error>> {
            ready(self.inherits_explicit_any)
        }

        fn tuple(
            &self,
            _db: &'db dyn Db,
            _env: &ProgramEnvironment<'db>,
            _specialization: Option<Specialization<'db>>,
        ) -> impl Future<Output = Result<TupleType<'db>, Self::Error>> {
            ready(Err(InstanceEffect::TupleNormalization))
        }
    }

    #[test]
    fn instance_uses_only_the_required_completed_facts() -> anyhow::Result<()> {
        let db = fixture()?;
        let env = db.program_environment();
        for (name, facts) in [
            (
                "Record",
                CompletedClassFacts {
                    typed_dict: true,
                    protocol: Err(InstanceEffect::ProtocolClassification),
                    inherits_explicit_any: Err(InstanceEffect::ExplicitAnyInheritance),
                },
            ),
            (
                "Interface",
                CompletedClassFacts {
                    typed_dict: false,
                    protocol: Ok(true),
                    inherits_explicit_any: Err(InstanceEffect::ExplicitAnyInheritance),
                },
            ),
            (
                "FromAny",
                CompletedClassFacts {
                    typed_dict: false,
                    protocol: Ok(false),
                    inherits_explicit_any: Ok(true),
                },
            ),
        ] {
            let class = class(&db, name)?;
            let expected = Type::instance(&db, &env, class);
            assert_eq!(
                try_poll_immediate(Type::instance_with(&db, &env, &facts, class)),
                Poll::Ready(Ok(expected)),
                "{name}",
            );
            if name == "FromAny" {
                assert!(
                    expected
                        .as_nominal_instance()
                        .is_some_and(|instance| { instance.inherits_from_explicit_any() })
                );
            }
        }
        Ok(())
    }
}
