use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::MroIterator;
use super::root::{
    MroRootEffects, MroRootFacts, MroRootWork, MroTailRequest, mro_first_with,
    mro_tail_request_with, sealed,
};
#[cfg(feature = "experimental-analysis")]
use super::{DuplicateBaseError, StaticMroErrorKind};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class_base::ClassBase;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::Specialization;
use crate::types::mro::attempt::AttemptMroEffects;
use crate::types::mro::root::apply_optional_class_specialization_sync;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::source_read::read_source;
use crate::types::tuple::TupleType;
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, GenericContext, KnownClass, StaticClassLiteral, Type,
};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/mro.py",
        r#"
from enum import Enum
from typing import NamedTuple, TypedDict

class A: ...
class B(A): ...
class C(B): ...
class Generic[T]: ...
Dynamic = type("Dynamic", (), {})
Named = NamedTuple("Named", [("value", int)])
Typed = TypedDict("Typed", {"value": int})
Enumeration = Enum("Enumeration", {"VALUE": 1})
"#,
    )?;
    Ok(db)
}

fn class<'db>(db: &'db TestDb, name: &str) -> ClassLiteral<'db> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/mro.py").unwrap(),
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .unwrap()
}

#[cfg(feature = "experimental-analysis")]
#[test]
fn cached_error_retirement_includes_nested_duplicate_indices() -> anyhow::Result<()> {
    let db = database()?;
    let class = ClassType::NonGeneric(class(&db, "A"));
    let cases = [
        (StaticMroErrorKind::InheritanceCycle, 7),
        (StaticMroErrorKind::Pep695ClassWithGenericInheritance, 7),
        (
            StaticMroErrorKind::InvalidBases(Box::new([
                (0, Type::unknown()),
                (1, Type::unknown()),
            ])),
            9,
        ),
        (
            StaticMroErrorKind::UnresolvableMro {
                bases_list: Box::new([Type::unknown(), Type::unknown()]),
                generic_index: None,
            },
            10,
        ),
        (
            StaticMroErrorKind::DuplicateBases(Box::new([
                DuplicateBaseError {
                    duplicate_base: ClassBase::Any,
                    first_index: 0,
                    later_indices: Box::new([1, 2, 3]),
                },
                DuplicateBaseError {
                    duplicate_base: ClassBase::Any,
                    first_index: 4,
                    later_indices: Box::new([5]),
                },
            ])),
            17,
        ),
    ];
    for (kind, expected) in cases {
        let error = kind.into_mro_error_with_object(class, ClassBase::Any);
        // All error kinds retain three fallback entries, with Any standing in for object.
        assert_eq!(
            &error.fallback_mro()[..],
            [
                ClassBase::Class(class),
                ClassBase::unknown(),
                ClassBase::Any
            ],
        );
        assert_eq!(error.retirement_work(), Some(expected));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
enum Call<'db> {
    Context(StaticClassLiteral<'db>),
    Default(StaticClassLiteral<'db>),
    Normalize(Specialization<'db>),
}

struct PreparedLikeRoots<'db> {
    db: &'db dyn Db,
    contexts: Vec<(StaticClassLiteral<'db>, Option<GenericContext<'db>>)>,
    calls: RefCell<Vec<Call<'db>>>,
    allow_default: bool,
}

impl<'db> PreparedLikeRoots<'db> {
    fn new(
        db: &'db dyn Db,
        contexts: Vec<(StaticClassLiteral<'db>, Option<GenericContext<'db>>)>,
    ) -> Self {
        Self {
            db,
            contexts,
            calls: RefCell::new(Vec::new()),
            allow_default: false,
        }
    }
}

impl sealed::Sealed for PreparedLikeRoots<'_> {}

impl<'db> MroRootFacts<'db> for PreparedLikeRoots<'db> {
    type Error = &'static str;
}

impl<'db> MroRootEffects<'db> for PreparedLikeRoots<'db> {
    async fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        self.calls.borrow_mut().push(Call::Context(class));
        self.contexts
            .iter()
            .find(|(key, _)| *key == class)
            .map(|(_, context)| *context)
            .ok_or("missing context")
    }

    async fn checkpoint(&self, _work: MroRootWork) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.calls.borrow_mut().push(Call::Default(class));
        if self.generic_context(class).await?.is_none() {
            return Ok(ClassType::NonGeneric(class.into()));
        }
        if self.allow_default {
            Ok(class.default_specialization(self.db))
        } else {
            Err("default specialization")
        }
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.calls
            .borrow_mut()
            .push(Call::Normalize(specialization));
        if specialization.tuple(self.db).is_some() {
            Err("tuple runtime specialization")
        } else {
            Ok(specialization.tuple_runtime_element_specialization(self.db))
        }
    }
}

#[test]
fn static_first_entries_distinguish_missing_context_default_and_supplied_specialization()
-> anyhow::Result<()> {
    let db = database()?;
    let plain = class(&db, "A").as_static().unwrap();
    let generic = class(&db, "Generic").as_static().unwrap();
    let context = generic.generic_context(&db).unwrap();
    let supplied = context.specialize(&db, [Type::int_literal(7)].as_slice());
    let effects = PreparedLikeRoots::new(&db, vec![(plain, None), (generic, Some(context))]);

    // A supplied specialization does not make a non-generic class generic.
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            plain.into(),
            Some(supplied),
            &effects
        )),
        Poll::Ready(Ok(ClassBase::Class(ClassType::NonGeneric(plain.into())))),
    );
    assert_eq!(*effects.calls.borrow(), [Call::Context(plain)]);
    effects.calls.borrow_mut().clear();
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            generic.into(),
            Some(supplied),
            &effects
        )),
        Poll::Ready(Ok(ClassBase::Class(ClassType::Generic(GenericAlias::new(
            &db, generic, supplied
        ))))),
    );
    assert_eq!(*effects.calls.borrow(), [Call::Context(generic)]);
    assert_eq!(
        MroIterator::new(&db, generic.into(), Some(supplied)).next(),
        Some(ClassBase::Class(ClassType::Generic(GenericAlias::new(
            &db, generic, supplied
        )))),
    );
    effects.calls.borrow_mut().clear();
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            generic.into(),
            None,
            &effects
        )),
        Poll::Ready(Err("default specialization")),
    );
    assert_eq!(
        *effects.calls.borrow(),
        [Call::Default(generic), Call::Context(generic)]
    );

    let missing = PreparedLikeRoots::new(&db, Vec::new());
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            generic.into(),
            Some(supplied),
            &missing
        )),
        Poll::Ready(Err("missing context")),
    );
    assert_eq!(*missing.calls.borrow(), [Call::Context(generic)]);
    Ok(())
}

#[test]
fn first_entry_defaults_do_not_replace_the_original_tail_request() -> anyhow::Result<()> {
    let db = database()?;
    let generic = class(&db, "Generic").as_static().unwrap();
    let context = generic.generic_context(&db).unwrap();
    let mut effects = PreparedLikeRoots::new(&db, vec![(generic, Some(context))]);
    effects.allow_default = true;
    let expected = GenericAlias::new(&db, generic, context.default_specialization(&db, None));
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            generic.into(),
            None,
            &effects
        )),
        Poll::Ready(Ok(ClassBase::Class(ClassType::Generic(expected)))),
    );
    assert_eq!(
        MroIterator::new(&db, generic.into(), None).next(),
        Some(ClassBase::Class(ClassType::Generic(expected)))
    );
    effects.calls.borrow_mut().clear();
    let Poll::Ready(Ok(MroTailRequest::Static(root, specialization))) =
        try_poll_immediate(mro_tail_request_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            generic.into(),
            None,
            &effects,
        ))
    else {
        panic!("the original unspecialized tail must remain available");
    };
    assert_eq!(root, generic);
    assert_eq!(specialization, None);
    assert!(effects.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn declaration_defaults_agree_across_public_and_direct_root_entries() -> anyhow::Result<()> {
    const SOURCE: &str = r#"
from typing import Generic, ParamSpec, TypeVar, TypeVarTuple, Unpack
class Plain: ...
class Dependent[T = int, U = list[T]]: ...
class MissingParams[**P]: ...
class DefaultParams[**P = [int]]: ...
class MissingTuple[*Ts]: ...
class DefaultTuple[*Ts = *tuple[int, str]]: ...
D = TypeVar("D", default=int)
E = TypeVar("E", default=list[D])
P = ParamSpec("P", default=[int])
Ts = TypeVarTuple("Ts", default=Unpack[tuple[int, str]])
class Legacy(Generic[D, E]): ...
class Inherited(Legacy[D, E]): ...
class LegacyParams(Generic[P]): ...
class LegacyTuple(Generic[*Ts]): ...
class Outer[T]:
    class Nested[U = T]: ...
Nested = Outer.Nested
BuiltinTuple = tuple
"#;
    for name in [
        "Plain",
        "Dependent",
        "MissingParams",
        "DefaultParams",
        "MissingTuple",
        "DefaultTuple",
        "Legacy",
        "Inherited",
        "LegacyParams",
        "LegacyTuple",
        "Nested",
        "BuiltinTuple",
    ] {
        let build = || {
            TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .with_file("/src/mro.py", SOURCE)
                .build()
        };
        let ordinary = build()?;
        let Some(owner) = class(&ordinary, name).as_static() else {
            anyhow::bail!("missing ordinary {name}")
        };
        owner.generic_context(&ordinary);
        ordinary.clone().clear_salsa_events();
        let expected = owner.default_specialization(&ordinary);
        let source_reads = |db: &TestDb| {
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
                .collect::<Vec<_>>()
        };
        let expected_reads = source_reads(&ordinary);
        let expected_display = Type::from(expected)
            .display(&ordinary, &ordinary.program_environment())
            .to_string();

        for route in 0..4 {
            let db = build()?;
            let Some(owner) = class(&db, name).as_static() else {
                anyhow::bail!("missing {name}")
            };
            let context = owner.generic_context(&db);
            db.clone().clear_salsa_events();
            let effects = AttemptMroEffects::new(&db);
            let (result, _) = expansion_probe::run_mro(&db, 100_000, || match route {
                0 => read_source(&effects, || owner.default_specialization(&db)).map(Some),
                1 => read_source(&effects, || owner.apply_optional_specialization(&db, None))
                    .map(Some),
                2 => apply_optional_class_specialization_sync(&db, owner, None, &effects).map(Some),
                _ => {
                    let mut roots = PreparedLikeRoots::new(&db, vec![(owner, context)]);
                    roots.allow_default = true;
                    match try_poll_immediate(mro_first_with(
                        crate::types::mro::field_reads::MroFieldReads::new(&db),
                        owner.into(),
                        None,
                        &roots,
                    )) {
                        Poll::Ready(Ok(ClassBase::Class(result))) => Ok(Some(result)),
                        _ => Ok(None),
                    }
                }
            });
            let actual = result
                .and_then(|result| result)
                .map_err(|error: Incomplete| anyhow::anyhow!("{name}, route {route}: {error:?}"))?
                .ok_or_else(|| {
                    anyhow::anyhow!("{name}, route {route}: missing immediate root result")
                })?;
            assert_eq!(source_reads(&db), expected_reads, "{name}, route {route}");
            assert_eq!(
                actual,
                owner.default_specialization(&db),
                "{name}, route {route}"
            );
            assert_eq!(
                Type::from(actual)
                    .display(&db, &db.program_environment())
                    .to_string(),
                expected_display
            );
            if let ClassType::Generic(alias) = actual {
                assert_eq!(
                    Some(alias.specialization(&db).generic_context(&db)),
                    context
                );
            }
        }
    }
    Ok(())
}

#[test]
fn tuple_payload_is_preserved_in_the_first_entry_and_rejected_only_at_the_tail()
-> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let tuple = KnownClass::Tuple.try_to_class_literal(&db, &env).unwrap();
    let context = tuple.generic_context(&db).unwrap();
    let shaped = context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::homogeneous(&db, &env, Type::int_literal(3)),
    );
    let effects = PreparedLikeRoots::new(&db, vec![(tuple, Some(context))]);
    assert_eq!(
        try_poll_immediate(mro_first_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            tuple.into(),
            Some(shaped),
            &effects
        )),
        Poll::Ready(Ok(ClassBase::Class(ClassType::Generic(GenericAlias::new(
            &db, tuple, shaped
        ))))),
    );
    assert_eq!(*effects.calls.borrow(), [Call::Context(tuple)]);
    effects.calls.borrow_mut().clear();
    assert!(matches!(
        try_poll_immediate(mro_tail_request_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            tuple.into(),
            Some(shaped),
            &effects
        )),
        Poll::Ready(Err("tuple runtime specialization")),
    ));
    assert_eq!(*effects.calls.borrow(), [Call::Normalize(shaped)]);

    let unshaped = context.specialize(&db, [Type::unknown()].as_slice());
    let Poll::Ready(Ok(MroTailRequest::Static(root, specialization))) =
        try_poll_immediate(mro_tail_request_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            tuple.into(),
            Some(unshaped),
            &effects,
        ))
    else {
        panic!("a specialization without tuple metadata uses the canonical fast path");
    };
    assert_eq!(root, tuple);
    assert_eq!(specialization, Some(unshaped));
    Ok(())
}

#[test]
fn dynamic_first_and_tail_requests_preserve_every_literal_kind() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["Dynamic", "Named", "Typed", "Enumeration"] {
        let literal = class(&db, name);
        let effects = PreparedLikeRoots::new(&db, Vec::new());
        assert_eq!(
            try_poll_immediate(mro_first_with(
                crate::types::mro::field_reads::MroFieldReads::new(&db),
                literal,
                None,
                &effects
            )),
            Poll::Ready(Ok(ClassBase::Class(ClassType::NonGeneric(literal)))),
        );
        let mut ordinary = MroIterator::new(&db, literal, None);
        assert_eq!(
            ordinary.next(),
            Some(ClassBase::Class(ClassType::NonGeneric(literal)))
        );
        assert!(ordinary.cursor.subsequent_elements.is_none());
        let Poll::Ready(Ok(tail)) = try_poll_immediate(mro_tail_request_with(
            crate::types::mro::field_reads::MroFieldReads::new(&db),
            literal,
            None,
            &effects,
        )) else {
            panic!("dynamic dispatch must produce its typed tail request");
        };
        match (literal, tail) {
            (ClassLiteral::Dynamic(expected), MroTailRequest::Dynamic(actual)) => {
                assert_eq!(actual, expected);
            }
            (
                ClassLiteral::DynamicNamedTuple(expected),
                MroTailRequest::DynamicNamedTuple(actual),
            ) => assert_eq!(actual, expected),
            (
                ClassLiteral::DynamicTypedDict(expected),
                MroTailRequest::DynamicTypedDict(actual),
            ) => assert_eq!(actual, expected),
            (ClassLiteral::DynamicEnum(expected), MroTailRequest::DynamicEnum(actual)) => {
                assert_eq!(actual, expected);
            }
            _ => panic!("the tail request changed the literal kind"),
        }
        assert!(effects.calls.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn ordinary_iteration_keeps_the_tail_lazy_and_yields_the_root_once() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let a = ClassBase::Class(ClassType::NonGeneric(class(&db, "A")));
    let b = ClassBase::Class(ClassType::NonGeneric(class(&db, "B")));
    let root = class(&db, "C");
    let c = ClassBase::Class(ClassType::NonGeneric(root));
    let object = ClassBase::object(&db, &env);
    let mut iterator = MroIterator::new(&db, root, None);
    assert!(iterator.cursor.subsequent_elements.is_none());
    assert_eq!(iterator.next(), Some(c));
    assert!(iterator.cursor.subsequent_elements.is_none());
    assert_eq!(iterator.next_back(), Some(object));
    assert_eq!(iterator.next(), Some(b));
    assert_eq!(iterator.next_back(), Some(a));
    for _ in 0..3 {
        assert_eq!(iterator.next(), None);
        assert_eq!(iterator.next_back(), None);
    }
    assert_eq!(
        MroIterator::new(&db, root, None).rev().collect::<Vec<_>>(),
        [object, a, b, c]
    );

    let object_literal = KnownClass::Object.try_to_class_literal(&db, &env).unwrap();
    let mut singleton = MroIterator::new(&db, object_literal.into(), None);
    assert_eq!(singleton.next_back(), Some(object));
    assert_eq!(singleton.next_back(), None);
    assert_eq!(singleton.next(), None);
    Ok(())
}
