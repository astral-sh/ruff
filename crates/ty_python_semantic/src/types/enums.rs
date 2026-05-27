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
        Type, UnionType,
        function::FunctionType,
        set_theoretic::{
            RecursivelyDefined,
            builder::{IntersectionBuilder, UnionBuilder},
        },
    },
};
use ty_python_core::{definition::DefinitionKind, place_table, scope::ScopeId, use_def_map};

#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(crate) struct EnumMetadata<'db> {
    pub(crate) members: FxIndexMap<Name, Type<'db>>,
    pub(crate) aliases: FxHashMap<Name, Name>,

    /// Members whose values were defined using `auto()`.
    pub(crate) auto_members: FxHashSet<Name>,

    /// The explicit `_value_` annotation type, if declared.
    pub(crate) value_annotation: Option<Type<'db>>,

    /// The custom `__init__` function, if defined on this enum.
    ///
    /// When present, member values are validated by synthesizing a call to
    /// `__init__` rather than by simple type assignability.
    pub(crate) init_function: Option<FunctionType<'db>>,

    /// The custom `__new__` function, if defined on this enum.
    ///
    /// When present, the RHS of a member declaration is not necessarily the
    /// value exposed through `.value`; the method can assign `_value_`
    /// independently.
    pub(crate) new_function: Option<FunctionType<'db>>,

    /// The custom `_generate_next_value_` function, if defined on this enum.
    ///
    /// When present, defines the value returned by calls to `auto()`
    pub(crate) generate_next_value_function: Option<FunctionType<'db>>,

    /// Whether the enum metaclass may transform member values before they are
    /// passed to enum construction hooks.
    pub(crate) custom_enum_metaclass_new: bool,
}

impl get_size2::GetSize for EnumMetadata<'_> {}

/// Look up an instance member on the finite enum literals represented by a complement.
///
/// The enum-owned `.name`/`.value` attributes can often be answered directly. Other members
/// expand through the remaining literal union so descriptor lookup sees ordinary enum literals.
pub(super) fn instance_member_for_enum_complement<'db>(
    db: &'db dyn Db,
    complement: EnumComplement<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    if let Some(member) = special_member_for_enum_complement(db, complement, name) {
        member
    } else {
        complement
            .remaining_literal_union(db)
            .instance_member(db, name)
    }
}

/// Perform full member lookup for an enum complement while preserving the caller's policy.
///
/// This mirrors `instance_member_for_enum_complement`, but routes the non-special case through
/// general member lookup so descriptor and class-variable policy is still applied.
pub(super) fn member_lookup_for_enum_complement<'db>(
    db: &'db dyn Db,
    complement: EnumComplement<'db>,
    name: &str,
    policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    if let Some(member) = special_member_for_enum_complement(db, complement, name) {
        member
    } else {
        complement
            .remaining_literal_union(db)
            .member_lookup_with_policy(db, name.into(), policy)
    }
}

/// Return a precise enum-owned `.name`/`.value` attribute for a complement when possible.
///
/// If a complement carries dynamic rest components, expanding it to the remaining literals would
/// make `.name` and `.value` imprecise. These enum-owned attributes can instead be computed
/// directly from the remaining canonical members.
fn special_member_for_enum_complement<'db>(
    db: &'db dyn Db,
    complement: EnumComplement<'db>,
    name: &str,
) -> Option<PlaceAndQualifiers<'db>> {
    if matches!(name, "name" | "_name_" | "value" | "_value_")
        && complement.rest(db).iter().all(Type::is_dynamic)
        && let Some(member_ty) = complement.member_type(db, name)
    {
        Some(Place::bound(member_ty).into())
    } else {
        None
    }
}

impl<'db> EnumMetadata<'db> {
    fn empty() -> Self {
        EnumMetadata {
            members: FxIndexMap::default(),
            aliases: FxHashMap::default(),
            auto_members: FxHashSet::default(),
            value_annotation: None,
            init_function: None,
            new_function: None,
            generate_next_value_function: None,
            custom_enum_metaclass_new: false,
        }
    }

    /// Returns the type of `.value`/`._value_` for a given enum member.
    ///
    /// Priority: explicit `_value_` annotation, then custom construction hooks
    /// or metaclass value transformation → `Any`, then `_generate_next_value_`
    /// return type for `auto()` members, then the inferred member value type.
    pub(crate) fn value_type(&self, db: &'db dyn Db, member_name: &Name) -> Option<Type<'db>> {
        if !self.members.contains_key(member_name) {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation)
        } else if self.init_function.is_some()
            || self.new_function.is_some()
            || self.custom_enum_metaclass_new
        {
            Some(Type::Dynamic(DynamicType::Any))
        } else if let Some(func_ty) = self.generate_next_value_function
            && self.auto_members.contains(member_name)
        {
            Some(func_ty.signature(db).overload_return_type_or_unknown(db))
        } else {
            self.members.get(member_name).copied()
        }
    }

    /// Returns the type of `.name`/`._name_` for a given enum member.
    ///
    /// This is always a string literal of the member name.
    pub(crate) fn name_type(&self, db: &'db dyn Db, member_name: &Name) -> Option<Type<'db>> {
        self.members
            .contains_key(member_name)
            .then(|| Type::string_literal(db, member_name.as_str()))
    }

    /// Returns the type of `.value`/`._value_` for an enum instance that is not
    /// narrowed to a specific member (e.g. `x: MyEnum` where `MyEnum` has multiple members).
    ///
    /// If there is an explicit `_value_` annotation, returns that.
    /// If there is a custom `__init__` or `__new__` or a custom enum
    /// metaclass may transform member values, returns `Any`.
    /// Otherwise, returns the union of each member's `value_type`, which
    /// applies `_generate_next_value_`'s return type to `auto()` members.
    pub(crate) fn instance_value_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.members.is_empty() {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation)
        } else if self.init_function.is_some()
            || self.new_function.is_some()
            || self.custom_enum_metaclass_new
        {
            Some(Type::Dynamic(DynamicType::Any))
        } else {
            let union = self
                .members
                .keys()
                .filter_map(|name| self.value_type(db, name))
                .fold(UnionBuilder::new(db), UnionBuilder::add)
                .build();
            Some(union)
        }
    }

    /// Returns the type of `.name`/`._name_` for an enum instance that is not
    /// narrowed to a specific member (e.g. `x: MyEnum` where `MyEnum` has multiple members).
    ///
    /// Returns the union of all member name string literals.
    pub(crate) fn instance_name_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.members.is_empty() {
            return None;
        }
        let union = self
            .members
            .keys()
            .map(|name| Type::string_literal(db, name.as_str()))
            .fold(UnionBuilder::new(db), UnionBuilder::add)
            .build();
        Some(union)
    }

    pub(crate) fn resolve_member<'a>(&'a self, name: &'a Name) -> Option<&'a Name> {
        if self.members.contains_key(name) {
            Some(name)
        } else {
            self.aliases.get(name)
        }
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
    pub(crate) enum_class: ClassLiteral<'db>,
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
        db: &'db dyn Db,
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> Option<Self> {
        let mut enum_class = None;
        let mut rest = SmallVec::<[Type<'db>; 1]>::default();
        for positive in positive {
            let Type::NominalInstance(instance) = positive else {
                rest.push(*positive);
                continue;
            };

            let class = instance.class_literal(db);
            if enum_metadata(db, class).is_none() {
                rest.push(*positive);
                continue;
            }

            if enum_class.replace(class).is_some() {
                return None;
            }
        }

        let enum_class = enum_class?;
        let metadata = enum_metadata(db, enum_class)?;

        let mut excluded_names = FxHashSet::default();
        for negative in negative {
            let enum_literal = negative.as_enum_literal()?;
            if enum_literal.enum_class(db) != enum_class {
                return None;
            }

            let name = enum_literal.name(db);
            let canonical_name = metadata.resolve_member(name).unwrap_or(name);
            excluded_names.insert(canonical_name.clone());
        }

        (!excluded_names.is_empty()).then(|| {
            let excluded_names: FxOrderSet<Name> = metadata
                .members
                .keys()
                .filter(|name| excluded_names.contains(*name))
                .cloned()
                .collect();
            let rest: FxOrderSet<Type<'db>> = rest.into_iter().collect();
            Self::new(db, enum_class, excluded_names, rest)
        })
    }

    /// Return metadata for the enum class whose members are represented by this complement.
    pub(crate) fn metadata(self, db: &'db dyn Db) -> &'db EnumMetadata<'db> {
        enum_metadata(db, self.enum_class(db)).expect("Enum complement class is an enum")
    }

    fn remaining_member_names(self, db: &'db dyn Db) -> impl Iterator<Item = &'db Name> {
        self.metadata(db)
            .members
            .keys()
            .filter(move |name| !self.excluded_names(db).contains(*name))
    }

    /// Count the canonical enum members still represented by this complement.
    fn remaining_member_count(self, db: &'db dyn Db) -> usize {
        self.metadata(db).members.len() - self.excluded_names(db).len()
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
    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.is_singleton(db)
            && !self
                .enum_class(db)
                .to_non_generic_instance(db)
                .overrides_equality(db)
    }

    /// Return `true` when this complement can be losslessly split into single-valued literals.
    ///
    /// This permits finite-union narrowing over large complements without materializing the
    /// alternatives for complements that still carry positive rest components.
    pub(crate) fn has_finite_single_valued_alternatives(self, db: &'db dyn Db) -> bool {
        self.rest(db).is_empty()
            && self.remaining_member_count(db) > 0
            && !self
                .enum_class(db)
                .to_non_generic_instance(db)
                .overrides_equality(db)
    }

    /// Expand this complement to the enum literals that remain possible.
    pub fn remaining_literal_types(self, db: &'db dyn Db) -> Vec<Type<'db>> {
        self.remaining_member_names(db)
            .map(|name| self.remaining_literal_type(db, name.clone()))
            .collect()
    }

    /// Expand this complement to the union of enum literals that remain possible.
    pub(crate) fn remaining_literal_union(self, db: &'db dyn Db) -> Type<'db> {
        let alternatives = self.remaining_literal_types(db);
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
    fn remaining_literal_type(self, db: &'db dyn Db, name: Name) -> Type<'db> {
        let literal = Type::enum_literal(EnumLiteralType::new(db, self.enum_class(db), name));
        if self.rest(db).is_empty() {
            return literal;
        }

        let mut builder = IntersectionBuilder::new(db).add_positive(literal);
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
        db: &'db dyn Db,
        max_literals: usize,
    ) -> Option<Vec<Type<'db>>> {
        if !self.rest(db).is_empty() {
            return None;
        }

        let remaining_count = self.remaining_member_count(db);
        if remaining_count == 0 || remaining_count > max_literals {
            return None;
        }

        Some(self.remaining_literal_types(db))
    }

    /// Return the type of a member attribute for all enum literals remaining in this complement.
    ///
    /// This handles `.name`, `.value`, `._name_`, and `._value_` by unioning the corresponding
    /// attribute type from each remaining canonical enum member.
    pub(crate) fn member_type(self, db: &'db dyn Db, member_name: &str) -> Option<Type<'db>> {
        let metadata = self.metadata(db);
        let is_enum_subclass = Type::ClassLiteral(self.enum_class(db))
            .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db));
        let mut builder = UnionBuilder::new(db);
        let mut found_member = false;

        for name in self.remaining_member_names(db) {
            let member_ty = (match member_name {
                "name" if is_enum_subclass => metadata.name_type(db, name),
                "_name_" => metadata.name_type(db, name),
                "value" if is_enum_subclass => metadata.value_type(db, name),
                "_value_" => metadata.value_type(db, name),
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
    pub(crate) fn to_intersection(self, db: &'db dyn Db) -> Type<'db> {
        let enum_class = self.enum_class(db);
        let mut positive = FxOrderSet::from_iter([enum_class.to_non_generic_instance(db)]);
        positive.extend(self.rest(db).iter().copied());

        let mut negative = NegativeIntersectionElements::default();
        for name in self.excluded_names(db) {
            negative.insert(Type::enum_literal(EnumLiteralType::new(
                db,
                enum_class,
                name.clone(),
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
    let ignore_place = place_from_bindings(db, ignore_bindings).place;

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

/// If `value_ty` is a hashable literal and already exists in `enum_values`,
/// record it as an alias and return `true`. Otherwise track it as canonical.
fn try_register_alias<'db>(
    value_ty: Type<'db>,
    name: &Name,
    enum_values: &mut FxHashMap<Type<'db>, Name>,
    aliases: &mut FxHashMap<Name, Name>,
) -> bool {
    if !matches!(
        value_ty.as_literal_value_kind(),
        Some(
            LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
        )
    ) {
        return false;
    }
    if let Some(canonical) = enum_values.get(&value_ty) {
        aliases.insert(name.clone(), canonical.clone());
        return true;
    }
    enum_values.insert(value_ty, name.clone());
    false
}

/// Returns the value to use when checking whether an enum member is an alias.
///
/// For ordinary members, this is the inferred value type. For `auto()` members
/// with a custom `_generate_next_value_`, aliasing is based on the generated
/// value instead of the pre-generator placeholder used while collecting
/// members.
///
/// Returns `None` for `auto()` members when `__new__` or a custom metaclass can
/// rewrite `_value_` before alias registration, because neither the generated
/// value nor the placeholder is reliable alias evidence in that case.
fn alias_detection_value<'db>(
    db: &'db dyn Db,
    value_ty: Type<'db>,
    is_auto: bool,
    generate_next_value_function: Option<FunctionType<'db>>,
    user_defined_new_function: Option<FunctionType<'db>>,
    custom_enum_metaclass_new: bool,
) -> Option<Type<'db>> {
    if !is_auto {
        Some(value_ty)
    } else if user_defined_new_function.is_some() || custom_enum_metaclass_new {
        None
    } else if let Some(func_ty) = generate_next_value_function {
        Some(func_ty.signature(db).overload_return_type_or_unknown(db))
    } else {
        Some(value_ty)
    }
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
            let mut members = FxIndexMap::default();
            let mut aliases = FxHashMap::default();
            let mut enum_values: FxHashMap<Type<'db>, Name> = FxHashMap::default();
            for (name, ty) in spec.members(db) {
                if try_register_alias(*ty, name, &mut enum_values, &mut aliases) {
                    continue;
                }
                members.insert(name.clone(), *ty);
            }
            members.shrink_to_fit();
            aliases.shrink_to_fit();

            return Some(EnumMetadata {
                members,
                aliases,
                auto_members: FxHashSet::default(),
                value_annotation: None,
                init_function: None,
                new_function: None,
                generate_next_value_function: None,
                custom_enum_metaclass_new: false,
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

    if !is_enum_class_by_inheritance(db, class) {
        return None;
    }

    let scope_id = class.body_scope(db);
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    let mut enum_values: FxHashMap<Type<'db>, Name> = FxHashMap::default();
    let mut auto_counter = 0;
    let mut auto_members = FxHashSet::default();
    let mut prev_value_was_non_literal_int = false;
    let mut prev_bool_literal = None;
    let ignored_names = enum_ignored_names(db, scope_id);

    // Look up custom construction hooks, falling back to parent enum classes.
    let init_function = custom_init(db, scope_id).or_else(|| inherited_init(db, class));
    let user_defined_new_function =
        custom_new(db, scope_id).or_else(|| inherited_user_defined_new(db, class));
    let new_function = user_defined_new_function.or_else(|| inherited_new(db, class));
    let custom_enum_metaclass_new = custom_enum_metaclass_new(db, class);
    let generate_next_value_function = custom_generate_next_value(db, scope_id)
        .or_else(|| inherited_generate_next_value(db, class));

    let mut aliases = FxHashMap::default();

    let members = use_def_map
        .all_end_of_scope_symbol_bindings()
        .filter_map(|(symbol_id, bindings)| {
            let name = table.symbol(symbol_id).name();

            if name.starts_with("__") {
                // Skip private attributes (`__private`) and dunders (`__module__`, etc.).
                // CPython's enum metaclass never treats these as members.
                return None;
            }

            if matches!(name.as_str(), "_ignore_" | "_value_" | "_name_")
                || ignored_names.contains(name)
            {
                // Skip ignored attributes
                return None;
            }

            let inferred = place_from_bindings(db, bindings).place;
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
                                    ty.member(db, "value")
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
                                        .is_subtype_of(db, KnownClass::StrEnum.to_subclass_of(db))
                                    {
                                        Type::string_literal(db, &name.to_lowercase())
                                    } else {
                                        let custom_mixins: SmallVec<[Option<KnownClass>; 1]> =
                                            class
                                                .iter_mro(db, None)
                                                .skip(1)
                                                .filter_map(ClassBase::into_class)
                                                .filter(|class| {
                                                    !Type::from(*class).is_subtype_of(
                                                        db,
                                                        KnownClass::Enum.to_subclass_of(db),
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
                                                KnownClass::Int.to_instance(db)
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
                                db,
                                "__get__".into(),
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

            let alias_value_ty = alias_detection_value(
                db,
                value_ty,
                auto_members.contains(name),
                generate_next_value_function,
                user_defined_new_function,
                custom_enum_metaclass_new,
            );
            if let Some(alias_value_ty) = alias_value_ty
                && try_register_alias(alias_value_ty, name, &mut enum_values, &mut aliases)
            {
                return None;
            }

            let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);

            if !explicit_member_wrapper
                && declarations.clone().any_reachable(db, |declaration| {
                    declaration.is_defined_and(|declaration| {
                        !matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(assignment)
                                if assignment
                                    .value(&parsed_module(db, declaration.file(db)).load(db))
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
                && value_ty.is_assignable_to(db, KnownClass::Int.to_instance(db));
            prev_bool_literal =
                value_ty
                    .as_literal_value_kind()
                    .and_then(|literal| match literal {
                        LiteralValueTypeKind::Bool(value) => Some(value),
                        _ => None,
                    });

            Some((name.clone(), value_ty))
        })
        .collect::<FxIndexMap<_, _>>();

    if members.is_empty() {
        // Enum subclasses without members are not considered enums.
        return None;
    }

    let custom_value_annotation = custom_value_annotation(db, scope_id);
    let value_annotation = custom_value_annotation.or_else(|| {
        if custom_enum_metaclass_new {
            inherited_user_defined_value_annotation(db, class)
        } else {
            inherited_value_annotation(db, class)
        }
    });

    Some(EnumMetadata {
        members,
        aliases,
        auto_members,
        value_annotation,
        init_function,
        new_function,
        generate_next_value_function,
        custom_enum_metaclass_new,
    })
}

/// Returns whether an enum's metaclass has a custom `__new__` before the stdlib
/// `EnumType`/`EnumMeta` implementation.
///
/// Such a metaclass can rewrite the class dictionary's member values before the
/// stdlib enum constructor validates and forwards them to `__new__`/`__init__`.
fn custom_enum_metaclass_new<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    let Some(metaclass) = class.metaclass(db).to_class_type(db) else {
        return false;
    };

    metaclass
        .class_literal(db)
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|base| base.class_literal(db).as_static())
        .take_while(|base| base.known(db) != Some(KnownClass::EnumType))
        .any(|base| custom_new(db, base.body_scope(db)).is_some())
}

/// Iterates over parent enum classes in the MRO, skipping known enum
/// infrastructure classes but including `IntEnum`, `Flag`, and `IntFlag`
/// which declare `_value_` annotations that normally should be inherited.
fn iter_parent_enum_classes<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> impl Iterator<Item = StaticClassLiteral<'db>> + 'db {
    class
        .iter_mro(db, None)
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
            (is_traversable && is_enum_class_by_inheritance(db, base)).then_some(base)
        })
}

/// Returns the `_value_` annotation type if one is declared in the given scope.
fn custom_value_annotation<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Option<Type<'db>> {
    let symbol_id = place_table(db, scope).symbol_id("_value_")?;
    let declarations = use_def_map(db, scope).end_of_scope_symbol_declarations(symbol_id);
    place_from_declarations(db, declarations)
        .ignore_conflicting_declarations()
        .ignore_possibly_undefined()
}

/// Looks up an inherited `_value_` annotation from parent enum classes in the MRO.
fn inherited_value_annotation<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    iter_parent_enum_classes(db, class)
        .find_map(|base| custom_value_annotation(db, base.body_scope(db)))
}

/// Looks up an inherited `_value_` annotation from user-defined parent enum classes in the MRO.
fn inherited_user_defined_value_annotation<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<Type<'db>> {
    iter_parent_enum_classes(db, class)
        .filter(|base| base.known(db).is_none())
        .find_map(|base| custom_value_annotation(db, base.body_scope(db)))
}

/// Looks up an inherited `__init__` from parent enum classes in the MRO.
fn inherited_init<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<FunctionType<'db>> {
    iter_parent_enum_classes(db, class).find_map(|base| custom_init(db, base.body_scope(db)))
}

/// Looks up an inherited `__new__` from parent enum classes in the MRO.
fn inherited_new<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<FunctionType<'db>> {
    iter_parent_enum_classes(db, class).find_map(|base| custom_new(db, base.body_scope(db)))
}

/// Looks up an inherited `__new__` from user-defined parent enum classes in the MRO.
fn inherited_user_defined_new<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<FunctionType<'db>> {
    iter_parent_enum_classes(db, class)
        .filter(|base| base.known(db).is_none())
        .find_map(|base| custom_new(db, base.body_scope(db)))
}

/// Looks up an inherited `_generate_next_value_` from parent enum classes in the MRO.
fn inherited_generate_next_value<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Option<FunctionType<'db>> {
    iter_parent_enum_classes(db, class)
        .find_map(|base| custom_generate_next_value(db, base.body_scope(db)))
}

/// Returns the custom `__init__` function type if one is defined on the enum.
fn custom_init<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Option<FunctionType<'db>> {
    let init_symbol_id = place_table(db, scope).symbol_id("__init__")?;
    let init_type = place_from_declarations(
        db,
        use_def_map(db, scope).end_of_scope_symbol_declarations(init_symbol_id),
    )
    .ignore_conflicting_declarations()
    .ignore_possibly_undefined()?;

    match init_type {
        Type::FunctionLiteral(f) => Some(f),
        _ => None,
    }
}

/// Returns the custom `__new__` function type if one is defined on the enum.
fn custom_new<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Option<FunctionType<'db>> {
    let new_symbol_id = place_table(db, scope).symbol_id("__new__")?;
    let new_type = place_from_declarations(
        db,
        use_def_map(db, scope).end_of_scope_symbol_declarations(new_symbol_id),
    )
    .ignore_conflicting_declarations()
    .ignore_possibly_undefined()?;

    match new_type {
        Type::FunctionLiteral(f) => Some(f),
        _ => None,
    }
}

/// Returns the custom `_generate_next_value_` function type if one is defined on the enum.
fn custom_generate_next_value<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> Option<FunctionType<'db>> {
    let symbol_id_opt = place_table(db, scope).symbol_id("_generate_next_value_");
    let new_symbol_id = symbol_id_opt?;
    let new_type = place_from_declarations(
        db,
        use_def_map(db, scope).end_of_scope_symbol_declarations(new_symbol_id),
    )
    .ignore_conflicting_declarations()
    .ignore_possibly_undefined();
    let new_type = new_type?;
    match new_type {
        Type::FunctionLiteral(f) => Some(f),
        _ => None,
    }
}

pub(crate) fn enum_member_literals<'a, 'db: 'a>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    exclude_member: Option<&'a Name>,
) -> Option<impl Iterator<Item = Type<'a>> + 'a> {
    enum_metadata(db, class).map(|metadata| {
        metadata
            .members
            .keys()
            .filter(move |name| Some(*name) != exclude_member)
            .map(move |name| Type::enum_literal(EnumLiteralType::new(db, class, name.clone())))
    })
}

pub(crate) fn is_single_member_enum<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
    enum_metadata(db, class).is_some_and(|metadata| metadata.members.len() == 1)
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
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> bool {
    Type::ClassLiteral(ClassLiteral::Static(class))
        .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db))
        || class
            .metaclass(db)
            .is_subtype_of(db, KnownClass::EnumType.to_subclass_of(db))
}

/// Extracts the inner value type from an `enum.nonmember()` wrapper.
///
/// At runtime, the enum metaclass unwraps `nonmember(value)`, so accessing the attribute
/// returns the inner value, not the `nonmember` wrapper.
///
/// Returns `Some(value_type)` if the type is a `nonmember[T]`, otherwise `None`.
pub(crate) fn try_unwrap_nonmember_value<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Type<'db>> {
    match ty {
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Nonmember) => {
            Some(
                ty.member(db, "value")
                    .place
                    .ignore_possibly_undefined()
                    .unwrap_or(Type::unknown()),
            )
        }
        _ => None,
    }
}
