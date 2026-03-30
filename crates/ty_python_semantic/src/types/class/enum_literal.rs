use ruff_db::diagnostic::Span;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, NodeIndex};
use ruff_text_size::{Ranged, TextRange};

use crate::Db;
use crate::place::{Place, PlaceAndQualifiers};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::ScopeId;
use crate::types::Type;
use crate::types::class::known::KnownClass;
use crate::types::class::{ClassLiteral, ClassType, MemberLookupPolicy};
use crate::types::class_base::ClassBase;
use crate::types::member::Member;
use crate::types::mro::Mro;
use crate::types::todo_type;

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumSpec<'db> {
    #[returns(deref)]
    pub(crate) members: Box<[(Name, Type<'db>)]>,
    pub(crate) has_known_members: bool,
}

impl get_size2::GetSize for EnumSpec<'_> {}

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
        cycle_initial=dynamic_enum_mro_cycle_initial
    )]
    pub(crate) fn mro(self, db: &'db dyn Db) -> Mro<'db> {
        let self_base = ClassBase::Class(ClassType::NonGeneric(self.into()));
        let mixin_mro: Vec<ClassBase<'db>> = self
            .mixin_type(db)
            .and_then(|t| ClassBase::try_from_type(db, t, None))
            .map(|base| base.mro(db, None).collect())
            .unwrap_or_default();
        let base = self
            .base_class(db)
            .to_class_literal(db)
            .as_class_literal()
            .expect("enum base should be a class literal")
            .default_specialization(db);
        std::iter::once(self_base)
            .chain(mixin_mro)
            .chain(base.iter_mro(db))
            .collect()
    }

    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        let spec = self.spec(db);
        if !spec.has_known_members(db) {
            return Member::definitely_declared(todo_type!("functional `Enum` syntax"));
        }
        if spec.members(db).iter().any(|(n, _)| n.as_str() == name) {
            let class_lit = ClassLiteral::DynamicEnum(self);
            let enum_lit =
                crate::types::literal::EnumLiteralType::new(db, class_lit, Name::new(name));
            return Member::definitely_declared(Type::enum_literal(enum_lit));
        }
        Member::unbound()
    }

    pub(crate) fn class_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        let spec = self.spec(db);
        if !spec.has_known_members(db) {
            return Place::bound(todo_type!("functional `Enum` syntax")).into();
        }
        let own = self.own_class_member(db, name);
        if !own.is_undefined() {
            return own.inner;
        }
        if let Some(mixin) = self.mixin_type(db) {
            if let Some(ClassBase::Class(class)) = ClassBase::try_from_type(db, mixin, None) {
                let result = class.class_member(db, name, MemberLookupPolicy::default());
                if !result.place.is_undefined() {
                    return result;
                }
            }
        }
        self.base_class(db)
            .to_class_literal(db)
            .as_class_literal()
            .map(|cls| cls.class_member(db, name, MemberLookupPolicy::default()))
            .unwrap_or_else(|| Place::Undefined.into())
    }

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        if !self.spec(db).has_known_members(db) {
            return Place::bound(todo_type!("functional `Enum` syntax")).into();
        }
        if let Some(mixin) = self.mixin_type(db) {
            if let Some(ClassBase::Class(class)) = ClassBase::try_from_type(db, mixin, None) {
                let result = class.instance_member(db, name);
                if !result.place.is_undefined() {
                    return result;
                }
            }
        }
        self.base_class(db)
            .to_instance(db)
            .instance_member(db, name)
    }

    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        if !self.spec(db).has_known_members(db) {
            return Member::definitely_declared(todo_type!("functional `Enum` syntax"));
        }
        let _ = name;
        Member::unbound()
    }
}

fn dynamic_enum_mro_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    self_: DynamicEnumLiteral<'db>,
) -> Mro<'db> {
    Mro::from_error(db, ClassType::NonGeneric(ClassLiteral::DynamicEnum(self_)))
}
