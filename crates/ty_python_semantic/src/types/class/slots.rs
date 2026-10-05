use super::member_source::{
    InlineMemberSourceEffects, MemberSourceEffects, SynchronousMemberSourceEffects,
};
use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use ty_python_core::{
    BindingWithConstraintsIterator, UseDefMap, place_table, scope::ScopeId, symbol::ScopedSymbolId,
    use_def_map,
};

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::IngredientImpl;

use crate::place::{DefinedPlace, Definedness, Place, place_from_bindings};
use crate::types::class::{CodeGeneratorKind, StaticClassLiteral};
use crate::types::generics::Specialization;
use crate::types::{
    ClassBase, DataclassFlags, DataclassParams, KnownClass, SpecialFormType, Type,
    definition_expression_type, tuple::Tuple,
};
use crate::{Db, FxIndexSet, ProgramEnvironment};

pub(in crate::types) mod layout;

/// The information that can be recovered from a class's own `__slots__` assignment.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) enum SlotDefinition {
    /// Every declared slot name is statically known.
    Names(Box<[Name]>),
    /// The declaration is definitely nonempty, but at least one name is unknown.
    NonEmpty,
    /// The class has no slot declaration, or its declaration cannot be resolved statically.
    DynamicOrNone,
}

/// An interpreter-created `types.MemberDescriptorType` for an instance slot.
///
/// Its `__get__` and `__set__` methods access the memory reserved for the slot in each instance,
/// without invoking the Python-level getter, setter, or deleter callbacks used by a `property`.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct SlotDescriptorType<'db> {
    #[returns(copy)]
    pub(crate) value_type: Type<'db>,
}

impl get_size2::GetSize for SlotDescriptorType<'_> {}

/// Whether instances can store attributes in an ordinary instance dictionary.
///
/// Ordinary Python classes provide this storage, while classes that use slots throughout their
/// inheritance chain can omit it. A slotted class can inherit an instance dictionary from a base
/// class or request one explicitly:
///
/// ```python
/// class Slotted:
///     __slots__ = ("value",)
///
/// class WithDictionary(Slotted):
///     __slots__ = ("__dict__",)
/// ```
///
/// This describes `instance.__dict__`, not `Class.__dict__`: the class's own namespace remains
/// available regardless of its instance layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) enum InstanceDictionary {
    /// Instances definitely have dictionary-backed attribute storage.
    Present,
    /// Instances definitely lack dictionary-backed attribute storage.
    Absent,
    /// A base class or dynamic slot declaration prevents determining the instance layout.
    ///
    /// Unknown storage remains permissive when checking attribute access and assignment.
    Unknown,
}

impl InstanceDictionary {
    /// Classify interpreter-managed storage that cannot be recovered from stub declarations.
    fn for_known_class(class: KnownClass) -> Option<Self> {
        match class {
            KnownClass::Object
            | KnownClass::Bool
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Memoryview
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Range
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::Super
            | KnownClass::GenericAlias
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::MemberDescriptorType
            | KnownClass::GetSetDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NotImplementedType
            | KnownClass::BuiltinFunctionType
            | KnownClass::EllipsisType
            | KnownClass::NoneType => Some(Self::Absent),
            // Typeshed adds these abstract bases to builtin sequences and mappings even though
            // they do not occur in their runtime inheritance chains or provide instance storage.
            KnownClass::Sequence | KnownClass::Mapping | KnownClass::MutableMapping => {
                Some(Self::Absent)
            }
            // This synthetic base supplies named-tuple members without changing instance layouts.
            KnownClass::NamedTupleFallback => Some(Self::Absent),
            KnownClass::Type
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::Warning
            | KnownClass::NotImplementedError
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::ModuleType
            | KnownClass::FunctionType => Some(Self::Present),
            KnownClass::Enum
            | KnownClass::EnumProperty
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::IntEnum
            | KnownClass::Flag
            | KnownClass::IntFlag
            | KnownClass::ABCMeta
            | KnownClass::SupportsKeysAndGetItem
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::AsyncGenerator
            | KnownClass::Deprecated
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ParamSpec
            | KnownClass::ExtensionsParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::ProtocolMeta
            | KnownClass::TypeVarTuple
            | KnownClass::ExtensionsTypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::ExtensionsTypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::Hashable
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::AsyncIterator
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ExtensionTypedDictFallback
            | KnownClass::Sentinel
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::NamedTupleLike
            | KnownClass::TypedDictFallback
            | KnownClass::Template
            | KnownClass::Path
            | KnownClass::FunctoolsPartial
            | KnownClass::ConstraintSet
            | KnownClass::ConstraintSetSolution
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::TyExtensionsAsyncIterable
            | KnownClass::TyExtensionsAsyncIterator
            | KnownClass::TyExtensionsIterable
            | KnownClass::TyExtensionsIterator
            | KnownClass::UnittestTestCase
            | KnownClass::PydanticBaseModel
            | KnownClass::PydanticBaseSettings
            | KnownClass::PydanticConfigDict
            | KnownClass::PydanticRootModel
            | KnownClass::PydanticStrict
            | KnownClass::PytestParametrizeMarkDecorator => None,
        }
    }

    /// Combine two base layouts while preserving any definitely present dictionary.
    fn inherited_with(self, other: Self) -> Self {
        match (self, other) {
            (Self::Present, _) | (_, Self::Present) => Self::Present,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Absent, Self::Absent) => Self::Absent,
        }
    }
}

/// The slots and dictionary storage inherited by instances of a class.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) struct InstanceLayout {
    slots: Box<[Name]>,
    dictionary: InstanceDictionary,
}

impl InstanceLayout {
    #[cfg(feature = "experimental-analysis")]
    pub(super) fn retirement_work(&self) -> Option<usize> {
        2usize.checked_add(self.slots.len())
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn slot_names(&self) -> &[Name] {
        &self.slots
    }

    pub(in crate::types) fn unknown() -> Self {
        Self {
            slots: Box::default(),
            dictionary: InstanceDictionary::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SlotSelectorWork {
    Begin,
    Header,
    PlaceTable,
    Symbol,
    UseDef,
    Bindings,
    BindingAdvance,
    ExplicitBases,
    BaseAdvance,
    BaseCompare {
        inline_bytes: usize,
    },
    Version,
    SlotDefinition,
    Layout,
    NameAdvance,
    NameCompare {
        candidate_bytes: usize,
        requested_bytes: usize,
    },
    DictionaryName {
        requested_bytes: usize,
    },
    Stub,
    Publish,
}

impl SlotSelectorWork {
    pub(in crate::types) fn work_units(self) -> Option<usize> {
        match self {
            Self::BaseCompare { inline_bytes } => 1usize.checked_add(inline_bytes),
            Self::NameCompare {
                candidate_bytes,
                requested_bytes,
            } => 1usize
                .checked_add(candidate_bytes)?
                .checked_add(requested_bytes),
            Self::DictionaryName { requested_bytes } => {
                1usize.checked_add(8)?.checked_add(requested_bytes)
            }
            _ => Some(1),
        }
    }
}

pub(in crate::types) trait SlotSelectorEffects<'db>:
    MemberSourceEffects<'db>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>)
    -> Result<ScopeId<'db>, Self::Error>;
    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error>;
    async fn dataclass_flags(
        &self,
        params: DataclassParams<'db>,
    ) -> Result<DataclassFlags, Self::Error>;
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;
    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>)
    -> Result<bool, Self::Error>;

    async fn slot_checkpoint(&self, work: SlotSelectorWork) -> Result<(), Self::Error>;
    async fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<bool>, Self::Error>;
    async fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;
    async fn source_python_version(
        &self,
        scope: ScopeId<'db>,
    ) -> Result<PythonVersion, Self::Error>;
    async fn slot_definition(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db SlotDefinition, Self::Error>;
    async fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db InstanceLayout, Self::Error>;
    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
}

pub(in crate::types) trait SynchronousSlotSelectorEffects<'db>:
    SynchronousMemberSourceEffects<'db>
{
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
    fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error>;
    fn dataclass_flags(&self, params: DataclassParams<'db>) -> Result<DataclassFlags, Self::Error>;
    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    fn slot_checkpoint(&self, work: SlotSelectorWork) -> Result<(), Self::Error>;
    fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<bool>, Self::Error>;
    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;
    fn source_python_version(&self, scope: ScopeId<'db>) -> Result<PythonVersion, Self::Error>;
    fn slot_definition(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db SlotDefinition, Self::Error>;
    fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db InstanceLayout, Self::Error>;
    fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
}

impl<'db> SynchronousSlotSelectorEffects<'db> for InlineMemberSourceEffects<'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error> {
        Ok(class.body_scope(self.db))
    }
    fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error> {
        Ok(class.dataclass_params(self.db))
    }
    fn dataclass_flags(&self, params: DataclassParams<'db>) -> Result<DataclassFlags, Self::Error> {
        Ok(params.flags(self.db))
    }
    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_bases(self.db))
    }

    fn slot_checkpoint(&self, _: SlotSelectorWork) -> Result<(), Self::Error> {
        Ok(())
    }
    fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<bool>, Self::Error> {
        Ok(next_slot_binding_has_definition(bindings))
    }
    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(class.explicit_bases(self.db))
    }
    fn source_python_version(&self, scope: ScopeId<'db>) -> Result<PythonVersion, Self::Error> {
        Ok(ProgramEnvironment::from_scope(scope).python_version(self.db))
    }
    fn slot_definition(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db SlotDefinition, Self::Error> {
        Ok(class.slot_definition(self.db))
    }
    fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db InstanceLayout, Self::Error> {
        Ok(class.instance_layout(self.db))
    }
    fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.file(self.db).is_stub(self.db))
    }
}

fn slot_bindings<'map, 'db>(
    use_def: &'map UseDefMap<'db>,
    symbol: ScopedSymbolId,
) -> BindingWithConstraintsIterator<'map, 'db> {
    use_def.end_of_scope_symbol_bindings(symbol)
}

pub(in crate::types) fn next_slot_binding_has_definition<'map, 'db>(
    bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
) -> Option<bool> {
    bindings
        .next()
        .map(|binding| binding.binding.definition().is_some())
}

fn slot_definition_names(definition: &SlotDefinition) -> Option<&[Name]> {
    match definition {
        SlotDefinition::Names(names) => Some(names),
        SlotDefinition::NonEmpty | SlotDefinition::DynamicOrNone => None,
    }
}
fn slot_layout_names(layout: &InstanceLayout) -> &[Name] {
    &layout.slots
}
fn slot_layout_has_dictionary(layout: &InstanceLayout) -> bool {
    layout.dictionary != InstanceDictionary::Absent
}
fn slot_name_at(slots: &[Name], index: usize) -> Option<&Name> {
    slots.get(index)
}
fn slot_base_at<'db>(bases: &[Type<'db>], index: usize) -> Option<Type<'db>> {
    bases.get(index).copied()
}
fn slot_name_equal(candidate: &Name, requested: &str) -> bool {
    candidate.as_str() == requested
}
fn slot_is_dictionary_name(name: &str) -> bool {
    name == "__dict__"
}
fn slot_is_named_tuple_base(base: Type<'_>) -> bool {
    base == Type::SpecialForm(SpecialFormType::NamedTuple)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn own_class_binding_with<'a, 'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    name: &'a str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    effects.slot_checkpoint(SlotSelectorWork::Header).await?;
    let scope = effects.body_scope(class).await?;
    effects
        .slot_checkpoint(SlotSelectorWork::PlaceTable)
        .await?;
    let table = effects.place_table(scope).await?;
    effects.slot_checkpoint(SlotSelectorWork::Symbol).await?;
    let Some(symbol) = effects.symbol_id(table, name).await? else {
        effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
        return Ok(false);
    };
    effects.slot_checkpoint(SlotSelectorWork::UseDef).await?;
    let use_def = effects.use_def_map(scope).await?;
    effects.slot_checkpoint(SlotSelectorWork::Bindings).await?;
    let mut bindings = slot_bindings(use_def, symbol);
    loop {
        match effects.next_binding_has_definition(&mut bindings).await? {
            Some(true) => {
                effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
                return Ok(true);
            }
            None => {
                effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
                return Ok(false);
            }
            Some(false) => {}
        }
    }
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn generated_slots_with<'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    effects.slot_checkpoint(SlotSelectorWork::Header).await?;
    if let Some(params) = effects.dataclass_params(class).await? {
        effects.slot_checkpoint(SlotSelectorWork::Header).await?;
        let flags = effects.dataclass_flags(params).await?;
        if DataclassFlags::contains(&flags, DataclassFlags::SLOTS) {
            let scope = effects.body_scope(class).await?;
            effects.slot_checkpoint(SlotSelectorWork::Version).await?;
            if effects.source_python_version(scope).await? >= PythonVersion::PY310 {
                effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
                return Ok(true);
            }
        }
    }
    let result = named_tuple_slots_with(class, effects).await?;
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn named_tuple_slots_with<'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    effects.slot_checkpoint(SlotSelectorWork::Header).await?;
    let result = if effects.has_explicit_bases(class).await? {
        effects
            .slot_checkpoint(SlotSelectorWork::ExplicitBases)
            .await?;
        let bases = effects.explicit_bases(class).await?;
        slot_named_tuple_base_with(bases, effects).await?
    } else {
        false
    };
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn slot_names_with<'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<Option<&'db [Name]>, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    let result = if own_class_binding_with(class, "__slots__", effects).await?
        || generated_slots_with(class, effects).await?
    {
        effects
            .slot_checkpoint(SlotSelectorWork::SlotDefinition)
            .await?;
        let definition = effects.slot_definition(class).await?;
        slot_definition_names(definition)
    } else {
        None
    };
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn instance_slot_with<'a, 'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    name: &'a str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    effects.slot_checkpoint(SlotSelectorWork::Layout).await?;
    let layout = effects.instance_layout(class).await?;
    let result = slot_name_contains_with(slot_layout_names(layout), name, effects).await?;
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn instance_dictionary_with<'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    if !own_class_binding_with(class, "__slots__", effects).await?
        && !generated_slots_with(class, effects).await?
    {
        effects.slot_checkpoint(SlotSelectorWork::Header).await?;
        if let None = effects.known(class).await? {
            effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
            return Ok(true);
        }
    }
    effects.slot_checkpoint(SlotSelectorWork::Layout).await?;
    let layout = effects.instance_layout(class).await?;
    let result = slot_layout_has_dictionary(layout);
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn lacks_instance_storage_with<'a, 'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    name: &'a str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    let result = if let Some(_) = slot_names_with(class, effects).await? {
        !instance_slot_with(class, name, effects).await?
            && !instance_dictionary_with(class, effects).await?
    } else {
        false
    };
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
pub(in crate::types) async fn own_slot_descriptor_with<'a, 'db, E: SlotSelectorEffects<'db>>(
    class: StaticClassLiteral<'db>,
    name: &'a str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    // The inherited `object.__dict__` annotation already describes dictionary access. A
    // synthesized slot descriptor would incorrectly replace the class's own namespace.
    effects
        .slot_checkpoint(SlotSelectorWork::DictionaryName {
            requested_bytes: str::len(name),
        })
        .await?;
    let result = if slot_is_dictionary_name(name) {
        false
    } else if let Some(slots) = slot_names_with(class, effects).await? {
        slot_name_contains_with(slots, name, effects).await?
            && (generated_slots_with(class, effects).await?
                || !own_class_binding_with(class, name, effects).await?
                || {
                    effects.slot_checkpoint(SlotSelectorWork::Stub).await?;
                    effects.is_stub(class).await?
                        && instance_slot_with(class, name, effects).await?
                })
    } else {
        false
    };
    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_slot_selector]
async fn slot_name_contains_with<'a, 'b, 'db, E: SlotSelectorEffects<'db>>(
    slots: &'a [Name],
    name: &'b str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    let mut index = 0;
    loop {
        effects
            .slot_checkpoint(SlotSelectorWork::NameAdvance)
            .await?;
        let Some(candidate) = slot_name_at(slots, index) else {
            effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
            return Ok(false);
        };
        effects
            .slot_checkpoint(SlotSelectorWork::NameCompare {
                candidate_bytes: str::len(Name::as_str(candidate)),
                requested_bytes: str::len(name),
            })
            .await?;
        if slot_name_equal(candidate, name) {
            effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
            return Ok(true);
        }
        index += 1;
    }
}

#[ty_mapping_probe_macros::dual_slot_selector]
async fn slot_named_tuple_base_with<'a, 'db, E: SlotSelectorEffects<'db>>(
    bases: &'a [Type<'db>],
    effects: &E,
) -> Result<bool, E::Error> {
    effects.slot_checkpoint(SlotSelectorWork::Begin).await?;
    let mut index = 0;
    loop {
        effects
            .slot_checkpoint(SlotSelectorWork::BaseAdvance)
            .await?;
        let Some(base) = slot_base_at(bases, index) else {
            effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
            return Ok(false);
        };
        effects
            .slot_checkpoint(SlotSelectorWork::BaseCompare {
                inline_bytes: Type::inline_payload_bytes(base),
            })
            .await?;
        if slot_is_named_tuple_base(base) {
            effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
            return Ok(true);
        }
        index += 1;
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Returns whether this class body explicitly defines `__slots__`.
    pub(crate) fn has_explicit_slots(self, db: &'db dyn Db) -> bool {
        match own_class_binding_sync(self, "__slots__", &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns whether a binding for this name reaches the end of the class body.
    fn has_own_class_binding(self, db: &'db dyn Db, name: &str) -> bool {
        match own_class_binding_sync(self, name, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns this class's explicit or generated slot names when they are statically known.
    ///
    /// Inherited slots are excluded; callers that need the complete layout should use
    /// [`Self::has_instance_slot`]. A dynamic declaration returns `None` rather than guessing.
    pub(crate) fn slot_names(self, db: &'db dyn Db) -> Option<&'db [Name]> {
        match slot_names_sync(self, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns whether this class definitely introduces at least one instance slot.
    pub(super) fn has_nonempty_slots(self, db: &'db dyn Db) -> bool {
        (self.has_explicit_slots(db) || self.has_generated_slots(db))
            && match self.slot_definition(db) {
                SlotDefinition::Names(names) => !names.is_empty(),
                SlotDefinition::NonEmpty => true,
                SlotDefinition::DynamicOrNone => false,
            }
    }

    /// Returns whether this class synthesizes slots through a dataclass or named tuple.
    pub(in crate::types) fn has_generated_slots(self, db: &'db dyn Db) -> bool {
        match generated_slots_sync(self, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns whether this class directly inherits the synthesized named-tuple layout.
    fn has_named_tuple_slots(self, db: &'db dyn Db) -> bool {
        match named_tuple_slots_sync(self, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Resolves explicit slots, empty named-tuple layouts, and slotted dataclass fields.
    ///
    /// Tuple and string values retain their inferred literal types; mutable list, set, and
    /// dictionary literals are resolved from the indexed reaching assignment.
    fn slot_definition(self, db: &'db dyn Db) -> &'db SlotDefinition {
        slot_definition(db, self)
    }

    /// Collects slot storage and instance-dictionary availability across the complete MRO.
    ///
    /// ```python
    /// class Base:
    ///     __slots__ = ("value",)
    ///
    /// class Child(Base):
    ///     __slots__ = ("other", "__dict__")
    /// ```
    ///
    /// Here, `Child` has both slots and can also store additional dictionary-backed attributes.
    fn instance_layout(self, db: &'db dyn Db) -> &'db InstanceLayout {
        instance_layout(db, self)
    }

    /// Returns whether instance dictionary storage exists or cannot be ruled out.
    fn has_instance_dictionary(self, db: &'db dyn Db) -> bool {
        match instance_dictionary_sync(self, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns whether this class or any base defines a slot with the given name.
    pub(crate) fn has_instance_slot(self, db: &'db dyn Db, name: &str) -> bool {
        match instance_slot_sync(self, name, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Whether a known slotted layout has no instance storage available for `name`.
    ///
    /// An unknown layout remains permissive, as do builtins whose C-level storage is not fully
    /// described by their stubs.
    pub(crate) fn lacks_instance_storage(self, db: &'db dyn Db, name: &str) -> bool {
        match lacks_instance_storage_sync(self, name, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Whether this class creates a descriptor for `name` in its own namespace.
    pub(in crate::types) fn has_own_slot_descriptor(self, db: &'db dyn Db, name: &str) -> bool {
        match own_slot_descriptor_sync(self, name, &InlineMemberSourceEffects::new(db)) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Synthesizes the class descriptor created for an instance slot.
    ///
    /// ```python
    /// class Example:
    ///     __slots__ = ("value", "__weakref__")
    /// ```
    ///
    /// Ordinary slots use `MemberDescriptorType` descriptors. The weak-reference slot uses the
    /// `GetSetDescriptorType` descriptor declared in typeshed.
    pub(super) fn own_slot_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Type<'db> {
        if name == "__weakref__" {
            return KnownClass::GetSetDescriptorType.to_instance(db, env);
        }

        let value_ty = self
            .own_instance_member(db, env, name)
            .ignore_possibly_undefined()
            .map(|ty| ty.apply_optional_specialization(db, specialization))
            .unwrap_or_else(Type::unknown);

        Type::SlotDescriptor(SlotDescriptorType::new(db, value_ty))
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) SlotDefinitionConfiguration), self_ty = StaticClassLiteral<'db>,
    returns(ref),
    cycle_initial=|_, _, _| SlotDefinition::DynamicOrNone,
    heap_size=ruff_memory_usage::heap_size,
)]
fn slot_definition<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> SlotDefinition {
    let body_scope = class.body_scope(db);
    // A bare annotation does not bind `__slots__`, but an annotated assignment does:
    //
    //     __slots__: tuple[str, ...]
    //     __slots__: tuple[str, ...] = ("value",)
    let Some(symbol) = place_table(db, body_scope)
        .symbol_id("__slots__")
        .filter(|_| class.has_explicit_slots(db))
    else {
        if class.has_named_tuple_slots(db) {
            return SlotDefinition::Names(Box::default());
        }

        if !class.has_generated_slots(db) {
            return SlotDefinition::DynamicOrNone;
        }

        // Dataclasses generate slots for their fields, excluding inherited storage:
        //
        //     class Base:
        //         __slots__ = ("inherited",)
        //
        //     @dataclass(slots=True, weakref_slot=True)
        //     class Child(Base):
        //         inherited: int
        //         value: int
        //
        // Here, `Child.__slots__` contains only `value` and `__weakref__`.
        let field_policy = CodeGeneratorKind::DataclassLike(None);
        let inherited_slots: FxIndexSet<_> = class
            .iter_mro(db, None)
            .skip(1)
            .filter_map(ClassBase::into_class)
            .filter_map(|class| class.static_class_literal(db).map(|(class, _)| class))
            .filter_map(|class| class.slot_names(db))
            .flatten()
            .cloned()
            .collect();
        let weakref_name = Name::new_static("__weakref__");
        let mut names: Vec<_> = class
            .fields(db, None, field_policy)
            .keys()
            .filter(|name| !inherited_slots.contains(*name))
            .cloned()
            .collect();
        if class.has_dataclass_param(db, field_policy, DataclassFlags::WEAKREF_SLOT)
            && !inherited_slots.contains(&weakref_name)
        {
            names.push(weakref_name);
        }
        return SlotDefinition::Names(names.into_boxed_slice());
    };

    // A conditional assignment does not establish one definite layout:
    //
    //     if condition:
    //         __slots__ = ("value",)
    let env = ProgramEnvironment::from_scope(body_scope);
    let use_def = use_def_map(db, body_scope);
    let bindings = use_def.end_of_scope_symbol_bindings(symbol);
    let Place::Defined(DefinedPlace {
        ty: slots_ty,
        definedness: Definedness::AlwaysDefined,
        ..
    }) = place_from_bindings(db, &env, bindings).place
    else {
        return SlotDefinition::DynamicOrNone;
    };

    // A single string is itself a slot name: `__slots__ = "value"`.
    if let Some(name) = slots_ty.as_string_literal() {
        return SlotDefinition::Names(Box::new([Name::new(name.value(db))]));
    }

    // Tuple inference preserves individual names, including names supplied indirectly:
    //
    //     names = ("first", "second")
    //     __slots__ = names
    //
    // An unknown element prevents recovering every name. A variable-length tuple still
    // proves the declaration is nonempty when its minimum length is greater than zero.
    if let Some(tuple) = slots_ty.tuple_instance_spec(db, &env) {
        match &*tuple {
            Tuple::Fixed(tuple) => {
                return tuple
                    .iter_all_elements()
                    .map(|element| {
                        element
                            .as_string_literal()
                            .map(|literal| Name::new(literal.value(db)))
                    })
                    .collect::<Option<Box<[_]>>>()
                    .map_or(SlotDefinition::NonEmpty, SlotDefinition::Names);
            }
            Tuple::Variable(_) if tuple.len().minimum() > 0 => {
                return SlotDefinition::NonEmpty;
            }
            Tuple::Variable(_) => {}
        }
    }

    // Mutable container types do not retain their individual literal elements:
    //
    //     __slots__ = ["value"]
    //     __slots__ = {"value"}
    //     __slots__ = {"value": "Documentation"}
    //
    // Recover each element's inferred string-literal type from the single reaching class-body
    // assignment instead, so names supplied through other variables are also recognized.
    let Ok(definition) = use_def
        .end_of_scope_symbol_bindings(symbol)
        .filter_map(|binding| binding.binding.definition())
        .exactly_one()
    else {
        return SlotDefinition::DynamicOrNone;
    };

    let parsed = parsed_module(db, class.python_file(db)).load(db);
    let Some(value) = definition.kind(db).value(&parsed) else {
        return SlotDefinition::DynamicOrNone;
    };

    let literal_slot_name = |expression: &ast::Expr| {
        definition_expression_type(db, definition, expression)
            .as_string_literal()
            .map(|literal| Name::new(literal.value(db)))
    };

    let names = match value {
        ast::Expr::List(list) => list.elts.iter().map(literal_slot_name).collect(),
        ast::Expr::Set(set) => set.elts.iter().map(literal_slot_name).collect(),
        ast::Expr::Dict(dictionary) => dictionary
            .items
            .iter()
            .map(|item| item.key.as_ref().and_then(literal_slot_name))
            .collect(),
        _ => None,
    };

    names.map_or(SlotDefinition::DynamicOrNone, SlotDefinition::Names)
}

#[salsa::tracked(configuration = (pub(in crate::types) InstanceLayoutConfiguration), self_ty = StaticClassLiteral<'db>, attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _| InstanceLayout::unknown(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn instance_layout<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> InstanceLayout {
    match layout::instance_layout_sync(
        class,
        layout::InstanceLayoutFacts,
        &InlineMemberSourceEffects::new(db),
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn instance_layout_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<InstanceLayoutConfiguration> {
    instance_layout::fn_ingredient_(db, db.zalsa())
}

#[cfg(feature = "experimental-analysis")]
crate::types::class::runtime::class_memo_schema! {
    pub(super) type ClassMemoSchema<'db> = crate::types::StaticClassLiteral<'static>;
    pub(super) fn register_class_memos;
    (slot_definition, crate::types::class::runtime::SlotDefinitionProfile),
            (instance_layout, crate::types::class::runtime::InstanceLayoutProfile)
}
