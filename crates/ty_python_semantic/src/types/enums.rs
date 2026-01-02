use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use crate::{
    Db, FxIndexMap,
    place::{Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations},
    semantic_index::{place_table, use_def_map},
    types::{
        ClassBase, ClassLiteral, DynamicType, EnumLiteralType, KnownClass, MemberLookupPolicy,
        Type, TypeQualifiers,
    },
};

#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(crate) struct EnumMetadata<'db> {
    pub(crate) members: FxIndexMap<Name, Type<'db>>,
    pub(crate) aliases: FxHashMap<Name, Name>,
}

impl get_size2::GetSize for EnumMetadata<'_> {}

impl EnumMetadata<'_> {
    fn empty() -> Self {
        EnumMetadata {
            members: FxIndexMap::default(),
            aliases: FxHashMap::default(),
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

#[allow(clippy::unnecessary_wraps)]
fn enum_metadata_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _class: ClassLiteral<'db>,
) -> Option<EnumMetadata<'db>> {
    Some(EnumMetadata::empty())
}

/// List all members of an enum.
#[allow(clippy::ref_option, clippy::unnecessary_wraps)]
#[salsa::tracked(returns(as_ref), cycle_initial=enum_metadata_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn enum_metadata<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> Option<EnumMetadata<'db>> {
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
            Place::Defined(Type::StringLiteral(ignored_names), _, _, _) => {
                Some(ignored_names.value(db).split_ascii_whitespace().collect())
            }
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
                Place::Defined(ty, _, _, _) => {
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
                                let auto_value_ty = if Type::ClassLiteral(class)
                                    .is_subtype_of(db, KnownClass::StrEnum.to_subclass_of(db))
                                {
                                    Type::string_literal(db, &name.to_lowercase())
                                } else {
                                    let custom_mixins: smallvec::SmallVec<[Option<KnownClass>; 1]> =
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
                            Place::Undefined | Place::Defined(Type::Dynamic(_), _, _, _) => ty,

                            Place::Defined(_, _, _, _) => {
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
                    place: Place::Defined(Type::Dynamic(DynamicType::Unknown), _, _, _),
                    qualifiers,
                } if qualifiers.contains(TypeQualifiers::FINAL) => {}
                PlaceAndQualifiers {
                    place: Place::Undefined,
                    ..
                } => {
                    // Undeclared attributes are considered members
                }
                PlaceAndQualifiers {
                    place: Place::Defined(Type::NominalInstance(instance), _, _, _),
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

    Some(EnumMetadata { members, aliases })
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
pub(crate) fn is_enum_class_by_inheritance<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
    Type::ClassLiteral(class).is_subtype_of(db, KnownClass::Enum.to_subclass_of(db))
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
