use std::{collections::BTreeMap, fmt::Write, ops::Deref};

use itertools::Itertools;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use crate::{
    Db, FxOrderSet,
    place::{Boundness, Place, PlaceAndQualifiers, place_from_bindings, place_from_declarations},
    semantic_index::{SemanticIndex, definition::Definition, place_table, use_def_map},
    types::{
        AttributeAssignmentError, BoundTypeVarInstance, CallArguments, CallableType, ClassBase,
        ClassLiteral, ClassType, Constraints, HasRelationToVisitor, InferContext, KnownFunction,
        NormalizedVisitor, OptionConstraintsExtension, PropertyInstanceType, ScopedPlaceId,
        Signature, Type, TypeMapping, TypeQualifiers, TypeRelation, TypeVarVariance, UnionType,
        VarianceInferable,
        constraints::{IteratorConstraintsExtension, ResultConstraintsExtension},
        diagnostic::report_undeclared_protocol_member,
    },
};

impl<'db> ClassLiteral<'db> {
    /// Returns `Some` if this is a protocol class, `None` otherwise.
    pub(super) fn into_protocol_class(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        self.is_protocol(db)
            .then_some(ProtocolClass(ClassType::NonGeneric(self)))
    }
}

impl<'db> ClassType<'db> {
    /// Returns `Some` if this is a protocol class, `None` otherwise.
    pub(super) fn into_protocol_class(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        self.is_protocol(db).then_some(ProtocolClass(self))
    }
}

/// Representation of a single `Protocol` class definition.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        self.class_literal(db)
            .0
            .known_function_decorators(db)
            .contains(&KnownFunction::RuntimeCheckable)
    }

    /// Iterate through the body of the protocol class. Check that all definitions
    /// in the protocol class body are either explicitly declared directly in the
    /// class body, or are declared in a superclass of the protocol class.
    pub(super) fn validate_members(self, context: &InferContext, index: &SemanticIndex<'db>) {
        let db = context.db();
        let interface = self.interface(db);
        let body_scope = self.class_literal(db).0.body_scope(db);
        let class_place_table = index.place_table(body_scope.file_scope_id(db));

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
                        let superclass_scope = superclass.class_literal(db).0.body_scope(db);
                        let Some(scoped_symbol_id) =
                            place_table(db, superclass_scope).symbol_id(symbol_name)
                        else {
                            return false;
                        };
                        !place_from_declarations(
                            db,
                            index
                                .use_def_map(superclass_scope.file_scope_id(db))
                                .end_of_scope_declarations(ScopedPlaceId::Symbol(scoped_symbol_id)),
                        )
                        .into_place_and_conflicting_declarations()
                        .0
                        .place
                        .is_unbound()
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
}

impl<'db> Deref for ProtocolClass<'db> {
    type Target = ClassType<'db>;

    fn deref(&self) -> &Self::Target {
        &self.0
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
                            get_type: Some(ty),
                            set_type: None,
                        },
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

    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        self.member_by_name(db, name)
            .map(|member| {
                member
                    .instance_get_type(db)
                    .map(|get_type| PlaceAndQualifiers {
                        place: Place::bound(get_type),
                        qualifiers: member.qualifiers(),
                    })
                    .unwrap_or(Place::Unbound.into())
            })
            .unwrap_or_else(|| Type::object(db).instance_member(db, name))
    }

    /// Return `true` if if all members on `self` are also members of `other`.
    ///
    /// TODO: this method should consider the types of the members as well as their names.
    pub(super) fn is_sub_interface_of<C: Constraints<'db>>(
        self,
        db: &'db dyn Db,
        other: Self,
        _visitor: &HasRelationToVisitor<'db, C>,
    ) -> C {
        // TODO: This could just return a bool as written, but this form is what will be needed to
        // combine the constraints when we do assignability checks on each member.
        self.inner(db).keys().when_all(db, |member_name| {
            C::from_bool(db, other.inner(db).contains_key(member_name))
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

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| (name.clone(), data.materialize(db, variance)))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    pub(super) fn specialized_and_normalized<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| {
                    (
                        name.clone(),
                        data.apply_type_mapping(db, type_mapping).normalized(db),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        )
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        for data in self.inner(db).values() {
            data.find_legacy_typevars(db, binding_context, typevars);
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
                member
                    .instance_get_type(db)
                    .into_iter()
                    .map(|get_type| get_type.variance_of(db, typevar))
                    .chain(
                        member
                            .instance_set_type()
                            .into_iter()
                            .map(|set_type| set_type.variance_of(db, typevar).flip()),
                    )
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProtocolMemberData<'db> {
    kind: ProtocolMemberKind<'db>,
    qualifiers: TypeQualifiers,
}

impl<'db> ProtocolMemberData<'db> {
    fn normalized(&self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    fn normalized_impl(&self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self {
            kind: self.kind.normalized_impl(db, visitor),
            qualifiers: self.qualifiers,
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self {
            kind: self.kind.apply_type_mapping(db, type_mapping),
            qualifiers: self.qualifiers,
        }
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        self.kind
            .find_legacy_typevars(db, binding_context, typevars);
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self {
            kind: self.kind.materialize(db, variance),
            qualifiers: self.qualifiers,
        }
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
                        if let Some(getter) = get_type {
                            d.field("get_type", &format_args!("`{}`", &getter.display(self.db)));
                        }
                        if let Some(setter) = set_type {
                            d.field("set_type", &format_args!("`{}`", &setter.display(self.db)));
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

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, Hash, get_size2::GetSize)]
enum ProtocolMemberKind<'db> {
    Method(CallableType<'db>),
    Property {
        get_type: Option<Type<'db>>,
        set_type: Option<Type<'db>>,
    },
    Attribute(Type<'db>),
}

impl<'db> ProtocolMemberKind<'db> {
    fn from_property_instance(property: PropertyInstanceType<'db>, db: &'db dyn Db) -> Self {
        struct PropertyMember<'db> {
            get_type: Option<Type<'db>>,
            set_type: Option<Type<'db>>,
        }

        fn inner<'db>(
            db: &'db dyn Db,
            property: PropertyInstanceType<'db>,
        ) -> Option<PropertyMember<'db>> {
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
                    Some(parameter.annotated_type().unwrap_or_else(Type::unknown))
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

            Some(PropertyMember { get_type, set_type })
        }

        inner(db, property)
            .map(|member| ProtocolMemberKind::Property {
                get_type: member.get_type,
                set_type: member.set_type,
            })
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
            ProtocolMemberKind::Attribute(attribute) => {
                ProtocolMemberKind::Attribute(attribute.normalized_impl(db, visitor))
            }
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        match self {
            ProtocolMemberKind::Method(callable) => {
                ProtocolMemberKind::Method(callable.apply_type_mapping(db, type_mapping))
            }
            ProtocolMemberKind::Property { get_type, set_type } => ProtocolMemberKind::Property {
                get_type: get_type.map(|ty| ty.apply_type_mapping(db, type_mapping)),
                set_type: set_type.map(|ty| ty.apply_type_mapping(db, type_mapping)),
            },
            ProtocolMemberKind::Attribute(attribute) => {
                ProtocolMemberKind::Attribute(attribute.apply_type_mapping(db, type_mapping))
            }
        }
    }

    fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        match self {
            ProtocolMemberKind::Method(callable) => {
                callable.find_legacy_typevars(db, binding_context, typevars);
            }
            ProtocolMemberKind::Property { get_type, set_type } => {
                if let Some(getter) = get_type {
                    getter.find_legacy_typevars(db, binding_context, typevars);
                }
                if let Some(setter) = set_type {
                    setter.find_legacy_typevars(db, binding_context, typevars);
                }
            }
            ProtocolMemberKind::Attribute(attribute) => {
                attribute.find_legacy_typevars(db, binding_context, typevars);
            }
        }
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            ProtocolMemberKind::Method(callable) => {
                ProtocolMemberKind::Method(callable.materialize(db, variance))
            }
            ProtocolMemberKind::Property { get_type, set_type } => ProtocolMemberKind::Property {
                get_type: get_type.map(|ty| ty.materialize(db, variance)),
                set_type: set_type.map(|ty| ty.materialize(db, variance)),
            },
            ProtocolMemberKind::Attribute(attribute) => {
                ProtocolMemberKind::Attribute(attribute.materialize(db, variance))
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

    /// Must this member be present on an instance of a class `X`
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what type must that member have?
    pub(super) fn instance_get_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self.kind {
            ProtocolMemberKind::Method(callable) => Some(callable.bind_self(db)),
            ProtocolMemberKind::Property { get_type, .. } => *get_type,
            ProtocolMemberKind::Attribute(ty) => Some(*ty),
        }
    }

    /// Must this member be present on the class object `X` itself
    /// for `X` to be considered a subtype of the protocol?
    /// If so, what type must that member have when read from the class object itself?
    pub(super) fn meta_get_type(&self) -> Option<Type<'db>> {
        match self.kind {
            ProtocolMemberKind::Method(callable) => Some(Type::Callable(*callable)),
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
            ProtocolMemberKind::Property { set_type, .. } => set_type
                .map(Ok)
                .unwrap_or(Err(AttributeAssignmentError::ReadOnlyProperty)),
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
    pub(super) fn is_satisfied_by<C: Constraints<'db>>(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
        relation: TypeRelation,
        visitor: &HasRelationToVisitor<'db, C>,
    ) -> C {
        if let ProtocolMemberKind::Method(_) = &self.kind {
            // TODO: use the same generalised logic for method members
            // that we do for attribute/protocol members below.
            return C::from_bool(
                db,
                matches!(
                    other.to_meta_type(db).member(db, self.name).place,
                    Place::Type(_, Boundness::Bound)
                ),
            );
        }

        self.instance_get_type(db)
            .when_none_or(db, |get_type| {
                let Place::Type(attribute_type, Boundness::Bound) =
                    other.member(db, self.name).place
                else {
                    return C::unsatisfiable(db);
                };
                attribute_type.has_relation_to_impl(db, get_type, relation, visitor)
            })
            .and(db, || {
                self.instance_set_type().when_err_or(db, |set_type| {
                    C::from_bool(
                        db,
                        other
                            .validate_attribute_assignment(db, self.name, set_type)
                            .is_ok(),
                    )
                })
            })
            .and(db, || {
                self.meta_get_type().when_none_or(db, |get_type| {
                    let Place::Type(attribute_type, Boundness::Bound) =
                        other.class_member(db, Name::from(self.name)).place
                    else {
                        return C::unsatisfiable(db);
                    };
                    attribute_type.has_relation_to_impl(db, get_type, relation, visitor)
                })
            })
            .and(db, || {
                self.meta_set_type().when_err_or(db, |set_type| {
                    C::from_bool(
                        db,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize, Hash)]
enum BoundOnClass {
    Yes,
    No,
}

impl BoundOnClass {
    const fn from_qualifiers(qualifiers: TypeQualifiers) -> Self {
        if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
            BoundOnClass::Yes
        } else {
            BoundOnClass::No
        }
    }

    const fn is_yes(self) -> bool {
        matches!(self, BoundOnClass::Yes)
    }
}

/// Inner Salsa query for [`ProtocolClassLiteral::interface`].
#[salsa::tracked(cycle_fn=proto_interface_cycle_recover, cycle_initial=proto_interface_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
fn cached_protocol_interface<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    let mut members = BTreeMap::default();

    for (parent_protocol, specialization) in class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|class| {
            let (class, specialization) = class.class_literal(db);
            Some((class.into_protocol_class(db)?, specialization))
        })
    {
        let parent_scope = parent_protocol.class_literal(db).0.body_scope(db);
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
            let Some(ty) = place_from_bindings(db, bindings).ignore_possibly_unbound() else {
                continue;
            };
            direct_members.insert(
                symbol_id,
                (ty, TypeQualifiers::default(), BoundOnClass::Yes),
            );
        }

        for (symbol_id, declarations) in use_def_map.all_end_of_scope_symbol_declarations() {
            let place = place_from_declarations(db, declarations).ignore_conflicting_declarations();
            if let Some(new_type) = place.place.ignore_possibly_unbound() {
                direct_members
                    .entry(symbol_id)
                    .and_modify(|(ty, quals, _)| {
                        *ty = new_type;
                        *quals = place.qualifiers;
                    })
                    .or_insert_with(|| {
                        (
                            new_type,
                            place.qualifiers,
                            BoundOnClass::from_qualifiers(place.qualifiers),
                        )
                    });
            }
        }

        for (symbol_id, (ty, qualifiers, bound_on_class)) in direct_members {
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
                },
            );
        }
    }

    ProtocolInterface::new(db, members)
}

// If we use `expect(clippy::trivially_copy_pass_by_ref)` here,
// the lint expectation is unfulfilled on WASM
#[allow(clippy::trivially_copy_pass_by_ref)]
fn proto_interface_cycle_recover<'db>(
    _db: &dyn Db,
    _value: &ProtocolInterface<'db>,
    _count: u32,
    _class: ClassType<'db>,
) -> salsa::CycleRecoveryAction<ProtocolInterface<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn proto_interface_cycle_initial<'db>(
    db: &'db dyn Db,
    _class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    ProtocolInterface::empty(db)
}
