use std::{collections::BTreeMap, ops::Deref};

use itertools::Itertools;

use ruff_python_ast::name::Name;

use crate::{
    db::Db,
    semantic_index::{symbol_table, use_def_map},
    symbol::{symbol_from_bindings, symbol_from_declarations},
    types::{ClassBase, ClassLiteral, KnownFunction, Type, TypeQualifiers},
};

impl<'db> ClassLiteral<'db> {
    /// Returns `Some` if this is a protocol class, `None` otherwise.
    pub(super) fn into_protocol_class(self, db: &'db dyn Db) -> Option<ProtocolClassLiteral<'db>> {
        self.is_protocol(db).then_some(ProtocolClassLiteral(self))
    }
}

/// Representation of a single `Protocol` class definition.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct ProtocolClassLiteral<'db>(ClassLiteral<'db>);

impl<'db> ProtocolClassLiteral<'db> {
    /// Returns the protocol members of this class.
    ///
    /// A protocol's members define the interface declared by the protocol.
    /// They therefore determine how the protocol should behave with regards to
    /// assignability and subtyping.
    ///
    /// The list of members consists of all bindings and declarations that take place
    /// in the protocol's class body, except for a list of excluded attributes which should
    /// not be taken into account. (This list includes `__init__` and `__new__`, which can
    /// legally be defined on protocol classes but do not constitute protocol members.)
    ///
    /// It is illegal for a protocol class to have any instance attributes that are not declared
    /// in the protocol's class body. If any are assigned to, they are not taken into account in
    /// the protocol's list of members.
    pub(super) fn interface(self, db: &'db dyn Db) -> &'db ProtocolInterface<'db> {
        let _span = tracing::trace_span!("protocol_members", "class='{}'", self.name(db)).entered();
        cached_protocol_interface(db, *self)
    }

    pub(super) fn is_runtime_checkable(self, db: &'db dyn Db) -> bool {
        self.known_function_decorators(db)
            .contains(&KnownFunction::RuntimeCheckable)
    }
}

impl<'db> Deref for ProtocolClassLiteral<'db> {
    type Target = ClassLiteral<'db>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The interface of a protocol: the members of that protocol, and the types of those members.
#[derive(Debug, PartialEq, Eq, salsa::Update, Default, Clone, Hash)]
pub(super) struct ProtocolInterface<'db>(BTreeMap<Name, ProtocolMemberData<'db>>);

impl<'db> ProtocolInterface<'db> {
    /// Iterate over the members of this protocol.
    pub(super) fn members<'a>(&'a self) -> impl ExactSizeIterator<Item = ProtocolMember<'a, 'db>> {
        self.0.iter().map(|(name, data)| ProtocolMember {
            name,
            ty: data.ty,
            qualifiers: data.qualifiers,
        })
    }

    pub(super) fn member_by_name<'a>(&self, name: &'a str) -> Option<ProtocolMember<'a, 'db>> {
        self.0.get(name).map(|data| ProtocolMember {
            name,
            ty: data.ty,
            qualifiers: data.qualifiers,
        })
    }

    pub(super) fn includes_member(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Return `true` if all members of this protocol are fully static.
    pub(super) fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        self.members().all(|member| member.ty.is_fully_static(db))
    }

    /// Return `true` if if all members on `self` are also members of `other`.
    ///
    /// TODO: this method should consider the types of the members as well as their names.
    pub(super) fn is_sub_interface_of(&self, other: &Self) -> bool {
        self.0
            .keys()
            .all(|member_name| other.0.contains_key(member_name))
    }

    /// Return `true` if any of the members of this protocol type contain any `Todo` types.
    pub(super) fn contains_todo(&self, db: &'db dyn Db) -> bool {
        self.members().any(|member| member.ty.contains_todo(db))
    }

    pub(super) fn normalized(self, db: &'db dyn Db) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|(name, data)| (name, data.normalized(db)))
                .collect(),
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, salsa::Update)]
struct ProtocolMemberData<'db> {
    ty: Type<'db>,
    qualifiers: TypeQualifiers,
}

impl<'db> ProtocolMemberData<'db> {
    fn normalized(self, db: &'db dyn Db) -> Self {
        Self {
            ty: self.ty.normalized(db),
            qualifiers: self.qualifiers,
        }
    }
}

/// A single member of a protocol interface.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProtocolMember<'a, 'db> {
    name: &'a str,
    ty: Type<'db>,
    qualifiers: TypeQualifiers,
}

impl<'a, 'db> ProtocolMember<'a, 'db> {
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    pub(super) fn ty(&self) -> Type<'db> {
        self.ty
    }

    pub(super) fn qualifiers(&self) -> TypeQualifiers {
        self.qualifiers
    }
}

/// Returns `true` if a declaration or binding to a given name in a protocol class body
/// should be excluded from the list of protocol members of that class.
///
/// The list of excluded members is subject to change between Python versions,
/// especially for dunders, but it probably doesn't matter *too* much if this
/// list goes out of date. It's up to date as of Python commit 87b1ea016b1454b1e83b9113fa9435849b7743aa
/// (<https://github.com/python/cpython/blob/87b1ea016b1454b1e83b9113fa9435849b7743aa/Lib/typing.py#L1776-L1791>)
fn excluded_from_proto_members(member: &str) -> bool {
    matches!(
        member,
        "_is_protocol"
            | "__non_callable_proto_members__"
            | "__static_attributes__"
            | "__orig_class__"
            | "__match_args__"
            | "__weakref__"
            | "__doc__"
            | "__parameters__"
            | "__module__"
            | "_MutableMapping__marker"
            | "__slots__"
            | "__dict__"
            | "__new__"
            | "__protocol_attrs__"
            | "__init__"
            | "__class_getitem__"
            | "__firstlineno__"
            | "__abstractmethods__"
            | "__orig_bases__"
            | "_is_runtime_protocol"
            | "__subclasshook__"
            | "__type_params__"
            | "__annotations__"
            | "__annotate__"
            | "__annotate_func__"
            | "__annotations_cache__"
    )
}

/// Inner Salsa query for [`ProtocolClassLiteral::interface`].
#[salsa::tracked(return_ref, cycle_fn=proto_interface_cycle_recover, cycle_initial=proto_interface_cycle_initial)]
fn cached_protocol_interface<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> ProtocolInterface<'db> {
    let mut members = BTreeMap::default();

    for parent_protocol in class
        .iter_mro(db, None)
        .filter_map(ClassBase::into_class)
        .filter_map(|class| class.class_literal(db).0.into_protocol_class(db))
    {
        let parent_scope = parent_protocol.body_scope(db);
        let use_def_map = use_def_map(db, parent_scope);
        let symbol_table = symbol_table(db, parent_scope);

        members.extend(
            use_def_map
                .all_public_declarations()
                .flat_map(|(symbol_id, declarations)| {
                    symbol_from_declarations(db, declarations).map(|symbol| (symbol_id, symbol))
                })
                .filter_map(|(symbol_id, symbol)| {
                    symbol
                        .symbol
                        .ignore_possibly_unbound()
                        .map(|ty| (symbol_id, ty, symbol.qualifiers))
                })
                // Bindings in the class body that are not declared in the class body
                // are not valid protocol members, and we plan to emit diagnostics for them
                // elsewhere. Invalid or not, however, it's important that we still consider
                // them to be protocol members. The implementation of `issubclass()` and
                // `isinstance()` for runtime-checkable protocols considers them to be protocol
                // members at runtime, and it's important that we accurately understand
                // type narrowing that uses `isinstance()` or `issubclass()` with
                // runtime-checkable protocols.
                .chain(
                    use_def_map
                        .all_public_bindings()
                        .filter_map(|(symbol_id, bindings)| {
                            symbol_from_bindings(db, bindings)
                                .ignore_possibly_unbound()
                                .map(|ty| (symbol_id, ty, TypeQualifiers::default()))
                        }),
                )
                .map(|(symbol_id, member, qualifiers)| {
                    (symbol_table.symbol(symbol_id).name(), member, qualifiers)
                })
                .filter(|(name, _, _)| !excluded_from_proto_members(name))
                .map(|(name, ty, qualifiers)| {
                    let member = ProtocolMemberData { ty, qualifiers };
                    (name.clone(), member)
                }),
        );
    }

    ProtocolInterface(members)
}

fn proto_interface_cycle_recover<'db>(
    _db: &dyn Db,
    _value: &ProtocolInterface<'db>,
    _count: u32,
    _class: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<ProtocolInterface<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn proto_interface_cycle_initial<'db>(
    _db: &dyn Db,
    _class: ClassLiteral<'db>,
) -> ProtocolInterface<'db> {
    ProtocolInterface::default()
}
