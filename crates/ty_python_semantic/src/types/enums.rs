use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::{
    Db, FxIndexMap,
    place::{
        DefinedPlace, Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations,
    },
    semantic_index::{place_table, scope::ScopeId, use_def_map},
    types::{
        ClassBase, ClassLiteral, DynamicType, EnumLiteralType, KnownClass, LiteralValueTypeKind,
        MemberLookupPolicy, StaticClassLiteral, Type, TypeQualifiers, function::FunctionType,
    },
};

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
        }
    }

    /// Returns the type of `.value`/`._value_` for a given enum member.
    ///
    /// Priority: explicit `_value_` annotation, then `__init__` → `Any`,
    /// then the inferred member value type.
    pub(crate) fn value_type(&self, member_name: &Name) -> Option<Type<'db>> {
        if !self.members.contains_key(member_name) {
            return None;
        }
        if let Some(annotation) = self.value_annotation {
            Some(annotation)
        } else if self.init_function.is_some() {
            Some(Type::Dynamic(DynamicType::Any))
        } else {
            self.members.get(member_name).copied()
        }
    }

    pub(crate) fn resolve_member<'a>(&'a self, name: &'a Name) -> Option<&'a Name> {
        if self.members.contains_key(name) {
            Some(name)
        } else {
            self.aliases.get(name)
        }
    }
}

/// List all members of an enum.
#[allow(clippy::ref_option, clippy::unnecessary_wraps)]
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
        ClassLiteral::DynamicNamedTuple(..) => return None,
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

    // When an enum has a custom `__new__`, the raw assignment type doesn't represent the
    // member's value — `__new__` unpacks the arguments and explicitly sets `_value_`.
    // Fall back to `Any` for the member's value type.
    let custom_new_value_ty = if has_custom_enum_new(db, class) {
        Some(Type::any())
    } else {
        None
    };

    let mut enum_values: FxHashMap<Type<'db>, Name> = FxHashMap::default();
    let mut auto_counter = 0;
    let mut auto_members = FxHashSet::default();
    let ignored_names: Option<Vec<&str>> = if let Some(ignore) = table.symbol_id("_ignore_") {
        let ignore_bindings = use_def_map.reachable_symbol_bindings(ignore);
        let ignore_place = place_from_bindings(db, ignore_bindings).place;

        match ignore_place {
            Place::Defined(DefinedPlace { ty, .. }) => ty
                .as_string_literal()
                .map(|ignored_names| ignored_names.value(db).split_ascii_whitespace().collect()),

            // TODO: support the list-variant of `_ignore_`.
            Place::Undefined => None,
        }
    } else {
        None
    };

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

            if name == "_ignore_"
                || ignored_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&name.as_str()))
            {
                // Skip ignored attributes
                return None;
            }

            let inferred = place_from_bindings(db, bindings).place;

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
                            Some(KnownClass::Member) => Some(
                                ty.member(db, "value")
                                    .place
                                    .ignore_possibly_undefined()
                                    .unwrap_or(Type::unknown()),
                            ),

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
                                            Type::int_literal(auto_counter)
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

            // Duplicate values are aliases that are not considered separate members. This check is only
            // performed if we can infer a precise literal type for the enum member. If we only get `int`,
            // we don't know if it's a duplicate or not.
            if matches!(
                value_ty.as_literal_value_kind(),
                Some(
                    LiteralValueTypeKind::Bool(_)
                        | LiteralValueTypeKind::Int(_)
                        | LiteralValueTypeKind::String(_)
                        | LiteralValueTypeKind::Bytes(_)
                )
            ) {
                if let Some(canonical) = enum_values.get(&value_ty) {
                    // This is a duplicate value, create an alias to the canonical (first) member
                    aliases.insert(name.clone(), canonical.clone());
                    return None;
                }

                // This is the first occurrence of this value, track it as the canonical member
                enum_values.insert(value_ty, name.clone());
            }

            let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);
            let declared =
                place_from_declarations(db, declarations).ignore_conflicting_declarations();

            match declared {
                PlaceAndQualifiers {
                    place:
                        Place::Defined(DefinedPlace {
                            ty: Type::Dynamic(DynamicType::Unknown),
                            ..
                        }),
                    qualifiers,
                } if qualifiers.contains(TypeQualifiers::FINAL) => {}
                PlaceAndQualifiers {
                    place: Place::Undefined,
                    ..
                } => {
                    // Undeclared attributes are considered members
                }
                PlaceAndQualifiers {
                    place:
                        Place::Defined(DefinedPlace {
                            ty: Type::NominalInstance(instance),
                            ..
                        }),
                    ..
                } if instance.has_known_class(db, KnownClass::Member) => {
                    // If the attribute is specifically declared with `enum.member`, it is considered a member
                }
                _ => {
                    // Declared attributes are considered non-members
                    return None;
                }
            }

            let final_value_ty = custom_new_value_ty.unwrap_or(value_ty);
            Some((name.clone(), final_value_ty))
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

    // Look up a custom `__init__`, falling back to parent enum classes.
    let init_function = custom_init(db, scope_id).or_else(|| inherited_init(db, class));

    Some(EnumMetadata {
        members,
        aliases,
        auto_members,
        value_annotation,
        init_function,
    })
}

/// Iterates over parent enum classes in the MRO, skipping known classes
/// (like `Enum`, `StrEnum`, etc.) that we handle specially.
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
            (base.known(db).is_none() && is_enum_class_by_inheritance(db, base)).then_some(base)
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

/// Returns `true` if the enum class (or a class in its MRO) defines a custom `__new__` method.
///
/// When an enum has a custom `__new__`, the assigned tuple values are unpacked as arguments to
/// `__new__`, and `_value_` is explicitly set inside the method body. This means we can't infer
/// the member's value type from the raw assignment — we fall back to `Any`.
fn has_custom_enum_new<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    // Check the enum class itself
    if has_own_dunder_new(db, class) {
        return true;
    }

    // Walk the MRO (skipping the class itself) looking for a custom `__new__`
    class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .any(|mro_class| {
            let Some(static_class) = mro_class.class_literal(db).as_static() else {
                return false;
            };

            // Skip classes defined in vendored typeshed (e.g. `object.__new__`,
            // `int.__new__`, `IntEnum.__new__`, `IntFlag.__new__`). These `__new__`
            // definitions exist for typing purposes and don't represent custom value
            // transformations. We specifically check for vendored paths rather than all
            // stub files, because a user-provided `.pyi` stub for a library with a
            // custom `__new__` should still be recognized.
            if static_class
                .body_scope(db)
                .file(db)
                .path(db)
                .is_vendored_path()
            {
                return false;
            }

            has_own_dunder_new(db, static_class)
        })
}

/// Returns `true` if the class defines `__new__` directly in its own body scope.
fn has_own_dunder_new<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    let scope = class.body_scope(db);
    let table = place_table(db, scope);
    table.symbol_id("__new__").is_some_and(|symbol_id| {
        let bindings = use_def_map(db, scope).reachable_symbol_bindings(symbol_id);
        matches!(place_from_bindings(db, bindings).place, Place::Defined(_))
    })
}
