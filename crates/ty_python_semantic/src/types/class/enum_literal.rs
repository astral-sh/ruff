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
    pub(crate) has_known_members: bool,
}

impl get_size2::GetSize for EnumSpec<'_> {}

/// Anchor for identifying a functional enum class literal.
///
/// This mirrors the dynamic `TypedDict` / `NamedTuple` pattern:
/// - assigned calls use the `Definition` as stable identity;
/// - dangling calls use a relative offset within the enclosing scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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

/// A class created via the functional enum syntax, e.g. `Enum("Color", "RED GREEN BLUE")`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DynamicEnumLiteral<'db> {
    #[returns(ref)]
    pub name: Name,
    #[returns(ref)]
    pub anchor: DynamicEnumAnchor<'db>,
    pub base_class: KnownClass,
    pub mixin_type: Option<Type<'db>>,
}

impl get_size2::GetSize for DynamicEnumLiteral<'_> {}

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
        bases.push(self.base_class(db).to_class_literal(db));
        bases.into_boxed_slice()
    }

    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let file = scope.file(db);
        let module = parsed_module(db, file).load(db);
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
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        KnownClass::EnumType.to_class_literal(db)
    }

    #[salsa::tracked(
        returns(ref),
        heap_size=ruff_memory_usage::heap_size,
        cycle_initial=|db, _, self_: DynamicEnumLiteral<'db>| {
            Ok(Mro::from([
                ClassBase::Class(ClassType::NonGeneric(ClassLiteral::DynamicEnum(self_))),
                ClassBase::object(db),
            ]))
        }
    )]
    pub(crate) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, DynamicMroError<'db>> {
        Mro::of_dynamic_enum(db, self)
    }

    fn has_known_members(self, db: &'db dyn Db) -> bool {
        self.spec(db).has_known_members(db)
    }

    fn mixin_class(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        let mixin = self.mixin_type(db)?;
        let ClassBase::Class(class) = ClassBase::try_from_type(db, mixin, None)? else {
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
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let spec = self.spec(db);
        if spec.has_known_members(db)
            && spec
                .members(db)
                .iter()
                .any(|(member_name, _)| member_name == name)
        {
            let class_lit = ClassLiteral::DynamicEnum(self);
            let enum_lit =
                crate::types::literal::EnumLiteralType::new(db, class_lit, Name::new(name));
            return Member::definitely_declared(Type::enum_literal(enum_lit));
        }
        Member::unbound()
    }

    /// Look up a class member by name, checking own members, mixin, and base class.
    ///
    /// If members are unknown and nothing was found in the MRO, returns `Unknown`
    /// as a last resort to avoid false `unresolved-attribute` errors.
    pub(crate) fn class_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let own = self.own_class_member(db, name);
        if !own.is_undefined() {
            return own.inner;
        }
        if let Some(mixin_class) = self.mixin_class(db) {
            let result = mixin_class.class_member(db, name, MemberLookupPolicy::default());
            if !result.place.is_undefined() {
                return result;
            }
        }
        let result = self
            .base_class(db)
            .to_class_literal(db)
            .as_class_literal()
            .map(|cls| cls.class_member(db, name, MemberLookupPolicy::default()))
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
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        if let Some(mixin_class) = self.mixin_class(db) {
            let result = mixin_class.instance_member(db, name);
            if !result.place.is_undefined() {
                return result;
            }
        }
        let result = self
            .base_class(db)
            .to_instance(db)
            .instance_member(db, name);

        self.with_unknown_member_fallback(db, result)
    }

    /// Functional enums don't define own instance attributes — `.name`, `.value`
    /// etc. come from the `Enum` base class, not from the dynamic enum itself.
    #[expect(clippy::unused_self)]
    pub(super) fn own_instance_member(self, _db: &'db dyn Db, _name: &str) -> Member<'db> {
        Member::unbound()
    }
}
