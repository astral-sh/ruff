use crate::SemanticEnvironment;
use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};
use ruff_text_size::{Ranged, TextRange};

use crate::Db;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::Type;
use crate::types::class::known::KnownClass;
use crate::types::class::{ClassLiteral, ClassType, MemberLookupPolicy};
use crate::types::class_base::ClassBase;
use crate::types::member::Member;
use crate::types::mro::{DynamicMroError, Mro};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

/// Functional enum member specification captured from the call site.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumSpec<'db> {
    #[returns(deref)]
    pub(crate) members: Box<[(Name, Type<'db>)]>,
    #[returns(copy)]
    pub(crate) has_known_members: bool,
}

impl<'db> EnumSpec<'db> {
    fn recursive_type_normalized_impl(
        self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let db = env.db();
        let members = self
            .members(db)
            .iter()
            .map(|(name, ty)| {
                let ty = ty.recursive_type_normalized_impl(env, div, true);
                let ty = if nested { ty? } else { ty.unwrap_or(div) };
                Some((name.clone(), ty))
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
        offset: u32,
        spec: EnumSpec<'db>,
    },
}

impl<'db> DynamicEnumAnchor<'db> {
    fn recursive_type_normalized_impl(
        &self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Definition { definition, spec } => Some(Self::Definition {
                definition: *definition,
                spec: spec.recursive_type_normalized_impl(env, div, nested)?,
            }),
            Self::ScopeOffset {
                scope,
                offset,
                spec,
            } => Some(Self::ScopeOffset {
                scope: *scope,
                offset: *offset,
                spec: spec.recursive_type_normalized_impl(env, div, nested)?,
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
    pub base_class: KnownClass,
    #[returns(copy)]
    pub mixin_type: Option<Type<'db>>,
}

impl get_size2::GetSize for DynamicEnumLiteral<'_> {}

impl<'db> DynamicEnumLiteral<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let db = env.db();
        let mixin_type = match self.mixin_type(db) {
            Some(mixin) => {
                let mixin = mixin.recursive_type_normalized_impl(env, div, true);
                Some(if nested { mixin? } else { mixin.unwrap_or(div) })
            }
            None => None,
        };

        Some(Self::new(
            db,
            self.name(db),
            self.anchor(db)
                .recursive_type_normalized_impl(env, div, nested)?,
            self.base_class(db),
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

    pub(crate) fn explicit_bases(self, env: &SemanticEnvironment<'db>) -> Box<[Type<'db>]> {
        let db = env.db();
        let mut bases = Vec::with_capacity(2);
        if let Some(mixin) = self.mixin_type(db) {
            bases.push(mixin);
        }
        bases.push(self.base_class(db).to_class_literal(env));
        bases.into_boxed_slice()
    }

    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let module = parsed_module(db, scope.python_file(db)).load(db);
        match self.anchor(db) {
            DynamicEnumAnchor::Definition { definition, .. } => definition
                .kind(db)
                .value(&module)
                .expect("DynamicEnumAnchor::Definition should only be used for assignments")
                .range(),
            DynamicEnumAnchor::ScopeOffset { offset, .. } => {
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("anchor should not be NodeIndex::NONE");
                let absolute_index = NodeIndex::from(anchor_u32 + offset);
                let node: &ast::ExprCall = module
                    .get_by_index(absolute_index)
                    .try_into()
                    .expect("scope offset should point to ExprCall");
                node.range()
            }
        }
    }

    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    #[expect(clippy::unused_self)]
    pub(crate) fn metaclass(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        KnownClass::EnumType.to_class_literal(env)
    }

    pub(crate) fn try_mro(
        self,
        env: &SemanticEnvironment<'db>,
    ) -> &'db Result<Mro<'db>, DynamicMroError<'db>> {
        let db = env.db();
        debug_assert_eq!(env.program(), self.scope(db).program(db));
        self.try_mro_inner(db)
    }

    #[salsa::tracked(
        returns(ref),
        heap_size=ruff_memory_usage::heap_size,
        cycle_initial=|db, _, self_: DynamicEnumLiteral<'db>| {
            Ok(Mro::from([
                ClassBase::Class(ClassType::NonGeneric(ClassLiteral::DynamicEnum(self_))),
                ClassBase::object(&SemanticEnvironment::from_file(db, self_.scope(db).python_file(db))),
            ]))
        }
    )]
    fn try_mro_inner(self, db: &'db dyn Db) -> Result<Mro<'db>, DynamicMroError<'db>> {
        let env = SemanticEnvironment::from_file(db, self.scope(db).python_file(db));
        Mro::of_dynamic_enum(&env, self)
    }

    fn has_known_members(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_members(db)
    }

    fn mixin_class(self, env: &SemanticEnvironment<'db>) -> Option<ClassType<'db>> {
        let db = env.db();
        let mixin = self.mixin_type(db)?;
        let ClassBase::Class(class) = ClassBase::try_from_type(env, mixin, None)? else {
            return None;
        };
        Some(class)
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
    pub(super) fn own_class_member(
        self,
        env: &SemanticEnvironment<'db>,
        name: &str,
    ) -> Member<'db> {
        let db = env.db();
        let spec = self.spec(db);
        if spec.has_known_members(db)
            && let Some(enum_class) = ClassLiteral::DynamicEnum(self).into_enum_class(env)
            && let Some(canonical_name) = enum_class.resolve_member(db, &Name::new(name))
        {
            let enum_lit =
                crate::types::literal::EnumLiteralType::new(db, enum_class, canonical_name);
            return Member::definitely_declared(Type::enum_literal(enum_lit));
        }
        Member::unbound()
    }

    /// Look up a class member by name, checking own members, mixin, and base class.
    ///
    /// If members are unknown and nothing was found in the MRO, returns `Unknown`
    /// as a last resort to avoid false `unresolved-attribute` errors.
    pub(crate) fn class_member(
        self,
        env: &SemanticEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        let db = env.db();
        let own = self.own_class_member(env, name);
        if !own.is_undefined() {
            return own.inner;
        }
        if let Some(mixin_class) = self.mixin_class(env) {
            let result = mixin_class.class_member(env, name, MemberLookupPolicy::default());
            if !result.place.is_undefined() {
                return result;
            }
        }
        let result = self
            .base_class(db)
            .to_class_literal(env)
            .as_class_literal()
            .map(|cls| cls.class_member(env, name, MemberLookupPolicy::default()))
            .unwrap_or_else(|| Place::Undefined.into());

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
        env: &SemanticEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        let db = env.db();
        if let Some(mixin_class) = self.mixin_class(env) {
            let result = mixin_class.instance_member(env, name);
            if !result.place.is_undefined() {
                return result;
            }
        }
        let result = self
            .base_class(db)
            .to_instance(env)
            .instance_member(env, name);

        self.with_unknown_member_fallback(db, result)
    }

    /// Functional enums don't define own instance attributes — `.name`, `.value`
    /// etc. come from the `Enum` base class, not from the dynamic enum itself.
    #[expect(clippy::unused_self)]
    pub(super) fn own_instance_member(
        self,
        _ctx: &SemanticEnvironment<'db>,
        _name: &str,
    ) -> Member<'db> {
        Member::unbound()
    }
}
