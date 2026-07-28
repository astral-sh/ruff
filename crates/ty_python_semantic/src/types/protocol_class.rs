use std::fmt::Write;
use std::{collections::BTreeMap, ops::Deref};

use itertools::Itertools;

use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::attribute_write::{
    AttributeWriteRequirement, ClassAttributeWriteMember, ExplicitAttributeWriteRequirement,
    FallbackAttributeWriteRequirement, InstanceAttributeWriteMember,
    ProtocolMemberWriteRequirement, attribute_write_requirement,
};
use crate::types::call::{CallArguments, CallDunderError};
use crate::types::relation::{DisjointnessChecker, TypeRelationChecker};
use crate::types::visitor::any_over_type;
use crate::types::{TypeContext, UpcastPolicy};
use crate::{
    Db, FxOrderSet,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, place_from_bindings,
        place_from_declarations,
    },
    types::{
        ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
        CallableType, ClassBase, ClassType, ErrorContext, FindLegacyTypeVarsVisitor,
        GenericContext, InstanceFallbackShadowsNonDataDescriptor, IntersectionType, KnownFunction,
        MemberLookupKey, MemberLookupPolicy, Parameter, PropertyInstanceType, ProtocolInstanceType,
        SelfBinding, Signature, StaticClassLiteral, Type, TypeMapping, TypeQualifiers,
        TypeVarBoundOrConstraints, TypeVarVariance, UnionType, VarianceInferable,
        constraints::{ConstraintSet, IteratorConstraintsExtension, OptionConstraintsExtension},
        context::InferContext,
        diagnostic::report_undeclared_protocol_member,
        generics::Specialization,
        signatures::walk_signature,
    },
};
use ty_python_core::{definition::Definition, place::ScopedPlaceId, place_table, use_def_map};

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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

    /// Walk the effective non-method member types declared by this protocol.
    ///
    /// Method relations have their own declaration-based recursion guard. Keeping them out of this
    /// walk also avoids requesting a method signature while one of its annotations is being
    /// inferred.
    pub(super) fn walk_recursive_member_types<V: super::visitor::TypeVisitor<'db> + ?Sized>(
        self,
        db: &'db dyn Db,
        visitor: &V,
    ) {
        let mut seen_members = FxHashSet::default();

        self.for_each_member_candidate(db, |name, candidate, specialization| {
            if !seen_members.insert(name.clone()) {
                return;
            }
            let candidate = candidate.apply_specialization(db, specialization);
            candidate.walk_recursive_member_types(db, visitor);
        });
    }

    /// Visits protocol member candidates in MRO order after applying declaration precedence.
    ///
    /// Consumers discard shadowed names before applying the accompanying specialization.
    fn for_each_member_candidate(
        self,
        db: &'db dyn Db,
        mut visit: impl FnMut(&Name, ProtocolMemberCandidate<'db>, Option<Specialization<'db>>),
    ) {
        for (parent_scope, specialization) in self
            .iter_mro(db)
            .filter_map(ClassBase::into_class)
            .filter_map(|class| {
                let (class_literal, specialization) = class.static_class_literal(db)?;
                let protocol_class = class_literal.into_protocol_class(db)?;
                Some((
                    protocol_class.static_class_literal(db)?.0.body_scope(db),
                    specialization,
                ))
            })
        {
            let use_def_map = use_def_map(db, parent_scope);
            let place_table = place_table(db, parent_scope);
            let mut direct_members = FxHashMap::default();

            // Bindings that are not declared in the class body are invalid protocol members, but
            // runtime-checkable protocols still consider them members for `isinstance()` and
            // `issubclass()`.
            for (symbol_id, bindings) in use_def_map.all_end_of_scope_symbol_bindings() {
                let place_and_definition = place_from_bindings(db, bindings);
                if let Some(ty) = place_and_definition.place.ignore_possibly_undefined() {
                    direct_members.insert(
                        symbol_id,
                        ProtocolMemberCandidate {
                            ty,
                            qualifiers: TypeQualifiers::default(),
                            definition: place_and_definition.first_definition,
                            bound_on_class: BoundOnClass::Yes,
                        },
                    );
                }
            }

            for (symbol_id, declarations) in use_def_map.all_end_of_scope_symbol_declarations() {
                let place_result = place_from_declarations(db, declarations);
                let first_declaration = place_result.first_declaration;
                let place = place_result.ignore_conflicting_declarations();
                if let Some(ty) = place.place.ignore_possibly_undefined() {
                    direct_members
                        .entry(symbol_id)
                        .and_modify(|candidate| {
                            candidate.ty = ty;
                            candidate.qualifiers = place.qualifiers;
                        })
                        .or_insert(ProtocolMemberCandidate {
                            ty,
                            qualifiers: place.qualifiers,
                            definition: first_declaration,
                            bound_on_class: BoundOnClass::No,
                        });
                }
            }

            #[expect(
                clippy::iter_over_hash_type,
                reason = "member names are unique within each class and both consumers are order-independent"
            )]
            for (symbol_id, candidate) in direct_members {
                let name = place_table.symbol(symbol_id).name();
                if excluded_from_proto_members(name) {
                    continue;
                }

                visit(name, candidate, specialization);
            }
        }
    }

    pub(super) fn is_runtime_checkable(self, db: &'db dyn Db) -> bool {
        self.static_class_literal(db)
            .is_some_and(|(class_literal, _)| {
                class_literal
                    .known_function_decorators(db)
                    .contains(&KnownFunction::RuntimeCheckable)
            })
    }

    /// Return whether `name` is declared by this protocol or one of its superclasses.
    ///
    /// Unlike [`ProtocolClass::interface`], this includes names deliberately excluded from a
    /// protocol's runtime interface. This distinction lets callers recognize declarations such as:
    ///
    /// ```python
    /// class P(Protocol):
    ///     __doc__: str
    /// ```
    pub(super) fn has_member_declaration(self, db: &'db dyn Db, name: &str) -> bool {
        self.iter_mro(db)
            .filter_map(ClassBase::into_class)
            .any(|superclass| {
                let Some((superclass_literal, _)) = superclass.static_class_literal(db) else {
                    return false;
                };
                let superclass_scope = superclass_literal.body_scope(db);
                let Some(scoped_symbol_id) = place_table(db, superclass_scope).symbol_id(name)
                else {
                    return false;
                };
                !place_from_declarations(
                    db,
                    use_def_map(db, superclass_scope)
                        .end_of_scope_declarations(ScopedPlaceId::Symbol(scoped_symbol_id)),
                )
                .ignore_conflicting_declarations()
                .place
                .is_undefined()
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

            if self.has_member_declaration(db, symbol_name) {
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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

/// Walk the member types exposed through an instance of a protocol.
///
/// This binds inferred method receivers and property accessors to `receiver_ty`, while leaving
/// explicit receiver annotations in place because they can affect which overload is exposed.
/// For example, walking `P[int]` visits the return type `int`, but not the inferred receiver type:
///
/// ```python
/// class P[T](Protocol):
///     def method(self) -> T: ...
/// ```
pub(super) fn walk_protocol_instance_interface<
    'db,
    V: super::visitor::TypeVisitor<'db> + ?Sized,
>(
    db: &'db dyn Db,
    interface: ProtocolInterface<'db>,
    receiver_ty: Type<'db>,
    visitor: &V,
) {
    for member in interface.members(db) {
        walk_protocol_instance_member(db, &member, receiver_ty, visitor);
    }
}

/// Walks the types of a protocol member after binding any implicit receiver to `receiver_ty`.
pub(super) fn walk_protocol_instance_member<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    member: &ProtocolMember<'_, 'db>,
    receiver_ty: Type<'db>,
    visitor: &V,
) {
    match member.data.kind {
        ProtocolMemberKind::Method(method, _) => {
            let Type::Callable(callable) = method.ty() else {
                visitor.visit_type(db, method.ty());
                return;
            };
            for signature in callable.signatures(db) {
                if signature.has_implicit_positional_receiver_annotation() {
                    let signature = signature.bind_self(db, Some(receiver_ty));
                    walk_signature(db, &signature, visitor);
                } else {
                    walk_signature(db, signature, visitor);
                }
            }
        }
        ProtocolMemberKind::Property { read, write } => {
            for member_type in [
                read,
                write.and_then(ProtocolMemberWrite::domain),
                write.and_then(ProtocolMemberWrite::descriptor_type),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(ty) = member_type.bind_self(db, receiver_ty) {
                    visitor.visit_type(db, ty);
                }
            }
        }
        ProtocolMemberKind::Attribute(attribute) => {
            if let Some(ty) = attribute.bind_self(db, receiver_ty) {
                visitor.visit_type(db, ty);
            }
        }
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
                (
                    Name::new(name),
                    ProtocolMemberData::property(Some(ProtocolMemberType::new(ty)), None, None),
                )
            })
            .collect();
        Self::new(db, members)
    }

    /// Synthesize a new protocol interface with the given methods.
    pub(super) fn with_methods<'a, M>(db: &'db dyn Db, members: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, CallableType<'db>)>,
    {
        let members: BTreeMap<_, _> = members
            .into_iter()
            .map(|(name, callable)| {
                (
                    Name::new(name),
                    ProtocolMemberData::method(db, callable, None),
                )
            })
            .collect();
        Self::new(db, members)
    }

    fn empty(db: &'db dyn Db) -> Self {
        Self::new(db, BTreeMap::default())
    }

    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        let prev_inner = previous.inner(db);
        let curr_inner = self.inner(db);

        let members: BTreeMap<_, _> = curr_inner
            .iter()
            .map(|(name, curr_data)| {
                let normalized = if let Some(prev_data) = prev_inner.get(name) {
                    curr_data.cycle_normalized(db, prev_data, cycle)
                } else {
                    curr_data.clone()
                };
                (name.clone(), normalized)
            })
            .collect();
        Self::new(db, members)
    }

    pub(super) fn members<'a>(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = ProtocolMember<'a, 'db>>
    where
        'db: 'a,
    {
        self.inner(db)
            .iter()
            .map(|(name, data)| ProtocolMember { name, data })
    }

    pub(super) fn filter_members(
        self,
        db: &'db dyn Db,
        mut predicate: impl FnMut(&ProtocolMember<'_, 'db>) -> bool,
    ) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .filter(|&(name, data)| predicate(&ProtocolMember { name, data }))
                .map(|(name, data)| (name.clone(), data.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn member_count(self, db: &'db dyn Db) -> usize {
        self.inner(db).len()
    }

    pub(super) fn non_method_members(self, db: &'db dyn Db) -> Vec<ProtocolMember<'db, 'db>> {
        self.members(db)
            .filter(|member| !member.is_method() && !member.has_todo_type())
            .collect()
    }

    fn member_by_name<'a>(self, db: &'db dyn Db, name: &'a str) -> Option<ProtocolMember<'a, 'db>> {
        self.inner(db)
            .get(name)
            .map(|data| ProtocolMember { name, data })
    }

    pub(super) fn includes_member(self, db: &'db dyn Db, name: &str) -> bool {
        self.inner(db).contains_key(name)
    }

    /// Returns whether `name` has an instance-write requirement of `type[T]`, where `T` belongs
    /// to `generic_context`.
    pub(super) fn includes_generic_writable_instance_member(
        self,
        db: &'db dyn Db,
        name: &str,
        generic_context: GenericContext<'db>,
    ) -> bool {
        self.member_by_name(db, name)
            .and_then(|member| member.capabilities(db).instance.write)
            .and_then(ProtocolMemberWrite::domain)
            .and_then(|write| write.resolve(db))
            .is_some_and(|write| {
                matches!(
                    write.ty(),
                    Type::SubclassOf(subclass_of)
                        if subclass_of.into_type_var().is_some_and(|typevar| {
                            generic_context.contains(db, typevar.identity(db))
                        })
                )
            })
    }

    /// Returns the declared instance-write requirement for a protocol member.
    ///
    /// `None` means that the protocol does not declare `name`; `Some((None, _))` means that the
    /// member exists but is read-only. A writable member's requirement is bound to `receiver_ty`
    /// before it is returned.
    pub(super) fn instance_write_requirement(
        self,
        db: &'db dyn Db,
        receiver_ty: Type<'db>,
        name: &str,
    ) -> Option<(Option<ProtocolMemberWriteRequirement<'db>>, TypeQualifiers)> {
        self.member_by_name(db, name).map(|member| {
            let capabilities = member.capabilities(db);
            (
                capabilities
                    .instance
                    .write
                    .and_then(|write| write.bind_requirement(db, receiver_ty)),
                member.qualifiers(),
            )
        })
    }

    /// Returns the write requirement exposed through `type[Protocol]` lookup.
    ///
    /// Only members required on every class object that satisfies the meta-protocol are available.
    /// Ordinary instance attributes are required on the constructed object instead.
    pub(super) fn meta_write_requirement(
        self,
        db: &'db dyn Db,
        receiver_ty: Type<'db>,
        name: &str,
    ) -> Option<(Option<Type<'db>>, TypeQualifiers)> {
        self.member_by_name(db, name).and_then(|member| {
            Some((
                member
                    .meta_access(db)?
                    .write
                    .and_then(|write| write.bind_compatibility_type(db, receiver_ty)),
                member.qualifiers(),
            ))
        })
    }

    /// Returns the callable signature exposed by instance access to a protocol's `__call__`
    /// method.
    ///
    /// The callable is already in its instance-bound form, so callers must not bind it again.
    pub(super) fn call_method(self, db: &'db dyn Db) -> Option<CallableType<'db>> {
        self.member_by_name(db, "__call__").and_then(|member| {
            if !member.is_method() {
                return None;
            }
            match member
                .capabilities(db)
                .instance
                .read
                .and_then(|read| read.resolve(db))
                .map(ProtocolMemberType::ty)
            {
                Some(Type::Callable(callable)) => Some(callable),
                _ => None,
            }
        })
    }

    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        self.member_by_name(db, name)
            .map(|member| {
                let capabilities = member.capabilities(db);
                PlaceAndQualifiers {
                    place: capabilities
                        .instance
                        .read
                        .and_then(|read| read.resolve(db))
                        .map(|read| Place::bound(read.ty()))
                        .unwrap_or(Place::Undefined)
                        .with_provenance(Provenance::from_definition(member.definition())),
                    qualifiers: member.qualifiers(),
                }
            })
            .unwrap_or_else(|| Type::object().member(db, name))
    }

    /// Looks up a member guaranteed to exist on every inhabitant of `type[Protocol]`.
    ///
    /// Methods retain their unbound signatures and `ClassVar`s retain their class-side types.
    /// Properties are only required on the constructed instance, so they are undefined even when
    /// the nominal protocol origin provides a property descriptor.
    pub(super) fn meta_member(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<PlaceAndQualifiers<'db>> {
        self.member_by_name(db, name).and_then(|member| {
            let read = member.meta_access(db)?.read;
            Some(PlaceAndQualifiers {
                place: read
                    .and_then(|read| read.resolve(db))
                    .map(|read| Place::bound(read.ty()))
                    .unwrap_or(Place::Undefined)
                    .with_provenance(Provenance::from_definition(member.definition())),
                qualifiers: member.qualifiers(),
            })
        })
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

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self::new(
            db,
            self.inner(db)
                .iter()
                .map(|(name, data)| {
                    (
                        name.clone(),
                        data.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
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

/// A protocol member's write capability.
///
/// Descriptor setters retain their call contract even when their accepted values cannot be
/// represented by a single [`Type`]. This keeps an unrepresentable domain distinct from an absent
/// setter and lets real assignments use normal call binding.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMemberWrite<'db> {
    Type(ProtocolMemberType<'db>),
    Descriptor {
        descriptor: ProtocolMemberType<'db>,
        domain: Option<ProtocolMemberType<'db>>,
    },
}

impl<'db> ProtocolMemberWrite<'db> {
    const fn from_type(member: ProtocolMemberType<'db>) -> Self {
        Self::Type(member)
    }

    fn descriptor(
        descriptor_ty: Type<'db>,
        domain: Option<Type<'db>>,
        definition: Option<Definition<'db>>,
    ) -> Self {
        Self::Descriptor {
            descriptor: ProtocolMemberType::with_definition(descriptor_ty, definition),
            domain: domain.map(|ty| ProtocolMemberType::with_definition(ty, definition)),
        }
    }

    const fn domain(self) -> Option<ProtocolMemberType<'db>> {
        match self {
            Self::Type(member) => Some(member),
            Self::Descriptor { domain, .. } => domain,
        }
    }

    const fn descriptor_type(self) -> Option<ProtocolMemberType<'db>> {
        match self {
            Self::Type(_) => None,
            Self::Descriptor { descriptor, .. } => Some(descriptor),
        }
    }

    fn display_type(self, db: &'db dyn Db) -> Option<ProtocolMemberType<'db>> {
        match self {
            Self::Type(member) => member.resolve(db),
            Self::Descriptor {
                domain: Some(domain),
                ..
            } => domain.resolve(db),
            Self::Descriptor { domain: None, .. } => Some(ProtocolMemberType::new(Type::unknown())),
        }
    }

    fn bind_requirement(
        self,
        db: &'db dyn Db,
        self_type: Type<'db>,
    ) -> Option<ProtocolMemberWriteRequirement<'db>> {
        match self {
            Self::Type(member) => Some(ProtocolMemberWriteRequirement::AssignableTo(
                member.bind_self(db, self_type)?,
            )),
            Self::Descriptor { descriptor, domain } => {
                Some(ProtocolMemberWriteRequirement::Descriptor {
                    descriptor_ty: descriptor.bind_self(db, self_type)?,
                    receiver_ty: self_type,
                    domain: domain.and_then(|domain| domain.bind_self(db, self_type)),
                })
            }
        }
    }

    fn bind_compatibility_type(self, db: &'db dyn Db, self_type: Type<'db>) -> Option<Type<'db>> {
        match self {
            Self::Type(member) => member.bind_self(db, self_type),
            Self::Descriptor { domain, .. } => Some(
                domain
                    .and_then(|domain| domain.bind_self(db, self_type))
                    .unwrap_or_else(Type::unknown),
            ),
        }
    }

    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        match (self, previous) {
            (Self::Type(current), Self::Type(previous)) => {
                Self::Type(current.cycle_normalized(db, previous, cycle))
            }
            (
                Self::Descriptor {
                    descriptor: current_descriptor,
                    domain: current_domain,
                },
                Self::Descriptor {
                    descriptor: previous_descriptor,
                    domain: previous_domain,
                },
            ) => Self::Descriptor {
                descriptor: current_descriptor.cycle_normalized(db, previous_descriptor, cycle),
                domain: cycle_normalized_optional_type(db, current_domain, previous_domain, cycle),
            },
            (current, _) => current,
        }
    }

    fn cycle_normalized_without_previous(self, db: &'db dyn Db, cycle: &salsa::Cycle) -> Self {
        let normalize = |member: ProtocolMemberType<'db>| {
            member.with_ty(member.ty().recursive_type_normalized(db, cycle))
        };
        match self {
            Self::Type(member) => Self::Type(normalize(member)),
            Self::Descriptor { descriptor, domain } => Self::Descriptor {
                descriptor: normalize(descriptor),
                domain: domain.map(normalize),
            },
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(match self {
            Self::Type(member) => {
                Self::Type(member.recursive_type_normalized_impl(db, div, nested)?)
            }
            Self::Descriptor { descriptor, domain } => Self::Descriptor {
                descriptor: descriptor.recursive_type_normalized_impl(db, div, nested)?,
                domain: match domain {
                    Some(domain) => Some(domain.recursive_type_normalized_impl(db, div, nested)?),
                    None => None,
                },
            },
        })
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::Type(member) => {
                Self::Type(member.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::Descriptor { descriptor, domain } => Self::Descriptor {
                descriptor: descriptor.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                domain: domain
                    .map(|domain| domain.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            },
        }
    }
}

impl<'db> VarianceInferable<'db> for ProtocolInterface<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarIdentity<'db>) -> TypeVarVariance {
        self.members(db)
            .flat_map(|member| {
                let capabilities = member.capabilities(db);
                [capabilities.instance, capabilities.class]
                    .into_iter()
                    .flat_map(|access| access.variances(db))
            })
            .map(|(ty, variance)| ty.with_polarity(variance).variance_of(db, typevar))
            .collect()
    }
}

/// A protocol member's exposed type and the context required to resolve it lazily.
///
/// Property accessors remain as callables until a relation needs their read or write type. Once
/// resolved, `Value` retains the accessor's binding context so that only its own `Self` type is
/// rebound during protocol checks.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMemberType<'db> {
    Value {
        ty: Type<'db>,
        // Accessor annotations can contain `Self` bound by the accessor definition. Retain that
        // context after reducing a property to its read/write types so relation checks only bind
        // the `Self` that belongs to this member.
        self_binding_context: Option<BindingContext<'db>>,
    },
    // Property accessors remain as raw callable types until a relation or ordinary member access
    // needs the exposed value type. Resolving every property while constructing a protocol
    // interface causes unrelated protocol checks to materialize large return-type unions.
    PropertyGetter(Type<'db>),
    PropertySetter(Type<'db>),
}

impl<'db> ProtocolMemberType<'db> {
    const fn new(ty: Type<'db>) -> Self {
        Self::Value {
            ty,
            self_binding_context: None,
        }
    }

    fn with_definition(ty: Type<'db>, definition: Option<Definition<'db>>) -> Self {
        Self::Value {
            ty,
            self_binding_context: definition.map(BindingContext::Definition),
        }
    }

    const fn property_getter(ty: Type<'db>) -> Self {
        Self::PropertyGetter(ty)
    }

    const fn property_setter(ty: Type<'db>) -> Self {
        Self::PropertySetter(ty)
    }

    const fn ty(self) -> Type<'db> {
        match self {
            Self::Value { ty, .. } | Self::PropertyGetter(ty) | Self::PropertySetter(ty) => ty,
        }
    }

    const fn with_ty(self, ty: Type<'db>) -> Self {
        match self {
            Self::Value {
                self_binding_context,
                ..
            } => Self::Value {
                ty,
                self_binding_context,
            },
            Self::PropertyGetter(_) => Self::PropertyGetter(ty),
            Self::PropertySetter(_) => Self::PropertySetter(ty),
        }
    }

    /// Resolves a stored property accessor to the value type exposed by that access.
    fn resolve(self, db: &'db dyn Db) -> Option<Self> {
        match self {
            Self::Value { .. } => Some(self),
            Self::PropertyGetter(getter) => property_get_member_type(db, getter),
            Self::PropertySetter(setter) => property_set_member_type(db, setter),
        }
    }

    /// Resolves this member type and binds member-local `Self` occurrences to `self_type`.
    fn bind_self(self, db: &'db dyn Db, self_type: Type<'db>) -> Option<Type<'db>> {
        let Self::Value {
            ty,
            self_binding_context,
        } = self.resolve(db)?
        else {
            return None;
        };
        if !ty.contains_self(db) {
            return Some(ty);
        }

        Some(ty.apply_type_mapping(
            db,
            &TypeMapping::BindSelf(SelfBinding::new(db, self_type, self_binding_context)),
            TypeContext::default(),
        ))
    }

    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        let ty = self.ty().cycle_normalized(db, previous.ty(), cycle);
        self.with_ty(ty)
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let ty = if nested {
            self.ty().recursive_type_normalized_impl(db, div, true)?
        } else {
            self.ty()
                .recursive_type_normalized_impl(db, div, true)
                .unwrap_or(div)
        };
        Some(self.with_ty(ty))
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let ty = self
            .ty()
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        self.with_ty(ty)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
/// The types supported by one way of accessing a protocol member.
///
/// `read` is covariant and `write` is contravariant. Either operation can be absent: for example,
/// an instance cannot write a `ClassVar`, while a normal instance attribute has no class access.
struct ProtocolMemberAccess<'db> {
    read: Option<ProtocolMemberType<'db>>,
    write: Option<ProtocolMemberWrite<'db>>,
}

impl<'db> ProtocolMemberAccess<'db> {
    const NONE: Self = Self {
        read: None,
        write: None,
    };

    const fn new(
        read: Option<ProtocolMemberType<'db>>,
        write: Option<ProtocolMemberWrite<'db>>,
    ) -> Self {
        Self { read, write }
    }

    fn variances(self, db: &'db dyn Db) -> impl Iterator<Item = (Type<'db>, TypeVarVariance)> {
        self.read
            .and_then(|member| member.resolve(db))
            .map(|member| (member.ty(), TypeVarVariance::Covariant))
            .into_iter()
            .chain(
                self.write
                    .and_then(ProtocolMemberWrite::domain)
                    .and_then(|member| member.resolve(db))
                    .map(|member| (member.ty(), TypeVarVariance::Contravariant)),
            )
    }
}

/// The readable and writable types exposed through instance and class access.
///
/// Instance access and class access each independently record readable and writable types. For
/// example, a mutable `ClassVar` is readable through both, writable through the class, and
/// read-only through an instance.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct ProtocolMemberCapabilities<'db> {
    instance: ProtocolMemberAccess<'db>,
    class: ProtocolMemberAccess<'db>,
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ProtocolMemberAccessMode {
    Instance,
    Class,
}

fn cycle_normalized_optional_type<'db>(
    db: &'db dyn Db,
    current: Option<ProtocolMemberType<'db>>,
    previous: Option<ProtocolMemberType<'db>>,
    cycle: &salsa::Cycle,
) -> Option<ProtocolMemberType<'db>> {
    match (current, previous) {
        (Some(current), Some(previous)) => Some(current.cycle_normalized(db, previous, cycle)),
        (Some(current), None) => {
            Some(current.with_ty(current.ty().recursive_type_normalized(db, cycle)))
        }
        (None, _) => None,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ProtocolMemberData<'db> {
    kind: ProtocolMemberKind<'db>,
    qualifiers: TypeQualifiers,
    definition: Option<Definition<'db>>,
}

impl<'db> ProtocolMemberData<'db> {
    fn method(
        db: &'db dyn Db,
        callable: CallableType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Self {
        let (method_kind, callable) = if callable.is_classmethod_like(db) {
            (
                ProtocolMethodKind::Class,
                protocol_bind_self(db, callable, None),
            )
        } else if callable.is_staticmethod_like(db) {
            (ProtocolMethodKind::Static, callable.into_regular(db))
        } else {
            (ProtocolMethodKind::Instance, callable)
        };

        Self {
            kind: ProtocolMemberKind::Method(
                ProtocolMemberType::with_definition(Type::Callable(callable), definition),
                method_kind,
            ),
            qualifiers: TypeQualifiers::default(),
            definition,
        }
    }

    fn property(
        read: Option<ProtocolMemberType<'db>>,
        write: Option<ProtocolMemberWrite<'db>>,
        definition: Option<Definition<'db>>,
    ) -> Self {
        Self {
            kind: ProtocolMemberKind::Property { read, write },
            qualifiers: TypeQualifiers::default(),
            definition,
        }
    }

    fn attribute(
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
        definition: Option<Definition<'db>>,
    ) -> Self {
        Self {
            kind: ProtocolMemberKind::Attribute(ProtocolMemberType::with_definition(
                ty, definition,
            )),
            qualifiers,
            definition,
        }
    }

    /// Derives the instance/class read/write capabilities exposed by this member.
    ///
    /// These are views of the canonical method, property, or attribute representation below;
    /// keeping them derived prevents the stored member kind and its capabilities from diverging.
    fn capabilities(&self, db: &'db dyn Db) -> ProtocolMemberCapabilities<'db> {
        match self.kind {
            ProtocolMemberKind::Method(member, kind) => {
                let instance_method = match (member.ty(), kind) {
                    (Type::Callable(callable), ProtocolMethodKind::Instance) => {
                        member.with_ty(Type::Callable(protocol_bind_self(db, callable, None)))
                    }
                    _ => member,
                };
                ProtocolMemberCapabilities {
                    instance: ProtocolMemberAccess::new(Some(instance_method), None),
                    class: ProtocolMemberAccess::new(Some(member), None),
                }
            }
            ProtocolMemberKind::Property { read, write } => ProtocolMemberCapabilities {
                instance: ProtocolMemberAccess::new(read, write),
                class: ProtocolMemberAccess::NONE,
            },
            ProtocolMemberKind::Attribute(member_ty) => {
                let is_class_var = self.qualifiers.contains(TypeQualifiers::CLASS_VAR);
                let is_final = self.qualifiers.contains(TypeQualifiers::FINAL);
                // A `Todo` records a protocol member form that is not modeled yet; do not infer a
                // write requirement from that temporary representation.
                let is_todo = member_ty.ty().is_todo();
                ProtocolMemberCapabilities {
                    instance: ProtocolMemberAccess::new(
                        Some(member_ty),
                        (!is_class_var && !is_final && !is_todo)
                            .then_some(ProtocolMemberWrite::from_type(member_ty)),
                    ),
                    class: if is_class_var {
                        ProtocolMemberAccess::new(
                            Some(member_ty),
                            (!is_final && !is_todo)
                                .then_some(ProtocolMemberWrite::from_type(member_ty)),
                        )
                    } else {
                        ProtocolMemberAccess::NONE
                    },
                }
            }
        }
    }

    fn cycle_normalized(&self, db: &'db dyn Db, previous: &Self, cycle: &salsa::Cycle) -> Self {
        Self {
            kind: self.kind.cycle_normalized(db, previous.kind, cycle),
            qualifiers: self.qualifiers,
            definition: self.definition,
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            kind: self.kind.recursive_type_normalized_impl(db, div, nested)?,
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
        _visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for member_type in self.kind.member_types() {
            member_type
                .ty()
                .find_legacy_typevars(db, binding_context, typevars);
        }
    }

    fn display(&self, db: &'db dyn Db) -> impl std::fmt::Display {
        struct ProtocolMemberDataDisplay<'db> {
            db: &'db dyn Db,
            kind: ProtocolMemberKind<'db>,
            qualifiers: TypeQualifiers,
        }

        impl std::fmt::Display for ProtocolMemberDataDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.kind {
                    ProtocolMemberKind::Method(member, _) => {
                        write!(f, "MethodMember(`{}`)", member.ty().display(self.db))
                    }
                    ProtocolMemberKind::Property { read, write } => {
                        let mut d = f.debug_struct("PropertyMember");
                        if let Some(read) = read.and_then(|read| read.resolve(self.db)) {
                            d.field("read", &format_args!("`{}`", read.ty().display(self.db)));
                        }
                        if let Some(write) = write.and_then(|write| write.display_type(self.db)) {
                            d.field("write", &format_args!("`{}`", write.ty().display(self.db)));
                        }
                        d.finish()
                    }
                    ProtocolMemberKind::Attribute(attribute) => {
                        f.write_str("AttributeMember(")?;
                        write!(f, "`{}`", attribute.ty().display(self.db))?;
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
            kind: self.kind,
            qualifiers: self.qualifiers,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMemberKind<'db> {
    Method(ProtocolMemberType<'db>, ProtocolMethodKind),
    Property {
        read: Option<ProtocolMemberType<'db>>,
        write: Option<ProtocolMemberWrite<'db>>,
    },
    Attribute(ProtocolMemberType<'db>),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMethodKind {
    Instance,
    Class,
    Static,
}

impl<'db> ProtocolMemberKind<'db> {
    fn member_types(self) -> impl Iterator<Item = ProtocolMemberType<'db>> {
        match self {
            Self::Method(method, _) => [Some(method), None, None],
            Self::Property { read, write } => [
                read,
                write.and_then(ProtocolMemberWrite::domain),
                write.and_then(ProtocolMemberWrite::descriptor_type),
            ],
            Self::Attribute(attribute) => [Some(attribute), None, None],
        }
        .into_iter()
        .flatten()
    }

    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        match (self, previous) {
            (Self::Method(current, kind), Self::Method(previous, _)) => {
                let (Type::Callable(current_callable), Type::Callable(previous_callable)) =
                    (current.ty(), previous.ty())
                else {
                    return Self::Method(current.cycle_normalized(db, previous, cycle), kind);
                };
                debug_assert_eq!(current_callable.kind(db), previous_callable.kind(db));
                let signatures = current_callable.signatures(db).cycle_normalized(
                    db,
                    previous_callable.signatures(db),
                    cycle,
                );
                Self::Method(
                    current.with_ty(Type::Callable(CallableType::new(
                        db,
                        signatures,
                        current_callable.kind(db),
                        current_callable.provenance(db),
                    ))),
                    kind,
                )
            }
            (
                Self::Property {
                    read: current_read,
                    write: current_write,
                },
                Self::Property {
                    read: previous_read,
                    write: previous_write,
                },
            ) => Self::Property {
                read: cycle_normalized_optional_type(db, current_read, previous_read, cycle),
                write: match (current_write, previous_write) {
                    (Some(current), Some(previous)) => {
                        Some(current.cycle_normalized(db, previous, cycle))
                    }
                    (Some(current), None) => {
                        Some(current.cycle_normalized_without_previous(db, cycle))
                    }
                    (None, _) => None,
                },
            },
            (Self::Attribute(current), Self::Attribute(previous)) => {
                Self::Attribute(current.cycle_normalized(db, previous, cycle))
            }
            (current, _) => current,
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(match self {
            Self::Method(member, kind) => Self::Method(
                member.recursive_type_normalized_impl(db, div, nested)?,
                kind,
            ),
            Self::Property { read, write } => Self::Property {
                read: match read {
                    Some(read) => Some(read.recursive_type_normalized_impl(db, div, nested)?),
                    None => None,
                },
                write: match write {
                    Some(write) => Some(write.recursive_type_normalized_impl(db, div, nested)?),
                    None => None,
                },
            },
            Self::Attribute(attribute) => {
                Self::Attribute(attribute.recursive_type_normalized_impl(db, div, nested)?)
            }
        })
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::Method(member, kind) => Self::Method(
                member.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                kind,
            ),
            Self::Property { read, write } => Self::Property {
                read: read.map(|read| read.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
                write: write
                    .map(|write| write.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            },
            Self::Attribute(attribute) => {
                Self::Attribute(attribute.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
        }
    }
}

/// A single member of a protocol interface.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProtocolMember<'a, 'db> {
    name: &'a str,
    data: &'a ProtocolMemberData<'db>,
}

/// Orders protocol members so that finite constraints are established before recursive relations.
///
/// The declaration order is significant because the derived ordering is used when comparing
/// protocol interfaces.
#[derive(Eq, Ord, PartialEq, PartialOrd)]
enum StructuralMemberPriority {
    /// A non-recursive member with at most one callable signature.
    Simple,
    /// A non-recursive callable member with multiple overloads.
    FiniteOverload,
    /// A member that may recurse through a protocol or type alias, or whose finiteness is unknown.
    Recursive,
}

fn walk_protocol_member<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    member: &ProtocolMember<'_, 'db>,
    visitor: &V,
) {
    for member_type in member.data.kind.member_types() {
        visitor.visit_type(db, member_type.ty());
    }
}

impl<'a, 'db> ProtocolMember<'a, 'db> {
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    pub(super) fn qualifiers(&self) -> TypeQualifiers {
        self.data.qualifiers
    }

    pub(super) fn is_method(&self) -> bool {
        matches!(self.data.kind, ProtocolMemberKind::Method(..))
    }

    /// Returns the priority for structurally comparing this member.
    ///
    /// Simple finite members are cheapest, followed by finite overloads. Recursive and
    /// alias-containing members are compared last because they can expand the same interface again.
    fn structural_member_priority(&self, db: &'db dyn Db) -> StructuralMemberPriority {
        let is_recursive_type = |ty| {
            any_over_type(db, ty, false, |nested| {
                matches!(nested, Type::ProtocolInstance(_) | Type::TypeAlias(_))
            })
        };

        let ProtocolMemberKind::Method(member, _) = self.data.kind else {
            let is_finite = self.data.kind.member_types().all(|member| {
                member
                    .resolve(db)
                    .is_some_and(|member| !is_recursive_type(member.ty()))
            });
            return if is_finite {
                StructuralMemberPriority::Simple
            } else {
                StructuralMemberPriority::Recursive
            };
        };
        let Type::Callable(callable) = member.ty() else {
            return StructuralMemberPriority::Recursive;
        };
        let signatures = callable.signatures(db);
        let finite_priority = match signatures.iter().len() {
            0 => return StructuralMemberPriority::Recursive,
            1 => StructuralMemberPriority::Simple,
            _ => StructuralMemberPriority::FiniteOverload,
        };

        let is_recursive = signatures.iter().any(|signature| {
            signature
                .receiver_constraint_types()
                .chain(
                    signature
                        .parameters()
                        .iter()
                        .skip(usize::from(
                            signature.has_implicit_positional_receiver_annotation(),
                        ))
                        .map(Parameter::annotated_type),
                )
                .chain(std::iter::once(signature.return_ty))
                .any(is_recursive_type)
        });
        if is_recursive {
            return StructuralMemberPriority::Recursive;
        }

        finite_priority
    }

    fn is_instance_method(&self) -> bool {
        matches!(
            self.data.kind,
            ProtocolMemberKind::Method(_, ProtocolMethodKind::Instance)
        )
    }

    /// Returns whether this member is dispatched through special-method lookup on the type.
    ///
    /// The names are the methods registered in CPython's `slotdefs` table or explicitly looked
    /// up on the type by Python or its standard library.
    fn uses_special_method_lookup(&self) -> bool {
        matches!(
            self.name,
            "__abs__"
                | "__add__"
                | "__aenter__"
                | "__aexit__"
                | "__aiter__"
                | "__and__"
                | "__anext__"
                | "__await__"
                | "__bool__"
                | "__buffer__"
                | "__bytes__"
                | "__call__"
                | "__ceil__"
                | "__complex__"
                | "__contains__"
                | "__copy__"
                | "__del__"
                | "__delattr__"
                | "__delete__"
                | "__delitem__"
                | "__dir__"
                | "__divmod__"
                | "__enter__"
                | "__eq__"
                | "__exit__"
                | "__float__"
                | "__floor__"
                | "__floordiv__"
                | "__format__"
                | "__fspath__"
                | "__ge__"
                | "__get__"
                | "__getattr__"
                | "__getattribute__"
                | "__getitem__"
                | "__getnewargs__"
                | "__getnewargs_ex__"
                | "__gt__"
                | "__hash__"
                | "__iadd__"
                | "__iand__"
                | "__ifloordiv__"
                | "__ilshift__"
                | "__imatmul__"
                | "__imod__"
                | "__imul__"
                | "__index__"
                | "__init__"
                | "__instancecheck__"
                | "__int__"
                | "__invert__"
                | "__ior__"
                | "__ipow__"
                | "__irshift__"
                | "__isub__"
                | "__iter__"
                | "__itruediv__"
                | "__ixor__"
                | "__le__"
                | "__len__"
                | "__length_hint__"
                | "__lshift__"
                | "__lt__"
                | "__matmul__"
                | "__missing__"
                | "__mod__"
                | "__mul__"
                | "__ne__"
                | "__neg__"
                | "__new__"
                | "__next__"
                | "__or__"
                | "__pos__"
                | "__pow__"
                | "__radd__"
                | "__rand__"
                | "__rdivmod__"
                | "__release_buffer__"
                | "__replace__"
                | "__repr__"
                | "__reversed__"
                | "__rfloordiv__"
                | "__rlshift__"
                | "__rmatmul__"
                | "__rmod__"
                | "__rmul__"
                | "__ror__"
                | "__round__"
                | "__rpow__"
                | "__rrshift__"
                | "__rshift__"
                | "__rsub__"
                | "__rtruediv__"
                | "__rxor__"
                | "__set__"
                | "__set_name__"
                | "__setattr__"
                | "__setitem__"
                | "__sizeof__"
                | "__str__"
                | "__sub__"
                | "__subclasscheck__"
                | "__truediv__"
                | "__trunc__"
                | "__xor__"
        )
    }

    fn is_class_method(&self) -> bool {
        matches!(
            self.data.kind,
            ProtocolMemberKind::Method(_, ProtocolMethodKind::Class)
        )
    }

    fn is_property(&self) -> bool {
        matches!(self.data.kind, ProtocolMemberKind::Property { .. })
    }

    pub(super) fn definition(&self) -> Option<Definition<'db>> {
        self.data.definition
    }

    fn capabilities(&self, db: &'db dyn Db) -> ProtocolMemberCapabilities<'db> {
        self.data.capabilities(db)
    }

    /// Returns the accesses that a candidate value must provide for this member.
    ///
    /// A module-level callable can satisfy an ordinary or static method through direct member
    /// access. A class object can likewise satisfy a class, static, or ordinary instance method;
    /// special instance methods instead use special-method lookup through the meta-type. Neither
    /// case needs a separate class-side check for the same member.
    fn implementation_capabilities(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> ProtocolMemberCapabilities<'db> {
        let capabilities = self.capabilities(db);
        if matches!(
            (ty, self.data.kind),
            (
                Type::ModuleLiteral(_),
                ProtocolMemberKind::Method(
                    _,
                    ProtocolMethodKind::Instance | ProtocolMethodKind::Static
                )
            )
        ) || (is_class_object_type(ty) && self.is_method())
        {
            ProtocolMemberCapabilities {
                class: ProtocolMemberAccess::NONE,
                ..capabilities
            }
        } else {
            capabilities
        }
    }

    fn meta_access(&self, db: &'db dyn Db) -> Option<ProtocolMemberAccess<'db>> {
        if self.has_todo_type() {
            return None;
        }
        Some(self.capabilities(db).class)
    }

    fn has_todo_type(&self) -> bool {
        self.data
            .kind
            .member_types()
            .any(|ty| matches!(ty, ProtocolMemberType::Value { ty, .. } if ty.is_todo()))
    }
}

fn property_get_member_type<'db>(
    db: &'db dyn Db,
    getter: Type<'db>,
) -> Option<ProtocolMemberType<'db>> {
    let mut get_types = Vec::new();
    let mut definition = None;
    for callable in &getter.try_upcast_to_callable(db)? {
        for signature in callable.signatures(db) {
            get_types.push(signature.return_ty);
            definition = definition.or(signature.definition());
        }
    }
    Some(ProtocolMemberType::with_definition(
        UnionType::from_elements(db, get_types),
        definition,
    ))
}

fn property_set_member_type<'db>(
    db: &'db dyn Db,
    setter: Type<'db>,
) -> Option<ProtocolMemberType<'db>> {
    let mut set_types = Vec::new();
    let mut definition = None;
    for callable in &setter.try_upcast_to_callable(db)? {
        for signature in callable.signatures(db) {
            set_types.push(signature.parameters().get_positional(1)?.annotated_type());
            definition = definition.or(signature.definition());
        }
    }
    Some(ProtocolMemberType::with_definition(
        UnionType::from_elements(db, set_types),
        definition,
    ))
}

/// Derive the observable instance capabilities of a descriptor-decorated protocol member.
fn descriptor_decorated_protocol_member<'db>(
    db: &'db dyn Db,
    descriptor_ty: Type<'db>,
    protocol: ClassType<'db>,
    definition: Option<Definition<'db>>,
) -> Option<ProtocolMemberData<'db>> {
    let descriptor_ty = descriptor_ty.resolve_type_alias(db);

    // Applying a generic descriptor decorator to a method that refers to an enclosing type
    // variable can currently materialize that variable as `Unknown`. Reducing the descriptor to
    // its `__get__` result would then erase the remaining descriptor structure and weaken the
    // protocol member to a bare `Unknown`.
    if super::visitor::any_over_type(db, descriptor_ty, false, |ty| ty.is_unknown()) {
        return None;
    }

    let Place::Defined(DefinedPlace {
        definedness: Definedness::AlwaysDefined,
        ..
    }) = descriptor_ty
        .class_member_with_policy(db, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
    else {
        return None;
    };

    let receiver_ty = Type::instance(db, protocol);
    let (read_ty, _) =
        descriptor_ty.try_call_dunder_get(db, Some(receiver_ty), receiver_ty.to_meta_type(db))?;
    let read = Some(ProtocolMemberType::with_definition(read_ty, definition));

    let write = match descriptor_setter_domain(db, descriptor_ty, receiver_ty) {
        DescriptorSetterDomain::Missing => None,
        DescriptorSetterDomain::Known(domain) => Some(ProtocolMemberWrite::descriptor(
            descriptor_ty,
            Some(domain),
            definition,
        )),
        DescriptorSetterDomain::Deferred => Some(ProtocolMemberWrite::descriptor(
            descriptor_ty,
            None,
            definition,
        )),
    };

    Some(ProtocolMemberData::property(read, write, definition))
}

#[derive(Copy, Clone)]
enum DescriptorSetterDomain<'db> {
    Missing,
    Known(Type<'db>),
    Deferred,
}

/// Derive the values accepted by every possible descriptor setter when they fit in [`Type`].
fn descriptor_setter_domain<'db>(
    db: &'db dyn Db,
    descriptor_ty: Type<'db>,
    receiver_ty: Type<'db>,
) -> DescriptorSetterDomain<'db> {
    match descriptor_ty {
        Type::Union(union) => {
            let mut write_types = Vec::with_capacity(union.elements(db).len());
            for descriptor_ty in union.elements(db) {
                match single_descriptor_setter_domain(db, *descriptor_ty, receiver_ty) {
                    DescriptorSetterDomain::Missing => return DescriptorSetterDomain::Missing,
                    DescriptorSetterDomain::Known(write_ty) => write_types.push(write_ty),
                    DescriptorSetterDomain::Deferred => return DescriptorSetterDomain::Deferred,
                }
            }
            IntersectionType::bounded_from_elements(db, write_types).map_or(
                DescriptorSetterDomain::Deferred,
                DescriptorSetterDomain::Known,
            )
        }
        _ => single_descriptor_setter_domain(db, descriptor_ty, receiver_ty),
    }
}

/// Derive the values accepted by one possible runtime descriptor.
fn single_descriptor_setter_domain<'db>(
    db: &'db dyn Db,
    descriptor_ty: Type<'db>,
    receiver_ty: Type<'db>,
) -> DescriptorSetterDomain<'db> {
    let Place::Defined(DefinedPlace {
        ty: setter_ty,
        definedness: Definedness::AlwaysDefined,
        ..
    }) = descriptor_ty
        .member_lookup_with_policy(
            db,
            "__set__",
            MemberLookupPolicy::REQUIRE_CONCRETE | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )
        .place
    else {
        return DescriptorSetterDomain::Missing;
    };

    let Some(callables) = setter_ty.try_upcast_to_callable(db) else {
        return DescriptorSetterDomain::Deferred;
    };
    let mut callable_domains = Vec::with_capacity(callables.iter().len());
    for callable in &callables {
        let mut write_types = Vec::new();
        for signature in callable.signatures(db) {
            match descriptor_setter_signature_domain(db, signature, descriptor_ty, receiver_ty) {
                DescriptorSetterSignatureDomain::Inapplicable => {}
                DescriptorSetterSignatureDomain::Known(write_ty) => write_types.push(write_ty),
                DescriptorSetterSignatureDomain::Deferred => {
                    return DescriptorSetterDomain::Deferred;
                }
            }
        }
        callable_domains.push(UnionType::from_elements(db, write_types));
    }
    IntersectionType::bounded_from_elements(db, callable_domains).map_or(
        DescriptorSetterDomain::Deferred,
        DescriptorSetterDomain::Known,
    )
}

enum DescriptorSetterSignatureDomain<'db> {
    Inapplicable,
    Known(Type<'db>),
    Deferred,
}

/// Derive the values accepted by one `__set__` overload when they fit in [`Type`].
fn descriptor_setter_signature_domain<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    descriptor_ty: Type<'db>,
    receiver_ty: Type<'db>,
) -> DescriptorSetterSignatureDomain<'db> {
    let parameters = signature.parameters();
    let missing_required_parameter = || {
        if parameters.is_gradual() || parameters.as_slice().iter().any(Parameter::is_variadic) {
            DescriptorSetterSignatureDomain::Deferred
        } else {
            DescriptorSetterSignatureDomain::Inapplicable
        }
    };
    let Some(trailing_parameters) = parameters.as_slice().get(2..) else {
        return missing_required_parameter();
    };
    if !trailing_parameters.iter().all(|parameter| {
        parameter.default_type().is_some()
            || ((parameters.is_standard() || parameters.is_gradual())
                && (parameter.is_variadic() || parameter.is_keyword_variadic()))
    }) {
        return DescriptorSetterSignatureDomain::Inapplicable;
    }

    let Some(receiver_parameter) = parameters.get_positional(0) else {
        return missing_required_parameter();
    };
    let receiver_parameter = receiver_parameter
        .annotated_type()
        .bind_self_typevars(db, descriptor_ty);
    if contains_signature_typevar(db, signature, receiver_parameter) {
        return DescriptorSetterSignatureDomain::Deferred;
    }
    if !receiver_ty.is_assignable_to(db, receiver_parameter) {
        return DescriptorSetterSignatureDomain::Inapplicable;
    }

    let Some(write_parameter) = parameters.get_positional(1) else {
        return missing_required_parameter();
    };
    let write_ty = write_parameter
        .annotated_type()
        .bind_self_typevars(db, descriptor_ty);
    if !contains_signature_typevar(db, signature, write_ty) {
        return DescriptorSetterSignatureDomain::Known(write_ty);
    }

    let Type::TypeVar(typevar) = write_ty else {
        return DescriptorSetterSignatureDomain::Deferred;
    };
    let Some(generic_context) = signature.generic_context else {
        return DescriptorSetterSignatureDomain::Deferred;
    };
    if !generic_context.contains(db, typevar.identity(db))
        || !typevar
            .binding_context(db)
            .definition()
            .is_some_and(|definition| definition.kind(db).is_function_def())
    {
        return DescriptorSetterSignatureDomain::Deferred;
    }

    match typevar.typevar(db).bound_or_constraints(db) {
        None => DescriptorSetterSignatureDomain::Known(Type::object()),
        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
            DescriptorSetterSignatureDomain::Known(bound.bind_self_typevars(db, descriptor_ty))
        }
        Some(TypeVarBoundOrConstraints::Constraints(_)) => {
            DescriptorSetterSignatureDomain::Deferred
        }
    }
}

fn contains_signature_typevar<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    ty: Type<'db>,
) -> bool {
    signature.generic_context.is_some_and(|generic_context| {
        super::visitor::any_over_type(db, ty, true, |ty| {
            matches!(ty, Type::TypeVar(typevar) if generic_context.contains(db, typevar.identity(db)))
        })
    })
}

fn property_set_type<'db>(
    db: &'db dyn Db,
    property: PropertyInstanceType<'db>,
    receiver_ty: Type<'db>,
) -> Option<Type<'db>> {
    property_set_member_type(db, property.setter(db)?)?.bind_self(db, receiver_ty)
}

fn is_class_object_type(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
    )
}

fn protocol_member_read_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    receiver_ty: Type<'db>,
    member: &ProtocolMember<'_, 'db>,
    access: ProtocolMemberAccessMode,
) -> Option<Type<'db>> {
    // A callback protocol describes call syntax. Use the candidate's callable type instead of an
    // explicitly resolved `__call__` attribute, which can differ for class objects.
    if access == ProtocolMemberAccessMode::Instance
        && member.is_method()
        && member.name == "__call__"
    {
        return Some(ty);
    }

    // Module-level functions and ordinary methods on class objects are matched through direct
    // member access. Special instance methods still use special-method lookup on the meta-type.
    let place = if access == ProtocolMemberAccessMode::Instance
        && member.is_instance_method()
        && !matches!(ty, Type::ModuleLiteral(_))
        && (!is_class_object_type(ty) || member.uses_special_method_lookup())
    {
        Type::invoke_descriptor_protocol(
            db,
            MemberLookupKey::new(
                db,
                ty,
                member.name,
                // The undefined fallback excludes instance members. Keep the class
                // member lookup from reintroducing dynamic instance fallbacks.
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            ),
            ty,
            Place::Undefined.into(),
            InstanceFallbackShadowsNonDataDescriptor::No,
        )
        .place
    } else {
        receiver_ty.member(db, member.name).place
    };

    match place {
        Place::Defined(DefinedPlace {
            ty: attribute_type,
            definedness: Definedness::AlwaysDefined,
            ..
        }) => Some(attribute_type),
        _ => None,
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Checks a synthetic protocol-member write using normal attribute-assignment lookup.
    ///
    /// Resolution is shared with real assignments, but this path evaluates the result using the
    /// active type relation and constraints instead of inferring an expression or emitting an
    /// assignment diagnostic.
    fn check_property_write(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        member_name: &str,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let requirement = attribute_write_requirement(db, ty, member_name);
        self.check_property_write_requirement(db, &requirement, member_name, value_ty)
    }

    fn check_property_write_requirement(
        &self,
        db: &'db dyn Db,
        requirement: &AttributeWriteRequirement<'db>,
        member_name: &str,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match requirement {
            AttributeWriteRequirement::All { element_tys, .. } => {
                let mut result = self.always();
                for element_ty in *element_tys {
                    let requirement = attribute_write_requirement(db, *element_ty, member_name);
                    let element_result = self.check_property_write_requirement(
                        db,
                        &requirement,
                        member_name,
                        value_ty,
                    );
                    result = result.and(db, self.constraints, || element_result);
                    if result.is_trivially_never_satisfied() {
                        break;
                    }
                }
                result
            }
            AttributeWriteRequirement::Any { intersection, .. } => {
                let mut result = self.never();
                for element_ty in intersection.positive(db) {
                    let requirement = attribute_write_requirement(db, *element_ty, member_name);
                    let element_result = self.check_property_write_requirement(
                        db,
                        &requirement,
                        member_name,
                        value_ty,
                    );
                    result = result.or(db, self.constraints, || element_result);
                    if result.is_trivially_always_satisfied() {
                        break;
                    }
                }
                result
            }
            AttributeWriteRequirement::Unconstrained => self.always(),
            AttributeWriteRequirement::CannotAssign => self.never(),
            AttributeWriteRequirement::Module(Some(write_ty)) => {
                self.check_type_pair(db, value_ty, *write_ty)
            }
            AttributeWriteRequirement::ProtocolMember {
                write: Some(ProtocolMemberWriteRequirement::AssignableTo(write_ty)),
                ..
            } => self.check_type_pair(db, value_ty, *write_ty),
            AttributeWriteRequirement::ProtocolMember {
                write: Some(ProtocolMemberWriteRequirement::Descriptor { domain, .. }),
                ..
            } => self.check_type_pair(db, value_ty, domain.unwrap_or_else(Type::unknown)),
            AttributeWriteRequirement::Module(None)
            | AttributeWriteRequirement::ProtocolMember { write: None, .. } => self.never(),
            AttributeWriteRequirement::Instance { object_ty, member } => {
                self.check_instance_property_write(db, *object_ty, member, member_name, value_ty)
            }
            AttributeWriteRequirement::Class { object_ty, member } => {
                self.check_class_property_write(db, *object_ty, member, value_ty)
            }
        }
    }

    fn check_instance_property_write(
        &self,
        db: &'db dyn Db,
        object_ty: Type<'db>,
        member: &InstanceAttributeWriteMember<'db>,
        member_name: &str,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let setattr_result = object_ty.try_call_dunder_with_policy(
            db,
            "__setattr__",
            &mut CallArguments::positional([Type::string_literal(db, member_name), value_ty]),
            TypeContext::default(),
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
        );
        if match &setattr_result {
            Ok(bindings) => bindings.return_type(db).is_never(),
            Err(error) => error.return_type(db).is_some_and(|ty| ty.is_never()),
        } {
            return self.never();
        }

        match member {
            InstanceAttributeWriteMember::ClassVar => self.never(),
            InstanceAttributeWriteMember::Explicit { member, fallback } => {
                let member_result =
                    self.check_explicit_property_write(db, object_ty, member, value_ty);
                if let Some(fallback) = fallback {
                    let fallback_result =
                        self.check_fallback_property_write(db, fallback, value_ty);
                    member_result.and(db, self.constraints, || fallback_result)
                } else {
                    member_result
                }
            }
            InstanceAttributeWriteMember::Instance(fallback) => {
                self.check_fallback_property_write(db, fallback, value_ty)
            }
            InstanceAttributeWriteMember::SetAttr => {
                if !matches!(
                    setattr_result,
                    Ok(_) | Err(CallDunderError::PossiblyUnbound { .. })
                ) {
                    return self.never();
                }
                self.check_setattr_property_write(db, object_ty, value_ty)
            }
        }
    }

    fn check_class_property_write(
        &self,
        db: &'db dyn Db,
        object_ty: Type<'db>,
        member: &ClassAttributeWriteMember<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match member {
            ClassAttributeWriteMember::Explicit { member, fallback } => {
                let member_result =
                    self.check_explicit_property_write(db, object_ty, member, value_ty);
                if member_result.is_trivially_never_satisfied() {
                    return member_result;
                }
                if let Some(fallback) = fallback {
                    let fallback_result =
                        self.check_fallback_property_write(db, fallback, value_ty);
                    member_result.and(db, self.constraints, || fallback_result)
                } else {
                    member_result
                }
            }
            ClassAttributeWriteMember::ClassAttribute(fallback) => {
                self.check_fallback_property_write(db, fallback, value_ty)
            }
            ClassAttributeWriteMember::Unresolved { .. } => self.never(),
        }
    }

    fn check_explicit_property_write(
        &self,
        db: &'db dyn Db,
        object_ty: Type<'db>,
        requirement: &ExplicitAttributeWriteRequirement<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if requirement.qualifiers().contains(TypeQualifiers::FINAL) {
            return self.never();
        }
        match requirement {
            ExplicitAttributeWriteRequirement::Descriptor {
                descriptor_ty,
                setter_ty,
                ..
            } => {
                if let Some(property) = descriptor_ty.as_property_instance()
                    && let Some(set_type) = property_set_type(db, property, object_ty)
                {
                    return self.check_type_pair(db, value_ty, set_type);
                }
                self.check_descriptor_property_write(
                    db,
                    *descriptor_ty,
                    *setter_ty,
                    object_ty,
                    value_ty,
                )
            }
            ExplicitAttributeWriteRequirement::AssignableTo { ty, .. } => {
                self.check_type_pair(db, value_ty, *ty)
            }
        }
    }

    fn check_descriptor_property_write(
        &self,
        db: &'db dyn Db,
        descriptor_ty: Type<'db>,
        setter_ty: Type<'db>,
        object_ty: Type<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if setter_ty
            .try_call(
                db,
                &CallArguments::positional([descriptor_ty, object_ty, Type::unknown()]),
            )
            .is_err()
        {
            return self.never();
        }

        self.check_callable_write_parameter(db, setter_ty, 2, descriptor_ty, value_ty)
    }

    fn check_setattr_property_write(
        &self,
        db: &'db dyn Db,
        object_ty: Type<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let Place::Defined(DefinedPlace { ty: setattr_ty, .. }) = object_ty
            .member_lookup_with_policy(
                db,
                "__setattr__",
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
        else {
            return self.never();
        };

        self.check_callable_write_parameter(db, setattr_ty, 1, object_ty, value_ty)
    }

    fn check_callable_write_parameter(
        &self,
        db: &'db dyn Db,
        callable_ty: Type<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let Type::Union(union) = value_ty {
            return union
                .elements(db)
                .iter()
                .when_all(db, self.constraints, |value_ty| {
                    self.check_callable_write_parameter(
                        db,
                        callable_ty,
                        parameter_index,
                        self_ty,
                        *value_ty,
                    )
                });
        }

        callable_ty
            .try_upcast_to_callable_with_policy(db, UpcastPolicy::from(self.relation))
            .when_some_and(db, self.constraints, |callables| {
                callables.iter().when_all(db, self.constraints, |callable| {
                    callable.signatures(db).into_iter().when_any(
                        db,
                        self.constraints,
                        |signature| {
                            let parameters = signature.parameters();
                            parameters
                                .get_positional(parameter_index)
                                .or_else(|| {
                                    parameters.variadic().and_then(|(index, parameter)| {
                                        (index <= parameter_index).then_some(parameter)
                                    })
                                })
                                .map(|parameter| {
                                    parameter.annotated_type().bind_self_typevars(db, self_ty)
                                })
                                .when_some_and(db, self.constraints, |write_ty| {
                                    self.check_type_pair(db, value_ty, write_ty)
                                })
                        },
                    )
                })
            })
    }

    fn check_fallback_property_write(
        &self,
        db: &'db dyn Db,
        requirement: &FallbackAttributeWriteRequirement<'db>,
        value_ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match requirement {
            FallbackAttributeWriteRequirement::AssignableTo { ty, qualifiers, .. } => {
                if qualifiers.contains(TypeQualifiers::FINAL) {
                    self.never()
                } else {
                    self.check_type_pair(db, value_ty, *ty)
                }
            }
            FallbackAttributeWriteRequirement::PossiblyMissing => self.always(),
        }
    }

    fn check_protocol_member_read(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        receiver_ty: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        required_ty: ProtocolMemberType<'db>,
        access: ProtocolMemberAccessMode,
    ) -> ConstraintSet<'db, 'c> {
        let Some(attribute_type) = protocol_member_read_type(db, ty, receiver_ty, member, access)
        else {
            return self.never();
        };

        // `Self` in a protocol member names the value satisfying the protocol. `Self` in a
        // method on a class object names instances of that class: a `@classmethod` returning
        // `Self` returns `Factory`, not `type[Factory]`. Keep the bindings separate so a method
        // that returns an instance cannot satisfy a protocol that promises the class object.
        let protocol_self_binding_ty = ty.literal_fallback_instance(db).unwrap_or(ty);
        let implementation_self_binding_ty = ty
            .to_instance_approximation(db)
            .or_else(|| ty.literal_fallback_instance(db))
            .unwrap_or(ty);
        let implementation_receiver_binding_ty = if member.is_class_method() {
            implementation_self_binding_ty.to_meta_type(db)
        } else {
            implementation_self_binding_ty
        };
        let protocol_receiver_binding_ty = if member.is_class_method() {
            protocol_self_binding_ty.to_meta_type(db)
        } else {
            protocol_self_binding_ty
        };

        // Checking a class object against a protocol's instance capabilities can expose the
        // property descriptor itself rather than the value returned by its getter. Compatibility
        // for properties on class objects is not yet modeled; retain the previous name-only
        // behavior until generic upper-bound solving can handle the large recursive unions this
        // otherwise creates.
        if member.is_property() && matches!(attribute_type, Type::PropertyInstance(_)) {
            return self.always();
        }

        if member.is_method() && access == ProtocolMemberAccessMode::Instance {
            let Some(required_ty) = required_ty.resolve(db) else {
                return self.never();
            };
            let Type::Callable(required_callable) = required_ty.ty() else {
                return self.never();
            };
            attribute_type
                .try_upcast_to_callable_with_policy(db, UpcastPolicy::from(self.relation))
                .when_some_and(db, self.constraints, |callables| {
                    self.check_callables_vs_callable(
                        db,
                        &callables.map(|callable| {
                            callable.apply_self_with_receiver(
                                db,
                                implementation_receiver_binding_ty,
                                implementation_self_binding_ty,
                            )
                        }),
                        required_callable.apply_self_with_receiver(
                            db,
                            protocol_receiver_binding_ty,
                            protocol_self_binding_ty,
                        ),
                    )
                })
        } else if member.is_instance_method() {
            let Some(required_ty) = required_ty.resolve(db) else {
                return self.never();
            };
            let Type::Callable(required_callable) = required_ty.ty() else {
                return self.never();
            };
            attribute_type
                .try_upcast_to_callable_with_policy(db, UpcastPolicy::from(self.relation))
                .when_some_and(db, self.constraints, |callables| {
                    callables.iter().when_all(db, self.constraints, |callable| {
                        if callable.is_function_like(db) {
                            self.check_callable_pair(
                                db,
                                callable.bind_self(db, Some(implementation_self_binding_ty)),
                                protocol_bind_self(
                                    db,
                                    required_callable,
                                    Some(protocol_self_binding_ty),
                                ),
                            )
                        } else {
                            self.check_callable_pair(db, *callable, required_callable)
                        }
                    })
                })
        } else if member.is_method() {
            let Some(required_ty) = required_ty.resolve(db) else {
                return self.never();
            };
            let Type::Callable(required_callable) = required_ty.ty() else {
                return self.never();
            };
            self.check_type_pair(
                db,
                attribute_type,
                Type::Callable(required_callable.apply_self_with_receiver(
                    db,
                    protocol_receiver_binding_ty,
                    protocol_self_binding_ty,
                )),
            )
        } else {
            required_ty
                .bind_self(db, protocol_self_binding_ty)
                .when_some_and(db, self.constraints, |required_ty| {
                    let result = self.check_type_pair(db, attribute_type, required_ty);
                    if let Some(context) = self.report_context()
                        && result.is_never_satisfied(db)
                    {
                        context.push(ErrorContext::ProtocolMemberReadTypeIncompatible {
                            source: attribute_type,
                            target: required_ty,
                        });
                    }
                    result
                })
        }
    }

    /// Checks the read and write capabilities required through instance access or class access.
    ///
    /// Reads are checked covariantly and writes contravariantly. For ordinary methods, the
    /// instance-side signature check is authoritative and class access only establishes presence.
    fn type_satisfies_protocol_member_access(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        receiver_ty: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        required: ProtocolMemberAccess<'db>,
        access: ProtocolMemberAccessMode,
    ) -> ConstraintSet<'db, 'c> {
        if access == ProtocolMemberAccessMode::Class
            && member.is_instance_method()
            && required.read.is_some()
        {
            // The instance-side check is authoritative for the signature of a method
            // implementation. Class access only establishes that the member is present. Callable
            // types and several callable literal forms do not expose a useful `__call__` member
            // through their meta-type.
            return ConstraintSet::from_bool(
                self.constraints,
                member.name == "__call__"
                    || protocol_member_read_type(
                        db,
                        ty,
                        receiver_ty,
                        member,
                        ProtocolMemberAccessMode::Class,
                    )
                    .is_some(),
            );
        }

        let read_result = required.read.map_or_else(
            || self.always(),
            |required_ty| {
                self.check_protocol_member_read(db, ty, receiver_ty, member, required_ty, access)
            },
        );

        read_result.and(db, self.constraints, || {
            required.write.map_or_else(
                || self.always(),
                |write| {
                    let fallback_ty = ty.literal_fallback_instance(db).unwrap_or(ty);
                    let receiver_ty = if access == ProtocolMemberAccessMode::Instance
                        && matches!(ty, Type::LiteralValue(_))
                    {
                        fallback_ty
                    } else {
                        receiver_ty
                    };
                    write
                        .bind_compatibility_type(db, fallback_ty)
                        .when_some_and(db, self.constraints, |write_ty| {
                            let result =
                                self.check_property_write(db, receiver_ty, member.name, write_ty);
                            if let Some(context) = self.report_context()
                                && result.is_never_satisfied(db)
                            {
                                context.push(ErrorContext::ProtocolMemberWriteTypeIncompatible {
                                    target: write_ty,
                                });
                            }
                            result
                        })
                },
            )
        })
    }

    /// Return `true` if `ty` provides every access required by this protocol member.
    pub(super) fn type_satisfies_protocol_member(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
    ) -> ConstraintSet<'db, 'c> {
        let capabilities = member.implementation_capabilities(db, ty);
        if let Some(context) = self.report_context() {
            let instance_read_missing = capabilities.instance.read.is_some()
                && protocol_member_read_type(
                    db,
                    ty,
                    ty,
                    member,
                    ProtocolMemberAccessMode::Instance,
                )
                .is_none();
            let class_read_missing = capabilities.class.read.is_some()
                && !(member.is_instance_method() && member.name == "__call__")
                && protocol_member_read_type(
                    db,
                    ty,
                    ty.to_meta_type(db),
                    member,
                    ProtocolMemberAccessMode::Class,
                )
                .is_none();
            if instance_read_missing || class_read_missing {
                if instance_read_missing
                    && is_class_object_type(ty)
                    && member.is_instance_method()
                    && member.uses_special_method_lookup()
                {
                    context.push(ErrorContext::ProtocolSpecialMethodNotDefinedOnMetaType);
                }
                context.push(ErrorContext::ProtocolMemberNotDefined {
                    member_name: member.name.into(),
                    ty,
                });
                return self.never();
            }
        }

        let result = self
            .type_satisfies_protocol_member_access(
                db,
                ty,
                ty,
                member,
                capabilities.instance,
                ProtocolMemberAccessMode::Instance,
            )
            .and(db, self.constraints, || {
                self.type_satisfies_protocol_member_access(
                    db,
                    ty,
                    ty.to_meta_type(db),
                    member,
                    capabilities.class,
                    ProtocolMemberAccessMode::Class,
                )
            });
        if let Some(context) = self.report_context()
            && result.is_never_satisfied(db)
        {
            context.push(ErrorContext::ProtocolMemberIncompatible {
                member_name: member.name.into(),
            });
        }
        result
    }

    /// Checks the members that a class object must provide to inhabit `type[Protocol]`.
    ///
    /// Ordinary instance attributes and properties are deliberately absent from this check. They
    /// are requirements on the object produced by constructing the class, not on the class object
    /// itself. `ClassVar`s and methods are checked through class access; unlike ordinary protocol
    /// matching, method access compares the unbound signature instead of checking only presence.
    pub(super) fn check_meta_protocol_members(
        &self,
        db: &'db dyn Db,
        instance_ty: Type<'db>,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        protocol
            .interface(db)
            .members(db)
            .when_all(db, self.constraints, |member| {
                let required = member.capabilities(db).class;
                if required.read.is_none() && required.write.is_none() {
                    return self.always();
                }

                let result = if member.is_method() {
                    required.read.map_or_else(
                        || self.always(),
                        |required_ty| {
                            self.check_protocol_member_read(
                                db,
                                instance_ty,
                                meta_ty,
                                &member,
                                required_ty,
                                ProtocolMemberAccessMode::Class,
                            )
                        },
                    )
                } else {
                    self.type_satisfies_protocol_member_access(
                        db,
                        instance_ty,
                        meta_ty,
                        &member,
                        required,
                        ProtocolMemberAccessMode::Class,
                    )
                };

                if let Some(context) = self.report_context()
                    && result.is_never_satisfied(db)
                {
                    context.push(ErrorContext::ProtocolMemberIncompatible {
                        member_name: member.name.into(),
                    });
                }
                result
            })
    }

    /// Compares either instance access or class access when relating two protocol members.
    ///
    /// Both members bind `Self` to the source protocol type; readable types are compared
    /// covariantly and writable types contravariantly.
    fn check_protocol_member_access_pair(
        &self,
        db: &'db dyn Db,
        source_type: Type<'db>,
        source_member: &ProtocolMember<'_, 'db>,
        target_member: &ProtocolMember<'_, 'db>,
        access: ProtocolMemberAccessMode,
    ) -> ConstraintSet<'db, 'c> {
        let source_capabilities = source_member.capabilities(db);
        let target_capabilities = target_member.capabilities(db);

        if access == ProtocolMemberAccessMode::Class
            && source_member.is_method()
            && target_member.is_instance_method()
        {
            // The instance-side check is authoritative for an ordinary method's signature. Class
            // access only establishes that the source member is also present on the class.
            return ConstraintSet::from_bool(
                self.constraints,
                source_capabilities.class.read.is_some(),
            );
        }

        let (source, target) = match access {
            ProtocolMemberAccessMode::Instance => {
                (source_capabilities.instance, target_capabilities.instance)
            }
            ProtocolMemberAccessMode::Class => {
                (source_capabilities.class, target_capabilities.class)
            }
        };

        let read_result = match (source.read, target.read) {
            (_, None) => self.always(),
            (None, Some(_)) => self.never(),
            (Some(source), Some(target)) => {
                let bind_read = |member_type: ProtocolMemberType<'db>,
                                 member: &ProtocolMember<'_, 'db>| {
                    let member_type = member_type.resolve(db)?;
                    if member.is_method()
                        && let Type::Callable(callable) = member_type.ty()
                    {
                        Some(Type::Callable(callable.apply_self(db, source_type)))
                    } else {
                        member_type.bind_self(db, source_type)
                    }
                };
                let (Some(source), Some(target)) = (
                    bind_read(source, source_member),
                    bind_read(target, target_member),
                ) else {
                    return self.never();
                };
                let result = self.check_type_pair(db, source, target);
                if let Some(context) = self.report_context()
                    && !target_member.is_method()
                    && result.is_never_satisfied(db)
                {
                    context
                        .push(ErrorContext::ProtocolMemberReadTypeIncompatible { source, target });
                }
                result
            }
        };

        read_result.and(db, self.constraints, || {
            match (source.write, target.write) {
                (_, None) => self.always(),
                (None, Some(_)) => {
                    if let Some(context) = self.report_context() {
                        context.push(ErrorContext::ProtocolMemberNotWritable);
                    }
                    self.never()
                }
                (Some(source), Some(target)) => {
                    let (Some(target), Some(source)) = (
                        target.bind_compatibility_type(db, source_type),
                        source.bind_compatibility_type(db, source_type),
                    ) else {
                        return self.never();
                    };
                    let result = self.check_type_pair(db, target, source);
                    if let Some(context) = self.report_context()
                        && result.is_never_satisfied(db)
                    {
                        context.push(ErrorContext::ProtocolMemberWriteTypeIncompatible { target });
                    }
                    result
                }
            }
        })
    }

    pub(super) fn check_protocol_interface_pair(
        &self,
        db: &'db dyn Db,
        source_type: Type<'db>,
        source: ProtocolInterface<'db>,
        target: ProtocolInterface<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if source.member_count(db) < target.member_count(db)
            && !self.is_context_collection_enabled()
        {
            return self.never();
        }

        target
            .members(db)
            .sorted_by_cached_key(|member| member.structural_member_priority(db))
            .when_all(db, self.constraints, |target_member| {
                let source_member = source.member_by_name(db, target_member.name);

                if let Some(context) = self.report_context()
                    && source_member.is_none()
                {
                    context.push(ErrorContext::ProtocolMemberNotDefined {
                        member_name: target_member.name.into(),
                        ty: source_type,
                    });
                    return self.never();
                }

                let result = source_member.when_some_and(db, self.constraints, |source_member| {
                    self.check_protocol_member_access_pair(
                        db,
                        source_type,
                        &source_member,
                        &target_member,
                        ProtocolMemberAccessMode::Instance,
                    )
                    .and(db, self.constraints, || {
                        self.check_protocol_member_access_pair(
                            db,
                            source_type,
                            &source_member,
                            &target_member,
                            ProtocolMemberAccessMode::Class,
                        )
                    })
                });
                if let Some(context) = self.report_context()
                    && result.is_never_satisfied(db)
                {
                    context.push(ErrorContext::ProtocolMemberIncompatible {
                        member_name: target_member.name.into(),
                    });
                }
                result
            })
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    /// Conservatively proves that `ty` lacks an instance write required by `member`.
    ///
    /// This currently recognizes only a concrete read-only property. Unknown or unresolved write
    /// behavior is not sufficient to prove disjointness.
    pub(super) fn protocol_member_write_is_definitely_missing_from_ty(
        &self,
        db: &'db dyn Db,
        member: &ProtocolMember<'_, 'db>,
        ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if member.capabilities(db).instance.write.is_none() {
            return self.never();
        }

        let Place::Defined(DefinedPlace {
            ty: Type::PropertyInstance(actual_property),
            definedness: Definedness::AlwaysDefined,
            ..
        }) = ty.class_member(db, member.name()).place
        else {
            return self.never();
        };

        ConstraintSet::from_bool(self.constraints, actual_property.setter(db).is_none())
    }

    /// Checks whether `ty` is disjoint from the readable type required by `member`.
    ///
    /// Method members are compared conservatively through their non-`Never` return types rather
    /// than their full callable signatures.
    pub(super) fn protocol_member_has_disjoint_type_from_ty(
        &self,
        db: &'db dyn Db,
        member: &ProtocolMember<'_, 'db>,
        ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // An unbound property descriptor does not establish that the value returned by its
        // getter is disjoint from the required property type.
        if member.is_property() && matches!(ty, Type::PropertyInstance(_)) {
            return self.never();
        }
        let capabilities = member.capabilities(db);
        if !member.is_method() {
            capabilities
                .instance
                .read
                .when_some_and(db, self.constraints, |read_ty| {
                    read_ty
                        .resolve(db)
                        .when_some_and(db, self.constraints, |read_ty| {
                            self.check_type_pair(db, ty, read_ty.ty())
                        })
                })
        } else {
            let Some(Type::Callable(method)) = capabilities
                .instance
                .read
                .and_then(|read| read.resolve(db))
                .map(ProtocolMemberType::ty)
            else {
                return self.never();
            };
            if !callable_has_only_non_never_returns(db, method) {
                return self.never();
            }

            let Some(callables) = ty.try_upcast_to_callable_with_policy(db, UpcastPolicy::Sound)
            else {
                return self.never();
            };

            callables.iter().when_all(db, self.constraints, |callable| {
                if !callable_has_only_non_never_returns(db, *callable) {
                    return self.never();
                }

                // Disjointness distributes over unions. Compare the overload return arms
                // directly so that recursive return types do not require canonicalizing an
                // intermediate union merely to distribute it again.
                method
                    .signatures(db)
                    .iter()
                    .when_all(db, self.constraints, |method_signature| {
                        callable.signatures(db).iter().when_all(
                            db,
                            self.constraints,
                            |callable_signature| {
                                self.check_type_pair(
                                    db,
                                    method_signature.return_ty,
                                    callable_signature.return_ty,
                                )
                            },
                        )
                    })
            })
        }
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
}

#[derive(Debug, Copy, Clone)]
struct ProtocolMemberCandidate<'db> {
    ty: Type<'db>,
    qualifiers: TypeQualifiers,
    definition: Option<Definition<'db>>,
    bound_on_class: BoundOnClass,
}

impl<'db> ProtocolMemberCandidate<'db> {
    fn apply_specialization(
        mut self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        self.ty = self.ty.apply_optional_specialization(db, specialization);
        self
    }

    fn is_bound_method_like(self, db: &'db dyn Db) -> bool {
        self.bound_on_class.is_yes()
            && match self.ty {
                Type::FunctionLiteral(_) => true,
                Type::Callable(callable) => callable.is_method_like(db),
                _ => false,
            }
    }

    fn walk_recursive_member_types<V: super::visitor::TypeVisitor<'db> + ?Sized>(
        self,
        db: &'db dyn Db,
        visitor: &V,
    ) {
        match self.ty {
            Type::PropertyInstance(property) => {
                // A property exposes its getter return and setter value types. Walking the
                // accessor callables themselves would also visit their receiver and make every
                // generic protocol property appear recursive.
                for member in [
                    property.getter(db).map(ProtocolMemberType::property_getter),
                    property.setter(db).map(ProtocolMemberType::property_setter),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(member) = member.resolve(db) {
                        visitor.visit_type(db, member.ty());
                    }
                }
            }
            _ if self.is_bound_method_like(db) => {}
            _ => visitor.visit_type(db, self.ty),
        }
    }
}

/// Inner Salsa query for [`ProtocolClass::interface`].
#[salsa::tracked(
    returns(copy),
    cycle_initial=|db, _, _| ProtocolInterface::empty(db),
    cycle_fn=proto_interface_cycle_recover,
    heap_size=ruff_memory_usage::heap_size,
)]
fn cached_protocol_interface<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    let mut members = BTreeMap::default();

    ProtocolClass(class).for_each_member_candidate(db, |name, candidate, specialization| {
        if members.contains_key(name) {
            return;
        }

        let candidate = candidate.apply_specialization(db, specialization);
        let ProtocolMemberCandidate {
            ty,
            qualifiers,
            definition,
            bound_on_class,
        } = candidate;

        let member = match ty {
            Type::PropertyInstance(property) => ProtocolMemberData::property(
                property.getter(db).map(ProtocolMemberType::property_getter),
                property
                    .setter(db)
                    .map(ProtocolMemberType::property_setter)
                    .map(ProtocolMemberWrite::from_type),
                definition,
            ),
            Type::Callable(callable) if bound_on_class.is_yes() && callable.is_method_like(db) => {
                ProtocolMemberData::method(db, callable, definition)
            }
            Type::FunctionLiteral(function)
                if bound_on_class.is_yes()
                    || function.is_staticmethod(db)
                    || function.is_classmethod(db) =>
            {
                ProtocolMemberData::method(db, function.into_callable_type(db), definition)
            }
            _ if bound_on_class.is_yes()
                && definition.is_some_and(|definition| definition.kind(db).is_function_def()) =>
            {
                if let Some(descriptor) =
                    descriptor_decorated_protocol_member(db, ty, class, definition)
                {
                    descriptor
                } else {
                    ProtocolMemberData::attribute(ty, qualifiers, definition)
                }
            }
            _ => ProtocolMemberData::attribute(ty, qualifiers, definition),
        };

        members.insert(name.clone(), member);
    });

    ProtocolInterface::new(db, members)
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn proto_interface_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &ProtocolInterface<'db>,
    value: ProtocolInterface<'db>,
    _class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    value.cycle_normalized(db, *previous, cycle)
}

/// Bind `self` unless this is a `Callable[P, R]` dunder, and *also* discard the functionlike-ness
/// of the callable.
///
/// This additional upcasting is required in order for protocols with `__call__` method
/// members to be considered assignable to `Callable` types, since the `Callable` supertype
/// of the `__call__` method will be function-like but a `Callable` type is not.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn protocol_bind_self<'db>(
    db: &'db dyn Db,
    callable: CallableType<'db>,
    self_type: Option<Type<'db>>,
) -> CallableType<'db> {
    callable.bind_self(db, self_type).into_regular(db)
}

/// Return `true` if a callable has at least one overload and none return `Never`.
///
/// Return-type disjointness is a pragmatic approximation for method members: a callable returning
/// `Never` could satisfy otherwise-incompatible signatures, so it must not establish disjointness.
fn callable_has_only_non_never_returns<'db>(db: &'db dyn Db, callable: CallableType<'db>) -> bool {
    let mut signatures = callable.signatures(db).iter();
    let Some(first) = signatures.next() else {
        // An empty signature previously produced `Unknown`, which cannot establish disjointness.
        return false;
    };

    !first.return_ty.resolve_type_alias(db).is_never()
        && signatures.all(|signature| !signature.return_ty.resolve_type_alias(db).is_never())
}

/// Protocol compatibility can only succeed if every required member is present.
///
/// Check that necessary condition up front so we can avoid expensive per-member type
/// comparisons and generic protocol solving when the actual type is plainly missing a member.
pub(super) fn has_all_protocol_members_defined<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
) -> bool {
    let target_interface = protocol.interface(db);

    match ty {
        Type::ProtocolInstance(source_protocol) => {
            let source_interface = source_protocol.interface(db);

            source_interface.member_count(db) >= target_interface.member_count(db)
                && target_interface
                    .members(db)
                    .all(|member| source_interface.includes_member(db, member.name()))
        }
        _ => target_interface.members(db).all(|member| {
            matches!(
                ty.member(db, member.name()).place,
                Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                })
            )
        }),
    }
}
