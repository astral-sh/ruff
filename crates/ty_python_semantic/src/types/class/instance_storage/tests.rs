use std::cell::RefCell;
use std::future::Future;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use ty_python_core::{
    BindingWithConstraintsIterator, PlaceTable, ProgramFile, UseDefMap, place_table,
    scope::ScopeId, symbol::ScopedSymbolId, use_def_map,
};

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::{
    DefinedPlace, Definedness, PlaceWithDefinition, Provenance, PublicTypePolicy, TypeOrigin,
    global_symbol,
};
use crate::types::TypeQualifiers;
use crate::types::class::member_source::{
    MemberSourceEffects, MemberSourceWork, sealed as source_sealed,
};
use crate::types::class::{
    InstanceLayout, SlotDefinition, SlotSelectorEffects, SlotSelectorWork,
    SynchronousSlotSelectorEffects, generated_slots_with, instance_dictionary_with,
    instance_slot_with, lacks_instance_storage_with, named_tuple_slots_with,
    next_slot_binding_has_definition, own_class_binding_with, own_slot_descriptor_with,
    slot_names_with,
};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::{DataclassFlags, DataclassParams};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/storage.py",
            r#"
from dataclasses import dataclass
from typing import NamedTuple, TypedDict
from enum import Enum

def choose() -> bool: ...
class Plain: ...
class Explicit(object): ...
class Bare:
    __slots__: tuple[str, ...]
class Assigned:
    __slots__ = ("éclair", "雪", "later")
class Conditional:
    if choose():
        __slots__ = ("value",)
class Empty:
    __slots__ = ()
class Dictionary:
    __slots__ = ("__dict__",)
class Unknown:
    __slots__ = choose()
class Shadowed:
    __slots__ = ("value",)
    value = 1
class Record(TypedDict):
    value: int
class TupleRecord(NamedTuple):
    value: int
@dataclass(slots=True)
class Generated:
    value: int
@dataclass
class Unslotted:
    value: int
class Generic[T]:
    value: T
Alias = Generic[int]
Dynamic = type("Dynamic", (), {})
DynamicTuple = NamedTuple("DynamicTuple", [("value", int)])
DynamicDict = TypedDict("DynamicDict", {"value": int})
DynamicEnum = Enum("DynamicEnum", "VALUE")
"#,
        )
        .build()
}
fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassType<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/storage.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .and_then(|ty| ty.to_class_type(db))
        .ok_or_else(|| anyhow::anyhow!("missing class {name}"))
}
fn static_class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    class(db, name)?
        .static_class_literal(db)
        .map(|(class, _)| class)
        .ok_or_else(|| anyhow::anyhow!("expected static class {name}"))
}
fn finish<T>(future: impl Future<Output = Result<T, &'static str>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result.map_err(anyhow::Error::msg),
        Poll::Pending => anyhow::bail!("recording storage unexpectedly suspended"),
    }
}
fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}
fn member() -> Member<'static> {
    Member {
        inner: PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace {
                ty: Type::int_literal(37),
                origin: TypeOrigin::Declared,
                definedness: Definedness::PossiblyUndefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            }),
            qualifiers: TypeQualifiers::CLASS_VAR | TypeQualifiers::FINAL,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StorageEvent {
    Work(InstanceStorageWork),
    Call(&'static str),
}
struct Storage<'db> {
    db: &'db TestDb,
    events: RefCell<Vec<StorageEvent>>,
    payloads: RefCell<Vec<(ClassType<'db>, Option<Specialization<'db>>)>>,
    specialization: RefCell<Vec<Option<Specialization<'db>>>>,
    reject: Option<usize>,
    typed_dict: bool,
    lacks_storage: bool,
    ordinary: bool,
    fallback: bool,
}
impl<'db> Storage<'db> {
    fn new(db: &'db TestDb) -> Self {
        Self {
            db,
            events: RefCell::default(),
            payloads: RefCell::default(),
            specialization: RefCell::default(),
            reject: None,
            typed_dict: false,
            lacks_storage: false,
            ordinary: false,
            fallback: false,
        }
    }
    fn record(&self, event: StorageEvent) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        events.push(event);
        if self.reject == Some(events.len() - 1) {
            Err("refused storage")
        } else {
            Ok(())
        }
    }
    fn call(
        &self,
        name: &'static str,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<(), &'static str> {
        self.record(StorageEvent::Call(name))?;
        self.payloads
            .borrow_mut()
            .push((ClassType::NonGeneric(class), specialization));
        Ok(())
    }
}
impl sealed::Sealed for Storage<'_> {}
impl<'db> ClassInstanceStorageEffects<'db> for Storage<'db> {
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
    type Error = &'static str;
    async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.record(StorageEvent::Work(work))
    }
    async fn dynamic_instance_member(
        &self,
        _: &ProgramEnvironment<'db>,
        class: DynamicClassLiteral<'db>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.call("dynamic", ClassLiteral::Dynamic(class), None)?;
        Ok(member().inner)
    }
    async fn named_tuple_instance_member(
        &self,
        _: &ProgramEnvironment<'db>,
        class: DynamicNamedTupleLiteral<'db>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.call("named_tuple", ClassLiteral::DynamicNamedTuple(class), None)?;
        Ok(member().inner)
    }
    async fn enum_instance_member(
        &self,
        _: &ProgramEnvironment<'db>,
        class: DynamicEnumLiteral<'db>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.call("enum", ClassLiteral::DynamicEnum(class), None)?;
        Ok(member().inner)
    }
    async fn dynamic_own_instance_member(
        &self,
        class: DynamicClassLiteral<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.call("own_dynamic", ClassLiteral::Dynamic(class), None)?;
        Ok(member())
    }
    async fn named_tuple_own_instance_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.call(
            "own_named_tuple",
            ClassLiteral::DynamicNamedTuple(class),
            None,
        )?;
        Ok(member())
    }
    async fn enum_own_instance_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.call("own_enum", ClassLiteral::DynamicEnum(class), None)?;
        Ok(member())
    }
    async fn is_typed_dict(&self, _: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record(StorageEvent::Call("typed_dict"))?;
        Ok(self.typed_dict)
    }
    async fn static_instance_member(
        &self,
        _: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.call("static", ClassLiteral::Static(class), specialization)?;
        Ok(member().inner)
    }
    async fn static_own_instance_member(
        &self,
        _: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        _: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.call("own_static", ClassLiteral::Static(class), None)?;
        Ok(member())
    }
    async fn specialize_place(
        &self,
        value: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.record(StorageEvent::Call("specialize_place"))?;
        self.specialization.borrow_mut().push(specialization);
        assert_eq!(value, member().inner);
        Ok(value)
    }
    async fn specialize_member(
        &self,
        value: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.record(StorageEvent::Call("specialize_member"))?;
        self.specialization.borrow_mut().push(specialization);
        assert_eq!(value, member());
        Ok(value)
    }
}
impl<'db> StaticInstanceStorageEffects<'db> for Storage<'db> {
    type Error = &'static str;
    async fn storage_checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.record(StorageEvent::Work(work))
    }
    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record(StorageEvent::Call("typed_dict"))?;
        Ok(if self.ordinary {
            infallible(SynchronousStaticInstanceStorageEffects::is_typed_dict(
                &InlineInstanceStorageEffects::new(self.db),
                class,
            ))
        } else {
            self.typed_dict
        })
    }
    async fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        self.record(StorageEvent::Call("storage"))?;
        Ok(if self.ordinary {
            infallible(
                SynchronousStaticInstanceStorageEffects::lacks_instance_storage(
                    &InlineInstanceStorageEffects::new(self.db),
                    class,
                    name,
                ),
            )
        } else {
            self.lacks_storage
        })
    }
    async fn mro_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<InstanceMemberResult<'db>, Self::Error> {
        self.record(StorageEvent::Call("mro"))?;
        Ok(if self.ordinary {
            infallible(
                SynchronousStaticInstanceStorageEffects::mro_instance_member(
                    &InlineInstanceStorageEffects::new(self.db),
                    env,
                    class,
                    specialization,
                    name,
                ),
            )
        } else if self.fallback {
            InstanceMemberResult::TypedDict
        } else {
            InstanceMemberResult::Done(member().inner)
        })
    }
    async fn typed_dict_fallback(
        &self,
        _: &ProgramEnvironment<'db>,
        _: StaticClassLiteral<'db>,
        _: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.record(StorageEvent::Call("fallback"))?;
        Ok(member().inner)
    }
}
impl<'db> InstanceClassificationEffects<'db> for Storage<'db> {
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_bases(self.db))
    }

    type Error = &'static str;
    async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.record(StorageEvent::Work(work))
    }
    async fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        self.record(StorageEvent::Call("flags"))?;
        Ok(class.instance_flags(self.db))
    }
}

fn run_class_storage<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    own: bool,
    effects: &Storage<'db>,
) -> anyhow::Result<PlaceAndQualifiers<'db>> {
    if own {
        finish(class_own_instance_member_with(
            env,
            class,
            "value",
            effects,
        ))
        .map(|member| member.inner)
    } else {
        finish(class_instance_member_with(
            env,
            class,
            "value",
            effects,
        ))
    }
}

#[test]
fn class_storage_keeps_native_family_and_specialization_requests() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    for (name, full_call, own_call) in [
        ("Dynamic", Some("dynamic"), Some("own_dynamic")),
        ("DynamicTuple", Some("named_tuple"), Some("own_named_tuple")),
        ("DynamicDict", None, None),
        ("DynamicEnum", Some("enum"), Some("own_enum")),
        ("Plain", Some("static"), Some("own_static")),
        ("Alias", Some("static"), Some("own_static")),
    ] {
        let class = class(&db, name)?;
        for own in [false, true] {
            let effects = Storage::new(&db);
            let output = run_class_storage(&db, &env, class, own, &effects)?;
            let expected_call = if own { own_call } else { full_call };
            assert_eq!(
                output,
                if expected_call.is_some() {
                    member().inner
                } else {
                    PlaceAndQualifiers::default()
                }
            );
            let events = effects.events.into_inner();
            if let Some(expected_call) = expected_call {
                assert!(events.contains(&StorageEvent::Call(expected_call)));
                let (literal, expected_specialization) = match class {
                    ClassType::Generic(alias) => (
                        ClassType::NonGeneric(ClassLiteral::Static(alias.origin(&db))),
                        Some(alias.specialization(&db)),
                    ),
                    other => (other, None),
                };
                assert_eq!(
                    *effects.payloads.borrow(),
                    [(literal, if own { None } else { expected_specialization })]
                );
                assert_eq!(
                    *effects.specialization.borrow(),
                    expected_specialization
                        .into_iter()
                        .map(Some)
                        .collect::<Vec<_>>()
                );
                if expected_specialization.is_some() {
                    let fields = events
                        .iter()
                        .filter_map(|event| match event {
                            StorageEvent::Work(
                                work @ (InstanceStorageWork::AliasOrigin
                                | InstanceStorageWork::AliasSpecialization),
                            ) => Some(*work),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        fields,
                        if own {
                            vec![
                                InstanceStorageWork::AliasSpecialization,
                                InstanceStorageWork::AliasOrigin,
                            ]
                        } else {
                            vec![
                                InstanceStorageWork::AliasOrigin,
                                InstanceStorageWork::AliasSpecialization,
                            ]
                        }
                    );
                }
            } else {
                assert!(
                    !events
                        .iter()
                        .any(|event| matches!(event, StorageEvent::Call(_)))
                );
            }
            for reject in 0..events.len() {
                let mut refused = Storage::new(&db);
                refused.reject = Some(reject);
                assert!(run_class_storage(&db, &env, class, own, &refused).is_err());
                assert_eq!(*refused.events.borrow(), events[..=reject]);
            }
        }
    }
    Ok(())
}

#[test]
fn static_storage_keeps_guards_before_original_mro() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let class = static_class(&db, "Plain")?;
    for (typed_dict, lacks_storage, fallback, calls) in [
        (true, false, false, vec!["typed_dict"]),
        (false, true, false, vec!["typed_dict", "storage"]),
        (false, false, false, vec!["typed_dict", "storage", "mro"]),
        (
            false,
            false,
            true,
            vec!["typed_dict", "storage", "mro", "fallback"],
        ),
    ] {
        let mut effects = Storage::new(&db);
        effects.typed_dict = typed_dict;
        effects.lacks_storage = lacks_storage;
        effects.fallback = fallback;
        let result = finish(static_instance_member_with(
            &env, class, None, "value", &effects,
        ))?;
        assert_eq!(
            result,
            if typed_dict || lacks_storage {
                Place::Undefined.into()
            } else {
                member().inner
            }
        );
        let events = effects.events.into_inner();
        assert_eq!(
            events
                .iter()
                .filter_map(|event| if let StorageEvent::Call(call) = event {
                    Some(*call)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            calls
        );
        for reject in 0..events.len() {
            let mut effects = Storage::new(&db);
            effects.typed_dict = typed_dict;
            effects.lacks_storage = lacks_storage;
            effects.fallback = fallback;
            effects.reject = Some(reject);
            assert!(
                finish(static_instance_member_with(
                    &env, class, None, "value", &effects
                ))
                .is_err()
            );
            assert_eq!(*effects.events.borrow(), events[..=reject]);
        }
    }
    // This comparison invokes ordinary dependencies after the recorded guards. It exercises
    // the real MRO, union builder, and lookup-local environment without a runtime attempt.
    for known in [
        KnownClass::Object,
        KnownClass::Type,
        KnownClass::TypedDictFallback,
    ] {
        let class = known
            .try_to_class_literal(&db, &env)
            .ok_or_else(|| anyhow::anyhow!("missing {known:?}"))?;
        for name in ["value", "__getattr__"] {
            let mut effects = Storage::new(&db);
            effects.ordinary = true;
            let result = finish(static_instance_member_with(
                &env, class, None, name, &effects,
            ))?;
            assert_eq!(result, class.instance_member(&db, &env, None, name));
            let events = effects.events.into_inner();
            if class.is_typed_dict(&db) {
                assert!(!events.contains(&StorageEvent::Call("storage")));
                assert!(!events.contains(&StorageEvent::Call("mro")));
            } else {
                assert!(events.contains(&StorageEvent::Call("mro")));
            }
        }
    }
    Ok(())
}

#[test]
fn typed_dict_header_preserves_lazy_flags_request() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let mut classes = vec![
        static_class(&db, "Plain")?,
        static_class(&db, "Explicit")?,
        static_class(&db, "Record")?,
    ];
    for known in [
        KnownClass::Object,
        KnownClass::Type,
        KnownClass::TypedDictFallback,
    ] {
        classes.push(
            known
                .try_to_class_literal(&db, &env)
                .ok_or_else(|| anyhow::anyhow!("missing known class"))?,
        );
    }
    for class in classes {
        let effects = Storage::new(&db);
        assert_eq!(
            finish(static_is_typed_dict_with(class, &effects))?,
            class.is_typed_dict(&db)
        );
        let events = effects.events.into_inner();
        assert_eq!(
            events.contains(&StorageEvent::Call("flags")),
            MroFieldReads::new(&db)
                .typed_dict_without_inference(class)
                .is_none()
        );
        for reject in 0..events.len() {
            let mut refused = Storage::new(&db);
            refused.reject = Some(reject);
            assert!(finish(static_is_typed_dict_with(class, &refused)).is_err());
            assert_eq!(*refused.events.borrow(), events[..=reject]);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotEvent {
    Work(SlotSelectorWork),
    Call(&'static str),
}
struct Slots<'db> {
    db: &'db TestDb,
    events: RefCell<Vec<SlotEvent>>,
    reject: Option<usize>,
    definition: Option<&'db SlotDefinition>,
    bases: Option<&'db [Type<'db>]>,
    version: Option<PythonVersion>,
    stub: Option<bool>,
}
impl<'db> Slots<'db> {
    fn new(db: &'db TestDb) -> Self {
        Self {
            db,
            events: RefCell::default(),
            reject: None,
            definition: None,
            bases: None,
            version: None,
            stub: None,
        }
    }
    fn record(&self, event: SlotEvent) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        events.push(event);
        if self.reject == Some(events.len() - 1) {
            Err("refused slot")
        } else {
            Ok(())
        }
    }
}
impl source_sealed::Sealed for Slots<'_> {}
impl<'db> MemberSourceEffects<'db> for Slots<'db> {
    type Error = &'static str;
    async fn checkpoint(&self, _: MemberSourceWork) -> Result<(), Self::Error> {
        Err("unexpected member-source checkpoint")
    }
    async fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error> {
        self.record(SlotEvent::Call("table"))?;
        Ok(place_table(self.db, scope))
    }
    async fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error> {
        self.record(SlotEvent::Call("use_def"))?;
        Ok(use_def_map(self.db, scope))
    }
    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        self.record(SlotEvent::Call("symbol"))?;
        Ok(table.symbol_id(name))
    }
    async fn binding_place<'map>(
        &self,
        _: &ProgramEnvironment<'db>,
        _: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error> {
        Err("slot presence must not infer a binding")
    }
}
impl<'db> SlotSelectorEffects<'db> for Slots<'db> {
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.db))
    }
    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error> {
        Ok(class.dataclass_params(self.db))
    }
    async fn dataclass_flags(
        &self,
        params: DataclassParams<'db>,
    ) -> Result<DataclassFlags, Self::Error> {
        Ok(params.flags(self.db))
    }
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_bases(self.db))
    }

    async fn slot_checkpoint(&self, work: SlotSelectorWork) -> Result<(), Self::Error> {
        work.work_units().ok_or("work overflow")?;
        self.record(SlotEvent::Work(work))
    }
    async fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<bool>, Self::Error> {
        self.slot_checkpoint(SlotSelectorWork::BindingAdvance)
            .await?;
        Ok(next_slot_binding_has_definition(bindings))
    }
    async fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.record(SlotEvent::Call("bases"))?;
        Ok(self.bases.unwrap_or_else(|| {
            infallible(SynchronousSlotSelectorEffects::explicit_bases(
                &InlineMemberSourceEffects::new(self.db),
                class,
            ))
        }))
    }
    async fn source_python_version(
        &self,
        scope: ScopeId<'db>,
    ) -> Result<PythonVersion, Self::Error> {
        self.record(SlotEvent::Call("version"))?;
        Ok(self.version.unwrap_or_else(|| {
            infallible(SynchronousSlotSelectorEffects::source_python_version(
                &InlineMemberSourceEffects::new(self.db),
                scope,
            ))
        }))
    }
    async fn slot_definition(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db SlotDefinition, Self::Error> {
        self.record(SlotEvent::Call("definition"))?;
        Ok(self.definition.unwrap_or_else(|| {
            infallible(SynchronousSlotSelectorEffects::slot_definition(
                &InlineMemberSourceEffects::new(self.db),
                class,
            ))
        }))
    }
    async fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db InstanceLayout, Self::Error> {
        self.record(SlotEvent::Call("layout"))?;
        Ok(infallible(SynchronousSlotSelectorEffects::instance_layout(
            &InlineMemberSourceEffects::new(self.db),
            class,
        )))
    }
    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.record(SlotEvent::Call("stub"))?;
        Ok(self.stub.unwrap_or_else(|| {
            infallible(SynchronousSlotSelectorEffects::is_stub(
                &InlineMemberSourceEffects::new(self.db),
                class,
            ))
        }))
    }
}

#[test]
fn slot_bindings_refuse_before_native_advance() -> anyhow::Result<()> {
    let db = database()?;
    for (name, bound) in [
        ("Plain", false),
        ("Bare", false),
        ("Assigned", true),
        ("Conditional", true),
    ] {
        let class = static_class(&db, name)?;
        let effects = Slots::new(&db);
        assert_eq!(
            finish(own_class_binding_with(class, "__slots__", &effects))?,
            bound
        );
        let events = effects.events.into_inner();
        for reject in 0..events.len() {
            let mut effects = Slots::new(&db);
            effects.reject = Some(reject);
            assert!(finish(own_class_binding_with(class, "__slots__", &effects)).is_err());
            assert_eq!(*effects.events.borrow(), events[..=reject]);
        }
        let scope = class.body_scope(&db);
        let Some(symbol) = place_table(&db, scope).symbol_id("__slots__") else {
            assert!(!events.contains(&SlotEvent::Call("use_def")));
            continue;
        };
        let bindings = use_def_map(&db, scope).end_of_scope_symbol_bindings(symbol);
        let native = bindings
            .clone()
            .map(|binding| binding.binding.definition().is_some())
            .collect::<Vec<_>>();
        if name == "Conditional" {
            assert!(native.contains(&false));
            assert!(native.contains(&true));
        }
        for prefix in 0..=native.len() {
            let mut cursor = bindings.clone();
            let mut effects = Slots::new(&db);
            effects.reject = Some(prefix);
            for expected in &native[..prefix] {
                assert_eq!(
                    finish(effects.next_binding_has_definition(&mut cursor))?,
                    Some(*expected)
                );
            }
            let before = cursor.traversal_len();
            let next = cursor
                .clone()
                .next()
                .map(|binding| binding.binding.definition().is_some());
            assert!(finish(effects.next_binding_has_definition(&mut cursor)).is_err());
            assert_eq!(cursor.traversal_len(), before);
            assert_eq!(next_slot_binding_has_definition(&mut cursor), next);
        }
    }
    Ok(())
}

#[test]
fn slot_selectors_keep_lazy_source_and_query_order() -> anyhow::Result<()> {
    let db = database()?;
    for name in [
        "Plain",
        "Bare",
        "Assigned",
        "Conditional",
        "Empty",
        "Dictionary",
        "Unknown",
        "TupleRecord",
        "Generated",
        "Unslotted",
        "Shadowed",
    ] {
        let class = static_class(&db, name)?;
        for selector in 0..7 {
            let run = |effects: &Slots<'_>| -> anyhow::Result<bool> {
                match selector {
                    0 => finish(generated_slots_with(class, effects)),
                    1 => finish(named_tuple_slots_with(class, effects)),
                    2 => finish(slot_names_with(class, effects)).map(|names| names.is_some()),
                    3 => finish(instance_slot_with(class, "value", effects)),
                    4 => finish(instance_dictionary_with(class, effects)),
                    5 => finish(lacks_instance_storage_with(class, "value", effects)),
                    _ => finish(own_slot_descriptor_with(class, "value", effects)),
                }
            };
            let effects = Slots::new(&db);
            let result = run(&effects)?;
            match selector {
                0 => assert_eq!(result, class.has_generated_slots(&db)),
                2 => assert_eq!(result, class.slot_names(&db).is_some()),
                3 => assert_eq!(result, class.has_instance_slot(&db, "value")),
                5 => assert_eq!(result, class.lacks_instance_storage(&db, "value")),
                6 => assert_eq!(result, class.has_own_slot_descriptor(&db, "value")),
                _ => {}
            }
            let events = effects.events.into_inner();
            if selector == 0 && matches!(name, "Plain" | "Unslotted") {
                assert!(!events.contains(&SlotEvent::Call("version")));
            }
            if selector == 2 && name == "Plain" {
                assert!(!events.contains(&SlotEvent::Call("definition")));
            }
            if selector == 4 && name == "Plain" {
                assert!(!events.contains(&SlotEvent::Call("layout")));
                assert!(result);
            }
            if selector == 4 && name == "Empty" {
                assert!(!result);
            }
            if selector == 4 && name == "Dictionary" {
                assert!(result);
            }
            for reject in 0..events.len() {
                let mut effects = Slots::new(&db);
                effects.reject = Some(reject);
                assert!(
                    run(&effects).is_err(),
                    "{name} selector {selector} rejection {reject}"
                );
                assert_eq!(*effects.events.borrow(), events[..=reject]);
            }
        }
    }
    let generated = static_class(&db, "Generated")?;
    for (version, expected) in [(PythonVersion::PY39, false), (PythonVersion::PY310, true)] {
        let mut effects = Slots::new(&db);
        effects.version = Some(version);
        assert_eq!(finish(generated_slots_with(generated, &effects))?, expected);
        assert!(
            effects
                .events
                .borrow()
                .contains(&SlotEvent::Call("version"))
        );
    }
    let assigned = static_class(&db, "Assigned")?;
    for definition in [
        SlotDefinition::Names(vec![Name::new_static("value")].into_boxed_slice()),
        SlotDefinition::NonEmpty,
        SlotDefinition::DynamicOrNone,
    ] {
        let mut effects = Slots::new(&db);
        effects.definition = Some(&definition);
        assert_eq!(
            finish(slot_names_with(assigned, &effects))?.is_some(),
            matches!(definition, SlotDefinition::Names(_))
        );
        assert!(!effects.events.borrow().contains(&SlotEvent::Call("bases")));
    }
    let effects = Slots::new(&db);
    assert!(!finish(own_slot_descriptor_with(
        assigned, "__dict__", &effects
    ))?);
    assert_eq!(
        *effects.events.borrow(),
        [
            SlotEvent::Work(SlotSelectorWork::Begin),
            SlotEvent::Work(SlotSelectorWork::DictionaryName { requested_bytes: 8 }),
            SlotEvent::Work(SlotSelectorWork::Publish)
        ]
    );
    let shadowed = static_class(&db, "Shadowed")?;
    for stub in [false, true] {
        let mut effects = Slots::new(&db);
        effects.stub = Some(stub);
        assert_eq!(
            finish(own_slot_descriptor_with(shadowed, "value", &effects))?,
            stub
        );
        assert_eq!(
            effects.events.borrow().contains(&SlotEvent::Call("layout")),
            stub
        );
    }
    Ok(())
}

#[test]
fn slot_scans_charge_before_compare_and_short_circuit() -> anyhow::Result<()> {
    let db = database()?;
    let assigned = static_class(&db, "Assigned")?;
    for (names, requested, expected, comparisons) in [
        (vec!["雪", "unvisited much larger payload"], "雪", true, 1),
        (vec!["éclair", "雪", "later"], "later", true, 3),
        (vec!["éclair", "雪"], "missing", false, 2),
        (vec![], "missing", false, 0),
    ] {
        let definition = SlotDefinition::Names(names.iter().map(|name| Name::new(name)).collect());
        let mut effects = Slots::new(&db);
        effects.definition = Some(&definition);
        assert_eq!(
            finish(own_slot_descriptor_with(assigned, requested, &effects))?,
            expected
        );
        let events = effects.events.into_inner();
        let compared = events
            .iter()
            .filter_map(|event| match event {
                SlotEvent::Work(SlotSelectorWork::NameCompare {
                    candidate_bytes,
                    requested_bytes,
                }) => Some((*candidate_bytes, *requested_bytes)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            compared,
            names[..comparisons]
                .iter()
                .map(|name| (name.len(), requested.len()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == SlotEvent::Work(SlotSelectorWork::NameAdvance))
                .count(),
            comparisons + usize::from(!expected)
        );
        for reject in 0..events.len() {
            let mut effects = Slots::new(&db);
            effects.definition = Some(&definition);
            effects.reject = Some(reject);
            assert!(finish(own_slot_descriptor_with(assigned, requested, &effects)).is_err());
            assert_eq!(*effects.events.borrow(), events[..=reject]);
        }
    }
    let explicit = static_class(&db, "Explicit")?;
    let named_tuple = Type::SpecialForm(crate::types::SpecialFormType::NamedTuple);
    let todo = crate::types::todo_type!("slot base inline payload");
    for (bases, expected, comparisons) in [
        (vec![named_tuple, todo], true, 1),
        (vec![Type::int_literal(1), todo, named_tuple], true, 3),
        (vec![Type::unknown(), todo], false, 2),
        (vec![], false, 0),
    ] {
        let mut effects = Slots::new(&db);
        effects.bases = Some(&bases);
        assert_eq!(
            finish(named_tuple_slots_with(explicit, &effects))?,
            expected
        );
        let events = effects.events.into_inner();
        let quotes = events
            .iter()
            .filter_map(|event| match event {
                SlotEvent::Work(SlotSelectorWork::BaseCompare { inline_bytes }) => {
                    Some(*inline_bytes)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            quotes,
            bases[..comparisons]
                .iter()
                .copied()
                .map(Type::inline_payload_bytes)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == SlotEvent::Work(SlotSelectorWork::BaseAdvance))
                .count(),
            comparisons + usize::from(!expected)
        );
        for reject in 0..events.len() {
            let mut effects = Slots::new(&db);
            effects.bases = Some(&bases);
            effects.reject = Some(reject);
            assert!(finish(named_tuple_slots_with(explicit, &effects)).is_err());
            assert_eq!(*effects.events.borrow(), events[..=reject]);
        }
    }
    for quote in [
        SlotSelectorWork::BaseCompare {
            inline_bytes: usize::MAX,
        },
        SlotSelectorWork::NameCompare {
            candidate_bytes: usize::MAX,
            requested_bytes: 0,
        },
        SlotSelectorWork::NameCompare {
            candidate_bytes: 0,
            requested_bytes: usize::MAX,
        },
        SlotSelectorWork::DictionaryName {
            requested_bytes: usize::MAX,
        },
    ] {
        assert_eq!(quote.work_units(), None);
        let effects = Slots::new(&db);
        assert!(finish(effects.slot_checkpoint(quote)).is_err());
        assert!(effects.events.borrow().is_empty());
    }
    assert_eq!(SlotSelectorWork::BindingAdvance.work_units(), Some(1));
    assert_eq!(
        SlotSelectorWork::NameCompare {
            candidate_bytes: 3,
            requested_bytes: 6
        }
        .work_units(),
        Some(10)
    );
    Ok(())
}

#[test]
fn class_storage_typed_dict_guard_prevents_storage_and_specialization() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    for name in ["Plain", "Alias"] {
        let class = class(&db, name)?;
        let mut effects = Storage::new(&db);
        effects.typed_dict = true;
        assert_eq!(
            finish(class_instance_member_with(
                &env,
                class,
                "value",
                &effects
            ))?,
            Place::Undefined.into()
        );
        assert!(effects.payloads.borrow().is_empty());
        assert!(effects.specialization.borrow().is_empty());
        let events = effects.events.into_inner();
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    StorageEvent::Call(name) => Some(*name),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            ["typed_dict"]
        );
        for reject in 0..events.len() {
            let mut effects = Storage::new(&db);
            effects.typed_dict = true;
            effects.reject = Some(reject);
            assert!(
                finish(class_instance_member_with(
                    &env,
                    class,
                    "value",
                    &effects
                ))
                .is_err()
            );
            assert_eq!(*effects.events.borrow(), events[..=reject]);
        }
    }
    Ok(())
}
