use crate::ProgramEnvironment;
use ruff_db::diagnostic::Span;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;

use crate::Db;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::class::{
    ClassLiteral, ClassType, DynamicClassHeaderAnchor, DynamicClassScopeOffset, MemberLookupPolicy,
    dynamic_class_header_range,
};
use crate::types::class_base::ClassBase;
use crate::types::member::Member;
use crate::types::mro::{DynamicMroError, Mro};
use crate::types::{Type, TypeQualifiers};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

/// A member from a functional enum call, before applying user-defined construction methods.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct FunctionalEnumMember<'db> {
    pub(crate) name: Name,
    /// The inferred value at the call site, including standard generation and any
    /// supported `type=` conversion.
    pub(crate) value: Type<'db>,
    /// For bare names and `auto()`, a custom `_generate_next_value_` can replace `value`.
    pub(crate) is_auto: bool,
}

/// Functional enum member specification captured from the call site.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumSpec<'db> {
    #[returns(deref)]
    pub(crate) members: Box<[FunctionalEnumMember<'db>]>,
    #[returns(copy)]
    pub(crate) has_known_members: bool,
}

impl<'db> EnumSpec<'db> {
    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let members = self
            .members(db)
            .iter()
            .map(|member| {
                let value = member
                    .value
                    .recursive_type_normalized_impl(db, env, div, true);
                Some(FunctionalEnumMember {
                    value: if nested { value? } else { value.unwrap_or(div) },
                    ..member.clone()
                })
            })
            .collect::<Option<Box<_>>>()?;

        Some(Self::new(db, members, self.has_known_members(db)))
    }
}

impl get_size2::GetSize for EnumSpec<'_> {}

/// Anchor for identifying a functional enum class literal.
///
/// This mirrors the dynamic `TypedDict` / `NamedTuple` pattern:
/// - assigned calls use the `Definition` as stable identity;
/// - dangling calls use a relative offset within the enclosing scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum DynamicEnumAnchor<'db> {
    Definition {
        definition: Definition<'db>,
        spec: EnumSpec<'db>,
    },
    ScopeOffset {
        scope: ScopeId<'db>,
        offset: DynamicClassScopeOffset,
        spec: EnumSpec<'db>,
    },
}

impl<'db> DynamicEnumAnchor<'db> {
    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Definition { definition, spec } => Some(Self::Definition {
                definition: *definition,
                spec: spec.recursive_type_normalized_impl(db, env, div, nested)?,
            }),
            Self::ScopeOffset {
                scope,
                offset,
                spec,
            } => Some(Self::ScopeOffset {
                scope: *scope,
                offset: *offset,
                spec: spec.recursive_type_normalized_impl(db, env, div, nested)?,
            }),
        }
    }
}

/// A class created via the functional enum syntax, e.g. `Enum("Color", "RED GREEN BLUE")`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DynamicEnumLiteral<'db> {
    #[returns(ref)]
    pub name: Name,
    #[returns(ref)]
    pub anchor: DynamicEnumAnchor<'db>,
    #[returns(copy)]
    pub base_class: ClassLiteral<'db>,
    #[returns(copy)]
    pub mixin_type: Option<Type<'db>>,
}

impl get_size2::GetSize for DynamicEnumLiteral<'_> {}

impl<'db> DynamicEnumLiteral<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let mixin_type = match self.mixin_type(db) {
            Some(mixin) => {
                let mixin = mixin.recursive_type_normalized_impl(db, env, div, true);
                Some(if nested { mixin? } else { mixin.unwrap_or(div) })
            }
            None => None,
        };

        Some(Self::new(
            db,
            self.name(db),
            self.anchor(db)
                .recursive_type_normalized_impl(db, env, div, nested)?,
            self.base_class(db)
                .recursive_type_normalized_impl(db, env, div, nested)?,
            mixin_type,
        ))
    }
}

#[salsa::tracked]
impl<'db> DynamicEnumLiteral<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicEnumAnchor::Definition { definition, .. } => Some(*definition),
            DynamicEnumAnchor::ScopeOffset { .. } => None,
        }
    }

    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicEnumAnchor::Definition { definition, .. } => definition.scope(db),
            DynamicEnumAnchor::ScopeOffset { scope, .. } => *scope,
        }
    }

    pub(crate) fn spec(self, db: &'db dyn Db) -> EnumSpec<'db> {
        match self.anchor(db) {
            DynamicEnumAnchor::Definition { spec, .. }
            | DynamicEnumAnchor::ScopeOffset { spec, .. } => *spec,
        }
    }

    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let mut bases = Vec::with_capacity(2);
        if let Some(mixin) = self.mixin_type(db) {
            bases.push(mixin);
        }
        bases.push(Type::from(self.base_class(db)));
        bases.into_boxed_slice()
    }

    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let anchor = match self.anchor(db) {
            DynamicEnumAnchor::Definition { definition, .. } => {
                DynamicClassHeaderAnchor::Definition(*definition)
            }
            DynamicEnumAnchor::ScopeOffset { offset, .. } => {
                DynamicClassHeaderAnchor::ScopeOffset(*offset)
            }
        };
        dynamic_class_header_range(db, self.scope(db), anchor)
    }

    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.base_class(db).metaclass(db)
    }

    #[salsa::tracked(
        returns(ref),
        heap_size=ruff_memory_usage::heap_size,
        cycle_initial=|db, _, self_: DynamicEnumLiteral<'db>| {
            Ok(Mro::from([
                ClassBase::Class(ClassType::NonGeneric(ClassLiteral::DynamicEnum(self_))),
                ClassBase::object(db, &ProgramEnvironment::from_scope(self_.scope(db))),
            ]))
        }
    )]
    pub(crate) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, DynamicMroError<'db>> {
        Mro::of_dynamic_enum(db, self)
    }

    fn has_known_members(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_members(db)
    }

    fn with_unknown_member_fallback(
        self,
        db: &'db dyn Db,
        result: PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        if !self.has_known_members(db) && result.place.is_undefined() {
            Place::bound(Type::unknown()).into()
        } else {
            result
        }
    }

    /// Look up an own class member (not inherited) by name.
    ///
    /// For known members, returns the `EnumLiteralType` if `name` matches.
    /// For unknown members, returns `Member::unbound()` — the unknown-member
    /// fallback is handled in `class_member` as a last resort after checking
    /// the full MRO (matching the `NamedTuple` pattern).
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let spec = self.spec(db);
        if spec.has_known_members(db)
            && let Some(enum_class) = ClassLiteral::DynamicEnum(self).into_enum_class(db)
            && let Some(canonical_name) = enum_class.resolve_member(db, &Name::new(name))
        {
            let enum_lit =
                crate::types::literal::EnumLiteralType::new(db, enum_class, canonical_name);
            return Member {
                inner: Place::declared(Type::enum_literal(enum_lit))
                    .with_qualifiers(TypeQualifiers::READ_ONLY),
            };
        }
        Member::unbound()
    }

    /// Look up a class member by name, checking own members, mixin, and base class.
    ///
    /// If members are unknown and nothing was found in the MRO, returns `Unknown`
    /// as a last resort to avoid false `missing-attribute` errors.
    ///
    /// Looking up inherited members through the MRO respects `policy`. For example,
    /// `MRO_NO_OBJECT_FALLBACK` lets a caller check whether any class before `object`
    /// in the MRO supplies `__eq__`.
    ///
    /// The unknown-member fallback at the end does not consult `policy`. It exists to avoid false
    /// `missing-attribute` errors when the member names are not statically known, which is a
    /// property of the enum rather than of the lookup being performed.
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let class = ClassLiteral::DynamicEnum(self);
        let result = class.class_member_from_mro(db, env, name, policy, class.iter_mro(db));

        // When members are unknown (e.g. `Enum("E", some_var)`), any name could
        // still be a member. Only fall back to `Unknown` after exhausting the
        // mixin and enum-base lookups so inherited attributes like `__members__`
        // continue to resolve precisely.
        self.with_unknown_member_fallback(db, result)
    }

    /// Look up an instance member by name, checking mixin and base class.
    ///
    /// If members are unknown and nothing was found, returns `Unknown`
    /// as a last resort.
    pub(crate) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        let result = ClassLiteral::DynamicEnum(self).dynamic_instance_member(db, env, name);
        self.with_unknown_member_fallback(db, result)
    }

    /// Functional enums don't define own instance attributes — `.name`, `.value`
    /// etc. come from the `Enum` base class, not from the dynamic enum itself.
    #[expect(clippy::unused_self)]
    pub(super) fn own_instance_member(self, _db: &'db dyn Db, _name: &str) -> Member<'db> {
        Member::unbound()
    }
}
