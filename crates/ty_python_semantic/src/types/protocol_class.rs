use std::{collections::BTreeMap, fmt::Write, ops::Deref};

use itertools::Itertools;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use crate::{
    Db, FxOrderSet,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, place_from_bindings,
        place_from_declarations,
    },
    semantic_index::{definition::Definition, place::ScopedPlaceId, place_table, use_def_map},
    types::{
        ApplyTypeMappingVisitor, AttributeAssignmentError, BoundTypeVarInstance, CallArguments,
        CallableType, CallableTypeKind, ClassBase, ClassType, FindLegacyTypeVarsVisitor,
        HasRelationToVisitor, InstanceFallbackShadowsNonDataDescriptor, IsDisjointVisitor,
        KnownFunction, MemberLookupPolicy, NormalizedVisitor, PropertyInstanceType, Signature,
        StaticClassLiteral, Type, TypeContext, TypeMapping, TypeQualifiers, TypeRelation,
        TypeVarVariance, UnionType, VarianceInferable,
        constraints::{
            ConstraintSet, IteratorConstraintsExtension, OptionConstraintsExtension,
            ResultConstraintsExtension,
        },
        context::InferContext,
        diagnostic::report_undeclared_protocol_member,
        generics::InferableTypeVars,
    },
};

impl<'db> StaticClassLiteral<'db> {
    /// Returns `Some` if this is a protocol class, `None` otherwise.
    pub(super) fn into_protocol_class(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        self.is_protocol(db)
            .then_some(ProtocolClass(ClassType::NonGeneric(self.into())))
    }
}

impl<'db> ClassType<'db> {
    /// Returns `Some` if this is a protocol class, `None` otherwise.
    pub(super) fn into_protocol_class(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        self.is_protocol(db).then_some(ProtocolClass(self))
    }
}

/// Representation of a single `Protocol` class definition.
///
/// # Ordering
///
/// Ordering is based on the wrapped data's salsa-assigned id and not on its values.
/// The id may change between runs, or when e.g. a `ProtocolClass` was garbage-collected and recreated.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize, PartialOrd, Ord,
)]
pub(super) struct ProtocolClass<'db>(ClassType<'db>);

impl<'db> ProtocolClass<'db> {
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
        self.static_class_literal(db)
            .is_some_and(|(class_literal, _)| {
                class_literal
                    .known_function_decorators(db)
                    .contains(&KnownFunction::RuntimeCheckable)
            })
    }

    /// Iterate through the body of the protocol class. Check that all definitions
    /// in the protocol class body are either explicitly declared directly in the
    /// class body, or are declared in a superclass of the protocol class.
    pub(super) fn validate_members(self, context: &InferContext) {
        let db = context.db();
        let interface = self.interface(db);
        let Some((class_literal, _)) = self.static_class_literal(db) else {
            return;
        };
        let body_scope = class_literal.body_scope(db);
        let class_place_table = place_table(db, body_scope);

        for (symbol_id, mut bindings_iterator) in
            use_def_map(db, body_scope).all_end_of_scope_symbol_bindings()
        {
            let symbol_name = class_place_table.symbol(symbol_id).name();

            if !interface.includes_member(db, symbol_name) {
                continue;
            }

            let has_declaration =
                self.iter_mro(db)
                    .filter_map(ClassBase::into_class)
                    .any(|superclass| {
                        let Some((superclass_literal, _)) = superclass.static_class_literal(db)
                        else {
                            return false;
                        };
                        let superclass_scope = superclass_literal.body_scope(db);
                        let Some(scoped_symbol_id) =
                            place_table(db, superclass_scope).symbol_id(symbol_name)
                        else {
                            return false;
                        };
                        !place_from_declarations(
                            db,
                            use_def_map(db, superclass_scope)
                                .end_of_scope_declarations(ScopedPlaceId::Symbol(scoped_symbol_id)),
                        )
                        .into_place_and_conflicting_declarations()
                        .0
                        .place
                        .is_undefined()
                    });

            if has_declaration {
                continue;
            }

            let Some(first_definition) =
                bindings_iterator.find_map(|binding| binding.binding.definition())
            else {
                continue;
            };

            report_undeclared_protocol_member(context, first_definition, self, class_place_table);
        }
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self(
            self.0
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self(
            self.0.recursive_type_normalized_impl(db, div, nested)?,
        ))
    }
}

impl<'db> Deref for ProtocolClass<'db> {
    type Target = ClassType<'db>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'db> From<ProtocolClass<'db>> for Type<'db> {
    fn from(value: ProtocolClass<'db>) -> Self {
        Self::from(value.0)
    }
}

/// The interface of a protocol: the members of that protocol, and the types of those members.
///
/// # Ordering
/// Ordering is based on the protocol interface member's salsa-assigned id and not on its members.
/// The id may change between runs, or when the protocol instance members was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub(super) struct ProtocolInterface<'db> {
    #[returns(ref)]
    inner: BTreeMap<Name, ProtocolMemberData<'db>>,
}

impl get_size2::GetSize for ProtocolInterface<'_> {}

pub(super) fn walk_protocol_interface<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    interface: ProtocolInterface<'db>,
    visitor: &V,
) {
    for member in interface.members(db) {
        walk_protocol_member(db, &member, visitor);
    }
}

impl<'db> ProtocolInterface<'db> {
    /// Synthesize a new protocol interface with the given members.
    ///
    /// All created members will be covariant, read-only property members
    /// rather than method members or mutable attribute members.
    pub(super) fn with_property_members<'a, M>(db: &'db dyn Db, members: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        let members: BTreeMap<_, _> = members
            .into_iter()
            .map(|(name, ty)| {
                // Synthesize a read-only property (one that has a getter but no setter)
                // which returns the specified type from its getter.
                (
                    Name::new(name),
                    ProtocolMemberData {
                        qualifiers: TypeQualifiers::default(),
                        kind: ProtocolMemberKind::Property {
                            get_type: Some(ty.normalized(db)),
                            set_type: None,
                        },
                        definition: None,
                    },
                )
            })
            .collect();
        Self::new(db, members)
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self::new(db, BTreeMap::default())
    }

    pub(super) fn members<'a>(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = ProtocolMember<'a, 'db>>
    where
        'db: 'a,
    {
        self.inner(db).iter().map(|(name, data)| ProtocolMember {
            name,
            kind: &data.kind,
            qualifiers: data.qualifiers,
        })
    }

    pub(super) fn non_method_members(self, db: &'db dyn Db) -> Vec<ProtocolMember<'db, 'db>> {
        self.members(db)
            .filter(|member| {
                !member.is_method() && !member.instance_get_type(db).is_some_and(|ty| ty.is_todo())
            })
            .collect()
    }

    pub(super) fn member_by_name<'a>(
        self,
        db: &'db dyn Db,
        name: &'a str,
    ) -> Option<ProtocolMember<'a, 'db>> {
        self.inner(db).get(name).map(|data| ProtocolMember {
            name,
            kind: &data.kind,
            qualifiers: data.qualifiers,
        })
    }

    pub(super) fn includes_member(self, db: &'db dyn Db, name: &str) -> bool {
        self.inner(db).contains_key(name)
    }

    /// Returns the `__call__` method's callable type if this protocol has a `__call__` method member.
    pub(super) fn call_method(self, db: &'db dyn Db) -> Option<CallableType<'db>> {
        self.member_by_name(db, "__call__")
            .and_then(|member| match member.kind {
                ProtocolMemberKind::Method(callable) => Some(*callable),
                _ => None,
            })
    }

    pub(super) fn member_definition(self, db: &'db dyn Db, name: &str) -> Option<Definition<'db>> {
        self.inner(db).get(name).and_then(|data| data.definition)
    }

    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        self.member_by_name(db, name)
            .map(|member| {
                member
                    .instance_get_type(db)
                    .map(|get_type| PlaceAndQualifiers {
                        place: Place::bound(get_type),
                        qualifiers: member.qualifiers(),
                    })
                    .unwrap_or(Place::Undefined.into())
            })
            .unwrap_or_else(|| Type::object().member(db, name))
    }

    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        other.members(db).when_all(db, |other_member| {
            self.member_by_name(db, other_member.name)
                .when_some_and(|our_member| match (our_member.kind, other_member.kind) {
                    // Method members are always immutable;
                    // they can never be subtypes of/assignable to mutable attribute members.
                    (ProtocolMemberKind::Method(_), ProtocolMemberKind::Attribute(_)) => {
                        ConstraintSet::from(false)
                    }

                    // A property member can only be a subtype of an attribute member
                    // if the property is readable *and* writable, and the types are compatible.
                    (
                        ProtocolMemberKind::Property { get_type, set_type },
                        ProtocolMemberKind::Attribute(other_type),
                    ) => ConstraintSet::from(get_type.is_some() && set_type.is_some()).and(
                        db,
                        || {
                            check_get_set_relation(
                                db,
                                *get_type,
                                *set_type,
                                *other_type,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        },
                    ),

                    // A `@property` member can never be a subtype of a method member, as it is not necessarily
                    // accessible on the meta-type, whereas a method member must be.
                    (ProtocolMemberKind::Property { .. }, ProtocolMemberKind::Method(_)) => {
                        ConstraintSet::from(false)
                    }

                    // But an attribute member *can* be a subtype of a method member,
                    // providing it is marked `ClassVar`
                    (
                        ProtocolMemberKind::Attribute(our_type),
                        ProtocolMemberKind::Method(other_type),
                    ) => ConstraintSet::from(
                        our_member.qualifiers.contains(TypeQualifiers::CLASS_VAR),
                    )
                    .and(db, || {
                        our_type.has_relation_to_impl(
                            db,
                            Type::Callable(protocol_bind_self(db, *other_type, None)),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    }),

                    (
                        ProtocolMemberKind::Method(our_method),
                        ProtocolMemberKind::Method(other_method),
                    ) => protocol_bind_self(db, *our_method, None).has_relation_to_impl(
                        db,
                        protocol_bind_self(db, *other_method, None),
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    ),

                    (
                        ProtocolMemberKind::Attribute(our_type),
                        ProtocolMemberKind::Attribute(other_type),
                    ) => our_type
                        .has_relation_to_impl(
                            db,
                            *other_type,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                        .and(db, || {
                            other_type.has_relation_to_impl(
                                db,
                                *our_type,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        }),

                    // Protocol-protocol property comparison: check get/set types individually.
                    (
                        ProtocolMemberKind::Property {
                            get_type: our_get,
                            set_type: our_set,
                        },
                        ProtocolMemberKind::Property {
                            get_type: other_get,
                            set_type: other_set,
                        },
                    ) => {
                        // Get types are covariant: our get type must relate to other's get type.
                        let get_ok = match (*our_get, *other_get) {
                            (Some(our_ty), Some(other_ty)) => our_ty.has_relation_to_impl(
                                db,
                                other_ty,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            ),
                            (None, Some(_)) => ConstraintSet::from(false),
                            _ => ConstraintSet::from(true),
                        };
                        get_ok.and(db, || {
                            // Set types are contravariant: other's set type must relate to our set type.
                            match (*our_set, *other_set) {
                                (Some(our_ty), Some(other_ty)) => other_ty.has_relation_to_impl(
                                    db,
                                    our_ty,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                ),
                                (_, None) => ConstraintSet::from(true),
                                (None, Some(_)) => ConstraintSet::from(false),
                            }
                        })
                    }

                    // An attribute member can be a subtype of a property member if:
                    // - It satisfies the get type (covariant)
                    // - It satisfies the set type (contravariant)
                    (
                        ProtocolMemberKind::Attribute(our_type),
                        ProtocolMemberKind::Property {
                            get_type: other_get,
                            set_type: other_set,
                        },
                    ) => check_get_set_relation(
                        db,
                        *other_get,
                        *other_set,
                        *our_type,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    ),

                    // A method member can be a subtype of a read-only property member
                    // (if the get type is a callable), but not a writable one.
                    (
                        ProtocolMemberKind::Method(our_method),
                        ProtocolMemberKind::Property {
                            get_type: other_get,
                            set_type: other_set,
                        },
                    ) => ConstraintSet::from(other_set.is_none()).and(db, || match *other_get {
                        Some(other_get_ty) => {
                            Type::Callable(protocol_bind_self(db, *our_method, None))
                                .has_relation_to_impl(
                                    db,
                                    other_get_ty,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                        }
                        None => ConstraintSet::from(true),
                    }),
                })
        })
    }

    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| (name.clone(), data.normalized_impl(db, visitor)))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| {
                    Some((
                        name.clone(),
                        data.recursive_type_normalized_impl(db, div, nested)?,
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        ))
    }

    pub(super) fn specialized_and_normalized<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
    ) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| {
                    (
                        name.clone(),
                        data.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            &ApplyTypeMappingVisitor::default(),
                        )
                        .normalized(db),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        )
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for data in self.inner(db).values() {
            data.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    pub(super) fn display(self, db: &'db dyn Db) -> impl std::fmt::Display {
        struct ProtocolInterfaceDisplay<'db> {
            db: &'db dyn Db,
            interface: ProtocolInterface<'db>,
        }

        impl std::fmt::Display for ProtocolInterfaceDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_char('{')?;
                for (i, (name, data)) in self.interface.inner(self.db).iter().enumerate() {
                    write!(f, "\"{name}\": {data}", data = data.display(self.db))?;
                    if i < self.interface.inner(self.db).len() - 1 {
                        f.write_str(", ")?;
                    }
                }
                f.write_char('}')
            }
        }

        ProtocolInterfaceDisplay {
            db,
            interface: self,
        }
    }
}

impl<'db> VarianceInferable<'db> for ProtocolInterface<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.members(db)
            .flat_map(|member| {
                // Get types (instance and meta) contribute covariant variance.
                let get_variances = member
                    .instance_get_type(db)
                    .into_iter()
                    .chain(member.meta_get_type(db))
                    .map(|ty| ty.variance_of(db, typevar));

                // Set types (instance and meta) contribute contravariant variance (flipped).
                let set_variances = member
                    .instance_set_type()
                    .into_iter()
                    .chain(member.meta_set_type())
                    .map(|ty| ty.variance_of(db, typevar).flip());

                get_variances.chain(set_variances)
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProtocolMemberData<'db> {
    kind: ProtocolMemberKind<'db>,
    qualifiers: TypeQualifiers,
    definition: Option<Definition<'db>>,
}

impl<'db> ProtocolMemberData<'db> {
    fn normalized(&self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    fn normalized_impl(&self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self {
            kind: self.kind.normalized_impl(db, visitor),
            qualifiers: self.qualifiers,
            definition: None,
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            kind: match &self.kind {
                ProtocolMemberKind::Method(callable) => ProtocolMemberKind::Method(
                    callable.recursive_type_normalized_impl(db, div, nested)?,
                ),
                ProtocolMemberKind::Property { get_type, set_type } => {
                    ProtocolMemberKind::Property {
                        get_type: match get_type {
                            Some(ty) => Some(ty.recursive_type_normalized_impl(db, div, true)?),
                            None => None,
                        },
                        set_type: match set_type {
                            Some(ty) => Some(ty.recursive_type_normalized_impl(db, div, true)?),
                            None => None,
                        },
                    }
                }
                ProtocolMemberKind::Attribute(ty) if nested => {
                    ProtocolMemberKind::Attribute(ty.recursive_type_normalized_impl(db, div, true)?)
                }
                ProtocolMemberKind::Attribute(ty) => ProtocolMemberKind::Attribute(
                    ty.recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div),
                ),
            },
            qualifiers: self.qualifiers,
            definition: self.definition,
        })
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            kind: self
                .kind
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            qualifiers: self.qualifiers,
            definition: self.definition,
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.kind
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    fn display(&self, db: &'db dyn Db) -> impl std::fmt::Display + '_ {
        struct ProtocolMemberDataDisplay<'a, 'db> {
            db: &'db dyn Db,
            data: &'a ProtocolMemberKind<'db>,
            qualifiers: TypeQualifiers,
        }

        impl std::fmt::Display for ProtocolMemberDataDisplay<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match &self.data {
                    ProtocolMemberKind::Method(callable) => {
                        write!(f, "MethodMember(`{}`)", callable.display(self.db))
                    }
                    ProtocolMemberKind::Property { get_type, set_type } => {
                        let mut d = f.debug_struct("PropertyMember");
                        if let Some(get_type) = get_type {
                            d.field(
                                "get_type",
                                &format_args!("`{}`", &get_type.display(self.db)),
                            );
                        }
                        if let Some(set_type) = set_type {
                            d.field(
                                "set_type",
                                &format_args!("`{}`", &set_type.display(self.db)),
                            );
                        }
                        d.finish()
                    }
                    ProtocolMemberKind::Attribute(ty) => {
                        f.write_str("AttributeMember(")?;
                        write!(f, "`{}`", ty.display(self.db))?;
                        if self.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                            f.write_str("; ClassVar")?;
                        }
                        f.write_char(')')
                    }
                }
            }
        }

        ProtocolMemberDataDisplay {
            db,
            data: &self.kind,
            qualifiers: self.qualifiers,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, Hash, get_size2::GetSize)]
enum ProtocolMemberKind<'db> {
    Method(CallableType<'db>),
    Property {
        get_type: Option<Type<'db>>,
        set_type: Option<Type<'db>>,
    },
    Attribute(Type<'db>),
}

impl<'db> ProtocolMemberKind<'db> {
    /// Create a `Property` variant by extracting get/set types from a `PropertyInstanceType`.
    ///
    /// The get type is the return type of calling the getter with a single positional argument.
    /// The set type is the annotated type of the first value parameter of the setter callable
    /// (after binding self).
    ///
    /// Falls back to `Attribute(Type::PropertyInstance(property))` if the property's
    /// getter/setter signatures cannot be interpreted.
    fn from_property_instance(property: PropertyInstanceType<'db>, db: &'db dyn Db) -> Self {
        fn inner<'db>(
            db: &'db dyn Db,
            property: PropertyInstanceType<'db>,
        ) -> Option<(Option<Type<'db>>, Option<Type<'db>>)> {
            let get_type = match property.getter(db) {
                None => None,
                Some(getter) => Some(
                    getter
                        .try_call(db, &CallArguments::positional([Type::any()]))
                        .ok()?
                        .return_type(db),
                ),
            };

            let setter_signature = match property.setter(db) {
                None => None,
                Some(Type::Callable(callable)) => Some(callable.signatures(db)),
                Some(Type::FunctionLiteral(function)) => Some(function.signature(db)),
                _ => return None,
            };

            let set_type_from_signature = |sig: &Signature<'db>| match sig.parameters().as_slice() {
                [_, parameter] if parameter.is_positional() && parameter.form.is_value() => {
                    Some(parameter.annotated_type())
                }
                _ => None,
            };

            let set_type = if let Some(signature) = setter_signature {
                if let Some(ty) =
                    UnionType::try_from_elements(db, signature.iter().map(set_type_from_signature))
                {
                    Some(ty)
                } else {
                    return None;
                }
            } else {
                None
            };

            Some((get_type, set_type))
        }

        inner(db, property)
            .map(|(get_type, set_type)| ProtocolMemberKind::Property { get_type, set_type })
            .unwrap_or(ProtocolMemberKind::Attribute(Type::PropertyInstance(
                property,
            )))
    }

    fn normalized_impl(&self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            ProtocolMemberKind::Method(callable) => {
                ProtocolMemberKind::Method(callable.normalized_impl(db, visitor))
            }
            ProtocolMemberKind::Property { get_type, set_type } => ProtocolMemberKind::Property {
                get_type: get_type.map(|ty| ty.normalized_impl(db, visitor)),
                set_type: set_type.map(|ty| ty.normalized_impl(db, visitor)),
            },
            ProtocolMemberKind::Attribute(ty) => {
                ProtocolMemberKind::Attribute(ty.normalized_impl(db, visitor))
            }
        }
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            ProtocolMemberKind::Method(callable) => ProtocolMemberKind::Method(
                callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
            ProtocolMemberKind::Property { get_type, set_type } => ProtocolMemberKind::Property {
                get_type: get_type
                    .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
                set_type: set_type
                    .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            },
            ProtocolMemberKind::Attribute(ty) => ProtocolMemberKind::Attribute(
                ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self {
            ProtocolMemberKind::Method(callable) => {
                callable.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            ProtocolMemberKind::Property { get_type, set_type } => {
                if let Some(ty) = get_type {
                    ty.find_legacy_typevars(db, binding_context, typevars);
                }
                if let Some(ty) = set_type {
                    ty.find_legacy_typevars(db, binding_context, typevars);
                }
            }
            ProtocolMemberKind::Attribute(ty) => {
                ty.find_legacy_typevars(db, binding_context, typevars);
            }
        }
    }
}

/// A single member of a protocol interface.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProtocolMember<'a, 'db> {
    name: &'a str,
    kind: &'a ProtocolMemberKind<'db>,
    qualifiers: TypeQualifiers,
}

fn walk_protocol_member<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    member: &ProtocolMember<'_, 'db>,
    visitor: &V,
) {
    match member.kind {
        ProtocolMemberKind::Method(method) => visitor.visit_callable_type(db, *method),
        ProtocolMemberKind::Property { get_type, set_type } => {
            if let Some(get_type) = get_type {
                visitor.visit_type(db, *get_type);
            }
            if let Some(set_type) = set_type {
                visitor.visit_type(db, *set_type);
            }
        }
        ProtocolMemberKind::Attribute(ty) => visitor.visit_type(db, *ty),
    }
}

impl<'a, 'db> ProtocolMember<'a, 'db> {
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    pub(super) fn qualifiers(&self) -> TypeQualifiers {
        self.qualifiers
    }

    pub(super) const fn is_method(&self) -> bool {
        matches!(self.kind, ProtocolMemberKind::Method(_))
    }

    /// Must this member be present on an instance of a class `X`
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what type must that member have?
    pub(super) fn instance_get_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self.kind {
            ProtocolMemberKind::Method(callable) => {
                let signatures = if callable.is_staticmethod_like(db) {
                    // Staticmethods have no implicit `self`/`cls` parameter,
                    // so we must not call `bind_self` (which would strip the
                    // first positional parameter).
                    callable.signatures(db).clone()
                } else {
                    callable.signatures(db).bind_self(db, None)
                };
                // Normalize the kind to `Regular` so that protocol callables
                // are structurally comparable regardless of how they were defined.
                Some(Type::Callable(CallableType::new(
                    db,
                    signatures,
                    CallableTypeKind::Regular,
                )))
            }
            ProtocolMemberKind::Property { get_type, .. } => *get_type,
            ProtocolMemberKind::Attribute(ty) => Some(*ty),
        }
    }

    /// Must this member be present on the class object `X` itself
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what type must that member have when read from the class object itself?
    pub(super) fn meta_get_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self.kind {
            ProtocolMemberKind::Method(callable) => Some(Type::Callable(CallableType::new(
                db,
                callable.signatures(db).clone(),
                CallableTypeKind::Regular,
            ))),
            ProtocolMemberKind::Property { .. } => None,
            ProtocolMemberKind::Attribute(ty) => {
                if self.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                    Some(*ty)
                } else {
                    None
                }
            }
        }
    }

    /// Must this member be writable on an instance of a class `X`
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what types must it be permissible to write to that member?
    /// If not, what error should be returned when a user tries to write
    /// to this member on an instance?
    pub(super) fn instance_set_type(&self) -> Result<Type<'db>, AttributeAssignmentError<'db>> {
        match self.kind {
            ProtocolMemberKind::Property { set_type, .. } => {
                set_type.ok_or(AttributeAssignmentError::ReadOnlyProperty(None))
            }
            ProtocolMemberKind::Method(_) => Err(AttributeAssignmentError::CannotAssign),
            ProtocolMemberKind::Attribute(ty) => {
                if self.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                    Err(AttributeAssignmentError::CannotAssignToClassVar)
                } else {
                    Ok(*ty)
                }
            }
        }
    }

    /// Must this member be writable on the class object `X` itself
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what types must it be permissible to write to that
    /// member on the class object `X`? If not, what error should be
    /// returned when a user tries to write to this member on the
    /// class object itself?
    pub(super) fn meta_set_type(&self) -> Result<Type<'db>, AttributeAssignmentError<'db>> {
        match self.kind {
            ProtocolMemberKind::Property { .. } => {
                Err(AttributeAssignmentError::CannotAssignToInstanceAttr)
            }
            ProtocolMemberKind::Method(_) => Err(AttributeAssignmentError::CannotAssign),
            ProtocolMemberKind::Attribute(ty) => {
                if self.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                    Ok(*ty)
                } else {
                    Err(AttributeAssignmentError::CannotAssignToInstanceAttr)
                }
            }
        }
    }

    /// Return `true` if `other` contains an attribute/method/property that satisfies
    /// the part of the interface defined by this protocol member.
    pub(super) fn is_satisfied_by(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if let ProtocolMemberKind::Method(method) = self.kind {
            // `__call__` members must be special cased for several reasons:
            //
            // 1. Looking up `__call__` on the meta-type of a `Callable` type returns `Place::Undefined` currently
            // 2. Looking up `__call__` on the meta-type of a function-literal type currently returns a type that
            //    has an extremely vague signature (`(*args, **kwargs) -> Any`), which is not useful for protocol
            //    checking.
            // 3. Looking up `__call__` on the meta-type of a class-literal, generic-alias or subclass-of type is
            //    unfortunately not sufficient to obtain the `Callable` supertypes of these types, due to the
            //    complex interaction between `__new__`, `__init__` and metaclass `__call__`.
            let attribute_type = if self.name == "__call__" {
                other
            } else {
                let Place::Defined(DefinedPlace {
                    ty: attribute_type,
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) = other
                    .invoke_descriptor_protocol(
                        db,
                        self.name,
                        Place::Undefined.into(),
                        InstanceFallbackShadowsNonDataDescriptor::No,
                        MemberLookupPolicy::default(),
                    )
                    .place
                else {
                    return ConstraintSet::from(false);
                };
                attribute_type
            };

            // TODO: Instances of `typing.Self` in the protocol member should specialize to the
            // type that we are checking. Without this, we will treat `Self` as an inferable
            // typevar, and allow it to match against _any_ type.
            //
            // It's not very principled, but we also use the literal fallback type, instead of
            // `other` directly. This lets us check whether things like `Literal[0]` satisfy a
            // protocol that includes methods that have `typing.Self` annotations, without
            // overly constraining `Self` to that specific literal.
            //
            // With the new solver, we should be to replace all of this with an additional
            // constraint that enforces what `Self` can specialize to.
            let fallback_other = other.literal_fallback_instance(db).unwrap_or(other);
            return attribute_type
                .try_upcast_to_callable(db)
                .when_some_and(|callables| {
                    callables
                        .map(|callable| callable.apply_self(db, fallback_other))
                        .has_relation_to_impl(
                            db,
                            protocol_bind_self(db, *method, Some(fallback_other)),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                });
        }

        // Unified accessor-based check for non-method members.
        self.instance_get_type(db)
            .when_none_or(|get_type| {
                let Place::Defined(DefinedPlace {
                    ty: attribute_type,
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) = other.member(db, self.name).place
                else {
                    return ConstraintSet::from(false);
                };
                attribute_type.has_relation_to_impl(
                    db,
                    get_type,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            })
            .and(db, || {
                self.instance_set_type().when_err_or(|set_type| {
                    ConstraintSet::from(
                        other
                            .validate_attribute_assignment(db, self.name, set_type)
                            .is_ok(),
                    )
                })
            })
            .and(db, || {
                self.meta_get_type(db).when_none_or(|get_type| {
                    let Place::Defined(DefinedPlace {
                        ty: attribute_type,
                        definedness: Definedness::AlwaysDefined,
                        ..
                    }) = other.class_member(db, Name::from(self.name)).place
                    else {
                        return ConstraintSet::from(false);
                    };
                    attribute_type.has_relation_to_impl(
                        db,
                        get_type,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            })
            .and(db, || {
                self.meta_set_type().when_err_or(|set_type| {
                    ConstraintSet::from(
                        other
                            .to_meta_type(db)
                            .validate_attribute_assignment(db, self.name, set_type)
                            .is_ok(),
                    )
                })
            })
    }
}

/// Returns `true` if a declaration or binding to a given name in a protocol class body
/// should be excluded from the list of protocol members of that class.
///
/// The list of excluded members is subject to change between Python versions,
/// especially for dunders, but it probably doesn't matter *too* much if this
/// list goes out of date. It's up to date as of Python commit 87b1ea016b1454b1e83b9113fa9435849b7743aa
/// (<https://github.com/python/cpython/blob/87b1ea016b1454b1e83b9113fa9435849b7743aa/Lib/typing.py#L1776-L1814>)
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
    ) || member.starts_with("_abc_")
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BoundOnClass {
    Yes,
    No,
}

impl BoundOnClass {
    const fn is_yes(self) -> bool {
        matches!(self, BoundOnClass::Yes)
    }

    const fn from_qualifiers(qualifiers: TypeQualifiers) -> Self {
        if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
            BoundOnClass::Yes
        } else {
            BoundOnClass::No
        }
    }
}

/// Inner Salsa query for [`ProtocolClass::interface`].
#[salsa::tracked(cycle_initial=proto_interface_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn cached_protocol_interface<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    let mut members = BTreeMap::default();

    for (parent_scope, specialization) in class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|class| {
            let (class_literal, specialization) = class.static_class_literal(db)?;
            let protocol_class = class_literal.into_protocol_class(db)?;
            let parent_scope = protocol_class.static_class_literal(db)?.0.body_scope(db);
            Some((parent_scope, specialization))
        })
    {
        let use_def_map = use_def_map(db, parent_scope);
        let place_table = place_table(db, parent_scope);
        let mut direct_members = FxHashMap::default();

        // Bindings in the class body that are not declared in the class body
        // are not valid protocol members, and we plan to emit diagnostics for them
        // elsewhere. Invalid or not, however, it's important that we still consider
        // them to be protocol members. The implementation of `issubclass()` and
        // `isinstance()` for runtime-checkable protocols considers them to be protocol
        // members at runtime, and it's important that we accurately understand
        // type narrowing that uses `isinstance()` or `issubclass()` with
        // runtime-checkable protocols.
        for (symbol_id, bindings) in use_def_map.all_end_of_scope_symbol_bindings() {
            let place_and_definition = place_from_bindings(db, bindings);
            let Some(ty) = place_and_definition.place.ignore_possibly_undefined() else {
                continue;
            };
            direct_members.insert(
                symbol_id,
                (
                    ty,
                    TypeQualifiers::default(),
                    place_and_definition.first_definition,
                    BoundOnClass::Yes,
                ),
            );
        }

        for (symbol_id, declarations) in use_def_map.all_end_of_scope_symbol_declarations() {
            let place_result = place_from_declarations(db, declarations);
            let first_declaration = place_result.first_declaration;
            let place = place_result.ignore_conflicting_declarations();
            if let Some(new_type) = place.place.ignore_possibly_undefined() {
                direct_members
                    .entry(symbol_id)
                    .and_modify(|(ty, quals, _, _)| {
                        *ty = new_type;
                        *quals = place.qualifiers;
                    })
                    .or_insert((
                        new_type,
                        place.qualifiers,
                        first_declaration,
                        BoundOnClass::from_qualifiers(place.qualifiers),
                    ));
            }
        }

        for (symbol_id, (ty, qualifiers, definition, bound_on_class)) in direct_members {
            let name = place_table.symbol(symbol_id).name();
            if excluded_from_proto_members(name) {
                continue;
            }
            if members.contains_key(name) {
                continue;
            }

            let ty = ty.apply_optional_specialization(db, specialization);

            let member = match ty {
                Type::PropertyInstance(property) => {
                    ProtocolMemberKind::from_property_instance(property, db)
                }
                Type::Callable(callable)
                    if bound_on_class.is_yes() && callable.is_function_like(db) =>
                {
                    ProtocolMemberKind::Method(callable)
                }
                Type::FunctionLiteral(function) if function.is_classmethod(db) => {
                    ProtocolMemberKind::Method(function.into_callable_type(db))
                }
                Type::FunctionLiteral(function) if function.is_staticmethod(db) => {
                    ProtocolMemberKind::Method(function.into_callable_type(db))
                }
                Type::FunctionLiteral(function) if bound_on_class.is_yes() => {
                    ProtocolMemberKind::Method(function.into_callable_type(db))
                }
                _ => ProtocolMemberKind::Attribute(ty),
            };

            members.insert(
                name.clone(),
                ProtocolMemberData {
                    kind: member,
                    qualifiers,
                    definition,
                },
            );
        }
    }

    ProtocolInterface::new(db, members)
}

// If we use `expect(clippy::trivially_copy_pass_by_ref)` here,
// the lint expectation is unfulfilled on WASM
#[allow(clippy::trivially_copy_pass_by_ref)]
fn proto_interface_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    ProtocolInterface::empty(db)
}

/// Check that an attribute type satisfies a property's get/set type constraints.
///
/// Get types are covariant: `attr_type <: get_type`.
/// Set types are contravariant: `set_type <: attr_type`.
#[expect(clippy::too_many_arguments)]
fn check_get_set_relation<'db>(
    db: &'db dyn Db,
    get_type: Option<Type<'db>>,
    set_type: Option<Type<'db>>,
    attr_type: Type<'db>,
    inferable: InferableTypeVars<'_, 'db>,
    relation: TypeRelation<'db>,
    relation_visitor: &HasRelationToVisitor<'db>,
    disjointness_visitor: &IsDisjointVisitor<'db>,
) -> ConstraintSet<'db> {
    let get_ok = match get_type {
        Some(get_ty) => attr_type.has_relation_to_impl(
            db,
            get_ty,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        ),
        None => ConstraintSet::from(true),
    };
    get_ok.and(db, || match set_type {
        Some(set_ty) => set_ty.has_relation_to_impl(
            db,
            attr_type,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        ),
        None => ConstraintSet::from(true),
    })
}

/// Bind `self`, and *also* discard the functionlike-ness of the callable.
///
/// This additional upcasting is required in order for protocols with `__call__` method
/// members to be considered assignable to `Callable` types, since the `Callable` supertype
/// of the `__call__` method will be function-like but a `Callable` type is not.
fn protocol_bind_self<'db>(
    db: &'db dyn Db,
    callable: CallableType<'db>,
    self_type: Option<Type<'db>>,
) -> CallableType<'db> {
    if callable.is_staticmethod_like(db) {
        // Staticmethods have no implicit `self`/`cls` parameter,
        // so `bind_self` must not strip the first parameter.
        // Normalize the kind to `Regular` so that the callable is
        // structurally comparable with other protocol callables.
        return CallableType::new(
            db,
            callable.signatures(db).clone(),
            CallableTypeKind::Regular,
        );
    }
    CallableType::new(
        db,
        callable.signatures(db).bind_self(db, self_type),
        CallableTypeKind::Regular,
    )
}
