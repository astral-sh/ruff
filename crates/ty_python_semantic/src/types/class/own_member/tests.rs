use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{
    ClassTypeOwnMemberEffects, ClassTypeOwnMemberRequest, ClassTypeOwnMemberWork,
    OwnMemberLookupRequest, class_type_own_member_with, sealed,
};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::{
    DefinedPlace, Definedness, Place, Provenance, PublicTypePolicy, TypeOrigin, global_symbol,
};
use crate::types::class::{
    DynamicClassLiteral, DynamicEnumLiteral, DynamicNamedTupleLiteral, DynamicTypedDictLiteral,
};
use crate::types::generics::Specialization;
use crate::types::member::Member;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, GenericContext, KnownClass, StaticClassLiteral, Type,
    TypeQualifiers,
};

fn database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/classes.py",
        r#"
from enum import Enum
from typing import NamedTuple, TypedDict

class Plain: ...
class Generic[T]: ...
class Root[U]: ...
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
        system_path_to_file(db, "/src/classes.py").unwrap(),
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .unwrap()
}

fn rich_member<'db>() -> Member<'db> {
    Member {
        inner: Place::Defined(DefinedPlace {
            ty: Type::int_literal(11),
            origin: TypeOrigin::Inferred,
            definedness: Definedness::PossiblyUndefined,
            public_type_policy: PublicTypePolicy::Promote,
            provenance: Provenance::MultipleDefinitions,
        })
        .with_qualifiers(TypeQualifiers::CLASS_VAR | TypeQualifiers::FINAL),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Call<'db> {
    Dynamic(&'static str, ClassLiteral<'db>, String),
    TupleLen(ClassType<'db>, Option<Specialization<'db>>),
    TupleGetitem(&'db TupleSpec<'db>),
    TupleNew(
        ClassType<'db>,
        Option<Specialization<'db>>,
        Option<GenericContext<'db>>,
    ),
    Normalize(Specialization<'db>),
    Static(
        StaticClassLiteral<'db>,
        String,
        Option<GenericContext<'db>>,
        Option<Specialization<'db>>,
    ),
    Map(Type<'db>, Specialization<'db>),
}

struct RecordingEffects<'db> {
    db: &'db TestDb,
    calls: RefCell<Vec<Call<'db>>>,
    work: RefCell<Vec<ClassTypeOwnMemberWork>>,
    member: Member<'db>,
    normalized: Option<Specialization<'db>>,
    reject: Option<&'static str>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db TestDb, member: Member<'db>) -> Self {
        Self {
            db,
            calls: RefCell::new(Vec::new()),
            work: RefCell::new(Vec::new()),
            member,
            normalized: None,
            reject: None,
        }
    }

    fn record(&self, operation: &'static str, call: Call<'db>) -> Result<(), &'static str> {
        self.calls.borrow_mut().push(call);
        if self.reject == Some(operation) {
            Err(operation)
        } else {
            Ok(())
        }
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

impl<'db> ClassTypeOwnMemberEffects<'db> for RecordingEffects<'db> {
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(self.db))
    }
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(alias.specialization(self.db))
    }
    async fn is_tuple(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_tuple(self.db))
    }
    async fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(specialization.tuple(self.db))
    }
    type Error = &'static str;

    async fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> Result<(), Self::Error> {
        self.work.borrow_mut().push(work);
        Ok(())
    }

    async fn dynamic_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.record(
            "dynamic",
            Call::Dynamic("dynamic", class.into(), name.to_owned()),
        )?;
        Ok(self.member)
    }

    async fn named_tuple_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.record(
            "named_tuple",
            Call::Dynamic("named_tuple", class.into(), name.to_owned()),
        )?;
        Ok(self.member)
    }

    async fn typed_dict_member(
        &self,
        class: DynamicTypedDictLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.record(
            "typed_dict",
            Call::Dynamic("typed_dict", class.into(), name.to_owned()),
        )?;
        Ok(self.member)
    }

    async fn enum_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.record("enum", Call::Dynamic("enum", class.into(), name.to_owned()))?;
        Ok(self.member)
    }

    async fn tuple_len(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.record("tuple_len", Call::TupleLen(class, specialization))?;
        Ok(self.member)
    }

    async fn tuple_getitem(&self, tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error> {
        self.record("tuple_getitem", Call::TupleGetitem(tuple))?;
        Ok(self.member)
    }

    async fn tuple_new(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.record("tuple_new", Call::TupleNew(class, specialization, context))?;
        Ok(self.member)
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.record("normalize", Call::Normalize(specialization))?;
        Ok(self.normalized.unwrap_or(specialization))
    }

    async fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        self.record(
            "static",
            Call::Static(
                request.class,
                request.name.to_owned(),
                request.inherited_generic_context,
                request.specialization,
            ),
        )?;
        Ok(self.member)
    }

    async fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.record("map", Call::Map(ty, specialization))?;
        Ok(Type::int_literal(29))
    }
}

#[test]
fn each_dynamic_literal_uses_only_its_own_dependency() -> anyhow::Result<()> {
    let db = database()?;
    for (name, operation) in [
        ("Dynamic", "dynamic"),
        ("Named", "named_tuple"),
        ("Typed", "typed_dict"),
        ("Enumeration", "enum"),
    ] {
        let literal = class(&db, name);
        for reject in [false, true] {
            let mut effects = RecordingEffects::new(&db, rich_member());
            effects.reject = reject.then_some(operation);
            let request = ClassTypeOwnMemberRequest {
                class: ClassType::NonGeneric(literal),
                name: "member",
                inherited_generic_context: None,
            };
            let expected = if reject {
                Err(operation)
            } else {
                Ok(rich_member())
            };
            assert_eq!(
                try_poll_immediate(class_type_own_member_with( request, &effects)),
                Poll::Ready(expected),
            );
            assert_eq!(
                *effects.calls.borrow(),
                [Call::Dynamic(operation, literal, "member".to_owned())]
            );
            assert_eq!(
                effects
                    .work
                    .borrow()
                    .contains(&ClassTypeOwnMemberWork::Publish),
                !reject
            );
        }
    }
    Ok(())
}

#[test]
fn fallback_normalizes_before_lookup_and_maps_only_the_defined_type() -> anyhow::Result<()> {
    let db = database()?;
    let literal = class(&db, "Generic").as_static().unwrap();
    let context = literal.generic_context(&db).unwrap();
    let inherited = class(&db, "Root").as_static().unwrap().generic_context(&db);
    let supplied = context.specialize(&db, [Type::int_literal(1)].as_slice());
    let normalized = context.specialize(&db, [Type::int_literal(2)].as_slice());
    let specialized = ClassType::Generic(GenericAlias::new(&db, literal, supplied));

    for member in [rich_member(), Member::unbound()] {
        let mut effects = RecordingEffects::new(&db, member);
        effects.normalized = Some(normalized);
        let request = ClassTypeOwnMemberRequest {
            class: specialized,
            name: "member",
            inherited_generic_context: inherited,
        };
        assert_eq!(
            try_poll_immediate(class_type_own_member_with( request, &effects)),
            Poll::Ready(Ok(member.map_type(|_| Type::int_literal(29)))),
        );
        let mut expected = vec![
            Call::Normalize(supplied),
            Call::Static(literal, "member".to_owned(), inherited, Some(normalized)),
        ];
        if !member.is_undefined() {
            expected.push(Call::Map(Type::int_literal(11), normalized));
        }
        assert_eq!(*effects.calls.borrow(), expected);
    }

    let plain = class(&db, "Plain").as_static().unwrap();
    let effects = RecordingEffects::new(&db, rich_member());
    assert_eq!(
        try_poll_immediate(class_type_own_member_with(
            ClassTypeOwnMemberRequest {
                class: ClassType::NonGeneric(plain.into()),
                name: "member",
                inherited_generic_context: inherited,
            },
            &effects
        )),
        Poll::Ready(Ok(rich_member())),
    );
    assert_eq!(
        *effects.calls.borrow(),
        [Call::Static(plain, "member".to_owned(), inherited, None)]
    );
    Ok(())
}

#[test]
fn selected_tuple_builders_keep_the_original_payload_and_never_fall_back() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let tuple = KnownClass::Tuple.try_to_class_literal(&db, &env).unwrap();
    let context = tuple.generic_context(&db).unwrap();
    let inherited = class(&db, "Root").as_static().unwrap().generic_context(&db);
    let empty_payload = context.specialize(&db, [Type::unknown()].as_slice());
    let payload = context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::heterogeneous(&db, &env, [Type::int_literal(1), Type::int_literal(2)]),
    );
    for specialization in [None, Some(empty_payload), Some(payload)] {
        let class = specialization.map_or(ClassType::NonGeneric(tuple.into()), |specialization| {
            ClassType::Generic(GenericAlias::new(&db, tuple, specialization))
        });
        for (name, operation) in [
            ("__len__", "tuple_len"),
            ("__new__", "tuple_new"),
            ("__getitem__", "tuple_getitem"),
        ] {
            if name == "__getitem__" && specialization != Some(payload) {
                continue;
            }
            for reject in [false, true] {
                let mut effects = RecordingEffects::new(&db, rich_member());
                effects.reject = reject.then_some(operation);
                let request = ClassTypeOwnMemberRequest {
                    class,
                    name,
                    inherited_generic_context: inherited,
                };
                let expected = if reject {
                    Err(operation)
                } else {
                    Ok(rich_member())
                };
                assert_eq!(
                    try_poll_immediate(class_type_own_member_with( request, &effects)),
                    Poll::Ready(expected)
                );
                let expected_call = match name {
                    "__len__" => Call::TupleLen(class, specialization),
                    "__new__" => Call::TupleNew(class, specialization, inherited),
                    _ => Call::TupleGetitem(payload.tuple(&db).unwrap()),
                };
                assert_eq!(*effects.calls.borrow(), [expected_call]);
                assert_eq!(
                    effects
                        .work
                        .borrow()
                        .contains(&ClassTypeOwnMemberWork::Publish),
                    !reject
                );
            }
        }
    }
    Ok(())
}

#[test]
fn tuple_getitem_without_a_payload_and_other_names_use_the_static_fallback() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let tuple = KnownClass::Tuple.try_to_class_literal(&db, &env).unwrap();
    let context = tuple.generic_context(&db).unwrap();
    let plain = context.specialize(&db, [Type::unknown()].as_slice());
    let shaped = context.specialize_tuple(
        &db,
        Type::unknown(),
        TupleType::homogeneous(&db, &env, Type::unknown()),
    );
    for (name, specialization) in [
        ("__getitem__", None),
        ("__getitem__", Some(plain)),
        ("member", Some(shaped)),
    ] {
        let class = specialization.map_or(ClassType::NonGeneric(tuple.into()), |specialization| {
            ClassType::Generic(GenericAlias::new(&db, tuple, specialization))
        });
        let mut effects = RecordingEffects::new(&db, Member::unbound());
        effects.normalized = Some(plain);
        assert_eq!(
            try_poll_immediate(class_type_own_member_with(
                ClassTypeOwnMemberRequest {
                    class,
                    name,
                    inherited_generic_context: None,
                },
                &effects
            )),
            Poll::Ready(Ok(Member::unbound())),
        );
        let mut expected = Vec::new();
        if let Some(specialization) = specialization {
            expected.push(Call::Normalize(specialization));
        }
        expected.push(Call::Static(
            tuple,
            name.to_owned(),
            None,
            specialization.map(|_| plain),
        ));
        assert_eq!(*effects.calls.borrow(), expected);
    }
    Ok(())
}
