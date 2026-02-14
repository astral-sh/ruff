use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    Db, FxIndexMap,
    place::{
        DefinedPlace, Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations,
    },
    semantic_index::{place_table, scope::ScopeId, use_def_map},
    types::{
        ClassBase, ClassLiteral, DynamicType, EnumLiteralType, KnownClass, MemberLookupPolicy,
        StaticClassLiteral, Type, TypeQualifiers,
    },
};

#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(crate) struct EnumMetadata<'db> {
    pub(crate) members: FxIndexMap<Name, Type<'db>>,
    pub(crate) aliases: FxHashMap<Name, Name>,

    /// The type used for *validating* member value assignments.
    ///
    /// Priority: `__init__` → `Any`, else `_value_` annotation, else `Unknown`.
    pub(crate) value_sunder_type: Type<'db>,

    /// The explicit `_value_` annotation type, if declared.
    ///
    /// This is kept separate from `value_sunder_type` because `.value` access
    /// always prefers the `_value_` annotation, even when `__init__` exists.
    value_annotation: Option<Type<'db>>,
}

impl get_size2::GetSize for EnumMetadata<'_> {}

impl<'db> EnumMetadata<'db> {
    fn empty() -> Self {
        EnumMetadata {
            members: FxIndexMap::default(),
            aliases: FxHashMap::default(),
            value_sunder_type: Type::Dynamic(DynamicType::Unknown),
            value_annotation: None,
        }
    }

    /// Returns the type of `.value`/`._value_` for a given enum member.
    ///
    /// Priority: explicit `_value_` annotation, then `__init__` → `Any`,
    /// then the inferred member value type.
    pub(crate) fn value_type(&self, member_name: &Name) -> Option<Type<'db>> {
        if let Some(annotation) = self.value_annotation {
            // Check the member exists, but use the declared annotation type.
            self.members.contains_key(member_name).then_some(annotation)
        } else {
            match self.value_sunder_type {
                Type::Dynamic(DynamicType::Unknown) => self.members.get(member_name).copied(),
                declared => self.members.contains_key(member_name).then_some(declared),
            }
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

    let mut enum_values: FxHashMap<Type<'db>, Name> = FxHashMap::default();
    let mut auto_counter = 0;

    let ignored_names: Option<Vec<&str>> = if let Some(ignore) = table.symbol_id("_ignore_") {
        let ignore_bindings = use_def_map.reachable_symbol_bindings(ignore);
        let ignore_place = place_from_bindings(db, ignore_bindings).place;

        match ignore_place {
            Place::Defined(DefinedPlace {
                ty: Type::StringLiteral(ignored_names),
                ..
            }) => Some(ignored_names.value(db).split_ascii_whitespace().collect()),
            // TODO: support the list-variant of `_ignore_`.
            _ => None,
        }
    } else {
        None
    };

    let mut aliases = FxHashMap::default();

    let members = use_def_map
        .all_end_of_scope_symbol_bindings()
        .filter_map(|(symbol_id, bindings)| {
            let name = table.symbol(symbol_id).name();

            if name.starts_with("__") && !name.ends_with("__") {
                // Skip private attributes
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
                                            Type::IntLiteral(auto_counter)
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
                value_ty,
                Type::BooleanLiteral(_)
                    | Type::IntLiteral(_)
                    | Type::StringLiteral(_)
                    | Type::BytesLiteral(_)
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

            Some((name.clone(), value_ty))
        })
        .collect::<FxIndexMap<_, _>>();

    if members.is_empty() {
        // Enum subclasses without members are not considered enums.
        return None;
    }

    // Look up an explicit `_value_` annotation, if present.
    let value_annotation = place_table(db, scope_id)
        .symbol_id("_value_")
        .and_then(|symbol_id| {
            let declarations = use_def_map.end_of_scope_symbol_declarations(symbol_id);
            place_from_declarations(db, declarations)
                .ignore_conflicting_declarations()
                .ignore_possibly_undefined()
        });

    // Determine the expected type for member value validation:
    // (a) If `__init__` is defined, fall back to `Any` (member values are passed
    //     through `__init__`, not directly assigned to `_value_`).
    // (b) Otherwise, use an explicit `_value_` annotation if present.
    // (c) Otherwise, fall back to `Unknown` (no member value validation).
    let value_sunder_type = has_custom_init(db, scope_id)
        .or(value_annotation)
        .unwrap_or(Type::Dynamic(DynamicType::Unknown));

    Some(EnumMetadata {
        members,
        aliases,
        value_sunder_type,
        value_annotation,
    })
}

/// If the enum defines a custom `__init__`, member values are passed through it
/// rather than being assigned directly to `_value_`, so we fall back to `Any`.
fn has_custom_init<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Option<Type<'db>> {
    let init_symbol_id = place_table(db, scope).symbol_id("__init__")?;
    let init_type = place_from_declarations(
        db,
        use_def_map(db, scope).end_of_scope_symbol_declarations(init_symbol_id),
    )
    .ignore_conflicting_declarations()
    .ignore_possibly_undefined()?;

    matches!(init_type, Type::FunctionLiteral(_)).then_some(Type::Dynamic(DynamicType::Any))
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
            .map(move |name| Type::EnumLiteral(EnumLiteralType::new(db, class, name.clone())))
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
