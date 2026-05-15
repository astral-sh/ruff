use std::fmt::Write;

pub(crate) use self::dynamic_literal::{
    DynamicClassAnchor, DynamicClassLiteral, DynamicMetaclassConflict, dynamic_class_bases_argument,
};
pub(super) use self::enum_literal::{DynamicEnumAnchor, DynamicEnumLiteral, EnumSpec};
pub use self::known::KnownClass;
use self::named_tuple::synthesize_namedtuple_class_member;
pub(super) use self::named_tuple::{
    DynamicNamedTupleAnchor, DynamicNamedTupleLiteral, NamedTupleField, NamedTupleSpec,
};
pub(crate) use self::static_literal::{
    ExpandedClassBaseEntry, StaticClassLiteral, expanded_class_base_entries,
};
pub(super) use self::typed_dict::{DynamicTypedDictAnchor, DynamicTypedDictLiteral};
use super::{
    BoundTypeVarInstance, MemberLookupPolicy, MroIterator, SpecialFormType, SubclassOfType, Type,
    TypeQualifiers, class_base::ClassBase, function::FunctionType,
};
use super::{TypeVarVariance, display};
use crate::place::{DefinedPlace, TypeOrigin};
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension,
};
use crate::types::enums::enum_metadata;
use crate::types::function::{AbstractMethodKind, DataclassTransformerParams};
use crate::types::generics::{
    GenericContext, InferableTypeVars, Specialization, walk_specialization,
};
use crate::types::known_instance::DeprecatedInstance;
use crate::types::member::Member;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker,
};
use crate::types::signatures::{
    CallableSignature, Parameter, Parameters, Signature, SignatureRelationVisitor,
};
use crate::types::tuple::TupleSpec;
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, CallableTypes, DataclassParams,
    FindLegacyTypeVarsVisitor, IntersectionType, TypeContext, TypeMapping, UnionBuilder,
    VarianceInferable,
};
use crate::{
    Db, FxIndexMap, FxOrderSet,
    place::{
        Definedness, LookupError, LookupResult, Place, PlaceAndQualifiers, PublicTypePolicy,
        place_from_bindings, place_from_declarations,
    },
    types::{MetaclassCandidate, TypeDefinition, UnionType},
};
use ruff_db::diagnostic::Span;
use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast};
use ruff_text_size::TextRange;
use ty_python_core::definition::Definition;
use ty_python_core::{place_table, use_def_map};

mod dynamic_literal;
mod enum_literal;
mod known;
mod named_tuple;
mod static_literal;
mod typed_dict;

/// A category of classes with code generation capabilities (with synthesized methods).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum CodeGeneratorKind<'db> {
    /// Classes decorated with `@dataclass` or similar dataclass-like decorators
    DataclassLike(Option<DataclassTransformerParams<'db>>),
    /// Classes inheriting from `typing.NamedTuple`
    NamedTuple,
    /// Classes inheriting from `typing.TypedDict`
    TypedDict,
}

impl<'db> CodeGeneratorKind<'db> {
    pub(crate) fn from_class(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Self> {
        match class {
            ClassLiteral::Static(static_class) => {
                Self::from_static_class(db, static_class, specialization)
            }
            ClassLiteral::Dynamic(dynamic_class) => Self::from_dynamic_class(db, dynamic_class),
            ClassLiteral::DynamicNamedTuple(_) => Some(Self::NamedTuple),
            ClassLiteral::DynamicTypedDict(_) => Some(Self::TypedDict),
            ClassLiteral::DynamicEnum(_) => None,
        }
    }

    fn from_static_class(
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Self> {
        #[salsa::tracked(cycle_initial=|_, _, _, _| None,
            heap_size=ruff_memory_usage::heap_size
        )]
        fn code_generator_of_static_class<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
            specialization: Option<Specialization<'db>>,
        ) -> Option<CodeGeneratorKind<'db>> {
            // If a class is directly decorated as a dataclass, it's a dataclass.
            // If a class' metaclass is a dataclass transformer, it's a dataclass.
            // If a class inherits from a base class that is a dataclass
            // transformer, it's a dataclass (unless it is a subclass of `type`,
            // in which case we assume the subclass is itself also meant for use
            // as a metaclass dataclass transformer, not itself supposed to be a
            // dataclass.)
            if class.dataclass_params(db).is_some() {
                Some(CodeGeneratorKind::DataclassLike(None))
            } else if let Ok((_, Some(info))) = class.try_metaclass(db) {
                Some(CodeGeneratorKind::DataclassLike(Some(info.params)))
            } else if KnownClass::Type
                .try_to_class_literal(db)
                .is_none_or(|type_class| {
                    !class.is_subclass_of(
                        db,
                        None,
                        ClassType::NonGeneric(ClassLiteral::Static(type_class)),
                    )
                })
                && let Some(transformer_params) =
                    class.iter_mro(db, specialization).skip(1).find_map(|base| {
                        base.into_class().and_then(|class| {
                            class
                                .static_class_literal(db)
                                .and_then(|(lit, _)| lit.dataclass_transformer_params(db))
                        })
                    })
            {
                Some(CodeGeneratorKind::DataclassLike(Some(transformer_params)))
            } else if class
                .explicit_bases(db)
                .contains(&Type::SpecialForm(SpecialFormType::NamedTuple))
            {
                Some(CodeGeneratorKind::NamedTuple)
            } else if class.is_typed_dict(db) {
                Some(CodeGeneratorKind::TypedDict)
            } else {
                None
            }
        }

        code_generator_of_static_class(db, class, specialization)
    }

    fn from_dynamic_class(db: &'db dyn Db, class: DynamicClassLiteral<'db>) -> Option<Self> {
        #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
        fn code_generator_of_dynamic_class<'db>(
            db: &'db dyn Db,
            class: DynamicClassLiteral<'db>,
        ) -> Option<CodeGeneratorKind<'db>> {
            // Check if the dynamic class was passed to `dataclass()` as a function.
            if class.dataclass_params(db).is_some() {
                return Some(CodeGeneratorKind::DataclassLike(None));
            }

            // Dynamic classes can also inherit from classes with dataclass_transform.
            class.iter_mro(db).skip(1).find_map(|base| {
                base.into_class().and_then(|class| {
                    class
                        .static_class_literal(db)
                        .and_then(|(lit, _)| lit.dataclass_transformer_params(db))
                        .map(|params| CodeGeneratorKind::DataclassLike(Some(params)))
                })
            })
        }

        code_generator_of_dynamic_class(db, class)
    }

    pub(super) fn matches(
        self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> bool {
        matches!(
            (
                CodeGeneratorKind::from_class(db, class, specialization),
                self
            ),
            (Some(Self::DataclassLike(_)), Self::DataclassLike(_))
                | (Some(Self::NamedTuple), Self::NamedTuple)
                | (Some(Self::TypedDict), Self::TypedDict)
        )
    }

    pub(super) fn dataclass_transformer_params(self) -> Option<DataclassTransformerParams<'db>> {
        match self {
            Self::DataclassLike(params) => params,
            Self::NamedTuple | Self::TypedDict => None,
        }
    }
}

/// A specialization of a generic class with a particular assignment of types to typevars.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct GenericAlias<'db> {
    pub(crate) origin: StaticClassLiteral<'db>,
    pub(crate) specialization: Specialization<'db>,
}

pub(super) fn walk_generic_alias<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    alias: GenericAlias<'db>,
    visitor: &V,
) {
    walk_specialization(db, alias.specialization(db), visitor);
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for GenericAlias<'_> {}

impl<'db> GenericAlias<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.origin(db),
            self.specialization(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.origin(db).definition(db)
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let tcx = tcx
            .annotation
            .and_then(|ty| ty.specialization_of(db, self.origin(db)))
            .map(|specialization| specialization.types(db))
            .unwrap_or(&[]);

        Self::new(
            db,
            self.origin(db),
            self.specialization(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.specialization(db)
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    pub(crate) fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        self.origin(db).is_typed_dict(db)
    }
}

impl<'db> From<GenericAlias<'db>> for Type<'db> {
    fn from(alias: GenericAlias<'db>) -> Type<'db> {
        Type::GenericAlias(alias)
    }
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for GenericAlias<'db> {
    #[salsa::tracked(
        cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        let origin = self.origin(db);

        let specialization = self.specialization(db);

        // if the class is the thing defining the variable, then it can
        // reference it without it being applied to the specialization
        std::iter::once(origin.variance_of(db, typevar))
            .chain(
                specialization
                    .generic_context(db)
                    .variables(db)
                    .zip(specialization.types(db))
                    .map(|(generic_typevar, ty)| {
                        if let Some(explicit_variance) =
                            generic_typevar.typevar(db).explicit_variance(db)
                        {
                            ty.with_polarity(explicit_variance).variance_of(db, typevar)
                        } else {
                            // `with_polarity` composes the passed variance with the
                            // inferred one. The inference is done lazily, as we can
                            // sometimes determine the result just from the passed
                            // variance. This operation is commutative, so we could
                            // infer either first.  We choose to make the `StaticClassLiteral`
                            // variance lazy, as it is known to be expensive, requiring
                            // that we traverse all members.
                            //
                            // If salsa let us look at the cache, we could check first
                            // to see if the class literal query was already run.

                            let typevar_variance_in_substituted_type = ty.variance_of(db, typevar);
                            origin
                                .with_polarity(typevar_variance_in_substituted_type)
                                .variance_of(db, generic_typevar)
                        }
                    }),
            )
            .collect()
    }
}

/// A class literal, either defined via a `class` statement or a `type` function call.
#[derive(
    Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update, get_size2::GetSize,
)]
pub enum ClassLiteral<'db> {
    /// A class defined via a `class` statement.
    Static(StaticClassLiteral<'db>),
    /// A class created dynamically via `type(name, bases, dict)`.
    Dynamic(DynamicClassLiteral<'db>),
    /// A class created via `collections.namedtuple()` or `typing.NamedTuple()`.
    DynamicNamedTuple(DynamicNamedTupleLiteral<'db>),
    /// A class created via functional `TypedDict("Name", {...})`.
    DynamicTypedDict(DynamicTypedDictLiteral<'db>),
    /// A class created via functional enum syntax, e.g., `Enum("Color", "RED GREEN BLUE")`.
    DynamicEnum(DynamicEnumLiteral<'db>),
}

impl<'db> ClassLiteral<'db> {
    /// Return a `ClassLiteral` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class_literal(db)
            .as_class_literal()
            .expect("`object` should always be a non-generic class in typeshed")
    }

    /// Returns the name of the class.
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        match self {
            Self::Static(class) => class.name(db),
            Self::Dynamic(class) => class.name(db),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.name(db),
            Self::DynamicTypedDict(typeddict) => typeddict.name(db),
            Self::DynamicEnum(enum_lit) => enum_lit.name(db),
        }
    }

    /// Returns the known class, if any.
    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        self.as_static()?.known(db)
    }

    /// Returns whether this class has PEP 695 type parameters.
    pub(crate) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        self.as_static()
            .is_some_and(|class| class.has_pep_695_type_params(db))
    }

    /// Returns an iterator over the MRO.
    pub(crate) fn iter_mro(self, db: &'db dyn Db) -> MroIterator<'db> {
        MroIterator::new(db, self, None)
    }

    /// Returns the metaclass of this class.
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Static(class) => class.metaclass(db),
            Self::Dynamic(class) => class.metaclass(db),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.metaclass(db),
            Self::DynamicTypedDict(typeddict) => typeddict.metaclass(db),
            Self::DynamicEnum(enum_lit) => enum_lit.metaclass(db),
        }
    }

    /// Look up a class-level member by iterating through the MRO.
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Self::Static(class) => class.class_member(db, name, policy),
            Self::Dynamic(class) => class.class_member(db, name, policy),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.class_member(db, name, policy),
            Self::DynamicTypedDict(typeddict) => typeddict.class_member(db, name, policy),
            Self::DynamicEnum(enum_lit) => enum_lit.class_member(db, name),
        }
    }

    /// Look up a class-level member using a provided MRO iterator.
    ///
    /// This is used by `super()` to start the MRO lookup after the pivot class.
    pub(super) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Self::Static(class) => class.class_member_from_mro(db, name, policy, mro_iter),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => {
                // Dynamic classes don't have inherited generic context and are never `object`.
                let result = MroLookup::new(db, mro_iter).class_member(name, policy, None, false);
                match result {
                    ClassMemberResult::Done(result) => result.finalize(db),
                    ClassMemberResult::TypedDict => KnownClass::TypedDictFallback
                        .to_class_literal(db)
                        .find_name_in_mro_with_policy(db, name, policy)
                        .expect("Will return Some() when called on class literal"),
                }
            }
        }
    }

    /// Returns whether this is a known class.
    pub(crate) fn is_known(self, db: &'db dyn Db, known: KnownClass) -> bool {
        self.known(db) == Some(known)
    }

    /// Returns the default specialization for this class.
    ///
    /// For static classes, this applies default type arguments.
    /// For dynamic classes, this returns a non-generic class type.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::Static(class) => class.default_specialization(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => ClassType::NonGeneric(self),
        }
    }

    /// Returns the unknown specialization of this class.
    ///
    /// For non-generic classes, the class is returned unchanged.
    /// For a non-specialized generic class, we return a generic alias that maps each of the class's
    /// typevars to `Unknown`.
    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::Static(class) => class.unknown_specialization(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => ClassType::NonGeneric(self),
        }
    }

    /// Returns the identity specialization for this class (same as default for non-generic).
    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::Static(class) => class.identity_specialization(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => ClassType::NonGeneric(self),
        }
    }

    /// Returns the generic context if this is a generic class.
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.as_static().and_then(|class| class.generic_context(db))
    }

    /// Returns whether this class is a protocol.
    pub(crate) fn is_protocol(self, db: &'db dyn Db) -> bool {
        self.as_static().is_some_and(|class| class.is_protocol(db))
    }

    /// Returns whether this class is a `TypedDict`.
    pub fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        match self {
            Self::Static(class) => class.is_typed_dict(db),
            Self::DynamicTypedDict(_) => true,
            Self::Dynamic(_) | Self::DynamicNamedTuple(_) | Self::DynamicEnum(_) => false,
        }
    }

    /// Returns whether this class is `builtins.tuple` exactly
    pub(crate) fn is_tuple(self, db: &'db dyn Db) -> bool {
        match self {
            Self::Static(class) => class.is_tuple(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => false,
        }
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(crate) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self.metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Returns whether this class is type-check only.
    pub(crate) fn type_check_only(self, db: &'db dyn Db) -> bool {
        self.as_static()
            .is_some_and(|class| class.type_check_only(db))
    }

    /// Returns the file containing the class definition.
    pub(crate) fn file(self, db: &dyn Db) -> File {
        match self {
            Self::Static(class) => class.file(db),
            Self::Dynamic(class) => class.scope(db).file(db),
            Self::DynamicNamedTuple(class) => class.scope(db).file(db),
            Self::DynamicTypedDict(class) => class.scope(db).file(db),
            Self::DynamicEnum(enum_lit) => enum_lit.scope(db).file(db),
        }
    }

    /// Returns the range of the class's "header".
    ///
    /// For static classes, this is the class name and any arguments passed to the `class` statement.
    /// For dynamic classes, this is the entire `type()` call expression.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        match self {
            Self::Static(class) => class.header_range(db),
            Self::Dynamic(class) => class.header_range(db),
            Self::DynamicNamedTuple(class) => class.header_range(db),
            Self::DynamicTypedDict(class) => class.header_range(db),
            Self::DynamicEnum(enum_lit) => enum_lit.header_range(db),
        }
    }

    /// Returns the deprecated info if this class is deprecated.
    pub(crate) fn deprecated(self, db: &'db dyn Db) -> Option<DeprecatedInstance<'db>> {
        self.as_static().and_then(|class| class.deprecated(db))
    }

    /// Returns whether this class is final.
    pub(crate) fn is_final(self, db: &'db dyn Db) -> bool {
        match self {
            Self::Static(class) => class.is_final(db),
            Self::DynamicEnum(enum_lit) => {
                crate::types::enums::enum_metadata(db, Self::DynamicEnum(enum_lit))
                    .is_some_and(|metadata| !metadata.members.is_empty())
            }
            // Dynamic classes created via `type()`, `collections.namedtuple()`, etc. cannot be
            // marked as final.
            Self::Dynamic(_) | Self::DynamicNamedTuple(_) | Self::DynamicTypedDict(_) => false,
        }
    }

    /// Returns `true` if this class defines any ordering method (`__lt__`, `__le__`, `__gt__`,
    /// `__ge__`) in its own body (not inherited). Used by `@total_ordering` to determine if
    /// synthesis is valid.
    ///
    /// For dynamic classes, this checks if any ordering methods are provided in the namespace
    /// dictionary:
    /// ```python
    /// X = type("X", (), {"__lt__": lambda self, other: True})
    /// ```
    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        match self {
            Self::Static(class) => class.has_own_ordering_method(db),
            Self::Dynamic(class) => class.has_own_ordering_method(db),
            Self::DynamicNamedTuple(_) | Self::DynamicTypedDict(_) | Self::DynamicEnum(_) => false,
        }
    }

    /// Returns the static class definition if this is one.
    pub(crate) fn as_static(self) -> Option<StaticClassLiteral<'db>> {
        match self {
            Self::Static(class) => Some(class),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => None,
        }
    }

    /// Returns the definition of this class, if available.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self {
            Self::Static(class) => Some(class.definition(db)),
            Self::Dynamic(class) => class.definition(db),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.definition(db),
            Self::DynamicTypedDict(typeddict) => typeddict.definition(db),
            Self::DynamicEnum(enum_lit) => enum_lit.definition(db),
        }
    }

    /// Returns the type definition for this class.
    ///
    /// For static classes, returns `TypeDefinition::StaticClass`.
    /// For dynamic classes, returns `TypeDefinition::DynamicClass` if a definition is available.
    pub(crate) fn type_definition(self, db: &'db dyn Db) -> Option<TypeDefinition<'db>> {
        match self {
            Self::Static(class) => Some(TypeDefinition::StaticClass(class.definition(db))),
            Self::Dynamic(class) => class.definition(db).map(TypeDefinition::DynamicClass),
            Self::DynamicNamedTuple(namedtuple) => {
                namedtuple.definition(db).map(TypeDefinition::DynamicClass)
            }
            Self::DynamicTypedDict(typeddict) => {
                typeddict.definition(db).map(TypeDefinition::DynamicClass)
            }
            Self::DynamicEnum(enum_lit) => {
                enum_lit.definition(db).map(TypeDefinition::DynamicClass)
            }
        }
    }

    /// Returns the qualified name of this class.
    pub(super) fn qualified_name(self, db: &'db dyn Db) -> QualifiedClassName<'db> {
        QualifiedClassName::from_class_literal(db, self)
    }

    /// Returns a [`Span`] pointing to the definition of this class.
    ///
    /// For static classes, this is the class header (name and arguments).
    /// For dynamic classes, this is the `type()` call expression.
    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        match self {
            Self::Static(class) => class.header_span(db),
            Self::Dynamic(class) => class.header_span(db),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.header_span(db),
            Self::DynamicTypedDict(typeddict) => typeddict.header_span(db),
            Self::DynamicEnum(enum_lit) => enum_lit.header_span(db),
        }
    }

    /// Returns whether this class is a disjoint base.
    ///
    /// A class is considered a disjoint base if:
    /// - It has the `@disjoint_base` decorator (static classes only), or
    /// - It defines non-empty `__slots__`
    ///
    /// For dynamic classes created via `type()`, we check if `__slots__` is provided
    /// in the namespace dictionary:
    /// ```python
    /// >>> X = type("X", (), {"__slots__": ("a",)})
    /// >>> class Foo(int, X): ...
    /// ...
    /// Traceback (most recent call last):
    ///   File "<python-input-4>", line 1, in <module>
    ///     class Foo(int, X): ...
    /// TypeError: multiple bases have instance lay-out conflict
    /// ```
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        match self {
            Self::Static(class) => class.as_disjoint_base(db),
            Self::Dynamic(class) => class.as_disjoint_base(db),
            // Dynamic namedtuples define `__slots__ = ()`, but `__slots__` must be
            // non-empty for a class to be a disjoint base.
            // Dynamic TypedDicts don't define `__slots__`.
            Self::DynamicNamedTuple(_) | Self::DynamicTypedDict(_) | Self::DynamicEnum(_) => None,
        }
    }

    /// Returns a non-generic instance of this class.
    pub(crate) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Static(class) => class.to_non_generic_instance(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => Type::instance(db, ClassType::NonGeneric(self)),
        }
    }

    /// Returns the protocol class if this is a protocol.
    pub(super) fn into_protocol_class(
        self,
        db: &'db dyn Db,
    ) -> Option<super::protocol_class::ProtocolClass<'db>> {
        self.as_static()
            .and_then(|class| class.into_protocol_class(db))
    }

    /// Apply a specialization to this class.
    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> ClassType<'db> {
        match self {
            Self::Static(class) => class.apply_specialization(db, f),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => ClassType::NonGeneric(self),
        }
    }

    /// Returns the instance member lookup.
    pub(crate) fn instance_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Self::Static(class) => class.instance_member(db, specialization, name),
            Self::Dynamic(class) => class.instance_member(db, name),
            Self::DynamicNamedTuple(namedtuple) => namedtuple.instance_member(db, name),
            Self::DynamicTypedDict(_) => PlaceAndQualifiers::default(),
            Self::DynamicEnum(enum_lit) => enum_lit.instance_member(db, name),
        }
    }

    /// Returns the top materialization for this class.
    pub(crate) fn top_materialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::Static(class) => class.top_materialization(db),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => ClassType::NonGeneric(self),
        }
    }

    /// Returns the `TypedDict` member lookup.
    pub(crate) fn typed_dict_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Self::Static(class) => class.typed_dict_member(db, specialization, name, policy),
            Self::DynamicTypedDict(typeddict) => typeddict.class_member(db, name, policy),
            Self::Dynamic(_) | Self::DynamicNamedTuple(_) | Self::DynamicEnum(_) => {
                Place::Undefined.into()
            }
        }
    }

    /// Returns a new `ClassLiteral` with the given dataclass params, preserving all other fields.
    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        match self {
            Self::Static(class) => Self::Static(class.with_dataclass_params(db, dataclass_params)),
            Self::Dynamic(class) => {
                Self::Dynamic(class.with_dataclass_params(db, dataclass_params))
            }
            Self::DynamicNamedTuple(_) | Self::DynamicTypedDict(_) | Self::DynamicEnum(_) => self,
        }
    }

    /// Returns all of the explicit base class types for this class.
    ///
    /// Note that when this is a namedtuple this always returns a sequence
    /// of length one corresponding to `tuple`.
    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        match self {
            Self::Static(static_class) => static_class.explicit_bases(db).into(),
            Self::Dynamic(dynamic_class) => dynamic_class.explicit_bases(db).into(),
            Self::DynamicNamedTuple(namedtuple) => {
                [Type::from(namedtuple.tuple_base_class(db))].into()
            }
            Self::DynamicTypedDict(_) => {
                // TypedDicts always inherit from `dict`
                Box::default()
            }
            Self::DynamicEnum(enum_lit) => enum_lit.explicit_bases(db),
        }
    }
}

impl<'db> From<StaticClassLiteral<'db>> for ClassLiteral<'db> {
    fn from(literal: StaticClassLiteral<'db>) -> Self {
        ClassLiteral::Static(literal)
    }
}

impl<'db> From<DynamicClassLiteral<'db>> for ClassLiteral<'db> {
    fn from(literal: DynamicClassLiteral<'db>) -> Self {
        ClassLiteral::Dynamic(literal)
    }
}

impl<'db> From<DynamicNamedTupleLiteral<'db>> for ClassLiteral<'db> {
    fn from(literal: DynamicNamedTupleLiteral<'db>) -> Self {
        ClassLiteral::DynamicNamedTuple(literal)
    }
}

impl<'db> From<DynamicTypedDictLiteral<'db>> for ClassLiteral<'db> {
    fn from(literal: DynamicTypedDictLiteral<'db>) -> Self {
        ClassLiteral::DynamicTypedDict(literal)
    }
}

impl<'db> From<DynamicEnumLiteral<'db>> for ClassLiteral<'db> {
    fn from(literal: DynamicEnumLiteral<'db>) -> Self {
        ClassLiteral::DynamicEnum(literal)
    }
}

/// Represents a class type, which might be a non-generic class, or a specialization of a generic
/// class.
#[derive(
    Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update, get_size2::GetSize,
)]
pub enum ClassType<'db> {
    // `NonGeneric` is intended to mean that the `ClassLiteral` has no type parameters. There are
    // places where we currently violate this rule (e.g. so that we print `Foo` instead of
    // `Foo[Unknown]`), but most callers who need to make a `ClassType` from a `ClassLiteral`
    // should use `StaticClassLiteral::default_specialization` instead of assuming
    // `ClassType::NonGeneric`.
    NonGeneric(ClassLiteral<'db>),
    Generic(GenericAlias<'db>),
}

#[salsa::tracked]
impl<'db> ClassType<'db> {
    /// Return a `ClassType` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        ClassType::NonGeneric(ClassLiteral::object(db))
    }

    pub(super) const fn is_generic(self) -> bool {
        matches!(self, Self::Generic(_))
    }

    pub(super) const fn into_generic_alias(self) -> Option<GenericAlias<'db>> {
        match self {
            Self::NonGeneric(_) => None,
            Self::Generic(generic) => Some(generic),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::NonGeneric(_) => Some(self),
            Self::Generic(generic) => Some(Self::Generic(
                generic.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }

    pub(super) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        self.class_literal(db).has_pep_695_type_params(db)
    }

    /// Returns the underlying class literal for this class, ignoring any specialization.
    ///
    /// For a non-generic class, this returns the class literal directly.
    /// For a generic alias, this returns the alias's origin.
    pub(crate) fn class_literal(self, db: &'db dyn Db) -> ClassLiteral<'db> {
        match self {
            Self::NonGeneric(literal) => literal,
            Self::Generic(generic) => ClassLiteral::Static(generic.origin(db)),
        }
    }

    /// Returns the underlying class literal and specialization, if any.
    ///
    /// For a non-generic class, this returns the class literal directly.
    /// For a generic alias, this returns the alias's origin.
    pub(crate) fn class_literal_and_specialization(
        self,
        db: &'db dyn Db,
    ) -> (ClassLiteral<'db>, Option<Specialization<'db>>) {
        match self {
            Self::NonGeneric(literal) => (literal, None),
            Self::Generic(generic) => (
                ClassLiteral::Static(generic.origin(db)),
                Some(generic.specialization(db)),
            ),
        }
    }

    /// Returns the statement-defined class literal and specialization for this class.
    /// For a non-generic class, this is the class itself. For a generic alias, this is the alias's origin.
    pub(crate) fn static_class_literal(
        self,
        db: &'db dyn Db,
    ) -> Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)> {
        match self {
            Self::NonGeneric(ClassLiteral::Static(class)) => Some((class, None)),
            Self::NonGeneric(
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_),
            ) => None,
            Self::Generic(generic) => Some((generic.origin(db), Some(generic.specialization(db)))),
        }
    }

    /// Returns the statement-defined class literal and specialization for this class, with an additional
    /// specialization applied if the class is generic.
    pub(crate) fn static_class_literal_specialized(
        self,
        db: &'db dyn Db,
        additional_specialization: Option<Specialization<'db>>,
    ) -> Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)> {
        match self {
            Self::NonGeneric(ClassLiteral::Static(class)) => Some((class, None)),
            Self::NonGeneric(
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_),
            ) => None,
            Self::Generic(generic) => Some((
                generic.origin(db),
                Some(
                    generic
                        .specialization(db)
                        .apply_optional_specialization(db, additional_specialization),
                ),
            )),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.class_literal(db).name(db)
    }

    pub(super) fn qualified_name(self, db: &'db dyn Db) -> QualifiedClassName<'db> {
        self.class_literal(db).qualified_name(db)
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        self.class_literal(db).known(db)
    }

    /// Returns the definition for this class, if available.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        self.class_literal(db).definition(db)
    }

    /// Returns the type definition for this class.
    pub(crate) fn type_definition(self, db: &'db dyn Db) -> Option<TypeDefinition<'db>> {
        self.class_literal(db).type_definition(db)
    }

    /// Return `Some` if this class is known to be a [`DisjointBase`], or `None` if it is not.
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        self.class_literal(db).as_disjoint_base(db)
    }

    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    /// Return `true` if this class is a `TypedDict`.
    pub(crate) fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        self.class_literal(db).is_typed_dict(db)
    }

    /// Return `true` if this class is a subtype of (any specialization of) `class_literal`.
    pub(crate) fn is_subtype_of_class_literal(
        self,
        db: &'db dyn Db,
        class_literal: ClassLiteral<'db>,
    ) -> bool {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .any(|base| base.class_literal(db) == class_literal)
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::NonGeneric(_) => self,
            Self::Generic(generic) => {
                Self::Generic(generic.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
        }
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self {
            Self::NonGeneric(_) => {}
            Self::Generic(generic) => {
                generic.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`StaticClassLiteral::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(self, db: &'db dyn Db) -> MroIterator<'db> {
        match self {
            Self::NonGeneric(class) => class.iter_mro(db),
            Self::Generic(generic) => MroIterator::new(
                db,
                ClassLiteral::Static(generic.origin(db)),
                Some(generic.specialization(db)),
            ),
        }
    }

    /// Iterate over the method resolution order ("MRO") of the class, optionally applying an
    /// additional specialization to it if the class is generic.
    pub(super) fn iter_mro_specialized(
        self,
        db: &'db dyn Db,
        additional_specialization: Option<Specialization<'db>>,
    ) -> MroIterator<'db> {
        match self {
            Self::NonGeneric(class) => class.iter_mro(db),
            Self::Generic(generic) => MroIterator::new(
                db,
                ClassLiteral::Static(generic.origin(db)),
                Some(
                    generic
                        .specialization(db)
                        .apply_optional_specialization(db, additional_specialization),
                ),
            ),
        }
    }

    /// Is this class final?
    pub(super) fn is_final(self, db: &'db dyn Db) -> bool {
        self.class_literal(db).is_final(db)
    }

    /// Returns a map of methods on this class that were defined as abstract on a superclass
    /// and have not been overridden with a concrete implementation anywhere in the MRO
    ///
    /// The value of the map is a struct containing information about the abstract method.
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn abstract_methods(self, db: &'db dyn Db) -> FxIndexMap<Name, AbstractMethod<'db>> {
        fn type_as_abstract_method<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            defining_class: ClassType<'db>,
        ) -> Option<AbstractMethodKind> {
            match ty {
                Type::FunctionLiteral(function) => function.as_abstract_method(db, defining_class),
                Type::BoundMethod(method) => {
                    method.function(db).as_abstract_method(db, defining_class)
                }
                Type::PropertyInstance(property) => {
                    // A property is abstract if any of its accessors is abstract.
                    property
                        .getter(db)
                        .and_then(|getter| type_as_abstract_method(db, getter, defining_class))
                        .or_else(|| {
                            property.setter(db).and_then(|setter| {
                                type_as_abstract_method(db, setter, defining_class)
                            })
                        })
                        .or_else(|| {
                            property.deleter(db).and_then(|deleter| {
                                type_as_abstract_method(db, deleter, defining_class)
                            })
                        })
                }
                _ => None,
            }
        }

        let mut abstract_methods: FxIndexMap<Name, _> = FxIndexMap::default();

        // Iterate through the MRO in reverse order,
        // skipping `object` (we know it doesn't define any abstract methods)
        for supercls in self.iter_mro(db).rev().skip(1) {
            let ClassBase::Class(class) = supercls else {
                continue;
            };

            // Currently we do not recognize dynamic classes as being able to define abstract methods,
            // but we do recognise them as being able to override abstract methods defined in static classes.
            let ClassLiteral::Static(class_literal) = class.class_literal(db) else {
                abstract_methods
                    .retain(|name, _| class.own_class_member(db, None, name).is_undefined());
                continue;
            };

            let scope = class_literal.body_scope(db);
            let place_table = place_table(db, scope);
            let use_def_map = use_def_map(db, class_literal.body_scope(db));

            // Treat abstract methods from superclasses as having been overridden
            // if this class has a synthesized method by that name,
            // or this class has a `ClassVar` declaration by that name
            abstract_methods.retain(|name, _| {
                if class_literal
                    .own_synthesized_member(db, None, None, name)
                    .is_some()
                {
                    return false;
                }

                place_table.symbol_id(name).is_none_or(|symbol_id| {
                    let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);
                    !place_from_declarations(db, declarations)
                        .ignore_conflicting_declarations()
                        .qualifiers
                        .contains(TypeQualifiers::CLASS_VAR)
                })
            });

            for (symbol_id, bindings_iterator) in use_def_map.all_end_of_scope_symbol_bindings() {
                let name = place_table.symbol(symbol_id).name();
                let place_and_definition = place_from_bindings(db, bindings_iterator);
                let Place::Defined(DefinedPlace { ty, .. }) = place_and_definition.place else {
                    continue;
                };
                let Some(definition) = place_and_definition.first_definition else {
                    continue;
                };
                if let Some(kind) = type_as_abstract_method(db, ty, class) {
                    let abstract_method = AbstractMethod {
                        defining_class: class,
                        definition,
                        kind,
                    };
                    abstract_methods.insert(name.clone(), abstract_method);
                } else {
                    // If this method is concrete, remove it from the map of abstract methods.
                    abstract_methods.shift_remove(name);
                }
            }
        }

        abstract_methods
    }

    /// Returns `true` if any class in this class's MRO (excluding `object`) defines an ordering
    /// method (`__lt__`, `__le__`, `__gt__`, `__ge__`). Used by `@total_ordering` validation.
    pub(super) fn has_ordering_method_in_mro(self, db: &'db dyn Db) -> bool {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .filter(|class| !class.is_object(db))
            .any(|class| class.class_literal(db).has_own_ordering_method(db))
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, target: ClassType<'db>) -> bool {
        let constraints = ConstraintSetBuilder::new();
        let relation_visitor = HasRelationToVisitor::default(&constraints);
        let disjointness_visitor = IsDisjointVisitor::default(&constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker::subtyping(
            &constraints,
            InferableTypeVars::None,
            &relation_visitor,
            &disjointness_visitor,
            &signature_relation_visitor,
            &materialization_visitor,
        );
        checker
            .check_class_pair(db, self, target)
            .is_always_satisfied(db)
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::NonGeneric(class) => class.metaclass(db),
            Self::Generic(generic) => generic
                .origin(db)
                .metaclass(db)
                .apply_optional_specialization(db, Some(generic.specialization(db))),
        }
    }

    /// Return the [`DisjointBase`] that appears first in the MRO of this class.
    ///
    /// Returns `None` if this class does not have any disjoint bases in its MRO.
    #[salsa::tracked(
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(super) fn nearest_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .find_map(|base| base.as_disjoint_base(db))
    }

    /// Return `true` if this class could exist in the MRO of `other`.
    pub(super) fn could_exist_in_mro_of(
        self,
        db: &'db dyn Db,
        other: Self,
        constraints: &ConstraintSetBuilder<'db>,
    ) -> bool {
        other
            .iter_mro(db)
            .filter_map(ClassBase::into_class)
            .any(|class| match (self, class) {
                (ClassType::NonGeneric(this_class), ClassType::NonGeneric(other_class)) => {
                    this_class == other_class
                }
                (ClassType::Generic(this_alias), ClassType::Generic(other_alias)) => {
                    this_alias.origin(db) == other_alias.origin(db)
                        && !this_alias
                            .specialization(db)
                            .is_disjoint_from(
                                db,
                                other_alias.specialization(db),
                                constraints,
                                InferableTypeVars::None,
                            )
                            .is_always_satisfied(db)
                }
                (ClassType::NonGeneric(_), ClassType::Generic(_))
                | (ClassType::Generic(_), ClassType::NonGeneric(_)) => false,
            })
    }

    /// Return `true` if this class could coexist in an MRO with `other`.
    ///
    /// For two given classes `A` and `B`, it is often possible to say for sure
    /// that there could never exist any class `C` that inherits from both `A` and `B`.
    /// In these situations, this method returns `false`; in all others, it returns `true`.
    pub(super) fn could_coexist_in_mro_with(
        self,
        db: &'db dyn Db,
        other: Self,
        constraints: &ConstraintSetBuilder<'db>,
    ) -> bool {
        if self == other {
            return true;
        }

        if self.is_final(db) {
            return other.could_exist_in_mro_of(db, self, constraints);
        }

        if other.is_final(db) {
            return self.could_exist_in_mro_of(db, other, constraints);
        }

        // Two disjoint bases can only coexist in an MRO if one is a subclass of the other.
        if self
            .nearest_disjoint_base(db)
            .is_some_and(|disjoint_base_1| {
                other
                    .nearest_disjoint_base(db)
                    .is_some_and(|disjoint_base_2| {
                        !disjoint_base_1.could_coexist_in_mro_with(db, &disjoint_base_2)
                    })
            })
        {
            return false;
        }

        // Check to see whether the metaclasses of `self` and `other` are disjoint.
        // Avoid this check if the metaclass of either `self` or `other` is `type`,
        // however, since we end up with infinite recursion in that case due to the fact
        // that `type` is its own metaclass (and we know that `type` can coexist in an MRO
        // with any other arbitrary class, anyway).
        let type_class = KnownClass::Type.to_class_literal(db);
        let self_metaclass = self.metaclass(db);
        if self_metaclass == type_class {
            return true;
        }
        let other_metaclass = other.metaclass(db);
        if other_metaclass == type_class {
            return true;
        }
        let Some(self_metaclass_instance) = self_metaclass.to_instance(db) else {
            return true;
        };
        let Some(other_metaclass_instance) = other_metaclass.to_instance(db) else {
            return true;
        };
        if self_metaclass_instance
            .when_disjoint_from(
                db,
                other_metaclass_instance,
                constraints,
                InferableTypeVars::None,
            )
            .is_always_satisfied(db)
        {
            return false;
        }

        true
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(super) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self
            .metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Self::NonGeneric(class) => class.class_member(db, name, policy),
            Self::Generic(generic) => generic.origin(db).class_member_inner(
                db,
                Some(generic.specialization(db)),
                name,
                policy,
            ),
        }
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as `ClassVars` are considered.
    ///
    /// You must provide the `inherited_generic_context` that we should use for the `__new__` or
    /// `__init__` member. This is inherited from the containing class -­but importantly, from the
    /// class that the lookup is being performed on, and not the class containing the (possibly
    /// inherited) member.
    ///
    /// Returns [`Place::Undefined`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassType::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Member<'db> {
        fn synthesize_getitem_overload_signature<'db>(
            db: &'db dyn Db,
            index_annotation: Type<'db>,
            return_annotation: Type<'db>,
        ) -> Signature<'db> {
            let self_parameter = Parameter::positional_only(Some(Name::new_static("self")));
            let index_parameter = Parameter::positional_only(Some(Name::new_static("index")))
                .with_annotated_type(index_annotation);
            let parameters = Parameters::new(db, [self_parameter, index_parameter]);
            Signature::new(parameters, return_annotation)
        }

        let (class_literal, specialization) = match self {
            Self::NonGeneric(ClassLiteral::Dynamic(dynamic)) => {
                return dynamic.own_class_member(db, name);
            }
            Self::NonGeneric(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
                return namedtuple.own_class_member(db, name);
            }
            Self::NonGeneric(ClassLiteral::DynamicTypedDict(typeddict)) => {
                return typeddict.own_class_member(db, name);
            }
            Self::NonGeneric(ClassLiteral::DynamicEnum(enum_lit)) => {
                return enum_lit.own_class_member(db, name);
            }
            Self::NonGeneric(ClassLiteral::Static(class)) => (class, None),
            Self::Generic(generic) => (generic.origin(db), Some(generic.specialization(db))),
        };

        let fallback_member_lookup = || {
            class_literal
                .own_class_member(db, inherited_generic_context, specialization, name)
                .map_type(|ty| ty.apply_optional_specialization(db, specialization))
        };

        match name {
            "__len__" if class_literal.is_tuple(db) => {
                let return_type = specialization
                    .and_then(|spec| spec.tuple(db))
                    .and_then(|tuple| tuple.len().into_fixed_length())
                    .and_then(|len| i64::try_from(len).ok())
                    .map(Type::int_literal)
                    .unwrap_or_else(|| KnownClass::Int.to_instance(db));

                let parameters = Parameters::new(
                    db,
                    [Parameter::positional_only(Some(Name::new_static("self")))
                        .with_annotated_type(Type::instance(db, self))],
                );

                let synthesized_dunder_method =
                    Type::function_like_callable(db, Signature::new(parameters, return_type));

                Member::definitely_declared(synthesized_dunder_method)
            }

            "__getitem__" if class_literal.is_tuple(db) => {
                specialization
                    .and_then(|spec| spec.tuple(db))
                    .map(|tuple| {
                        let mut element_type_to_indices: FxIndexMap<Type<'db>, Vec<i64>> =
                            FxIndexMap::default();

                        match tuple {
                            // E.g. for `tuple[int, str]`, we will generate the following overloads:
                            //
                            //    __getitem__(self, index: Literal[0, -2], /) -> int
                            //    __getitem__(self, index: Literal[1, -1], /) -> str
                            //
                            TupleSpec::Fixed(fixed_length_tuple) => {
                                let tuple_length = fixed_length_tuple.len();

                                for (index, ty) in
                                    fixed_length_tuple.iter_all_elements().enumerate()
                                {
                                    let entry = element_type_to_indices.entry(ty).or_default();
                                    if let Ok(index) = i64::try_from(index) {
                                        entry.push(index);
                                    }
                                    if let Ok(index) = i64::try_from(tuple_length - index) {
                                        entry.push(0 - index);
                                    }
                                }
                            }

                            // E.g. for `tuple[str, *tuple[float, ...], bytes, range]`, we will generate the following overloads:
                            //
                            //    __getitem__(self, index: Literal[0], /) -> str
                            //    __getitem__(self, index: Literal[1], /) -> float | bytes
                            //    __getitem__(self, index: Literal[2], /) -> float | bytes | range
                            //    __getitem__(self, index: Literal[-1], /) -> range
                            //    __getitem__(self, index: Literal[-2], /) -> bytes
                            //    __getitem__(self, index: Literal[-3], /) -> float | str
                            //
                            TupleSpec::Variable(variable_length_tuple) => {
                                for (index, ty) in
                                    variable_length_tuple.prefix_elements().iter().enumerate()
                                {
                                    if let Ok(index) = i64::try_from(index) {
                                        element_type_to_indices.entry(*ty).or_default().push(index);
                                    }

                                    let one_based_index = index + 1;

                                    if let Ok(i) = i64::try_from(
                                        variable_length_tuple.suffix_elements().len()
                                            + one_based_index,
                                    ) {
                                        let overload_return = UnionType::from_elements(
                                            db,
                                            std::iter::once(variable_length_tuple.variable())
                                                .chain(
                                                    variable_length_tuple
                                                        .iter_prefix_elements()
                                                        .rev()
                                                        .take(one_based_index),
                                                ),
                                        );
                                        element_type_to_indices
                                            .entry(overload_return)
                                            .or_default()
                                            .push(0 - i);
                                    }
                                }

                                for (index, ty) in variable_length_tuple
                                    .iter_suffix_elements()
                                    .rev()
                                    .enumerate()
                                {
                                    if let Some(index) =
                                        index.checked_add(1).and_then(|i| i64::try_from(i).ok())
                                    {
                                        element_type_to_indices
                                            .entry(ty)
                                            .or_default()
                                            .push(0 - index);
                                    }

                                    if let Ok(i) = i64::try_from(
                                        variable_length_tuple.prefix_elements().len() + index,
                                    ) {
                                        let overload_return = UnionType::from_elements(
                                            db,
                                            std::iter::once(variable_length_tuple.variable())
                                                .chain(
                                                    variable_length_tuple
                                                        .iter_suffix_elements()
                                                        .take(index + 1),
                                                ),
                                        );
                                        element_type_to_indices
                                            .entry(overload_return)
                                            .or_default()
                                            .push(i);
                                    }
                                }
                            }
                        }

                        let all_elements_unioned =
                            UnionType::from_elements(db, tuple.all_elements());

                        let mut overload_signatures =
                            Vec::with_capacity(element_type_to_indices.len().saturating_add(2));

                        overload_signatures.extend(element_type_to_indices.into_iter().filter_map(
                            |(return_type, mut indices)| {
                                if return_type.is_equivalent_to(db, all_elements_unioned) {
                                    return None;
                                }

                                // Sorting isn't strictly required, but leads to nicer `reveal_type` output
                                indices.sort_unstable();

                                let index_annotation = UnionType::from_elements(
                                    db,
                                    indices.into_iter().map(Type::int_literal),
                                );

                                Some(synthesize_getitem_overload_signature(
                                    db,
                                    index_annotation,
                                    return_type,
                                ))
                            },
                        ));

                        // Fallback overloads: for `tuple[int, str]`, we will generate the following overloads:
                        //
                        //    __getitem__(self, index: int, /) -> int | str
                        //    __getitem__(self, index: slice[Any, Any, Any], /) -> tuple[int | str, ...]
                        //
                        // and for `tuple[str, *tuple[float, ...], bytes]`, we will generate the following overloads:
                        //
                        //    __getitem__(self, index: int, /) -> str | float | bytes
                        //    __getitem__(self, index: slice[Any, Any, Any], /) -> tuple[str | float | bytes, ...]
                        //
                        overload_signatures.push(synthesize_getitem_overload_signature(
                            db,
                            KnownClass::SupportsIndex.to_instance(db),
                            all_elements_unioned,
                        ));

                        overload_signatures.push(synthesize_getitem_overload_signature(
                            db,
                            KnownClass::Slice.to_instance(db),
                            Type::homogeneous_tuple(db, all_elements_unioned),
                        ));

                        let getitem_signature =
                            CallableSignature::from_overloads(overload_signatures);
                        let getitem_type = Type::Callable(CallableType::new(
                            db,
                            getitem_signature,
                            CallableTypeKind::FunctionLike,
                        ));
                        Member::definitely_declared(getitem_type)
                    })
                    .unwrap_or_else(fallback_member_lookup)
            }

            // ```py
            // class tuple:
            //     @overload
            //     def __new__(cls: type[tuple[()]], iterable: tuple[()] = ()) -> tuple[()]: ...
            //     @overload
            //     def __new__[T](cls: type[tuple[T, ...]], iterable: tuple[T, ...]) -> tuple[T, ...]: ...
            // ```
            "__new__" if class_literal.is_tuple(db) => {
                let mut iterable_parameter =
                    Parameter::positional_only(Some(Name::new_static("iterable")));

                let tuple = specialization.and_then(|spec| spec.tuple(db));

                match tuple {
                    Some(tuple) => {
                        // TODO: Once we support PEP 646 annotations for `*args` parameters, we can
                        // use the tuple itself as the argument type.
                        let tuple_len = tuple.len();

                        if tuple_len.minimum() == 0 && tuple_len.maximum().is_none() {
                            // If the tuple has no length restrictions,
                            // any iterable is allowed as long as the iterable has the correct element type.
                            let mut tuple_elements = tuple.iter_all_elements();
                            iterable_parameter = iterable_parameter.with_annotated_type(
                                KnownClass::Iterable
                                    .to_specialized_instance(db, &[tuple_elements.next().unwrap()]),
                            );
                            assert_eq!(
                                tuple_elements.next(),
                                None,
                                "Tuple specialization should not have more than one element when it has no length restriction"
                            );
                        } else {
                            // But if the tuple is of a fixed length, or has a minimum length, we require a tuple rather
                            // than an iterable, as a tuple is the only kind of iterable for which we can
                            // specify a fixed length, or that the iterable must be at least a certain length.
                            iterable_parameter =
                                iterable_parameter.with_annotated_type(Type::instance(db, self));
                        }
                    }
                    None => {
                        // If the tuple isn't specialized at all, we allow any argument as long as it is iterable.
                        iterable_parameter = iterable_parameter
                            .with_annotated_type(KnownClass::Iterable.to_instance(db));
                    }
                }

                // We allow the `iterable` parameter to be omitted for:
                // - a zero-length tuple
                // - an unspecialized tuple
                // - a tuple with no minimum length
                if tuple.is_none_or(|tuple| tuple.len().minimum() == 0) {
                    iterable_parameter =
                        iterable_parameter.with_default_type(Type::empty_tuple(db));
                }

                let parameters = Parameters::new(
                    db,
                    [
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(SubclassOfType::from(db, self)),
                        iterable_parameter,
                    ],
                );

                let synthesized_dunder = Type::function_like_callable(
                    db,
                    Signature::new_generic(inherited_generic_context, parameters, Type::unknown()),
                );

                Member::definitely_declared(synthesized_dunder)
            }

            _ => fallback_member_lookup(),
        }
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self {
            Self::NonGeneric(ClassLiteral::Dynamic(class)) => class.instance_member(db, name),
            Self::NonGeneric(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
                namedtuple.instance_member(db, name)
            }
            Self::NonGeneric(ClassLiteral::DynamicTypedDict(_)) => PlaceAndQualifiers::default(),
            Self::NonGeneric(ClassLiteral::DynamicEnum(enum_lit)) => {
                enum_lit.instance_member(db, name)
            }
            Self::NonGeneric(ClassLiteral::Static(class)) => {
                if class.is_typed_dict(db) {
                    return Place::Undefined.into();
                }
                class.instance_member(db, None, name)
            }
            Self::Generic(generic) => {
                let class_literal = generic.origin(db);
                let specialization = Some(generic.specialization(db));

                if class_literal.is_typed_dict(db) {
                    return Place::Undefined.into();
                }

                class_literal
                    .instance_member(db, specialization, name)
                    .map_type(|ty| ty.apply_optional_specialization(db, specialization))
            }
        }
    }

    /// Returns the converter input type for a dataclass field, if the field has a `converter`.
    pub(super) fn converter_input_type_for_field(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<Type<'db>> {
        match self {
            Self::NonGeneric(ClassLiteral::Static(class)) => {
                class.converter_input_type_for_field(db, name)
            }
            Self::Generic(generic) => generic
                .origin(db)
                .converter_input_type_for_field(db, name)
                .map(|ty| ty.apply_optional_specialization(db, Some(generic.specialization(db)))),
            Self::NonGeneric(
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_),
            ) => None,
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        match self {
            Self::NonGeneric(ClassLiteral::Dynamic(dynamic)) => {
                dynamic.own_instance_member(db, name)
            }
            Self::NonGeneric(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
                namedtuple.own_instance_member(db, name)
            }
            Self::NonGeneric(ClassLiteral::DynamicTypedDict(_)) => Member::default(),
            Self::NonGeneric(ClassLiteral::DynamicEnum(enum_lit)) => {
                enum_lit.own_instance_member(db, name)
            }
            Self::NonGeneric(ClassLiteral::Static(class_literal)) => {
                class_literal.own_instance_member(db, name)
            }
            Self::Generic(generic) => {
                let specialization = generic.specialization(db);
                generic
                    .origin(db)
                    .own_instance_member(db, name)
                    .map_type(|ty| ty.apply_optional_specialization(db, Some(specialization)))
            }
        }
    }

    /// Return a callable type (or union of callable types) that represents the callable
    /// constructor signature of this class.
    #[salsa::tracked(
        cycle_initial=|db, _, _| CallableTypes::one(CallableType::bottom(db)),
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(super) fn into_callable(self, db: &'db dyn Db) -> CallableTypes<'db> {
        // TODO: This mimics a lot of the logic in Type::try_call_from_constructor. Can we
        // consolidate the two? Can we invoke a class by upcasting the class into a Callable, and
        // then relying on the call binding machinery to Just Work™?

        // Dynamic classes don't have a generic context.
        let class_generic_context = self
            .static_class_literal(db)
            .and_then(|(class_literal, _)| class_literal.generic_context(db));

        let self_ty = Type::from(self);
        let metaclass_dunder_call_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__call__".into(),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .place;

        if let Place::Defined(DefinedPlace {
            ty: Type::BoundMethod(metaclass_dunder_call_function),
            ..
        }) = metaclass_dunder_call_function_symbol
        {
            // TODO: this intentionally diverges from step 1 in
            // https://typing.python.org/en/latest/spec/constructors.html#converting-a-constructor-to-callable
            // by always respecting the signature of the metaclass `__call__`, rather than
            // using a heuristic which makes unwarranted assumptions to sometimes ignore it.
            //
            // The only situation where we ignore the metaclass `__call__` is when the class is an actual enum
            // (i.e. not a memberless superclass like `Enum`, `StrEnum`, etc.). In this case, we want to fall
            // back to `Enum.__new__`/`StrEnum.__new__`/... which have more precise signatures for calls like
            // `Color("red")`, instead of the overloaded signature of `EnumMeta.__call__` which also accounts
            // for dynamic Enum creation.
            let is_actual_enum = enum_metadata(db, self.class_literal(db)).is_some();
            if !is_actual_enum {
                return CallableTypes::one(metaclass_dunder_call_function.into_callable_type(db));
            }
        }

        let dunder_new_function_symbol = self_ty.lookup_dunder_new(db);

        let dunder_new_signature = dunder_new_function_symbol
            .and_then(|place_and_quals| place_and_quals.ignore_possibly_undefined())
            .and_then(|ty| match ty {
                Type::FunctionLiteral(function) => Some(function.signature(db)),
                Type::Callable(callable) => Some(callable.signatures(db)),
                _ => None,
            });

        let dunder_new_function = if let Some(dunder_new_signature) = dunder_new_signature {
            // Step 3: If the return type of the `__new__` evaluates to a type that is not a subclass of this class,
            // then we should ignore the `__init__` and just return the `__new__` method.
            let returns_non_subclass = dunder_new_signature.overloads.iter().any(|signature| {
                !signature.return_ty.is_assignable_to(
                    db,
                    self_ty
                        .to_instance(db)
                        .expect("ClassType should be instantiable"),
                )
            });

            let instance_ty = Type::instance(db, self);
            let dunder_new_bound_method = CallableType::new(
                db,
                dunder_new_signature.bind_self(db, Some(instance_ty)),
                CallableTypeKind::Regular,
            );

            if returns_non_subclass {
                return CallableTypes::one(dunder_new_bound_method);
            }
            Some(dunder_new_bound_method)
        } else {
            None
        };

        let dunder_init_function_symbol = self_ty
            .member_lookup_with_policy(
                db,
                "__init__".into(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .place;

        let correct_return_type = self_ty.to_instance(db).unwrap_or_else(Type::unknown);

        // If the class defines an `__init__` method, then we synthesize a callable type with the
        // same parameters as the `__init__` method after it is bound, and with the return type of
        // the concrete type of `Self`.
        let synthesized_dunder_init_callable = if let Place::Defined(DefinedPlace { ty, .. }) =
            dunder_init_function_symbol
        {
            let signature = match ty {
                Type::FunctionLiteral(dunder_init_function) => {
                    Some(dunder_init_function.signature(db))
                }
                Type::Callable(callable) => Some(callable.signatures(db)),
                _ => None,
            };

            if let Some(signature) = signature {
                let synthesized_signature = |signature: &Signature<'db>| {
                    let self_annotation = signature
                        .parameters()
                        .get_positional(0)
                        .filter(|parameter| !parameter.inferred_annotation)
                        .map(Parameter::annotated_type)
                        .filter(|ty| {
                            ty.as_typevar()
                                .is_none_or(|bound_typevar| !bound_typevar.typevar(db).is_self(db))
                        });
                    let return_type = self_annotation.unwrap_or(correct_return_type);
                    let instance_ty = self_annotation.unwrap_or_else(|| Type::instance(db, self));
                    let generic_context = GenericContext::merge_optional(
                        db,
                        class_generic_context,
                        signature.generic_context,
                    );
                    Signature::new_generic(
                        generic_context,
                        signature.parameters().clone(),
                        return_type,
                    )
                    .with_definition(signature.definition())
                    .bind_self(db, Some(instance_ty))
                };

                let synthesized_dunder_init_signature = CallableSignature::from_overloads(
                    signature.overloads.iter().map(synthesized_signature),
                );

                Some(CallableType::new(
                    db,
                    synthesized_dunder_init_signature,
                    CallableTypeKind::Regular,
                ))
            } else {
                None
            }
        } else {
            None
        };

        match (dunder_new_function, synthesized_dunder_init_callable) {
            (Some(dunder_new_function), Some(synthesized_dunder_init_callable)) => {
                CallableTypes::from_elements([
                    dunder_new_function,
                    synthesized_dunder_init_callable,
                ])
            }
            (Some(constructor), None) | (None, Some(constructor)) => {
                CallableTypes::one(constructor)
            }
            (None, None) => {
                // If no `__new__` or `__init__` method is found, then we fall back to looking for
                // an `object.__new__` method.
                let new_function_symbol = self_ty
                    .member_lookup_with_policy(
                        db,
                        "__new__".into(),
                        MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(DefinedPlace {
                    ty: Type::FunctionLiteral(mut new_function),
                    ..
                }) = new_function_symbol
                {
                    if let Some(class_generic_context) = class_generic_context {
                        new_function =
                            new_function.with_inherited_generic_context(db, class_generic_context);
                    }
                    CallableTypes::one(
                        new_function
                            .into_bound_method_type(db, correct_return_type)
                            .into_callable_type(db),
                    )
                } else {
                    // Fallback if no `object.__new__` is found.
                    CallableTypes::one(CallableType::single(
                        db,
                        Signature::new_generic(
                            class_generic_context,
                            Parameters::empty(),
                            correct_return_type,
                        ),
                    ))
                }
            }
        }
    }

    pub(super) fn is_protocol(self, db: &'db dyn Db) -> bool {
        self.static_class_literal(db)
            .is_some_and(|(class, _)| class.is_protocol(db))
    }

    /// Returns a [`Span`] pointing to the definition of this class.
    ///
    /// For static classes, this is the class header (name and arguments).
    /// For dynamic classes, this is the `type()` call expression.
    pub(super) fn definition_span(self, db: &'db dyn Db) -> Span {
        self.class_literal(db).header_span(db)
    }
}

impl<'db> From<GenericAlias<'db>> for ClassType<'db> {
    fn from(generic: GenericAlias<'db>) -> ClassType<'db> {
        ClassType::Generic(generic)
    }
}

impl<'db> From<ClassLiteral<'db>> for Type<'db> {
    fn from(class: ClassLiteral<'db>) -> Type<'db> {
        Type::ClassLiteral(class)
    }
}

impl<'db> From<StaticClassLiteral<'db>> for Type<'db> {
    fn from(class: StaticClassLiteral<'db>) -> Type<'db> {
        Type::ClassLiteral(class.into())
    }
}

impl<'db> From<DynamicClassLiteral<'db>> for Type<'db> {
    fn from(class: DynamicClassLiteral<'db>) -> Type<'db> {
        Type::ClassLiteral(class.into())
    }
}

impl<'db> From<ClassType<'db>> for Type<'db> {
    fn from(class: ClassType<'db>) -> Type<'db> {
        match class {
            ClassType::NonGeneric(non_generic) => non_generic.into(),
            ClassType::Generic(generic) => generic.into(),
        }
    }
}

impl<'db> VarianceInferable<'db> for ClassType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            Self::NonGeneric(ClassLiteral::Static(class)) => class.variance_of(db, typevar),
            Self::NonGeneric(
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_),
            ) => TypeVarVariance::Bivariant,
            Self::Generic(generic) => generic.variance_of(db, typevar),
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_class_pair(
        &self,
        db: &'db dyn Db,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // Fast path: if source and target are the same class (possibly with different
        // specializations), we can compare them directly without walking the MRO.
        match (source, target) {
            (ClassType::NonGeneric(source), ClassType::NonGeneric(target)) if source == target => {
                return self.always();
            }
            (ClassType::Generic(source_alias), ClassType::Generic(target_alias))
                if source_alias.origin(db) == target_alias.origin(db) =>
            {
                return self.check_specialization_pair(
                    db,
                    source_alias.specialization(db),
                    target_alias.specialization(db),
                );
            }
            _ => {}
        }

        source.iter_mro(db).when_any(db, self.constraints, |base| {
            match base {
                ClassBase::Dynamic(_) | ClassBase::Divergent(_) => match self.relation {
                    TypeRelation::Subtyping
                    | TypeRelation::Redundancy { .. }
                    | TypeRelation::SubtypingAssuming => {
                        ConstraintSet::from_bool(self.constraints, target.is_object(db))
                    }
                    TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => {
                        ConstraintSet::from_bool(self.constraints, !target.is_final(db))
                    }
                },

                // Protocol, Generic, and TypedDict are special bases that don't match ClassType.
                ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict => self.never(),

                ClassBase::Class(source) => match (source, target) {
                    // Two non-generic classes match if they have the same class literal.
                    (
                        ClassType::NonGeneric(source_literal),
                        ClassType::NonGeneric(target_literal),
                    ) => {
                        ConstraintSet::from_bool(self.constraints, source_literal == target_literal)
                    }

                    // Two generic classes match if they have the same origin and compatible specializations.
                    (ClassType::Generic(source), ClassType::Generic(target)) => {
                        ConstraintSet::from_bool(
                            self.constraints,
                            source.origin(db) == target.origin(db),
                        )
                        .and(db, self.constraints, || {
                            self.check_specialization_pair(
                                db,
                                source.specialization(db),
                                target.specialization(db),
                            )
                        })
                    }

                    // Generic and non-generic classes don't match.
                    (ClassType::Generic(_), ClassType::NonGeneric(_))
                    | (ClassType::NonGeneric(_), ClassType::Generic(_)) => self.never(),
                },
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
pub(super) struct AbstractMethod<'db> {
    pub(super) defining_class: ClassType<'db>,
    pub(super) definition: Definition<'db>,
    pub(super) kind: AbstractMethodKind,
}

/// The decorator category for a method-like function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MethodDecorator {
    /// An instance method with an implicit instance receiver, conventionally named `self`.
    #[default]
    None,
    /// A classmethod with an implicit class receiver, conventionally named `cls`.
    ClassMethod,
    /// A staticmethod with no implicit receiver.
    StaticMethod,
}

impl MethodDecorator {
    /// Returns the decorator category for a function type.
    pub fn try_from_fn_type(db: &dyn Db, fn_type: FunctionType) -> Option<Self> {
        match (fn_type.is_classmethod(db), fn_type.is_staticmethod(db)) {
            (true, true) => None, // A method can't be static and class method at the same time.
            (true, false) => Some(Self::ClassMethod),
            (false, true) => Some(Self::StaticMethod),
            (false, false) => Some(Self::None),
        }
    }

    /// Returns a concise description of this decorator category.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            MethodDecorator::None => "an instance method",
            MethodDecorator::ClassMethod => "a classmethod",
            MethodDecorator::StaticMethod => "a staticmethod",
        }
    }
}

/// Kind-specific metadata for different types of fields
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) enum FieldKind<'db> {
    /// `NamedTuple` field metadata
    NamedTuple { default_ty: Option<Type<'db>> },
    /// dataclass field metadata
    Dataclass {
        /// The type of the default value for this field
        default_ty: Option<Type<'db>>,
        /// Whether or not this field is "init-only". If this is true, it only appears in the
        /// `__init__` signature, but is not accessible as a real field
        init_only: bool,
        /// Whether or not this field should appear in the signature of `__init__`.
        init: bool,
        /// Whether or not this field can only be passed as a keyword argument to `__init__`.
        kw_only: Option<bool>,
        /// The name for this field in the `__init__` signature, if specified.
        alias: Option<Box<str>>,
        /// The converter types for this field, if a `converter` was specified.
        /// The first element is the input type (first positional parameter), the second is the
        /// output type (return type of the converter callable).
        converter: Option<(Type<'db>, Type<'db>)>,
    },
    /// `TypedDict` field metadata
    TypedDict {
        /// Whether this field is required
        is_required: bool,
        /// Whether this field is marked read-only
        is_read_only: bool,
    },
}

/// Metadata regarding a dataclass field/attribute or a `TypedDict` "item" / key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct Field<'db> {
    /// The declared type of the field
    pub(crate) declared_ty: Type<'db>,
    /// Kind-specific metadata for this field
    pub(crate) kind: FieldKind<'db>,
    /// The first declaration of this field.
    /// This field is used for backreferences in diagnostics.
    pub(crate) first_declaration: Option<Definition<'db>>,
}

impl Field<'_> {
    pub(crate) const fn is_required(&self) -> bool {
        match &self.kind {
            FieldKind::NamedTuple { default_ty } => default_ty.is_none(),
            // A dataclass field is NOT required if `default` (or `default_factory`) is set
            // or if `init` has been set to `False`.
            FieldKind::Dataclass {
                init, default_ty, ..
            } => default_ty.is_none() && *init,
            FieldKind::TypedDict { is_required, .. } => *is_required,
        }
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        match &self.kind {
            FieldKind::TypedDict { is_read_only, .. } => *is_read_only,
            _ => false,
        }
    }
}

impl<'db> Field<'db> {
    /// Returns true if this field is a `dataclasses.KW_ONLY` sentinel.
    /// <https://docs.python.org/3/library/dataclasses.html#dataclasses.KW_ONLY>
    pub(crate) fn is_kw_only_sentinel(&self, db: &'db dyn Db) -> bool {
        self.declared_ty.is_instance_of(db, KnownClass::KwOnly)
    }
}

impl<'db> VarianceInferable<'db> for ClassLiteral<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            Self::Static(class) => class.variance_of(db, typevar),
            Self::Dynamic(_)
            | Self::DynamicNamedTuple(_)
            | Self::DynamicTypedDict(_)
            | Self::DynamicEnum(_) => TypeVarVariance::Bivariant,
        }
    }
}

/// Performs member lookups over an MRO (Method Resolution Order).
///
/// This struct encapsulates the shared logic for looking up class and instance
/// members by iterating through an MRO. Both `StaticClassLiteral` and `DynamicClassLiteral`
/// use this to avoid duplicating the MRO traversal logic.
pub(super) struct MroLookup<'db, I> {
    db: &'db dyn Db,
    mro_iter: I,
}

impl<'db, I: Iterator<Item = ClassBase<'db>>> MroLookup<'db, I> {
    /// Create a new MRO lookup from a database and an MRO iterator.
    pub(super) fn new(db: &'db dyn Db, mro_iter: I) -> Self {
        Self { db, mro_iter }
    }

    /// Look up a class member by iterating through the MRO.
    ///
    /// Parameters:
    /// - `name`: The member name to look up
    /// - `policy`: Controls which classes in the MRO to skip
    /// - `inherited_generic_context`: Generic context for `own_class_member` calls
    /// - `is_self_object`: Whether the class itself is `object` (affects policy filtering)
    ///
    /// Returns `ClassMemberResult::TypedDict` if a `TypedDict` base is encountered,
    /// allowing the caller to handle this case specially.
    ///
    /// If we encounter a dynamic type in the MRO, we save it and after traversal:
    /// 1. Use it as the type if no other classes define the attribute, or
    /// 2. Intersect it with the type from non-dynamic MRO members.
    pub(super) fn class_member(
        self,
        name: &str,
        policy: MemberLookupPolicy,
        inherited_generic_context: Option<GenericContext<'db>>,
        is_self_object: bool,
    ) -> ClassMemberResult<'db> {
        let db = self.db;
        let mut dynamic_type: Option<Type<'db>> = None;
        let mut lookup_result: LookupResult<'db> =
            Err(LookupError::Undefined(TypeQualifiers::empty()));

        for superclass in self.mro_iter {
            match superclass {
                ClassBase::Generic | ClassBase::Protocol => {
                    // Skip over these very special class bases that aren't really classes.
                }
                ClassBase::Dynamic(_) => {
                    // Note: calling `Type::from(superclass).member()` would be incorrect here.
                    // What we'd really want is a `Type::Any.own_class_member()` method,
                    // but adding such a method wouldn't make much sense -- it would always return `Any`!
                    dynamic_type.get_or_insert(Type::from(superclass));
                }
                ClassBase::Divergent(_) => {
                    dynamic_type.get_or_insert(Type::from(superclass));
                }
                ClassBase::Class(class) => {
                    let known = class.known(db);

                    // Only exclude `object` members if this is not an `object` class itself
                    if known == Some(KnownClass::Object)
                        && policy.mro_no_object_fallback()
                        && !is_self_object
                    {
                        continue;
                    }

                    if known == Some(KnownClass::Type) && policy.meta_class_no_type_fallback() {
                        continue;
                    }

                    if matches!(known, Some(KnownClass::Int | KnownClass::Str))
                        && policy.mro_no_int_or_str_fallback()
                    {
                        continue;
                    }

                    lookup_result = lookup_result.or_else(|lookup_error| {
                        lookup_error.or_fall_back_to(
                            db,
                            class
                                .own_class_member(db, inherited_generic_context, name)
                                .inner,
                        )
                    });
                }
                ClassBase::TypedDict => {
                    return ClassMemberResult::TypedDict;
                }
            }
            if lookup_result.is_ok() {
                break;
            }
        }

        ClassMemberResult::Done(CompletedMemberLookup {
            lookup_result,
            dynamic_type,
        })
    }

    /// Look up an instance member by iterating through the MRO.
    ///
    /// Unlike class member lookup, instance member lookup:
    /// - Uses `own_instance_member` to check each class
    /// - Builds a union of inferred types from multiple classes
    /// - Stops on the first definitely-declared attribute
    ///
    /// Returns `InstanceMemberResult::TypedDict` if a `TypedDict` base is encountered,
    /// allowing the caller to handle this case specially.
    pub(super) fn instance_member(self, name: &str) -> InstanceMemberResult<'db> {
        let db = self.db;
        let mut union = UnionBuilder::new(db);
        let mut union_qualifiers = TypeQualifiers::empty();
        let mut is_definitely_bound = false;

        for superclass in self.mro_iter {
            match superclass {
                ClassBase::Generic | ClassBase::Protocol => {
                    // Skip over these very special class bases that aren't really classes.
                }
                ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                    // We already return the dynamic type for class member lookup, so we can
                    // just return unbound here (to avoid having to build a union of the
                    // dynamic type with itself).
                    return InstanceMemberResult::Done(PlaceAndQualifiers::unbound());
                }
                ClassBase::Class(class) => {
                    if let member @ PlaceAndQualifiers {
                        place:
                            Place::Defined(DefinedPlace {
                                ty,
                                origin,
                                definedness: boundness,
                                ..
                            }),
                        qualifiers,
                    } = class.own_instance_member(db, name).inner
                    {
                        if boundness == Definedness::AlwaysDefined {
                            if origin.is_declared() {
                                // We found a definitely-declared attribute. Discard possibly collected
                                // inferred types from subclasses and return the declared type.
                                return InstanceMemberResult::Done(member);
                            }

                            is_definitely_bound = true;
                        }

                        // If the attribute is not definitely declared on this class, keep looking
                        // higher up in the MRO, and build a union of all inferred types (and
                        // possibly-declared types):
                        union = union.add(ty);

                        // TODO: We could raise a diagnostic here if there are conflicting type
                        // qualifiers
                        union_qualifiers |= qualifiers;
                    }
                }
                ClassBase::TypedDict => {
                    return InstanceMemberResult::TypedDict;
                }
            }
        }

        let result = if union.is_empty() {
            Place::Undefined.with_qualifiers(TypeQualifiers::empty())
        } else {
            let boundness = if is_definitely_bound {
                Definedness::AlwaysDefined
            } else {
                Definedness::PossiblyUndefined
            };

            Place::Defined(DefinedPlace {
                ty: union.build(),
                origin: TypeOrigin::Inferred,
                definedness: boundness,
                public_type_policy: PublicTypePolicy::Raw,
            })
            .with_qualifiers(union_qualifiers)
        };

        InstanceMemberResult::Done(result)
    }
}

/// Result of class member lookup from MRO iteration.
pub(super) enum ClassMemberResult<'db> {
    /// Found the member or exhausted the MRO.
    Done(CompletedMemberLookup<'db>),
    /// Encountered a `TypedDict` base.
    TypedDict,
}

pub(super) struct CompletedMemberLookup<'db> {
    lookup_result: LookupResult<'db>,
    dynamic_type: Option<Type<'db>>,
}

impl<'db> CompletedMemberLookup<'db> {
    /// Finalize the lookup result by handling dynamic type intersection.
    pub(super) fn finalize(self, db: &'db dyn Db) -> PlaceAndQualifiers<'db> {
        match (
            PlaceAndQualifiers::from(self.lookup_result),
            self.dynamic_type,
        ) {
            (symbol_and_qualifiers, None) => symbol_and_qualifiers,

            (
                PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace { ty, .. }),
                    qualifiers,
                },
                Some(dynamic),
            ) => Place::bound(IntersectionType::from_two_elements(db, ty, dynamic))
                .with_qualifiers(qualifiers),

            (
                PlaceAndQualifiers {
                    place: Place::Undefined,
                    qualifiers,
                },
                Some(dynamic),
            ) => Place::bound(dynamic).with_qualifiers(qualifiers),
        }
    }
}

/// Result of instance member lookup from MRO iteration.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum InstanceMemberResult<'db> {
    /// Found the member or exhausted the MRO
    Done(PlaceAndQualifiers<'db>),
    /// Encountered a `TypedDict` base - caller should handle this specially
    TypedDict,
}

// N.B. It would be incorrect to derive `Eq`, `PartialEq`, or `Hash` for this struct,
// because two `QualifiedClassName` instances might refer to different classes but
// have the same components. You'd expect them to compare equal, but they'd compare
// unequal if `PartialEq`/`Eq` were naively derived.
#[derive(Clone, Copy)]
pub(super) struct QualifiedClassName<'db> {
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
}

impl<'db> QualifiedClassName<'db> {
    pub(super) fn from_class_literal(db: &'db dyn Db, class: ClassLiteral<'db>) -> Self {
        Self { db, class }
    }

    /// Returns the components of the qualified name of this class, excluding this class itself.
    ///
    /// For example, calling this method on a class `C` in the module `a.b` would return
    /// `["a", "b"]`. Calling this method on a class `D` inside the namespace of a method
    /// `m` inside the namespace of a class `C` in the module `a.b` would return
    /// `["a", "b", "C", "<locals of function 'm'>"]`.
    pub(super) fn components_excluding_self(&self) -> Vec<String> {
        let (file, file_scope_id, skip_count) = match self.class {
            ClassLiteral::Static(class) => {
                let body_scope = class.body_scope(self.db);
                // Skip the class body scope itself.
                (
                    body_scope.file(self.db),
                    body_scope.file_scope_id(self.db),
                    1,
                )
            }
            ClassLiteral::Dynamic(class) => {
                // Dynamic classes don't have a body scope; start from the enclosing scope.
                let scope = class.scope(self.db);
                (scope.file(self.db), scope.file_scope_id(self.db), 0)
            }
            ClassLiteral::DynamicNamedTuple(namedtuple) => {
                // Dynamic namedtuples don't have a body scope; start from the enclosing scope.
                let scope = namedtuple.scope(self.db);
                (scope.file(self.db), scope.file_scope_id(self.db), 0)
            }
            ClassLiteral::DynamicTypedDict(typeddict) => {
                let scope = typeddict.scope(self.db);
                (scope.file(self.db), scope.file_scope_id(self.db), 0)
            }
            ClassLiteral::DynamicEnum(enum_lit) => {
                let scope = enum_lit.scope(self.db);
                (scope.file(self.db), scope.file_scope_id(self.db), 0)
            }
        };

        display::qualified_name_components_from_scope(self.db, file, file_scope_id, skip_count)
    }
}

impl std::fmt::Display for QualifiedClassName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for parent in self.components_excluding_self() {
            f.write_str(&parent)?;
            f.write_char('.')?;
        }
        f.write_str(self.class.name(self.db))
    }
}

/// CPython internally considers a class a "solid base" if it has an atypical instance memory layout,
/// with additional memory "slots" for each instance, besides the default object metadata and an
/// attribute dictionary. Per [PEP 800], however, we use the term "disjoint base" for this concept.
///
/// A "disjoint base" can be a class defined in a C extension which defines C-level instance slots,
/// or a Python class that defines non-empty `__slots__`. C-level instance slots are not generally
/// visible to Python code, but PEP 800 specifies that any class decorated with
/// `@typing_extensions.disjoint_base` should be treated by type checkers as a disjoint base; it is
/// assumed that classes with C-level instance slots will be decorated as such when they appear in
/// stub files.
///
/// Two disjoint bases can only coexist in a class's MRO if one is a subclass of the other. Knowing if
/// a class is "disjoint base" or not is therefore valuable for inferring whether two instance types or
/// two subclass-of types are disjoint from each other. It also allows us to detect possible
/// `TypeError`s resulting from class definitions.
///
/// [PEP 800]: https://peps.python.org/pep-0800/
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, get_size2::GetSize, salsa::Update)]
pub(super) struct DisjointBase<'db> {
    pub(super) class: ClassLiteral<'db>,
    pub(super) kind: DisjointBaseKind,
}

impl<'db> DisjointBase<'db> {
    /// Creates a [`DisjointBase`] instance where we know the class is a disjoint base
    /// because it has the `@disjoint_base` decorator on its definition
    fn due_to_decorator(class: StaticClassLiteral<'db>) -> Self {
        Self {
            class: ClassLiteral::Static(class),
            kind: DisjointBaseKind::DisjointBaseDecorator,
        }
    }

    /// Creates a [`DisjointBase`] instance where we know the class is a disjoint base
    /// because of its `__slots__` definition.
    fn due_to_dunder_slots(class: ClassLiteral<'db>) -> Self {
        Self {
            class,
            kind: DisjointBaseKind::DefinesSlots,
        }
    }

    /// Two disjoint bases can only coexist in a class's MRO if one is a subclass of the other
    fn could_coexist_in_mro_with(&self, db: &'db dyn Db, other: &Self) -> bool {
        self == other
            || self
                .class
                .default_specialization(db)
                .is_subclass_of(db, other.class.default_specialization(db))
            || other
                .class
                .default_specialization(db)
                .is_subclass_of(db, self.class.default_specialization(db))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub(super) enum DisjointBaseKind {
    /// We know the class is a disjoint base because it's either hardcoded in ty
    /// or has the `@disjoint_base` decorator.
    DisjointBaseDecorator,
    /// We know the class is a disjoint base because it has a non-empty `__slots__` definition.
    DefinesSlots,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct MetaclassError<'db> {
    kind: MetaclassErrorKind<'db>,
}

impl<'db> MetaclassError<'db> {
    /// Return an [`MetaclassErrorKind`] variant describing why we could not resolve the metaclass for this class.
    pub(super) fn reason(&self) -> &MetaclassErrorKind<'db> {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) enum MetaclassErrorKind<'db> {
    /// The class has incompatible metaclasses in its inheritance hierarchy.
    ///
    /// The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all
    /// its bases.
    Conflict {
        /// `candidate1` will either be the explicit `metaclass=` keyword in the class definition,
        /// or the inferred metaclass of a base class
        candidate1: MetaclassCandidate<'db>,

        /// `candidate2` will always be the inferred metaclass of a base class
        candidate2: MetaclassCandidate<'db>,

        /// Flag to indicate whether `candidate1` is the explicit `metaclass=` keyword or the
        /// inferred metaclass of a base class. This helps us give better error messages in diagnostics.
        candidate1_is_base_class: bool,
    },
    /// The metaclass is a parameterized generic class, which is not supported.
    GenericMetaclass,
    /// The metaclass is not callable
    NotCallable(Type<'db>),
    /// The metaclass is of a union type whose some members are not callable
    PartlyNotCallable(Type<'db>),
    /// A cycle was encountered attempting to determine the metaclass
    Cycle,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SlotsKind {
    /// `__slots__` is not found in the class.
    NotSpecified,
    /// `__slots__` is defined but empty: `__slots__ = ()`.
    Empty,
    /// `__slots__` is defined and is not empty: `__slots__ = ("a", "b")`.
    NotEmpty,
    /// `__slots__` is defined but its value is dynamic:
    /// * `__slots__ = tuple(a for a in b)`
    /// * `__slots__ = ["a", "b"]`
    Dynamic,
}

impl SlotsKind {
    fn from(db: &dyn Db, base: StaticClassLiteral) -> Self {
        let Place::Defined(DefinedPlace {
            ty: slots_ty,
            definedness: bound,
            ..
        }) = base
            .own_class_member(db, base.inherited_generic_context(db), None, "__slots__")
            .inner
            .place
        else {
            return Self::NotSpecified;
        };

        if matches!(bound, Definedness::PossiblyUndefined) {
            return Self::Dynamic;
        }

        match slots_ty {
            // __slots__ = ("a", "b")
            Type::NominalInstance(nominal) => match nominal
                .tuple_spec(db)
                .and_then(|spec| spec.len().into_fixed_length())
            {
                Some(0) => Self::Empty,
                Some(_) => Self::NotEmpty,
                None => Self::Dynamic,
            },

            // __slots__ = "abc"  # Same as `("abc",)`
            Type::LiteralValue(literal) if literal.is_string() => Self::NotEmpty,

            _ => Self::Dynamic,
        }
    }
}
