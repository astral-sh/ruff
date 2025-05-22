use std::{collections::BTreeMap, ops::Deref};

use itertools::{Either, Itertools};

use ruff_python_ast::name::Name;

use crate::{
    semantic_index::{symbol_table, use_def_map},
    symbol::{symbol_from_bindings, symbol_from_declarations},
    types::{
        ClassBase, ClassLiteral, KnownFunction, Type, TypeMapping, TypeQualifiers, TypeVarInstance,
    },
    {Db, FxOrderSet},
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
    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
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

/// # Ordering
/// Ordering is based on the protocol interface member's salsa-assigned id and not on its members.
/// The id may change between runs, or when the protocol instance members was garbage collected and recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub(super) struct ProtocolInterfaceMembers<'db> {
    #[returns(ref)]
    inner: BTreeMap<Name, ProtocolMemberData<'db>>,
}

/// The interface of a protocol: the members of that protocol, and the types of those members.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord)]
pub(super) enum ProtocolInterface<'db> {
    Members(ProtocolInterfaceMembers<'db>),
    SelfReference,
}

impl<'db> ProtocolInterface<'db> {
    pub(super) fn with_members<'a, M>(db: &'db dyn Db, members: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        let members: BTreeMap<_, _> = members
            .into_iter()
            .map(|(name, ty)| {
                (
                    Name::new(name),
                    ProtocolMemberData {
                        ty: ty.normalized(db),
                        qualifiers: TypeQualifiers::default(),
                    },
                )
            })
            .collect();
        Self::Members(ProtocolInterfaceMembers::new(db, members))
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self::Members(ProtocolInterfaceMembers::new(db, BTreeMap::default()))
    }

    pub(super) fn members<'a>(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = ProtocolMember<'a, 'db>>
    where
        'db: 'a,
    {
        match self {
            Self::Members(members) => {
                Either::Left(members.inner(db).iter().map(|(name, data)| ProtocolMember {
                    name,
                    ty: data.ty,
                    qualifiers: data.qualifiers,
                }))
            }
            Self::SelfReference => Either::Right(std::iter::empty()),
        }
    }

    pub(super) fn member_by_name<'a>(
        self,
        db: &'db dyn Db,
        name: &'a str,
    ) -> Option<ProtocolMember<'a, 'db>> {
        match self {
            Self::Members(members) => members.inner(db).get(name).map(|data| ProtocolMember {
                name,
                ty: data.ty,
                qualifiers: data.qualifiers,
            }),
            Self::SelfReference => None,
        }
    }

    /// Return `true` if all members of this protocol are fully static.
    pub(super) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.members(db).all(|member| member.ty.is_fully_static(db))
    }

    /// Return `true` if if all members on `self` are also members of `other`.
    ///
    /// TODO: this method should consider the types of the members as well as their names.
    pub(super) fn is_sub_interface_of(self, db: &'db dyn Db, other: Self) -> bool {
        match (self, other) {
            (Self::Members(self_members), Self::Members(other_members)) => self_members
                .inner(db)
                .keys()
                .all(|member_name| other_members.inner(db).contains_key(member_name)),
            _ => {
                unreachable!("Enclosing protocols should never be a self-reference marker")
            }
        }
    }

    /// Return `true` if the types of any of the members match the closure passed in.
    pub(super) fn any_over_type(
        self,
        db: &'db dyn Db,
        type_fn: &dyn Fn(Type<'db>) -> bool,
    ) -> bool {
        self.members(db)
            .any(|member| member.ty.any_over_type(db, type_fn))
    }

    pub(super) fn normalized(self, db: &'db dyn Db) -> Self {
        match self {
            Self::Members(members) => Self::Members(ProtocolInterfaceMembers::new(
                db,
                members
                    .inner(db)
                    .iter()
                    .map(|(name, data)| (name.clone(), data.normalized(db)))
                    .collect::<BTreeMap<_, _>>(),
            )),
            Self::SelfReference => Self::SelfReference,
        }
    }

    pub(super) fn specialized_and_normalized<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        match self {
            Self::Members(members) => Self::Members(ProtocolInterfaceMembers::new(
                db,
                members
                    .inner(db)
                    .iter()
                    .map(|(name, data)| {
                        (
                            name.clone(),
                            data.apply_type_mapping(db, type_mapping).normalized(db),
                        )
                    })
                    .collect::<BTreeMap<_, _>>(),
            )),
            Self::SelfReference => Self::SelfReference,
        }
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        match self {
            Self::Members(members) => {
                for data in members.inner(db).values() {
                    data.find_legacy_typevars(db, typevars);
                }
            }
            Self::SelfReference => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, salsa::Update)]
pub(super) struct ProtocolMemberData<'db> {
    ty: Type<'db>,
    qualifiers: TypeQualifiers,
}

impl<'db> ProtocolMemberData<'db> {
    fn normalized(&self, db: &'db dyn Db) -> Self {
        Self {
            ty: self.ty.normalized(db),
            qualifiers: self.qualifiers,
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self {
            ty: self.ty.apply_type_mapping(db, type_mapping),
            qualifiers: self.qualifiers,
        }
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        self.ty.find_legacy_typevars(db, typevars);
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
#[salsa::tracked(cycle_fn=proto_interface_cycle_recover, cycle_initial=proto_interface_cycle_initial)]
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
                    let ty = ty.replace_self_reference(db, class);
                    let member = ProtocolMemberData { ty, qualifiers };
                    (name.clone(), member)
                }),
        );
    }

    ProtocolInterface::Members(ProtocolInterfaceMembers::new(db, members))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn proto_interface_cycle_recover<'db>(
    _db: &dyn Db,
    _value: &ProtocolInterface<'db>,
    _count: u32,
    _class: ClassLiteral<'db>,
) -> salsa::CycleRecoveryAction<ProtocolInterface<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn proto_interface_cycle_initial<'db>(
    db: &'db dyn Db,
    _class: ClassLiteral<'db>,
) -> ProtocolInterface<'db> {
    ProtocolInterface::empty(db)
}
