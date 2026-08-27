use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use ty_python_core::{place_table, use_def_map};

use crate::place::{DefinedPlace, Definedness, Place, place_from_bindings};
use crate::types::class::{CodeGeneratorKind, StaticClassLiteral};
use crate::types::generics::Specialization;
use crate::types::{
    ClassBase, ClassLiteral, DataclassFlags, KnownClass, SpecialFormType, Type,
    definition_expression_type, tuple::Tuple,
};
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// The information that can be recovered from a class's own `__slots__` assignment.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
enum SlotDefinition {
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
enum InstanceDictionary {
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
struct InstanceLayout {
    slots: Box<[Name]>,
    dictionary: InstanceDictionary,
}

impl InstanceLayout {
    fn unknown() -> Self {
        Self {
            slots: Box::default(),
            dictionary: InstanceDictionary::Unknown,
        }
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Returns whether this class body explicitly defines `__slots__`.
    pub(crate) fn has_explicit_slots(self, db: &'db dyn Db) -> bool {
        self.has_own_class_binding(db, "__slots__")
    }

    /// Returns whether a binding for this name reaches the end of the class body.
    pub(super) fn has_own_class_binding(self, db: &'db dyn Db, name: &str) -> bool {
        let scope = self.body_scope(db);
        place_table(db, scope)
            .symbol_id(name)
            .is_some_and(|symbol| {
                use_def_map(db, scope)
                    .end_of_scope_symbol_bindings(symbol)
                    .any(|binding| binding.binding.definition().is_some())
            })
    }

    /// Returns this class's explicit or generated slot names when they are statically known.
    ///
    /// Inherited slots are excluded; callers that need the complete layout should use
    /// [`Self::has_instance_slot`]. A dynamic declaration returns `None` rather than guessing.
    pub(crate) fn slot_names(self, db: &'db dyn Db) -> Option<&'db [Name]> {
        if !self.has_explicit_slots(db) && !self.has_generated_slots(db) {
            return None;
        }

        match self.slot_definition(db) {
            SlotDefinition::Names(names) => Some(names),
            SlotDefinition::NonEmpty | SlotDefinition::DynamicOrNone => None,
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
    pub(super) fn has_generated_slots(self, db: &'db dyn Db) -> bool {
        self.dataclass_params(db).is_some_and(|parameters| {
            parameters.flags(db).contains(DataclassFlags::SLOTS)
                && ProgramEnvironment::from_scope(self.body_scope(db)).python_version(db)
                    >= PythonVersion::PY310
        }) || self.has_named_tuple_slots(db)
    }

    /// Returns whether this class directly inherits the synthesized named-tuple layout.
    fn has_named_tuple_slots(self, db: &'db dyn Db) -> bool {
        self.has_explicit_bases(db)
            && self
                .explicit_bases(db)
                .contains(&Type::SpecialForm(SpecialFormType::NamedTuple))
    }

    /// Resolves explicit slots, empty named-tuple layouts, and slotted dataclass fields.
    ///
    /// Tuple and string values retain their inferred literal types; mutable list, set, and
    /// dictionary literals are resolved from the indexed reaching assignment.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _| SlotDefinition::DynamicOrNone,
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn slot_definition(self, db: &'db dyn Db) -> SlotDefinition {
        let body_scope = self.body_scope(db);
        // A bare annotation does not bind `__slots__`, but an annotated assignment does:
        //
        //     __slots__: tuple[str, ...]
        //     __slots__: tuple[str, ...] = ("value",)
        let Some(symbol) = place_table(db, body_scope)
            .symbol_id("__slots__")
            .filter(|_| self.has_explicit_slots(db))
        else {
            if self.has_named_tuple_slots(db) {
                return SlotDefinition::Names(Box::default());
            }

            if !self.has_generated_slots(db) {
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
            let inherited_slots: FxIndexSet<_> = self
                .iter_mro(db, None)
                .skip(1)
                .filter_map(ClassBase::into_class)
                .filter_map(|class| class.static_class_literal(db).map(|(class, _)| class))
                .filter_map(|class| class.slot_names(db))
                .flatten()
                .cloned()
                .collect();
            let weakref_name = Name::new_static("__weakref__");
            let mut names: Vec<_> = self
                .fields(db, None, field_policy)
                .keys()
                .filter(|name| !inherited_slots.contains(*name))
                .cloned()
                .collect();
            if self.has_dataclass_param(db, field_policy, DataclassFlags::WEAKREF_SLOT)
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

        let parsed = parsed_module(db, self.python_file(db)).load(db);
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
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _| InstanceLayout::unknown(),
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn instance_layout(self, db: &'db dyn Db) -> InstanceLayout {
        if self.is_protocol(db) {
            return InstanceLayout::unknown();
        }

        let mut slots = FxIndexSet::default();
        let mut dictionary = InstanceDictionary::Absent;

        for base in self.iter_mro(db, None) {
            let base = match base {
                ClassBase::Class(base) => base,
                ClassBase::Any | ClassBase::Divergent(_) | ClassBase::Dynamic(_) => {
                    dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
                    continue;
                }
                ClassBase::TypedDict(_) | ClassBase::Generic | ClassBase::Protocol => continue,
            };

            let base = match base.class_literal(db) {
                ClassLiteral::Static(base) => base,
                // Functional named tuples synthesize empty slots, while TypedDict instances use
                // dictionary item storage rather than an instance-attribute dictionary.
                ClassLiteral::DynamicNamedTuple(_) | ClassLiteral::DynamicTypedDict(_) => continue,
                // Enum instances retain an instance dictionary even when the enum is created
                // through the functional API.
                ClassLiteral::DynamicEnum(_) => {
                    dictionary = InstanceDictionary::Present;
                    continue;
                }
                ClassLiteral::Dynamic(_) => {
                    dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
                    continue;
                }
            };

            if let Some(names) = base.slot_names(db) {
                if names.iter().any(|name| name == "__dict__") {
                    dictionary = InstanceDictionary::Present;
                }
                slots.extend(names.iter().cloned());
            } else if base.has_explicit_slots(db) {
                dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
            } else if let Some(known_dictionary) =
                base.known(db).and_then(InstanceDictionary::for_known_class)
            {
                dictionary = dictionary.inherited_with(known_dictionary);
            } else if !base.is_protocol(db) {
                dictionary = InstanceDictionary::Present;
            }
        }

        InstanceLayout {
            slots: slots.into_iter().collect(),
            dictionary,
        }
    }

    /// Returns whether instance dictionary storage exists or cannot be ruled out.
    fn has_instance_dictionary(self, db: &'db dyn Db) -> bool {
        if !self.has_explicit_slots(db) && !self.has_generated_slots(db) && self.known(db).is_none()
        {
            return true;
        }

        self.instance_layout(db).dictionary != InstanceDictionary::Absent
    }

    /// Returns whether this class or any base defines a slot with the given name.
    pub(crate) fn has_instance_slot(self, db: &'db dyn Db, name: &str) -> bool {
        self.instance_layout(db)
            .slots
            .iter()
            .any(|slot| slot == name)
    }

    /// Whether a known slotted layout has no instance storage available for `name`.
    ///
    /// An unknown layout remains permissive, as do builtins whose C-level storage is not fully
    /// described by their stubs.
    pub(crate) fn lacks_instance_storage(self, db: &'db dyn Db, name: &str) -> bool {
        self.slot_names(db).is_some()
            && !self.has_instance_slot(db, name)
            && !self.has_instance_dictionary(db)
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
