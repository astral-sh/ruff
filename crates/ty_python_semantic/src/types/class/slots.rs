use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use ty_python_core::{place_table, use_def_map};

use crate::place::{DefinedPlace, Definedness, Place, place_from_bindings};
use crate::types::class::{CodeGeneratorKind, StaticClassLiteral};
use crate::types::generics::Specialization;
use crate::types::{ClassBase, DataclassFlags, KnownClass, Type};
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// The information that can be recovered from a class's own `__slots__` assignment.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
enum SlotDefinition {
    /// Every declared slot name is statically known.
    Names(Box<[Name]>),
    /// The declaration is definitely nonempty, but at least one name is unknown.
    NonEmpty,
    /// Neither the names nor the presence of any slots can be established.
    Dynamic,
}

/// An interpreter-created `types.MemberDescriptorType` for an instance slot.
///
/// Unlike a Python property, an ordinary slot reads and writes its instance storage directly.
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
    /// Classify builtin storage and structural typing bases without relying on stub declarations.
    fn for_known_class(class: KnownClass) -> Self {
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
            | KnownClass::MemberDescriptorType
            | KnownClass::GetSetDescriptorType
            | KnownClass::Super
            | KnownClass::Sequence
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::Mapping
            | KnownClass::MutableMapping
            | KnownClass::Hashable
            | KnownClass::SupportsIndex => Self::Absent,
            KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::Warning
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod => Self::Present,
            _ => Self::Unknown,
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

fn literal_slot_name(expression: &ast::Expr) -> Option<Name> {
    expression
        .as_string_literal_expr()
        .map(|literal| Name::new(literal.value.to_str()))
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
            SlotDefinition::NonEmpty | SlotDefinition::Dynamic => None,
        }
    }

    /// Returns whether this class definitely introduces at least one instance slot.
    pub(super) fn has_nonempty_slots(self, db: &'db dyn Db) -> bool {
        (self.has_explicit_slots(db) || self.has_generated_slots(db))
            && match self.slot_definition(db) {
                SlotDefinition::Names(names) => !names.is_empty(),
                SlotDefinition::NonEmpty => true,
                SlotDefinition::Dynamic => false,
            }
    }

    /// Returns whether a dataclass decorator generates slots for this class.
    pub(super) fn has_generated_slots(self, db: &'db dyn Db) -> bool {
        self.dataclass_params(db).is_some_and(|parameters| {
            parameters.flags(db).contains(DataclassFlags::SLOTS)
                && ProgramEnvironment::from_scope(self.body_scope(db)).python_version(db)
                    >= PythonVersion::PY310
        })
    }

    /// Resolves explicit slot declarations and fields synthesized by slotted dataclasses.
    ///
    /// Tuple and string values retain their inferred literal types; mutable list, set, and
    /// dictionary literals are resolved from the indexed reaching assignment.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _| SlotDefinition::Dynamic,
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
            if !self.has_generated_slots(db) {
                return SlotDefinition::Dynamic;
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
            return SlotDefinition::Dynamic;
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
        // An unknown tuple element prevents recovering every name, but still proves the
        // declaration is nonempty.
        if let Type::NominalInstance(instance) = slots_ty
            && let Some(specification) = instance.tuple_spec(db, &env)
            && let Some(tuple) = specification.as_fixed_length()
        {
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

        // Mutable container types do not retain their individual literal elements:
        //
        //     __slots__ = ["value"]
        //     __slots__ = {"value"}
        //     __slots__ = {"value": "Documentation"}
        //
        // Recover their names from the single reaching class-body assignment instead.
        let Ok(definition) = use_def
            .end_of_scope_symbol_bindings(symbol)
            .filter_map(|binding| binding.binding.definition())
            .exactly_one()
        else {
            return SlotDefinition::Dynamic;
        };

        let parsed = parsed_module(db, self.python_file(db)).load(db);
        let Some(value) = definition.kind(db).value(&parsed) else {
            return SlotDefinition::Dynamic;
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

        names.map_or(SlotDefinition::Dynamic, SlotDefinition::Names)
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
            if matches!(base, ClassBase::Generic | ClassBase::Protocol) {
                continue;
            }

            let ClassBase::Class(base) = base else {
                dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
                continue;
            };
            let Some((base, _)) = base.static_class_literal(db) else {
                dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
                continue;
            };

            if let Some(names) = base.slot_names(db) {
                if names.iter().any(|name| name == "__dict__") {
                    dictionary = InstanceDictionary::Present;
                }
                slots.extend(names.iter().cloned());
            } else if base.has_explicit_slots(db) {
                dictionary = dictionary.inherited_with(InstanceDictionary::Unknown);
            } else if let Some(known) = base.known(db) {
                dictionary = dictionary.inherited_with(InstanceDictionary::for_known_class(known));
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
    pub(crate) fn has_instance_dictionary(self, db: &'db dyn Db) -> bool {
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
