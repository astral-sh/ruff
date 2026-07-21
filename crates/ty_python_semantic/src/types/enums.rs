use crate::SemanticContext;
use compact_str::ToCompactString;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::FxOrderSet;
use crate::{
    Db, FxIndexMap,
    place::{
        DefinedPlace, Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations,
    },
    reachability::DeclarationsIteratorExtension,
    types::{
        ClassBase, ClassLiteral, DynamicType, EnumLiteralType, IntersectionType, KnownClass,
        LiteralValueTypeKind, MemberLookupPolicy, NegativeIntersectionElements, StaticClassLiteral,
        Type, UnionType, binding_type,
        function::FunctionType,
        set_theoretic::{
            RecursivelyDefined,
            builder::{IntersectionBuilder, UnionBuilder},
        },
    },
};
use ty_python_core::{definition::DefinitionKind, place_table, scope::ScopeId, use_def_map};

/// A resolved enum method, retaining both whether it is analyzable and who defines it.
///
/// Standard-library methods and user-defined methods are both callable functions, but callers need
/// to distinguish them: standard-library methods have modeled behavior, while user-defined or
/// opaque methods may replace the member value arbitrarily.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, salsa::SalsaValue)]
pub(super) enum ResolvedEnumMethod<'db> {
    #[default]
    Absent,
    StandardLibrary(FunctionType<'db>),
    UserDefined(FunctionType<'db>),
    Opaque,
}

/// A built-in enum data-type mixin whose runtime value normalization ty models.
///
/// ```python
/// from enum import Enum
///
/// class Number(int, Enum):
///     FALSE = False
///     ZERO = 0  # Alias of `FALSE` after `int(False)` produces `0`.
/// ```
///
/// User-defined data types are excluded because their construction, attribute access, equality, and
/// hashing semantics can differ from the built-in scalar later in their MRO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnownEnumDataTypeMixin {
    Int,
    Str,
}

impl KnownEnumDataTypeMixin {
    /// Returns the scalar payload type after applying the built-in mixin's constructor.
    ///
    /// Literal conversions are preserved precisely, unions are normalized element-wise, and values
    /// whose conversion cannot be modeled precisely fall back to the mixin's instance type.
    fn normalize_value<'db>(self, ctx: &SemanticContext<'db>, value: Type<'db>) -> Type<'db> {
        let db = ctx.db();
        if let Type::Union(union) = value {
            return union.map(ctx, |element| self.normalize_value(ctx, *element));
        }

        match (self, value.as_literal_value_kind()) {
            (Self::Int, Some(LiteralValueTypeKind::Int(_)))
            | (Self::Str, Some(LiteralValueTypeKind::String(_))) => value,
            (Self::Int, Some(LiteralValueTypeKind::Bool(value))) => {
                Type::int_literal(i64::from(value))
            }
            (Self::Str, Some(LiteralValueTypeKind::Int(value))) => {
                Type::string_literal(db, value.to_compact_string())
            }
            (Self::Str, Some(LiteralValueTypeKind::Bool(value))) => {
                Type::string_literal(db, if value { "True" } else { "False" })
            }
            (Self::Int, _) => KnownClass::Int.to_instance(ctx),
            (Self::Str, _) => KnownClass::Str.to_instance(ctx),
        }
    }
}

impl<'db> ResolvedEnumMethod<'db> {
    pub(super) const fn function(self) -> Option<FunctionType<'db>> {
        match self {
            Self::StandardLibrary(function) | Self::UserDefined(function) => Some(function),
            Self::Absent | Self::Opaque => None,
        }
    }

    const fn is_present(self) -> bool {
        !matches!(self, Self::Absent)
    }

    const fn is_user_defined(self) -> bool {
        matches!(self, Self::UserDefined(_) | Self::Opaque)
    }

    const fn is_opaque(self) -> bool {
        matches!(self, Self::Opaque)
    }
}

/// Describes the runtime steps that may transform a declared enum member value.
///
/// Different consumers require different levels of conservatism. Value inference trusts known
/// standard-library data types but treats user-defined data types and constructors as possible
/// transformations, while alias detection follows the value captured before `__init__`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, salsa::SalsaValue)]
pub(super) struct EnumValueConstruction<'db> {
    pub(super) init: ResolvedEnumMethod<'db>,
    pub(super) new: ResolvedEnumMethod<'db>,
    generate_next_value: ResolvedEnumMethod<'db>,
    data_type: InheritedEnumDataType,
    pub(super) metaclass_may_transform_values: bool,
}

impl<'db> EnumValueConstruction<'db> {
    /// Returns whether a member value can be checked directly against an explicit `_value_`
    /// annotation.
    ///
    /// User-defined data types and constructor methods may replace `_value_`; a custom metaclass may
    /// rewrite the member value before construction.
    pub(crate) const fn can_validate_with_value_annotation(self) -> bool {
        matches!(self.init, ResolvedEnumMethod::Absent)
            && matches!(self.new, ResolvedEnumMethod::Absent)
            && !matches!(self.data_type, InheritedEnumDataType::Opaque)
            && !self.metaclass_may_transform_values
    }

    /// Returns whether the declared value cannot be used as the precise type of `.value`.
    ///
    /// Standard-library data types are trusted because their value normalization is either modeled
    /// directly or reflected in the inherited `_value_` annotation. A resolvable
    /// `_generate_next_value_` is excluded because its return type can be used for an `auto()` member
    /// instead.
    const fn member_value_may_be_transformed(self, is_auto: bool) -> bool {
        self.init.is_user_defined()
            || self.new.is_user_defined()
            || matches!(self.data_type, InheritedEnumDataType::Opaque)
            || self.metaclass_may_transform_values
            || (is_auto && self.generate_next_value.is_opaque())
    }

    /// Returns whether the declared member values cannot be combined into a precise instance
    /// `.value` type.
    ///
    /// `_generate_next_value_` is excluded because `value_type` incorporates its return type for
    /// each `auto()` member before the values are combined.
    const fn instance_value_may_be_transformed(self) -> bool {
        self.init.is_present()
            || self.new.is_present()
            || matches!(self.data_type, InheritedEnumDataType::Opaque)
            || self.metaclass_may_transform_values
    }

    /// Returns the payload after known built-in data-type construction, or `None` when the
    /// constructor may coerce it in a way that ty does not model.
    fn normalize_value(self, ctx: &SemanticContext<'db>, value: Type<'db>) -> Option<Type<'db>> {
        match self.data_type {
            InheritedEnumDataType::None => Some(value),
            InheritedEnumDataType::DeclaredValue(data_type) => {
                value_has_exact_known_class(ctx, value, data_type).then_some(value)
            }
            InheritedEnumDataType::Known(mixin) => Some(mixin.normalize_value(ctx, value)),
            InheritedEnumDataType::Opaque => None,
        }
    }

    /// Returns the value to use when checking whether an enum member is an alias.
    ///
    /// For an `auto()` member with a resolvable `_generate_next_value_`, this uses the generator's
    /// return type instead of the placeholder collected from the member declaration.
    ///
    /// Returns `None` when construction can rewrite `_value_` before alias registration, because
    /// the inferred value is not reliable alias evidence in those cases. `__init__` does not affect
    /// this result because alias registration uses the value captured before `__init__` runs.
    fn alias_detection_value(
        self,
        ctx: &SemanticContext<'db>,
        value_ty: Type<'db>,
        is_auto: bool,
    ) -> Option<Type<'db>> {
        let db = ctx.db();
        if self.new.is_user_defined()
            || matches!(self.data_type, InheritedEnumDataType::Opaque)
            || self.metaclass_may_transform_values
        {
            return None;
        }

        let value = if !is_auto {
            value_ty
        } else if self.generate_next_value.is_opaque() {
            return None;
        } else if let Some(function) = self.generate_next_value.function() {
            function.signature(db).overload_return_type_or_unknown(ctx)
        } else {
            value_ty
        };
        self.normalize_value(ctx, value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, salsa::SalsaValue)]
enum EnumValueAnnotation<'db> {
    /// An annotation declared on this enum or a user-defined parent enum.
    UserDefined(Type<'db>),
    /// An annotation inherited from a known standard-library enum class.
    StandardLibrary(Type<'db>),
}

impl<'db> EnumValueAnnotation<'db> {
    const fn ty(self) -> Type<'db> {
        match self {
            Self::UserDefined(ty) | Self::StandardLibrary(ty) => ty,
        }
    }
}

#[derive(Debug, PartialEq, Eq, salsa::SalsaValue)]
pub(crate) struct EnumMetadata<'db> {
    pub(crate) members: FxIndexMap<Name, Type<'db>>,
    pub(crate) aliases: FxHashMap<Name, Name>,

    /// Whether alias detection was precise for every member declaration.
    pub(super) aliases_are_known: bool,

    /// Members whose values were defined using `auto()`.
    pub(crate) auto_members: FxHashSet<Name>,

    /// The effective `_value_` annotation, including where it was defined.
    value_annotation: Option<EnumValueAnnotation<'db>>,

    /// How enum construction may transform declared member values.
    pub(super) value_construction: EnumValueConstruction<'db>,
}

impl get_size2::GetSize for EnumMetadata<'_> {}

pub(super) fn class_defines_property<'db>(
    ctx: &SemanticContext<'db>,
    class: ClassLiteral<'db>,
    name: &str,
) -> bool {
    let db = ctx.db();
    let Some(class) = Type::ClassLiteral(class).to_class_type(ctx) else {
        return false;
    };

    for base in class.iter_mro(ctx) {
        let ClassBase::Class(base) = base else {
            continue;
        };
        if matches!(
            base.known(db),
            Some(
                KnownClass::Enum
                    | KnownClass::StrEnum
                    | KnownClass::IntEnum
                    | KnownClass::Flag
                    | KnownClass::IntFlag
            )
        ) {
            return false;
        }
        if let Some(member) = base
            .own_class_member(ctx, None, name)
            .inner
            .place
            .raw_type()
        {
            return member.is_property_instance();
        }
    }

    false
}

/// An enum class literal with its canonical members, value types, and aliases.
///
/// This keeps the enum-specific information used by enum literals and enum complements alongside
/// the underlying class literal.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumClassLiteral<'db> {
    #[returns(copy)]
    pub(crate) class_literal: ClassLiteral<'db>,
    #[returns(ref)]
    pub(crate) members: Box<[(Name, Type<'db>)]>,
    #[returns(ref)]
    pub(crate) aliases: Box<[(Name, Name)]>,
    /// Whether the canonical member and alias sets are known exactly.
    #[returns(copy)]
    pub(super) aliases_are_known: bool,
    /// Whether the canonical members exhaust the runtime values of this enum class.
    ///
    /// `Flag` classes, transforming metaclasses, and enums with a custom `_missing_` method can
    /// create runtime members beyond those declared in the class body, so their declared members
    /// are not a closed value set.
    #[returns(copy)]
    pub(crate) members_are_exhaustive: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for EnumClassLiteral<'_> {}

impl<'db> ClassLiteral<'db> {
    pub(crate) fn into_enum_class(self, db: &'db dyn Db) -> Option<EnumClassLiteral<'db>> {
        enum_class_literal(db, self)
    }
}

#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
fn enum_class_literal<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> Option<EnumClassLiteral<'db>> {
    let metadata = enum_metadata(db, class)?;
    let ctx = SemanticContext::from_file(db, class.python_file(db));
    let members = metadata
        .members
        .keys()
        .map(|name| metadata.value_type(&ctx, name).map(|ty| (name.clone(), ty)))
        .collect::<Option<Box<[_]>>>()?;
    let mut aliases: Vec<_> = metadata
        .aliases
        .iter()
        .map(|(alias, member)| (alias.clone(), member.clone()))
        .collect();
    aliases.sort_unstable();
    let members_are_exhaustive = !metadata.value_construction.metaclass_may_transform_values
        && !Type::ClassLiteral(class).is_subtype_of(&ctx, KnownClass::Flag.to_subclass_of(&ctx))
        && !enum_has_custom_missing(&ctx, class);

    Some(EnumClassLiteral::new(
        db,
        class,
        members,
        aliases.into_boxed_slice(),
        metadata.aliases_are_known,
        members_are_exhaustive,
    ))
}

/// Return whether enum construction may create pseudo-members through a custom `_missing_` method.
fn enum_has_custom_missing<'db>(ctx: &SemanticContext<'db>, class: ClassLiteral<'db>) -> bool {
    let db = ctx.db();
    let ClassLiteral::Static(class) = class else {
        return false;
    };

    class
        .iter_mro(ctx, None)
        .filter_map(ClassBase::into_class)
        .take_while(|base| base.known(db) != Some(KnownClass::Enum))
        .filter_map(|base| base.class_literal(db).as_static())
        .any(|base| custom_enum_method(db, base.body_scope(db), "_missing_").is_some())
}

impl<'db> EnumClassLiteral<'db> {
    pub(crate) fn member_count(self, db: &'db dyn Db) -> usize {
        self.members(db).len()
    }

    pub(crate) fn member_names(self, db: &'db dyn Db) -> impl Iterator<Item = &'db Name> {
        self.members(db).iter().map(|(name, _)| name)
    }

    fn resolve_member_entry(self, db: &'db dyn Db, name: &Name) -> Option<&'db (Name, Type<'db>)> {
        let members = self.members(db);
        if let Some(member) = members.iter().find(|(member, _)| member == name) {
            return Some(member);
        }

        let aliases = self.aliases(db);
        let alias_index = aliases
            .binary_search_by(|(alias, _)| alias.cmp(name))
            .ok()?;
        let canonical_name = &aliases[alias_index].1;
        members.iter().find(|(member, _)| member == canonical_name)
    }

    pub(crate) fn resolve_member(self, db: &'db dyn Db, name: &Name) -> Option<&'db Name> {
        self.resolve_member_entry(db, name)
            .map(|(member, _)| member)
    }

    /// Returns the type of `.name`/`._name_` for a given enum member.
    ///
    /// This is the canonical member name when alias detection is precise. When alias detection is
    /// inconclusive, we intentionally favor useful literal inference and assume the declaration is
    /// canonical, even though custom enum construction could make it an alias of another member.
    pub(crate) fn name_type(self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        self.resolve_member(db, name)
            .map(|name| Type::string_literal(db, name))
    }

    pub(crate) fn value_type(self, db: &'db dyn Db, name: &Name) -> Option<Type<'db>> {
        self.resolve_member_entry(db, name)
            .map(|(_, value_type)| *value_type)
    }
}

/// Look up an instance member on the finite enum literals represented by a complement.
///
/// The enum-owned `.name`/`.value` attributes can often be answered directly. Other members
/// expand through the remaining literal union so descriptor lookup sees ordinary enum literals.
pub(super) fn instance_member_for_enum_complement<'db>(
    ctx: &SemanticContext<'db>,
    complement: EnumComplement<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    if let Some(member) = special_member_for_enum_complement(ctx, complement, name) {
        member
    } else {
        complement
            .remaining_literal_union(ctx)
            .instance_member(ctx, name)
    }
}

/// Perform full member lookup for an enum complement while preserving the caller's policy.
///
/// This mirrors `instance_member_for_enum_complement`, but routes the non-special case through
/// general member lookup so descriptor and class-variable policy is still applied.
pub(super) fn member_lookup_for_enum_complement<'db>(
    ctx: &SemanticContext<'db>,
    complement: EnumComplement<'db>,
    name: &str,
    policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    if let Some(member) = special_member_for_enum_complement(ctx, complement, name) {
        member
    } else {
        complement
            .remaining_literal_union(ctx)
            .member_lookup_with_policy(ctx, name, policy)
    }
}

/// Return a precise enum-owned `.name`/`.value` attribute for a complement when possible.
///
/// If a complement carries dynamic rest components, expanding it to the remaining literals would
/// make `.name` and `.value` imprecise. These enum-owned attributes can instead be computed
/// directly from the remaining canonical members.
fn special_member_for_enum_complement<'db>(
    ctx: &SemanticContext<'db>,
    complement: EnumComplement<'db>,
    name: &str,
) -> Option<PlaceAndQualifiers<'db>> {
    let db = ctx.db();
    if matches!(name, "name" | "_name_" | "value" | "_value_")
        && !class_defines_property(ctx, complement.enum_class(db), name)
        && complement.rest(db).iter().all(Type::is_dynamic)
        && let Some(member_ty) = complement.member_type(ctx, name)
    {
        Some(Place::bound(member_ty).into())
    } else {
        None
    }
}

/// Return whether a known standard-library constructor preserves the inferred value type.
///
/// An inherited `_value_` annotation identifies the constructor's runtime output class. Literal
/// values of that exact class retain their precision, while values of subclasses such as `bool`
/// are normalized to the annotated class by constructors such as `int.__new__`.
fn known_constructor_preserves_value_type<'db>(
    ctx: &SemanticContext<'db>,
    value: Type<'db>,
    annotation: Type<'db>,
) -> bool {
    let db = ctx.db();
    let annotation = annotation.resolve_type_alias(ctx);
    match value.resolve_type_alias(ctx) {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| known_constructor_preserves_value_type(ctx, *element, annotation)),
        Type::LiteralValue(literal) => literal.fallback_instance(ctx) == annotation,
        value => value == annotation,
    }
}

/// Return whether constructing `data_type` from `value` preserves the value's inferred runtime
/// class. This deliberately requires an exact known class rather than accepting subclasses whose
/// constructor may return an instance of the built-in base.
fn value_has_exact_known_class<'db>(
    ctx: &SemanticContext<'db>,
    value: Type<'db>,
    data_type: KnownClass,
) -> bool {
    let db = ctx.db();
    match value.resolve_type_alias(ctx) {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| value_has_exact_known_class(ctx, *element, data_type)),
        Type::LiteralValue(literal) => match literal.fallback_instance(ctx) {
            Type::NominalInstance(instance) => instance.has_known_class(db, data_type),
            _ => false,
        },
        Type::NominalInstance(instance) => instance.has_known_class(db, data_type),
        _ => false,
    }
}

impl<'db> EnumMetadata<'db> {
    fn empty() -> Self {
        EnumMetadata {
            members: FxIndexMap::default(),
            aliases: FxHashMap::default(),
            aliases_are_known: false,
            auto_members: FxHashSet::default(),
            value_annotation: None,
            value_construction: EnumValueConstruction::default(),
        }
    }

    /// Returns the type of `.value`/`._value_` for a given enum member.
    ///
    /// A user-defined `_value_` annotation takes priority. Otherwise, values transformed by
    /// user-defined data types, construction methods, or metaclasses become `Any`. Known built-in
    /// data types normalize the value directly. A literal is preserved when its runtime class
    /// matches an inherited `_value_` annotation; otherwise, the annotation describes the
    /// normalized value.
    pub(crate) fn value_type(
        &self,
        ctx: &SemanticContext<'db>,
        member_name: &Name,
    ) -> Option<Type<'db>> {
        if !self.members.contains_key(member_name) {
            return None;
        }

        if let Some(EnumValueAnnotation::UserDefined(annotation)) = self.value_annotation {
            return Some(annotation);
        }
        let Some(value) = self.concrete_value_type(ctx, member_name) else {
            return Some(Type::Dynamic(DynamicType::Any));
        };

        if let Some(EnumValueAnnotation::StandardLibrary(annotation)) = self.value_annotation
            && !known_constructor_preserves_value_type(ctx, value, annotation)
        {
            Some(annotation)
        } else {
            Some(value)
        }
    }

    /// Returns the normalized member payload when construction is known not to transform it.
    ///
    /// Unlike [`Self::value_type`], this does not substitute a `_value_` annotation for the
    /// concrete payload.
    pub(super) fn concrete_value_type(
        &self,
        ctx: &SemanticContext<'db>,
        member_name: &Name,
    ) -> Option<Type<'db>> {
        let db = ctx.db();
        let declared_value = self.members.get(member_name).copied()?;
        if self.member_value_may_be_transformed(member_name) {
            return None;
        }
        let value = if self.auto_members.contains(member_name)
            && self
                .value_construction
                .generate_next_value
                .is_user_defined()
            && let Some(func_ty) = self.value_construction.generate_next_value.function()
        {
            func_ty.signature(db).overload_return_type_or_unknown(ctx)
        } else {
            declared_value
        };
        self.value_construction.normalize_value(ctx, value)
    }

    /// Return whether enum construction may replace the value declared for `member_name`.
    ///
    /// An opaque `_generate_next_value_` only affects members declared with `auto()`.
    fn member_value_may_be_transformed(&self, member_name: &Name) -> bool {
        self.value_construction
            .member_value_may_be_transformed(self.auto_members.contains(member_name))
    }

    /// Returns the type of `.value`/`._value_` for an enum instance that is not
    /// narrowed to a specific member (e.g. `x: MyEnum` where `MyEnum` has multiple members).
    ///
    /// If there is an explicit `_value_` annotation, returns that.
    /// If there is a user-defined data type, a custom `__init__` or `__new__`, or a custom enum
    /// metaclass that may transform member values, returns `Any`.
    /// Otherwise, returns the union of each member's `value_type`, which
    /// applies `_generate_next_value_`'s return type to `auto()` members.
    pub(crate) fn instance_value_type(&self, ctx: &SemanticContext<'db>) -> Option<Type<'db>> {
        if self.members.is_empty() {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation.ty())
        } else if self.value_construction.instance_value_may_be_transformed() {
            Some(Type::Dynamic(DynamicType::Any))
        } else {
            let union = self
                .members
                .keys()
                .filter_map(|name| self.value_type(ctx, name))
                .fold(UnionBuilder::new(ctx), UnionBuilder::add)
                .build();
            Some(union)
        }
    }

    /// Returns the effective `_value_` annotation type without its provenance.
    pub(crate) fn value_annotation_type(&self) -> Option<Type<'db>> {
        self.value_annotation.map(EnumValueAnnotation::ty)
    }

    /// Returns the type of `.name`/`._name_` for an enum instance that is not
    /// narrowed to a specific member (e.g. `x: MyEnum` where `MyEnum` has multiple members).
    ///
    /// Returns the union of all member name string literals.
    pub(crate) fn instance_name_type(&self, ctx: &SemanticContext<'db>) -> Option<Type<'db>> {
        let db = ctx.db();
        if self.members.is_empty() {
            return None;
        }
        let union = self
            .members
            .keys()
            .map(|name| Type::string_literal(db, name))
            .fold(UnionBuilder::new(ctx), UnionBuilder::add)
            .build();
        Some(union)
    }
}

/// A compact representation of an enum type with excluded members.
///
/// This corresponds to intersection types like `Color & ~Literal[Color.RED]`, but is kept as its
/// own type shape so callers do not have to rediscover that intersection pattern independently.
/// The complement remains compact until some operation explicitly needs the finite literal
/// alternatives.
///
/// ```python
/// from enum import Enum
///
/// class Color(Enum):
///     RED = 1
///     BLUE = 2
///
/// def f(color: Color):
///     if color is not Color.RED:
///         reveal_type(color)  # Color, excluding Color.RED
/// ```
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumComplementType<'db> {
    #[returns(copy)]
    pub(crate) enum_class_literal: EnumClassLiteral<'db>,
    /// Canonical enum-member names excluded by this complement.
    #[returns(ref)]
    pub(crate) excluded_names: FxOrderSet<Name>,
    /// The rest of the intersection's positive components, such as `Any`, that must be kept when
    /// expanding the complement.
    #[returns(ref)]
    pub(crate) rest: FxOrderSet<Type<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for EnumComplementType<'_> {}

pub(crate) type EnumComplement<'db> = EnumComplementType<'db>;

#[salsa::tracked]
impl<'db> EnumComplementType<'db> {
    /// Recognize the compact enum-complement shape inside an intersection.
    pub(crate) fn from_intersection_parts(
        ctx: &SemanticContext<'db>,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> Option<Self> {
        let db = ctx.db();
        let mut enum_class = None;
        let mut rest = SmallVec::<[Type<'db>; 1]>::default();
        for positive in positive {
            let Type::NominalInstance(instance) = positive else {
                rest.push(*positive);
                continue;
            };

            let Some(enum_class_literal) = instance.class_literal(ctx).into_enum_class(db) else {
                rest.push(*positive);
                continue;
            };

            if enum_class.replace(enum_class_literal).is_some() {
                return None;
            }
        }

        let enum_class_literal = enum_class?;
        if !enum_class_literal.members_are_exhaustive(db) {
            return None;
        }
        let mut excluded_names = FxHashSet::default();
        for negative in negative {
            let enum_literal = negative.as_enum_literal()?;
            if enum_literal.enum_class_literal(db) != enum_class_literal {
                return None;
            }

            let name = enum_literal.name(db);
            let canonical_name = enum_class_literal.resolve_member(db, name)?;
            excluded_names.insert(canonical_name.clone());
        }

        (!excluded_names.is_empty()).then(|| {
            let excluded_names: FxOrderSet<Name> = enum_class_literal
                .member_names(db)
                .filter(|name| excluded_names.contains(*name))
                .cloned()
                .collect();
            let rest: FxOrderSet<Type<'db>> = rest.into_iter().collect();
            Self::new(db, enum_class_literal, excluded_names, rest)
        })
    }

    pub(crate) fn enum_class(self, db: &'db dyn Db) -> ClassLiteral<'db> {
        self.enum_class_literal(db).class_literal(db)
    }

    fn remaining_member_names(self, db: &'db dyn Db) -> impl Iterator<Item = &'db Name> {
        self.enum_class_literal(db)
            .member_names(db)
            .filter(move |name| !self.excluded_names(db).contains(*name))
    }

    /// Count the canonical enum members still represented by this complement.
    fn remaining_member_count(self, db: &'db dyn Db) -> usize {
        self.enum_class_literal(db).member_count(db) - self.excluded_names(db).len()
    }

    /// Return `true` when this complement represents exactly one enum literal.
    ///
    /// Complements with rest components are not singletons, because those positive intersection
    /// components must still be preserved even when only one enum member remains.
    pub(crate) fn is_singleton(self, db: &'db dyn Db) -> bool {
        self.rest(db).is_empty() && self.remaining_member_count(db) == 1
    }

    /// Return `true` when this complement is a single value under equality narrowing.
    ///
    /// Enums that override equality are excluded because one remaining enum literal can still
    /// compare equal to non-identical values.
    pub(crate) fn is_single_valued(self, ctx: &SemanticContext<'db>) -> bool {
        let db = ctx.db();
        self.is_singleton(db) && {
            let enum_class = self.enum_class(db);
            !enum_class
                .to_non_generic_instance(ctx)
                .overrides_equality(ctx)
        }
    }

    /// Expand this complement to the enum literals that remain possible.
    pub fn remaining_literal_types(self, ctx: &SemanticContext<'db>) -> Vec<Type<'db>> {
        let db = ctx.db();
        self.remaining_member_names(db)
            .map(|name| self.remaining_literal_type(ctx, name))
            .collect()
    }

    /// Expand this complement to the union of enum literals that remain possible.
    pub(crate) fn remaining_literal_union(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        let alternatives = self.remaining_literal_types(ctx);
        match alternatives.as_slice() {
            [] => Type::Never,
            [single] => *single,
            // Keep this exact. Routing these literals through `UnionBuilder` can widen very large
            // enum complements back to the original enum class, losing the excluded members that
            // made the compact complement useful in the first place.
            _ => Type::Union(UnionType::new(
                db,
                alternatives.into_boxed_slice(),
                RecursivelyDefined::No,
            )),
        }
    }

    /// Build the type for one remaining canonical member, preserving any positive rest components.
    fn remaining_literal_type(self, ctx: &SemanticContext<'db>, name: &Name) -> Type<'db> {
        let db = ctx.db();
        let literal =
            Type::enum_literal(EnumLiteralType::new(db, self.enum_class_literal(db), name));
        if self.rest(db).is_empty() {
            return literal;
        }

        let mut builder = IntersectionBuilder::new(ctx).add_positive(literal);
        for rest in self.rest(db) {
            builder = builder.add_positive(*rest);
        }
        builder.build()
    }

    /// Expand this complement for display only if the resulting `Literal[...]` remains concise.
    ///
    /// This keeps small enum complements readable as literal groups while preserving the compact
    /// intersection form for large generated enums.
    pub(crate) fn remaining_literal_types_for_display(
        self,
        ctx: &SemanticContext<'db>,
        max_literals: usize,
    ) -> Option<Vec<Type<'db>>> {
        let db = ctx.db();
        if !self.rest(db).is_empty() {
            return None;
        }

        let remaining_count = self.remaining_member_count(db);
        if remaining_count == 0 || remaining_count > max_literals {
            return None;
        }

        Some(self.remaining_literal_types(ctx))
    }

    /// Return the type of a member attribute for all enum literals remaining in this complement.
    ///
    /// This handles `.name`, `.value`, `._name_`, and `._value_` by unioning the corresponding
    /// attribute type from each remaining canonical enum member.
    pub(crate) fn member_type(
        self,
        ctx: &SemanticContext<'db>,
        member_name: &str,
    ) -> Option<Type<'db>> {
        let db = ctx.db();
        let enum_class_literal = self.enum_class_literal(db);
        let is_enum_subclass = Type::ClassLiteral(self.enum_class(db))
            .is_subtype_of(ctx, KnownClass::Enum.to_subclass_of(ctx));
        let mut builder = UnionBuilder::new(ctx);
        let mut found_member = false;

        for name in self.remaining_member_names(db) {
            let member_ty = (match member_name {
                "name" if is_enum_subclass => enum_class_literal.name_type(db, name),
                "_name_" => enum_class_literal.name_type(db, name),
                "value" if is_enum_subclass => enum_class_literal.value_type(db, name),
                "_value_" => enum_class_literal.value_type(db, name),
                _ => None,
            })?;

            builder = builder.add(member_ty);
            found_member = true;
        }

        found_member.then(|| builder.build())
    }

    /// Return `true` if users can spell an equivalent type for this complement.
    pub(crate) fn is_spellable(self, db: &'db dyn Db) -> bool {
        // A plain enum complement is an implementation detail for a type that users can still spell
        // as a union of the remaining enum literals. Complements with `rest` components, such as
        // `Color & Any & ~Literal[Color.RED]`, are not equivalent to that literal union because the
        // additional intersection components must remain.
        self.rest(db).is_empty()
    }

    /// Reconstruct the equivalent set-theoretic intersection.
    pub(crate) fn to_intersection(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        let enum_class = self.enum_class(db);
        let mut positive = FxOrderSet::from_iter([enum_class.to_non_generic_instance(ctx)]);
        positive.extend(self.rest(db).iter().copied());

        let mut negative = NegativeIntersectionElements::default();
        for name in self.excluded_names(db) {
            negative.insert(Type::enum_literal(EnumLiteralType::new(
                db,
                self.enum_class_literal(db),
                name,
            )));
        }

        Type::Intersection(IntersectionType::new(db, positive, negative))
    }
}

/// Returns the set of names listed in an enum's `_ignore_` attribute.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn enum_ignored_names<'db>(db: &'db dyn Db, scope_id: ScopeId<'db>) -> FxHashSet<Name> {
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    let Some(ignore) = table.symbol_id("_ignore_") else {
        return FxHashSet::default();
    };

    let ignore_bindings = use_def_map.reachable_symbol_bindings(ignore);
    let ctx = SemanticContext::from_file(db, scope_id.python_file(db));
    let ignore_place = place_from_bindings(&ctx, ignore_bindings).place;

    match ignore_place {
        Place::Defined(DefinedPlace { ty, .. }) => ty
            .as_string_literal()
            .map(|ignored_names| {
                ignored_names
                    .value(db)
                    .split_ascii_whitespace()
                    .map(Name::new)
                    .collect()
            })
            .unwrap_or_default(),

        // TODO: support the list-variant of `_ignore_`.
        Place::Undefined => FxHashSet::default(),
    }
}

/// If `value_ty` has a supported literal value, record it as canonical or as an alias of an existing
/// value. Returns `None` when the value is not precise enough for alias detection. Literal metadata
/// does not affect enum aliasing at runtime, so the map is keyed by [`LiteralValueTypeKind`] rather
/// than [`Type`].
fn try_register_alias<'db>(
    value_ty: Type<'db>,
    name: &Name,
    enum_values: &mut FxHashMap<LiteralValueTypeKind<'db>, Name>,
    aliases: &mut FxHashMap<Name, Name>,
) -> Option<bool> {
    let value = value_ty.as_literal_value_kind()?;
    if !matches!(
        value,
        LiteralValueTypeKind::Bool(_)
            | LiteralValueTypeKind::Int(_)
            | LiteralValueTypeKind::String(_)
            | LiteralValueTypeKind::Bytes(_)
    ) {
        return None;
    }
    if let Some(canonical) = enum_values.get(&value) {
        aliases.insert(name.clone(), canonical.clone());
        return Some(true);
    }
    enum_values.insert(value, name.clone());
    Some(false)
}

/// List all members of an enum.
#[salsa::tracked(returns(as_ref), cycle_initial=|_, _, _| Some(EnumMetadata::empty()), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn enum_metadata<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> Option<EnumMetadata<'db>> {
    let class = match class {
        ClassLiteral::Static(class) => class,
        ClassLiteral::Dynamic(..) => {
            // Classes created via `type` cannot be enums; the following fails at runtime:
            // ```python
            // import enum
            //
            // class BaseEnum(enum.Enum):
            //     pass
            //
            // MyEnum = type("MyEnum", (BaseEnum,), {"A": 1, "B": 2})
            // ```
            return None;
        }
        ClassLiteral::DynamicNamedTuple(..) | ClassLiteral::DynamicTypedDict(..) => return None,
        ClassLiteral::DynamicEnum(enum_lit) => {
            let spec = enum_lit.spec(db);
            if !spec.has_known_members(db) {
                return None;
            }
            let ctx = SemanticContext::from_file(db, enum_lit.scope(db).python_file(db));
            let value_construction = EnumValueConstruction {
                data_type: inherited_enum_data_type(&ctx, ClassLiteral::DynamicEnum(enum_lit)),
                ..EnumValueConstruction::default()
            };
            let mut members = FxIndexMap::default();
            let mut aliases = FxHashMap::default();
            let mut enum_values: FxHashMap<LiteralValueTypeKind<'db>, Name> = FxHashMap::default();
            for (name, ty) in spec.members(db) {
                if value_construction
                    .alias_detection_value(&ctx, *ty, false)
                    .and_then(|alias_value_ty| {
                        try_register_alias(alias_value_ty, name, &mut enum_values, &mut aliases)
                            // Identical raw literals remain aliases even when normalization widens.
                            .or_else(|| {
                                try_register_alias(*ty, name, &mut enum_values, &mut aliases)
                            })
                    })
                    == Some(true)
                {
                    continue;
                }
                members.insert(name.clone(), *ty);
            }
            members.shrink_to_fit();

            return Some(EnumMetadata {
                members,
                aliases,
                aliases_are_known: true,
                auto_members: FxHashSet::default(),
                value_annotation: None,
                value_construction,
            });
        }
    };
    // This is a fast path to avoid traversing the MRO of known classes
    if class
        .known(db)
        .is_some_and(|known_class| !known_class.is_enum_subclass_with_members())
    {
        return None;
    }

    let ctx = SemanticContext::from_file(db, class.python_file(db));

    if !is_enum_class_by_inheritance(&ctx, class) {
        return None;
    }

    let scope_id = class.body_scope(db);
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    let mut enum_values: FxHashMap<LiteralValueTypeKind<'db>, Name> = FxHashMap::default();
    let mut auto_counter = 0;
    let mut auto_members = FxHashSet::default();
    let mut aliases_are_known = true;
    let mut prev_value_was_non_literal_int = false;
    let mut prev_bool_literal = None;
    let ignored_names = enum_ignored_names(db, scope_id);

    // Look up custom construction methods, falling back to parent enum classes. An opaque binding
    // still shadows methods from classes later in the MRO.
    let data_type = inherited_enum_data_type(&ctx, ClassLiteral::Static(class));
    let user_defined_init = custom_enum_method(db, scope_id, "__init__")
        .or_else(|| inherited_user_defined_enum_method(&ctx, class, "__init__"));
    let init = resolve_enum_method(user_defined_init, || {
        inherited_known_enum_method(&ctx, class, "__init__")
    });
    // CPython checks `__new_member__` and then `__new__` on each enum base before continuing
    // through the MRO or falling back to the data-type constructor.
    let user_defined_new = custom_enum_method(db, scope_id, "__new__")
        .or_else(|| inherited_user_defined_enum_new(&ctx, class))
        .or_else(|| inherited_user_defined_mixin_new(&ctx, class));
    let new = resolve_enum_method(user_defined_new, || {
        inherited_known_enum_method(&ctx, class, "__new__")
    });
    let metaclass_may_transform_values = enum_metaclass_may_transform_values(&ctx, class);
    let user_defined_generate_next_value =
        custom_enum_method(db, scope_id, "_generate_next_value_")
            .or_else(|| inherited_user_defined_enum_method(&ctx, class, "_generate_next_value_"));
    let generate_next_value = resolve_enum_method(user_defined_generate_next_value, || {
        inherited_known_enum_method(&ctx, class, "_generate_next_value_")
    });
    let value_construction = EnumValueConstruction {
        init,
        new,
        generate_next_value,
        data_type,
        metaclass_may_transform_values,
    };

    let mut aliases = FxHashMap::default();

    let mut members = use_def_map
        .all_end_of_scope_symbol_bindings()
        .filter_map(|(symbol_id, bindings)| {
            let name = table.symbol(symbol_id).name();

            if name.starts_with("__") {
                // Skip private attributes (`__private`) and dunders (`__module__`, etc.).
                // CPython's enum metaclass never treats these as members.
                return None;
            }

            if matches!(
                name.as_str(),
                "_generate_next_value_" | "_ignore_" | "_value_" | "_name_"
            ) || ignored_names.contains(name)
            {
                // Skip ignored attributes
                return None;
            }

            let inferred = place_from_bindings(&ctx, bindings).place;
            let mut explicit_member_wrapper = false;

            let value_ty = match inferred {
                Place::Undefined => {
                    return None;
                }
                Place::Defined(DefinedPlace { ty, .. }) => {
                    let special_case = match ty {
                        Type::Callable(_) | Type::FunctionLiteral(_) => {
                            // Some types are specifically disallowed for enum members.
                            return None;
                        }
                        Type::NominalInstance(instance) => match instance.known_class(db) {
                            // enum.nonmember
                            Some(KnownClass::Nonmember) => return None,

                            // enum.member
                            Some(KnownClass::Member) => {
                                explicit_member_wrapper = true;
                                Some(
                                    ty.member(&ctx, "value")
                                        .place
                                        .ignore_possibly_undefined()
                                        .unwrap_or(Type::unknown()),
                                )
                            }

                            // enum.auto
                            Some(KnownClass::Auto) => {
                                auto_counter += 1;
                                auto_members.insert(name.clone());

                                // `StrEnum`s have different `auto()` behaviour to enums inheriting from `(str, Enum)`
                                let auto_value_ty =
                                    if Type::ClassLiteral(ClassLiteral::Static(class))
                                        .is_subtype_of(
                                            &ctx,
                                            KnownClass::StrEnum.to_subclass_of(&ctx),
                                        )
                                    {
                                        Type::string_literal(db, &*name.to_lowercase())
                                    } else {
                                        let custom_mixins: SmallVec<[Option<KnownClass>; 1]> =
                                            class
                                                .iter_mro(&ctx, None)
                                                .skip(1)
                                                .filter_map(ClassBase::into_class)
                                                .filter(|class| {
                                                    !Type::from(*class).is_subtype_of(
                                                        &ctx,
                                                        KnownClass::Enum.to_subclass_of(&ctx),
                                                    )
                                                })
                                                .map(|class| class.known(db))
                                                .filter(|class| {
                                                    !matches!(class, Some(KnownClass::Object))
                                                })
                                                .collect();

                                        // `IntEnum`s have the same `auto()` behaviour to enums inheriting from `(int, Enum)`,
                                        // and `IntEnum`s also have `int` in their MROs, so both cases are handled here.
                                        //
                                        // In general, the `auto()` behaviour for enums with non-`int` mixins is hard to predict,
                                        // so we fall back to `Any` in those cases.
                                        if matches!(
                                            custom_mixins.as_slice(),
                                            [] | [Some(KnownClass::Int)]
                                        ) {
                                            if prev_value_was_non_literal_int {
                                                KnownClass::Int.to_instance(&ctx)
                                            } else if let Some(prev_bool_literal) =
                                                prev_bool_literal
                                            {
                                                Type::int_literal(i64::from(prev_bool_literal) + 1)
                                            } else {
                                                Type::int_literal(auto_counter)
                                            }
                                        } else {
                                            Type::any()
                                        }
                                    };
                                Some(auto_value_ty)
                            }

                            _ => None,
                        },

                        _ => None,
                    };

                    if let Some(special_case) = special_case {
                        special_case
                    } else {
                        let dunder_get = ty
                            .member_lookup_with_policy(
                                &ctx,
                                "__get__",
                                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                            )
                            .place;

                        match dunder_get {
                            Place::Undefined
                            | Place::Defined(DefinedPlace {
                                ty: Type::Dynamic(_),
                                ..
                            }) => ty,

                            Place::Defined(_) => {
                                // Descriptors are not considered members.
                                return None;
                            }
                        }
                    }
                }
            };

            let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);

            if !explicit_member_wrapper
                && declarations.any_reachable(&ctx, |declaration| {
                    declaration.is_defined_and(|declaration| {
                        !matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(assignment)
                                if assignment
                                    .value(&parsed_module(db, declaration.python_file(db)).load(db))
                                    .is_some()
                        )
                    })
                })
            {
                return None;
            }

            // Track whether this member's value is a non-literal `int`, so a
            // following `auto()` knows to widen its result to `int`.
            prev_value_was_non_literal_int = value_ty.as_int_like_literal().is_none()
                && value_ty.is_assignable_to(&ctx, KnownClass::Int.to_instance(&ctx));
            prev_bool_literal =
                value_ty
                    .as_literal_value_kind()
                    .and_then(|literal| match literal {
                        LiteralValueTypeKind::Bool(value) => Some(value),
                        _ => None,
                    });

            match value_construction
                .alias_detection_value(&ctx, value_ty, auto_members.contains(name))
                .and_then(|alias_value_ty| {
                    try_register_alias(alias_value_ty, name, &mut enum_values, &mut aliases)
                }) {
                Some(true) => return None,
                Some(false) => {}
                None if value_construction.data_type != InheritedEnumDataType::None => {
                    aliases_are_known = false;
                }
                None => {}
            }

            Some((name.clone(), value_ty))
        })
        .collect::<FxIndexMap<_, _>>();

    if members.is_empty() {
        // Enum subclasses without members are not considered enums.
        return None;
    }

    let value_annotation = custom_value_annotation(&ctx, scope_id)
        .or_else(|| inherited_user_defined_value_annotation(&ctx, class))
        .map(EnumValueAnnotation::UserDefined)
        .or_else(|| {
            (!metaclass_may_transform_values)
                .then(|| inherited_value_annotation(&ctx, class))
                .flatten()
                .map(EnumValueAnnotation::StandardLibrary)
        });

    members.shrink_to_fit();

    Some(EnumMetadata {
        members,
        aliases,
        aliases_are_known,
        auto_members,
        value_annotation,
        value_construction,
    })
}

/// Returns whether an enum's metaclass has a custom value-transforming method before the stdlib
/// `EnumType`/`EnumMeta` implementation.
///
/// `__prepare__` can return a namespace that rewrites assignments, and `__new__` can rewrite the
/// completed class dictionary. Either method can therefore change member values before the stdlib
/// enum constructor validates and forwards them to `__new__`/`__init__`.
fn enum_metaclass_may_transform_values<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> bool {
    let db = ctx.db();
    let Some(metaclass) = class.metaclass(ctx).to_class_type(ctx) else {
        return false;
    };

    metaclass
        .class_literal(db)
        .iter_mro(ctx)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.class_literal(db).as_static())
        .take_while(|base| base.known(db) != Some(KnownClass::EnumType))
        .any(|base| {
            ["__prepare__", "__new__"]
                .into_iter()
                .any(|name| custom_enum_method(db, base.body_scope(db), name).is_some())
        })
}

/// Iterates over parent enum classes in the MRO, skipping known enum
/// infrastructure classes but including `IntEnum`, `Flag`, and `IntFlag`
/// which declare `_value_` annotations that normally should be inherited.
fn iter_parent_enum_classes<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> impl Iterator<Item = StaticClassLiteral<'db>> + 'db {
    let ctx = *ctx;
    let db = ctx.db();
    class
        .iter_mro(&ctx, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .filter_map(move |class_type| {
            let base = class_type.class_literal(db).as_static()?;
            let is_traversable = base.known(db).is_none_or(|k| {
                matches!(
                    k,
                    KnownClass::IntEnum | KnownClass::Flag | KnownClass::IntFlag
                )
            });
            (is_traversable && is_enum_class_by_inheritance(&ctx, base)).then_some(base)
        })
}

/// Returns the `_value_` annotation type if one is declared in the given scope.
fn custom_value_annotation<'db>(
    ctx: &SemanticContext<'db>,
    scope: ScopeId<'db>,
) -> Option<Type<'db>> {
    let db = ctx.db();
    let symbol_id = place_table(db, scope).symbol_id("_value_")?;
    let declarations = use_def_map(db, scope).end_of_scope_symbol_declarations(symbol_id);
    place_from_declarations(ctx, declarations)
        .ignore_conflicting_declarations()
        .ignore_possibly_undefined()
}

/// Looks up an inherited `_value_` annotation from parent enum classes in the MRO.
fn inherited_value_annotation<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    let db = ctx.db();
    iter_parent_enum_classes(ctx, class)
        .find_map(|base| custom_value_annotation(ctx, base.body_scope(db)))
}

/// Looks up an inherited `_value_` annotation from user-defined parent enum classes in the MRO.
fn inherited_user_defined_value_annotation<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    let db = ctx.db();
    iter_parent_enum_classes(ctx, class)
        .filter(|base| base.known(db).is_none())
        .find_map(|base| custom_value_annotation(ctx, base.body_scope(db)))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InheritedEnumDataType {
    #[default]
    None,
    DeclaredValue(KnownClass),
    Known(KnownEnumDataTypeMixin),
    Opaque,
}

/// Find the data type selected for this enum.
///
/// CPython searches each direct base independently. We only model a selected built-in data type
/// precisely when no user-defined non-enum base can affect member construction or attribute access.
fn inherited_enum_data_type<'db>(
    ctx: &SemanticContext<'db>,
    class: ClassLiteral<'db>,
) -> InheritedEnumDataType {
    let db = ctx.db();
    let mut selected = InheritedEnumDataType::None;

    for explicit_base in class.explicit_bases(ctx) {
        let Some(explicit_base) = explicit_base.to_class_type(ctx) else {
            return InheritedEnumDataType::Opaque;
        };
        let mut candidate = None;

        for base in explicit_base.iter_mro(ctx) {
            let Some(base) = base.into_class() else {
                return InheritedEnumDataType::Opaque;
            };
            let base = base.class_literal(db);
            if matches!(base, ClassLiteral::DynamicEnum(_)) {
                continue;
            }
            let Some(base) = base.as_static() else {
                return InheritedEnumDataType::Opaque;
            };

            if base.known(db) == Some(KnownClass::Object) || is_enum_class_by_inheritance(ctx, base)
            {
                continue;
            }

            let data_type = match base.known(db) {
                Some(KnownClass::Int) => InheritedEnumDataType::Known(KnownEnumDataTypeMixin::Int),
                Some(KnownClass::Str) => InheritedEnumDataType::Known(KnownEnumDataTypeMixin::Str),
                Some(known) => InheritedEnumDataType::DeclaredValue(known),
                None => return InheritedEnumDataType::Opaque,
            };
            candidate = Some(data_type);
            break;
        }

        let Some(candidate) = candidate else {
            continue;
        };
        selected = match (selected, candidate) {
            (InheritedEnumDataType::None, candidate) => candidate,
            (selected, candidate) if selected == candidate => selected,
            _ => return InheritedEnumDataType::Opaque,
        };
    }

    selected
}

#[derive(Clone, Copy)]
enum EnumMethodBinding<'db> {
    Function(FunctionType<'db>),
    Opaque,
}

/// Returns the enum method defined in `scope`, including opaque bindings.
fn custom_enum_method<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> Option<EnumMethodBinding<'db>> {
    let symbol_id = place_table(db, scope).symbol_id(name)?;
    let mut bindings = use_def_map(db, scope).end_of_scope_symbol_bindings(symbol_id);
    let binding = bindings.next()?;
    if bindings.next().is_some() {
        return Some(EnumMethodBinding::Opaque);
    }

    let definition = binding.binding.definition()?;
    if !definition.kind(db).is_function_def() {
        return Some(EnumMethodBinding::Opaque);
    }

    match binding_type(db, definition) {
        Type::FunctionLiteral(function) => Some(EnumMethodBinding::Function(function)),
        _ => Some(EnumMethodBinding::Opaque),
    }
}

/// Looks up the first user-defined enum method in the MRO.
fn inherited_user_defined_enum_method<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<EnumMethodBinding<'db>> {
    let db = ctx.db();
    iter_parent_enum_classes(ctx, class)
        .filter(|base| base.known(db).is_none())
        .find_map(|base| custom_enum_method(db, base.body_scope(db), name))
}

/// Looks up the first user-defined enum member constructor in the MRO.
fn inherited_user_defined_enum_new<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<EnumMethodBinding<'db>> {
    let db = ctx.db();
    iter_parent_enum_classes(ctx, class)
        .filter(|base| base.known(db).is_none())
        .find_map(|base| {
            let scope = base.body_scope(db);
            custom_enum_method(db, scope, "__new_member__")
                .or_else(|| custom_enum_method(db, scope, "__new__"))
        })
}

/// Looks up a user-defined `__new__` on a data-type mixin anywhere in the MRO, including through an
/// enum base.
///
/// When no enum class provides a member constructor, `EnumType` uses this method to construct the
/// scalar payload stored by the enum member.
fn inherited_user_defined_mixin_new<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<EnumMethodBinding<'db>> {
    let db = ctx.db();
    class
        .iter_mro(ctx, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .filter_map(|class| class.class_literal(db).as_static())
        .filter(|base| base.known(db).is_none())
        .find_map(|base| custom_enum_method(db, base.body_scope(db), "__new__"))
}

/// Looks up a resolvable method inherited from a known enum class.
fn inherited_known_enum_method<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<FunctionType<'db>> {
    let db = ctx.db();
    iter_parent_enum_classes(ctx, class)
        .filter(|base| base.known(db).is_some())
        .find_map(
            |base| match custom_enum_method(db, base.body_scope(db), name) {
                Some(EnumMethodBinding::Function(function)) => Some(function),
                Some(EnumMethodBinding::Opaque) | None => None,
            },
        )
}

/// Resolves a user-defined method without falling through an opaque binding to the known default.
fn resolve_enum_method<'db>(
    user_defined: Option<EnumMethodBinding<'db>>,
    known: impl FnOnce() -> Option<FunctionType<'db>>,
) -> ResolvedEnumMethod<'db> {
    match user_defined {
        Some(EnumMethodBinding::Function(function)) => ResolvedEnumMethod::UserDefined(function),
        Some(EnumMethodBinding::Opaque) => ResolvedEnumMethod::Opaque,
        None => known().map_or(
            ResolvedEnumMethod::Absent,
            ResolvedEnumMethod::StandardLibrary,
        ),
    }
}

/// Return the enum's canonical member literals when they exhaust its runtime domain.
pub(crate) fn enum_member_literals<'a, 'db: 'a>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    exclude_member: Option<&'a Name>,
) -> Option<impl Iterator<Item = Type<'a>> + 'a> {
    let enum_class_literal = class.into_enum_class(db)?;
    if !enum_class_literal.members_are_exhaustive(db) {
        return None;
    }
    Some(
        enum_class_literal
            .member_names(db)
            .filter(move |name| Some(*name) != exclude_member)
            .map(move |name| {
                Type::enum_literal(EnumLiteralType::new(db, enum_class_literal, name))
            }),
    )
}

/// Return whether the enum has exactly one possible runtime value.
pub(crate) fn is_single_member_enum<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
    class.into_enum_class(db).is_some_and(|enum_class| {
        enum_class.members_are_exhaustive(db) && enum_class.member_count(db) == 1
    })
}

pub(crate) fn is_enum_class<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    match ty {
        Type::ClassLiteral(class_literal) => enum_metadata(db, class_literal).is_some(),
        _ => false,
    }
}

/// Checks if a class is an enum class by inheritance (either a subtype of `Enum`
/// or has a metaclass that is a subtype of `EnumType`).
///
/// This is a lighter-weight check than `enum_metadata`, which additionally
/// verifies that the class has members.
pub(crate) fn is_enum_class_by_inheritance<'db>(
    ctx: &SemanticContext<'db>,
    class: StaticClassLiteral<'db>,
) -> bool {
    Type::ClassLiteral(ClassLiteral::Static(class))
        .is_subtype_of(ctx, KnownClass::Enum.to_subclass_of(ctx))
        || class
            .metaclass(ctx)
            .is_subtype_of(ctx, KnownClass::EnumType.to_subclass_of(ctx))
}

/// Extracts the inner value type from an `enum.nonmember()` wrapper.
///
/// At runtime, the enum metaclass unwraps `nonmember(value)`, so accessing the attribute
/// returns the inner value, not the `nonmember` wrapper.
///
/// Returns `Some(value_type)` if the type is a `nonmember[T]`, otherwise `None`.
pub(crate) fn try_unwrap_nonmember_value<'db>(
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
) -> Option<Type<'db>> {
    let db = ctx.db();
    match ty {
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Nonmember) => {
            Some(
                ty.member(ctx, "value")
                    .place
                    .ignore_possibly_undefined()
                    .unwrap_or(Type::unknown()),
            )
        }
        _ => None,
    }
}
