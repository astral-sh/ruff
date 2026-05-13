use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{
    Db, FxIndexMap,
    place::{DefinedPlace, Place, place_from_bindings, place_from_declarations},
    reachability::DeclarationsIteratorExtension,
    types::{
        ClassBase, ClassLiteral, DynamicType, EnumLiteralType, KnownClass, LiteralValueTypeKind,
        MemberLookupPolicy, StaticClassLiteral, Type, function::FunctionType,
        set_theoretic::builder::UnionBuilder,
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
}

impl get_size2::GetSize for EnumMetadata<'_> {}

impl<'db> EnumMetadata<'db> {
    fn empty() -> Self {
        EnumMetadata {
            members: FxIndexMap::default(),
            aliases: FxHashMap::default(),
            auto_members: FxHashSet::default(),
            value_annotation: None,
            init_function: None,
            new_function: None,
        }
    }

    /// Returns the type of `.value`/`._value_` for a given enum member.
    ///
    /// Priority: explicit `_value_` annotation, then custom construction hooks → `Any`,
    /// then the inferred member value type.
    pub(crate) fn value_type(&self, member_name: &Name) -> Option<Type<'db>> {
        if !self.members.contains_key(member_name) {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation)
        } else if self.init_function.is_some() || self.new_function.is_some() {
            Some(Type::Dynamic(DynamicType::Any))
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
    /// If there is a custom `__init__` or `__new__`, returns `Any`.
    /// Otherwise, returns the union of all member value types.
    pub(crate) fn instance_value_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.members.is_empty() {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation)
        } else if self.init_function.is_some() || self.new_function.is_some() {
            Some(Type::Dynamic(DynamicType::Any))
        } else {
            let union = self
                .members
                .values()
                .copied()
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
            return Some(EnumMetadata {
                members,
                aliases,
                auto_members: FxHashSet::default(),
                value_annotation: None,
                init_function: None,
                new_function: None,
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

            if try_register_alias(value_ty, name, &mut enum_values, &mut aliases) {
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

            //Ttrack whether this member's value is a non-literal `int`, so a
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

    // Look up an explicit `_value_` annotation, if present. Falls back to
    // checking parent enum classes in the MRO.
    let value_annotation =
        custom_value_annotation(db, scope_id).or_else(|| inherited_value_annotation(db, class));

    // Look up custom construction hooks, falling back to parent enum classes.
    let init_function = custom_init(db, scope_id).or_else(|| inherited_init(db, class));
    let new_function = custom_new(db, scope_id).or_else(|| inherited_new(db, class));

    Some(EnumMetadata {
        members,
        aliases,
        auto_members,
        value_annotation,
        init_function,
        new_function,
    })
}

/// Iterates over parent enum classes in the MRO, skipping known enum
/// infrastructure classes but including `IntEnum`, `Flag`, and `IntFlag`
/// which declare `_value_` annotations that should be inherited.
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
