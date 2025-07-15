use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use crate::{
    Db,
    place::{Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations},
    semantic_index::{place_table, use_def_map},
    types::{ClassLiteral, DynamicType, KnownClass, MemberLookupPolicy, Type, TypeQualifiers},
};

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct EnumMetadata {
    pub(crate) members: Box<[Name]>,
    pub(crate) aliases: FxHashMap<Name, Name>,
}

impl EnumMetadata {
    fn empty() -> Self {
        EnumMetadata {
            members: Box::new([]),
            aliases: FxHashMap::default(),
        }
    }

    pub(crate) fn resolve_member<'a>(&'a self, name: &'a Name) -> Option<&'a Name> {
        if self.members.contains(name) {
            Some(name)
        } else {
            self.aliases.get(name)
        }
    }
}

#[allow(clippy::ref_option)]
fn enum_metadata_cycle_recover(
    _db: &dyn Db,
    _value: &Option<EnumMetadata>,
    _count: u32,
    _class: ClassLiteral<'_>,
) -> salsa::CycleRecoveryAction<Option<EnumMetadata>> {
    salsa::CycleRecoveryAction::Iterate
}

#[allow(clippy::unnecessary_wraps)]
fn enum_metadata_cycle_initial(_db: &dyn Db, _class: ClassLiteral<'_>) -> Option<EnumMetadata> {
    Some(EnumMetadata::empty())
}

/// List all members of an enum.
#[allow(clippy::ref_option, clippy::unnecessary_wraps)]
#[salsa::tracked(returns(ref), cycle_fn=enum_metadata_cycle_recover, cycle_initial=enum_metadata_cycle_initial, heap_size=get_size2::GetSize::get_heap_size)]
pub(crate) fn enum_metadata<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> Option<EnumMetadata> {
    // This is a fast path to avoid traversing the MRO of known classes
    if class
        .known(db)
        .is_some_and(|known_class| !known_class.is_enum_subclass_with_members())
    {
        return None;
    }

    // TODO: This check needs to be extended (`EnumMeta`/`EnumType`)
    if !Type::ClassLiteral(class).is_subtype_of(db, KnownClass::Enum.to_subclass_of(db)) {
        return None;
    }

    let scope_id = class.body_scope(db);
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    let mut enum_values: FxHashMap<Type<'db>, Name> = FxHashMap::default();
    // TODO: handle `StrEnum` which uses lowercase names as values when using `auto()`.
    let mut auto_counter = 0;

    let ignored_names: Option<Vec<&str>> = if let Some(ignore) = table.place_id_by_name("_ignore_")
    {
        let ignore_bindings = use_def_map.all_reachable_bindings(ignore);
        let ignore_place = place_from_bindings(db, ignore_bindings);

        match ignore_place {
            Place::Type(Type::StringLiteral(ignored_names), _) => {
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
        .all_end_of_scope_bindings()
        .filter_map(|(place_id, bindings)| {
            let name = table.place_expr(place_id).as_name()?;

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

            let inferred = place_from_bindings(db, bindings);
            let value_ty = match inferred {
                Place::Unbound => {
                    return None;
                }
                Place::Type(ty, _) => {
                    match ty {
                        Type::Callable(_) | Type::FunctionLiteral(_) => {
                            // Some types are specifically disallowed for enum members.
                            return None;
                        }
                        // enum.nonmember
                        Type::NominalInstance(instance)
                            if instance.class.is_known(db, KnownClass::Nonmember) =>
                        {
                            return None;
                        }
                        // enum.member
                        Type::NominalInstance(instance)
                            if instance.class.is_known(db, KnownClass::Member) =>
                        {
                            ty.member(db, "value")
                                .place
                                .ignore_possibly_unbound()
                                .unwrap_or(Type::unknown())
                        }
                        // enum.auto
                        Type::NominalInstance(instance)
                            if instance.class.is_known(db, KnownClass::Auto) =>
                        {
                            auto_counter += 1;
                            Type::IntLiteral(auto_counter)
                        }
                        _ => {
                            let dunder_get = ty
                                .member_lookup_with_policy(
                                    db,
                                    "__get__".into(),
                                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                                )
                                .place;

                            match dunder_get {
                                Place::Unbound | Place::Type(Type::Dynamic(_), _) => ty,

                                Place::Type(_, _) => {
                                    // Descriptors are not considered members.
                                    return None;
                                }
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
                Type::IntLiteral(_) | Type::StringLiteral(_) | Type::BytesLiteral(_)
            ) {
                if let Some(previous) = enum_values.insert(value_ty, name.clone()) {
                    aliases.insert(name.clone(), previous);
                    return None;
                }
            }

            let declarations = use_def_map.end_of_scope_declarations(place_id);
            let declared = place_from_declarations(db, declarations);

            match declared {
                Ok(PlaceAndQualifiers {
                    place: Place::Type(Type::Dynamic(DynamicType::Unknown), _),
                    qualifiers,
                }) if qualifiers.contains(TypeQualifiers::FINAL) => {}
                Ok(PlaceAndQualifiers {
                    place: Place::Unbound,
                    ..
                }) => {
                    // Undeclared attributes are considered members
                }
                Ok(PlaceAndQualifiers {
                    place: Place::Type(Type::NominalInstance(instance), _),
                    ..
                }) if instance.class.is_known(db, KnownClass::Member) => {
                    // If the attribute is specifically declared with `enum.member`, it is considered a member
                }
                _ => {
                    // Declared attributes are considered non-members
                    return None;
                }
            }

            Some(name)
        })
        .cloned()
        .collect::<Box<_>>();

    if members.is_empty() {
        // Enum subclasses without members are not considered enums.
        return None;
    }

    Some(EnumMetadata { members, aliases })
}
