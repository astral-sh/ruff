use crate::{Program, ProgramEnvironment};
use std::fmt::Write;
use std::{collections::BTreeMap, ops::Deref};

use itertools::Itertools;

use ruff_python_ast::name::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::types::attribute_write::{
    DescriptorSetterDomain, ProtocolMemberWriteRequirement, descriptor_setter_domain,
    property_setter_value_type,
};
use crate::types::call::CallArguments;
use crate::types::member_observation::{
    MemberLookupDemand, MemberLookupOptions, ObservedMember, bind_descriptor, lookup_member,
    lookup_member_with_options,
};
use crate::types::overrides::{VariableKind, effective_superclass_variable_kind};
use crate::types::projection::{
    CallableSelfBinding, ObservationEdge, ObservedType, ObservedTypePair,
};
use crate::types::recursive::RecursiveOperation;
use crate::types::relation::{
    DisjointnessChecker, RelationContext, TypeRelation, TypeRelationChecker,
};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{TypeContext, TypeNormalization, UpcastPolicy};
use crate::{
    Db, FxOrderSet,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, place_from_declarations,
    },
    types::{
        ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
        CallableType, ClassBase, ClassLiteral, ClassType, ErrorContext, FindLegacyTypeVarsVisitor,
        GenericAlias, GenericContext, KnownFunction, KnownInstanceType, MaterializationKind,
        MemberLookupPolicy, Parameter, ProtocolInstanceType, SelfBinding, Signature,
        StaticClassLiteral, Type, TypeMapping, TypeQualifiers, TypeVarVariance, UnionType,
        VarianceInferable, VarianceTerm,
        constraints::{
            ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension,
            OptionConstraintsExtension,
        },
        context::InferContext,
        diagnostic::{INVALID_PROTOCOL, report_undeclared_protocol_member},
        generics::{ApplySpecialization, Specialization},
        member::class_member,
        signatures::CallableSignature,
        variance::infer_protocol_variance,
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

    /// Expose one structural layer of the protocol while retaining its nominal identity.
    ///
    /// The recursive constructor requests this body with identity arguments. References to
    /// protocol instances in its members remain closed recursive applications, so substituting
    /// the constructor's arguments does not expand those members again.
    pub(super) fn recursive_body(self, db: &'db dyn Db) -> Type<'db> {
        Type::ProtocolInstance(ProtocolInstanceType::from_interface(
            db,
            self,
            self.interface(db),
        ))
    }

    /// Structural variance inference currently excludes recursive type aliases and descriptor
    /// writes whose accepted values cannot be represented by a single type, leaving no write
    /// domain to use contravariantly.
    ///
    /// TODO: Support recursive type aliases and descriptor writes with unrepresentable domains.
    pub(super) fn supports_variance_inference(self, db: &'db dyn Db) -> bool {
        self.static_class_literal(db)
            .is_some_and(|(class, _)| supports_protocol_variance_inference(db, class))
    }

    /// Returns the interface before an invariant specialization is materialized.
    ///
    /// A materialized generic origin retains its specialization for nominal identity and display.
    /// Building member requirements from that specialization, however, would first materialize an
    /// invariant type variable as a read and then reuse that result as its write. Strip only the
    /// pending marker while constructing the shared interface so reads and writes can each apply
    /// the original materialization in their own variance position.
    pub(super) fn unmaterialized_interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        let ClassType::Generic(alias) = *self else {
            return self.interface(db);
        };
        let specialization = alias.specialization(db);
        if specialization.materialization_kind(db).is_none() {
            return self.interface(db);
        }

        let alias = GenericAlias::new(
            db,
            alias.origin(db),
            specialization.with_materialization_kind(db, None),
        );
        ProtocolClass(ClassType::Generic(alias)).interface(db)
    }

    /// Walk the effective member types declared by this protocol, including method signatures.
    ///
    /// A method can introduce recursive applications through its parameters, return type, or
    /// explicit receiver annotation. Its local type parameters retain their own scope while the
    /// recursive-constructor analysis records those references.
    pub(super) fn walk_recursive_member_types<V: super::visitor::TypeVisitor<'db> + ?Sized>(
        self,
        db: &'db dyn Db,
        visitor: &V,
    ) {
        let mut seen_members = FxHashSet::default();

        self.for_each_member_candidate(
            db,
            visitor.program_environment(),
            |name, candidate, specialization| {
                if !seen_members.insert(name.clone()) {
                    return;
                }
                let candidate = candidate.apply_specialization(
                    db,
                    visitor.program_environment(),
                    specialization,
                );
                candidate.walk_recursive_member_types(db, visitor);
            },
        );
    }

    /// Visits protocol member candidates in MRO order after applying declaration precedence.
    ///
    /// Consumers discard shadowed names before applying the accompanying specialization.
    fn for_each_member_candidate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
            for (symbol_id, _) in use_def_map.all_end_of_scope_symbol_bindings() {
                let name = place_table.symbol(symbol_id).name();
                // Defaults retain inherited annotations, just as they do for ordinary classes.
                let member = class_member(db, parent_scope, name).inner;
                if let Place::Defined(place) = member.place {
                    direct_members.insert(
                        symbol_id,
                        ProtocolMemberCandidate {
                            ty: place.ty,
                            qualifiers: member.qualifiers,
                            definition: place.provenance.definition(),
                            bound_on_class: BoundOnClass::Yes,
                        },
                    );
                }
            }

            for (symbol_id, declarations) in use_def_map.all_end_of_scope_symbol_declarations() {
                let place_result = place_from_declarations(db, env, declarations)
                    .with_imported_final(
                        db,
                        env,
                        use_def_map.end_of_scope_imported_final_candidates(symbol_id.into()),
                    );
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
        let Some((class, _)) = self.static_class_literal(db) else {
            return false;
        };
        let env = ProgramEnvironment::from_scope(class.body_scope(db));
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
                    &env,
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

    /// Validate explicitly declared type-variable variance against this protocol's interface.
    pub(super) fn validate_type_parameter_variance(self, context: &InferContext) {
        if !context.is_lint_enabled(&INVALID_PROTOCOL) {
            return;
        }

        let db = context.db();
        let Some((class, _)) = self.static_class_literal(db) else {
            return;
        };
        // TODO: Validate protocols with inherited members too. This single-base pattern skips
        // subclasses such as `class Child(Base[T], Protocol[T])`, even when their declared
        // variance disagrees with the inherited interface.
        let [Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(generic_context))] =
            class.explicit_bases(db)
        else {
            return;
        };
        if class.has_pep_695_type_params(db) || class.try_mro(db, None).is_err() {
            return;
        }
        let env = ProgramEnvironment::from_scope(class.body_scope(db));
        if generic_context.variables(db).any(|typevar| {
            typevar.is_typevartuple(db) || typevar.typevar(db).default_type(db, &env).is_some()
        }) {
            return;
        }
        let Some(protocol) = class.identity_specialization(db).into_protocol_class(db) else {
            return;
        };
        if !protocol.supports_variance_inference(db) {
            return;
        }

        for typevar in generic_context.variables(db) {
            if typevar.is_paramspec(db) {
                continue;
            }

            let Some(declared_variance) = typevar.typevar(db).explicit_variance(db) else {
                continue;
            };

            let inferred_variance =
                match infer_protocol_variance(db, class, typevar.identity(db), declared_variance) {
                    TypeVarVariance::Bivariant => TypeVarVariance::Covariant,
                    variance => variance,
                };

            if inferred_variance == declared_variance {
                continue;
            }

            if let Some(builder) = context.report_lint(&INVALID_PROTOCOL, class.header_range(db)) {
                builder.into_diagnostic(format_args!(
                    "Type variable `{}` in protocol `{}` should be {}, but is {}",
                    typevar.typevar(db).name(db),
                    self.name(db),
                    inferred_variance.as_str(),
                    declared_variance.as_str(),
                ));
            }
        }
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self(
            self.0
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self(
            self.0
                .recursive_type_normalized_impl(db, env, div, nested)?,
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
    #[returns(copy)]
    pub(super) program: Program<'db>,

    #[returns(ref)]
    inner: BTreeMap<Name, ProtocolMemberData<'db>>,
}

impl get_size2::GetSize for ProtocolInterface<'_> {}

/// Ordered transformations of a protocol's exposed member types. Materialization is applied
/// after binding a method's receiver, while later substitutions remain outside that operation.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub(super) struct ProtocolInterfaceOperations<'db> {
    #[returns(ref)]
    operations: Box<[RecursiveOperation<'db>]>,
}

impl get_size2::GetSize for ProtocolInterfaceOperations<'_> {}

impl<'db> ProtocolInterfaceOperations<'db> {
    pub(super) fn append(
        db: &'db dyn Db,
        previous: Option<Self>,
        operation: RecursiveOperation<'db>,
    ) -> Self {
        let mut operations =
            previous.map_or_else(Vec::new, |previous| previous.operations(db).to_vec());
        if let Some(previous) = previous
            && let RecursiveOperation::Materialize(_, bounds) = operation
            && let Some(RecursiveOperation::Materialize(_, previous_bounds)) = operations.last()
            && (!bounds || *previous_bounds)
        {
            return previous;
        }
        operations.push(operation);
        Self::new(db, operations.into_boxed_slice())
    }

    pub(super) fn terminal_materialization(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self.operations(db).last() {
            Some(RecursiveOperation::Materialize(kind, _)) => Some(*kind),
            _ => None,
        }
    }

    pub(super) fn requires_replay(self, db: &'db dyn Db) -> bool {
        !matches!(
            self.operations(db).as_ref(),
            [] | [RecursiveOperation::Materialize(_, true)]
        )
    }

    pub(super) fn map_types(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.operations(db)
                .iter()
                .map(|operation| operation.map_types(db, mapping, visitor))
                .collect::<Box<[_]>>(),
        )
    }

    fn without_materialization(self, db: &'db dyn Db) -> Option<Self> {
        let operations: Box<[_]> = self
            .operations(db)
            .iter()
            .filter(|operation| !matches!(operation, RecursiveOperation::Materialize(..)))
            .cloned()
            .collect();
        (!operations.is_empty()).then(|| Self::new(db, operations))
    }

    fn apply(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut ty: Type<'db>,
        write: bool,
    ) -> Type<'db> {
        for operation in self.operations(db) {
            let mut visitor =
                ApplyTypeMappingVisitor::new(env).with_normalization(TypeNormalization::Structural);
            if let RecursiveOperation::Materialize(_, bounds) = operation {
                visitor.materialize_typevar_bounds_and_defaults = *bounds;
            }
            ty = operation.with_mapping(|mapping| {
                let mapping = if write { mapping.flip() } else { mapping };
                ty.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor)
            });
        }
        ty
    }
}

/// A protocol interface together with the materialization applied to its requirements.
///
/// The original interface remains shared. A member's readable and writable types are
/// materialized only when that member is accessed or compared.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ProtocolInterfaceView<'db> {
    interface: ProtocolInterface<'db>,
    materialization: Option<MaterializationKind>,
    operations: Option<ProtocolInterfaceOperations<'db>>,
}

impl<'db> ProtocolInterfaceView<'db> {
    pub(super) const fn new(
        interface: ProtocolInterface<'db>,
        materialization: Option<MaterializationKind>,
    ) -> Self {
        Self {
            interface,
            materialization,
            operations: None,
        }
    }

    pub(super) const fn with_operations(
        mut self,
        operations: Option<ProtocolInterfaceOperations<'db>>,
    ) -> Self {
        self.operations = operations;
        self
    }

    pub(super) const fn with_base(mut self, interface: ProtocolInterface<'db>) -> Self {
        self.interface = interface;
        self
    }

    pub(super) const fn base(self) -> ProtocolInterface<'db> {
        self.interface
    }

    /// Select a declaration expression for a protocol read or write. Resolving descriptors,
    /// binding receivers, and applying ordered operations belong to the observing caller;
    /// this selector only identifies the finite member expression that produces that result.
    pub(super) fn observation_child(
        self,
        db: &'db dyn Db,
        edge: &ObservationEdge,
    ) -> Option<Type<'db>> {
        let (name, class_access, write) = match edge {
            ObservationEdge::ProtocolMemberRead { name, class_access } => {
                (name, *class_access, false)
            }
            ObservationEdge::ProtocolMemberWrite { name, class_access } => {
                (name, *class_access, true)
            }
            _ => return None,
        };
        let member = self.member_by_name(db, name)?;
        let mode = if class_access {
            ProtocolMemberAccessMode::Class
        } else {
            ProtocolMemberAccessMode::Instance
        };
        let access = member.access(mode);
        if write {
            let write = access.write()?.declaration;
            write
                .domain()
                .or_else(|| write.descriptor_type())
                .map(ProtocolPropertyType::ty)
        } else {
            access.read()?;
            match member.data.kind {
                ProtocolMemberKind::Method(ty, kind) => {
                    if let Type::Callable(callable) = ty
                        && (kind == ProtocolMethodKind::Class
                            || (kind == ProtocolMethodKind::Instance
                                && mode == ProtocolMemberAccessMode::Instance))
                    {
                        Some(Type::Callable(protocol_bind_self(
                            db,
                            self.interface.program(db),
                            callable,
                            None,
                            None,
                        )))
                    } else {
                        Some(ty)
                    }
                }
                ProtocolMemberKind::Attribute { read, .. } => Some(read.ty),
                ProtocolMemberKind::Property { read, .. } => read.map(ProtocolPropertyType::ty),
            }
        }
    }

    pub(super) fn members<'a>(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = ProtocolMember<'a, 'db>>
    where
        'db: 'a,
    {
        self.interface
            .inner(db)
            .iter()
            .map(move |(name, data)| ProtocolMember {
                name,
                data,
                materialization: self.materialization,
                operations: self.operations,
            })
    }

    pub(super) fn member_count(self, db: &'db dyn Db) -> usize {
        self.interface.member_count(db)
    }

    pub(super) fn has_only_methods(self, db: &'db dyn Db) -> bool {
        self.members(db).all(|member| member.is_method())
    }

    /// Returns whether structural comparison can avoid recursive member expansion.
    pub(super) fn has_only_finite_members(self, db: &'db dyn Db) -> bool {
        let env = ProgramEnvironment::from_program(self.interface.program(db));
        self.members(db).all(|member| {
            !matches!(
                member.structural_member_priority(db, &env),
                StructuralMemberPriority::Recursive
            )
        })
    }

    pub(super) fn member_by_name<'a>(
        self,
        db: &'db dyn Db,
        name: &'a str,
    ) -> Option<ProtocolMember<'a, 'db>> {
        self.interface
            .inner(db)
            .get(name)
            .map(|data| ProtocolMember {
                name,
                data,
                materialization: self.materialization,
                operations: self.operations,
            })
    }

    pub(super) fn includes_member(self, db: &'db dyn Db, name: &str) -> bool {
        self.interface.includes_member(db, name)
    }

    /// Includes inherited `object` members that are guaranteed for every instance.
    ///
    /// Subclasses can disable `__hash__`, and slotted instances can omit the `__dict__` that
    /// typeshed declares on `object`.
    fn includes_member_or_object_fallback(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> bool {
        self.includes_member(db, name)
            || !matches!(name, "__hash__" | "__dict__")
                && object_member_names(db, self.interface.program(db)).contains(name)
                && matches!(
                    Type::object().member(db, env, name).place,
                    Place::Defined(place) if place.is_definitely_defined()
                )
    }

    /// Compare the original and materialized forms of members required by `required`.
    ///
    /// An unrelated materialized member must not prevent a protocol from retaining its
    /// nominal relationship to one of its bases.
    pub(super) fn differs_for_members_required_by(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        required: Self,
    ) -> bool {
        required.members(db).any(|required_member| {
            let Some(materialized) = self.member_by_name(db, required_member.name()) else {
                return false;
            };
            let original = ProtocolMember {
                name: materialized.name,
                data: materialized.data,
                materialization: None,
                operations: materialized
                    .operations
                    .and_then(|operations| operations.without_materialization(db)),
            };

            if materialized
                .access(ProtocolMemberAccessMode::Instance)
                .materialized_types(db, env)
                != original
                    .access(ProtocolMemberAccessMode::Instance)
                    .materialized_types(db, env)
            {
                return true;
            }

            // Class access to an ordinary instance method requires only that the method
            // exists. Its unbound `self` is not part of structural compatibility and can
            // recursively refer to this protocol, so do not materialize that signature.
            if materialized.is_instance_method() {
                return false;
            }

            materialized
                .access(ProtocolMemberAccessMode::Class)
                .materialized_types(db, env)
                != original
                    .access(ProtocolMemberAccessMode::Class)
                    .materialized_types(db, env)
        })
    }

    /// Returns the declared instance-write requirement for a protocol member.
    ///
    /// `None` means that the protocol does not declare `name`; `Some((None, _))` means that the
    /// member exists but is read-only. A writable member's requirement is bound to `receiver_ty`
    /// before it is returned.
    pub(super) fn instance_write_requirement_in_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: &ObservedType<'db>,
        name: &str,
        context: &RelationContext<'db>,
    ) -> Option<(Option<ProtocolMemberWriteRequirement<'db>>, TypeQualifiers)> {
        self.member_by_name(db, name).map(|member| {
            let declaration = receiver
                .project(
                    db,
                    env,
                    member.write_observation_edge(ProtocolMemberAccessMode::Instance),
                )
                .unwrap_or_else(|| receiver.unresolved());
            (
                member
                    .access(ProtocolMemberAccessMode::Instance)
                    .write()
                    .and_then(|write| {
                        write.requirement_in_context(
                            db,
                            env,
                            Some(receiver.ty),
                            &declaration,
                            context,
                        )
                    }),
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
        env: &ProgramEnvironment<'db>,
        receiver_ty: Type<'db>,
        name: &str,
    ) -> Option<(Option<Type<'db>>, TypeQualifiers)> {
        self.member_by_name(db, name).map(|member| {
            (
                member
                    .access(ProtocolMemberAccessMode::Class)
                    .write()
                    .and_then(|write| write.requirement(db, env, Some(receiver_ty)))
                    .and_then(|requirement| requirement.accepted_type()),
                member.qualifiers(),
            )
        })
    }

    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        self.instance_member_impl(db, env, None, name)
    }

    /// Read a member of a concrete protocol instance and retain its receiver obligation.
    pub(super) fn instance_member_with_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        self.instance_member_impl(db, env, Some(receiver), name)
    }

    /// Read the selected declaration under the same proof that selected the protocol receiver.
    pub(super) fn instance_member_in_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        self_type: Option<Type<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
        context: &RelationContext<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let Some(member) = self.member_by_name(db, name) else {
            let object = input.unchanged_or_unresolved(Type::object());
            let binding = if input.ty == receiver.ty {
                &object
            } else {
                receiver
            };
            return lookup_member(db, env, &object, binding, name, policy, context)
                .map_or_else(|| Place::Undefined.into(), |member| member.place(db));
        };
        let edge = member.read_observation_edge(ProtocolMemberAccessMode::Instance);
        let declaration = input
            .project(db, env, edge)
            .unwrap_or_else(|| input.unresolved());
        PlaceAndQualifiers {
            place: member
                .access(ProtocolMemberAccessMode::Instance)
                .read()
                .and_then(|read| {
                    read.result_type_in_context(db, env, None, self_type, &declaration, context)
                })
                .map(Place::bound)
                .unwrap_or(Place::Undefined)
                .with_provenance(Provenance::from_definition(member.definition())),
            qualifiers: member.qualifiers(),
        }
    }

    fn instance_member_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Option<Type<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        self.member_by_name(db, name)
            .map(|member| PlaceAndQualifiers {
                place: member
                    .access(ProtocolMemberAccessMode::Instance)
                    .read()
                    .and_then(|read| read.result_type(db, env, receiver))
                    .map(Place::bound)
                    .unwrap_or(Place::Undefined)
                    .with_provenance(Provenance::from_definition(member.definition())),
                qualifiers: member.qualifiers(),
            })
            .unwrap_or_else(|| Type::object().member(db, env, name))
    }

    /// Looks up a member guaranteed to exist on every inhabitant of `type[Protocol]`.
    ///
    /// Methods retain their unbound signatures and `ClassVar`s retain their class-side types.
    /// Properties are only required on the constructed instance, so they are undefined even when
    /// the nominal protocol origin provides a property descriptor.
    pub(super) fn meta_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver_type: Type<'db>,
        name: &str,
    ) -> Option<PlaceAndQualifiers<'db>> {
        self.member_by_name(db, name).map(|member| {
            let access = member.access(ProtocolMemberAccessMode::Class);
            PlaceAndQualifiers {
                place: access
                    .read()
                    .and_then(|read| {
                        read.result_type_with_receiver(db, env, Some(receiver_type), None)
                    })
                    .map(Place::bound)
                    .unwrap_or(Place::Undefined)
                    .with_provenance(Provenance::from_definition(member.definition())),
                qualifiers: member.qualifiers(),
            }
        })
    }

    pub(super) fn member_is_property(self, db: &'db dyn Db, name: &str) -> bool {
        self.member_by_name(db, name)
            .is_some_and(|member| member.is_property())
    }
}

pub(super) fn walk_protocol_interface<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    interface: ProtocolInterfaceView<'db>,
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
///
/// If a property's exposed type cannot be extracted, visit its accessor callable instead.
/// Extraction can fail for valid signatures, such as setters that accept the value via `*args`.
pub(super) fn walk_protocol_instance_interface<
    'db,
    V: super::visitor::TypeVisitor<'db> + ?Sized,
>(
    db: &'db dyn Db,
    interface: ProtocolInterfaceView<'db>,
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
    let env = visitor.program_environment();
    match member.data.kind {
        ProtocolMemberKind::Method(method, kind) => {
            let method = if let Type::Callable(callable) = method {
                let signatures = CallableSignature::from_overloads(
                    callable.signatures(db).iter().map(|signature| {
                        if signature.has_implicit_positional_receiver_annotation()
                            && kind != ProtocolMethodKind::Static
                        {
                            let runtime_type = if kind == ProtocolMethodKind::Class {
                                receiver_ty.to_meta_type(db, env)
                            } else {
                                receiver_ty
                            };
                            signature.bind_self_with_receiver(
                                db,
                                env,
                                Some(runtime_type),
                                Some(receiver_ty),
                            )
                        } else {
                            signature.clone()
                        }
                    }),
                );
                Type::Callable(callable.with_signatures(db, signatures))
            } else {
                method
            };
            visitor.visit_type(
                db,
                member
                    .access(ProtocolMemberAccessMode::Instance)
                    .materialize_type(db, env, method),
            );
        }
        ProtocolMemberKind::Property { .. } => {
            walk_protocol_member_access(
                db,
                member.access(ProtocolMemberAccessMode::Instance),
                Some(receiver_ty),
                visitor,
            );
        }
        ProtocolMemberKind::Attribute { .. } => {
            for mode in [
                ProtocolMemberAccessMode::Instance,
                ProtocolMemberAccessMode::Class,
            ] {
                walk_protocol_member_access(db, member.access(mode), Some(receiver_ty), visitor);
            }
        }
    }
}

impl<'db> ProtocolInterface<'db> {
    /// Synthesize a new protocol interface with the given members.
    ///
    /// All created members will be covariant, read-only property members
    /// rather than method members or mutable attribute members.
    pub(super) fn with_property_members<'a, M>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: M,
    ) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        let members: BTreeMap<_, _> = members
            .into_iter()
            .map(|(name, ty)| {
                (
                    Name::new(name),
                    ProtocolMemberData::property(Some(ProtocolPropertyType::new(ty)), None, None),
                )
            })
            .collect();
        Self::new(db, env.program(db), members)
    }

    /// Synthesize a new protocol interface with the given methods.
    pub(super) fn with_methods<'a, M>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: M,
    ) -> Self
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
        Self::new(db, env.program(db), members)
    }

    fn empty(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self::new(db, env.program(db), BTreeMap::default())
    }

    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        let prev_inner = previous.inner(db);
        let curr_inner = self.inner(db);

        let members: BTreeMap<_, _> = curr_inner
            .iter()
            .map(|(name, curr_data)| {
                let normalized = if let Some(prev_data) = prev_inner.get(name) {
                    curr_data.cycle_normalized(db, env, prev_data, cycle)
                } else {
                    curr_data.clone()
                };
                (name.clone(), normalized)
            })
            .collect();
        Self::new(db, env.program(db), members)
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
            data,
            materialization: None,
            operations: None,
        })
    }

    pub(super) fn filter_members(
        self,
        db: &'db dyn Db,
        mut predicate: impl FnMut(&ProtocolMember<'_, 'db>) -> bool,
    ) -> Self {
        Self::new(
            db,
            self.program(db),
            self.inner(db)
                .iter()
                .filter(|&(name, data)| {
                    predicate(&ProtocolMember {
                        name,
                        data,
                        materialization: None,
                        operations: None,
                    })
                })
                .map(|(name, data)| (name.clone(), data.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn member_count(self, db: &'db dyn Db) -> usize {
        self.inner(db).len()
    }

    pub(super) fn non_method_members(self, db: &'db dyn Db) -> Vec<ProtocolMember<'db, 'db>> {
        self.members(db)
            .filter(|member| !member.is_method())
            .collect()
    }

    pub(super) fn includes_member(self, db: &'db dyn Db, name: &str) -> bool {
        self.inner(db).contains_key(name)
    }

    /// The exposed read and write types, with their positions in variance inference.
    fn variance_types<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = (Type<'db>, TypeVarVariance)> + 'a {
        self.members(db).flat_map(move |member| {
            // Instance methods are checked only through their bound instance signature.
            let is_instance_method = member.is_instance_method();
            [
                ProtocolMemberAccessMode::Instance,
                ProtocolMemberAccessMode::Class,
            ]
            .into_iter()
            .filter(move |mode| *mode != ProtocolMemberAccessMode::Class || !is_instance_method)
            .flat_map(move |mode| member.access(mode).variances(db, env))
        })
    }

    /// Returns whether `name` has an instance-write requirement of `type[T]`, where `T` belongs
    /// to `generic_context`.
    pub(super) fn includes_generic_writable_instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        generic_context: GenericContext<'db>,
    ) -> bool {
        self.inner(db)
            .get(name)
            .and_then(|data| {
                ProtocolMemberAccess {
                    declaration: data,
                    mode: ProtocolMemberAccessMode::Instance,
                    materialization: None,
                    operations: None,
                }
                .write()
            })
            .and_then(|write| {
                write
                    .requirement(db, env, None)
                    .and_then(|requirement| requirement.accepted_type())
            })
            .is_some_and(|write| {
                matches!(
                    write,
                    Type::SubclassOf(subclass_of)
                        if subclass_of.into_type_var().is_some_and(|typevar| {
                            generic_context.contains(db, typevar.identity(db))
                        })
                )
            })
    }

    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        ProtocolInterfaceView::new(self, None).instance_member(db, env, name)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            env.program(db),
            self.inner(db)
                .iter()
                .map(|(name, data)| {
                    Some((
                        name.clone(),
                        data.recursive_type_normalized_impl(db, env, div, nested)?,
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
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self::new(
            db,
            visitor.env.program(db),
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
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for data in self.inner(db).values() {
            data.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
        }
    }

    pub(super) fn display<'env>(
        self,
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
    ) -> impl std::fmt::Display + 'env {
        std::fmt::from_fn(move |f| {
            f.write_char('{')?;
            for (i, (name, data)) in self.inner(db).iter().enumerate() {
                write!(f, "\"{name}\": {data}", data = data.display(db, env))?;
                if i < self.inner(db).len() - 1 {
                    f.write_str(", ")?;
                }
            }
            f.write_char('}')
        })
    }
}

/// A protocol member's write capability.
///
/// Descriptor setters retain their call contract even when their accepted values cannot be
/// represented by a single [`Type`]. This keeps an unrepresentable domain distinct from an absent
/// setter and lets real assignments use normal call binding.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMemberWrite<'db> {
    Type(ProtocolPropertyType<'db>),
    Descriptor {
        descriptor: ProtocolPropertyType<'db>,
        domain: Option<ProtocolPropertyType<'db>>,
    },
}

impl<'db> ProtocolMemberWrite<'db> {
    const fn from_type(member: ProtocolPropertyType<'db>) -> Self {
        Self::Type(member)
    }

    const fn domain(self) -> Option<ProtocolPropertyType<'db>> {
        match self {
            Self::Type(member) => Some(member),
            Self::Descriptor { domain, .. } => domain,
        }
    }

    const fn descriptor_type(self) -> Option<ProtocolPropertyType<'db>> {
        match self {
            Self::Type(_) => None,
            Self::Descriptor { descriptor, .. } => Some(descriptor),
        }
    }

    fn display_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        match self {
            Self::Type(member) => member.resolve(db, env),
            Self::Descriptor {
                domain: Some(domain),
                ..
            } => Some(domain.resolve(db, env).unwrap_or(Type::unknown())),
            Self::Descriptor { domain: None, .. } => Some(Type::unknown()),
        }
    }

    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match (self, previous) {
            (Self::Type(current), Self::Type(previous)) => {
                Self::Type(current.cycle_normalized(db, env, previous, cycle))
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
                descriptor: current_descriptor.cycle_normalized(
                    db,
                    env,
                    previous_descriptor,
                    cycle,
                ),
                domain: cycle_normalized_optional_type(
                    db,
                    env,
                    current_domain,
                    previous_domain,
                    cycle,
                ),
            },
            (current, _) => current,
        }
    }

    fn cycle_normalized_without_previous(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        let normalize = |member: ProtocolPropertyType<'db>| {
            member.with_ty(member.ty().recursive_type_normalized(db, env, cycle))
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(match self {
            Self::Type(member) => {
                Self::Type(member.recursive_type_normalized_impl(db, env, div, nested)?)
            }
            Self::Descriptor { descriptor, domain } => Self::Descriptor {
                descriptor: descriptor.recursive_type_normalized_impl(db, env, div, nested)?,
                domain: match domain {
                    Some(domain) => {
                        Some(domain.recursive_type_normalized_impl(db, env, div, nested)?)
                    }
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
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
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
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::join(
            db,
            self.variance_types(db, env)
                .map(|(ty, variance)| ty.with_polarity(variance).variance_of(db, env, typevar)),
        )
    }
}

/// A type annotation in a protocol together with the scope of its `typing.Self` references.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
struct ProtocolAnnotation<'db> {
    ty: Type<'db>,
    self_binding_context: Option<BindingContext<'db>>,
}

impl<'db> ProtocolAnnotation<'db> {
    fn bind_self(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
    ) -> Type<'db> {
        if !self.ty.contains_self(db, env) {
            return self.ty;
        }
        self.ty.apply_type_mapping(
            db,
            env,
            &TypeMapping::BindSelf(SelfBinding::new(
                db,
                env,
                self_type,
                self.self_binding_context,
            )),
            TypeContext::default(),
        )
    }

    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        Self {
            ty: self.ty.cycle_normalized(db, env, previous.ty, cycle),
            ..self
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let ty = if nested {
            self.ty.recursive_type_normalized_impl(db, env, div, true)?
        } else {
            self.ty
                .recursive_type_normalized_impl(db, env, div, true)
                .unwrap_or(div)
        };
        Some(Self { ty, ..self })
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self {
            ty: self
                .ty
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ..self
        }
    }
}

/// Describes where to obtain a protocol member's read type or accepted write type.
///
/// The type is either given directly by an annotation or extracted from a property accessor (the
/// return annotation of a getter or the value-parameter annotation of a setter). This also supports
/// ordinary attributes such as `name: str`, whose write type is given directly by `str`.
///
/// Accessor callables are retained until their annotations are needed. Resolving every property
/// while constructing a protocol interface would expand return-type unions even when the property
/// is unrelated to the current check.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolPropertyType<'db> {
    /// The type is provided directly by the stored annotation.
    /// For example, this attribute provides the annotation `str`:
    ///
    /// ```python
    /// class Named(Protocol):
    ///     name: str
    /// ```
    ///
    /// The annotation can still require `Self` substitution when used.
    Annotation(ProtocolAnnotation<'db>),
    /// The type should be extracted from the return annotation of the stored getter callable.
    /// For example:
    ///
    /// ```python
    /// class Named(Protocol):
    ///     @property
    ///     def name(self) -> str: ...
    /// ```
    ///
    /// Here, reading `name` produces `str`.
    PropertyGetter(Type<'db>),
    /// The type should be extracted from the `value`-parameter annotation of the stored
    /// setter callable. For example:
    ///
    /// ```python
    /// class Named(Protocol):
    ///     @property
    ///     def name(self) -> str: ...
    ///
    ///     @name.setter
    ///     def name(self, value: str | None) -> None: ...
    /// ```
    ///
    /// Here, assignment to `name` accepts `str | None`, from the `value` parameter.
    PropertySetter(Type<'db>),
    /// A descriptor access whose overloads are selected after protocol specialization.
    Descriptor {
        descriptor: ProtocolAnnotation<'db>,
        receiver: Type<'db>,
        access: ProtocolDescriptorAccess,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolDescriptorAccess {
    Get,
    Set,
}

impl<'db> ProtocolPropertyType<'db> {
    const fn new(ty: Type<'db>) -> Self {
        Self::Annotation(ProtocolAnnotation {
            ty,
            self_binding_context: None,
        })
    }

    const fn property_getter(ty: Type<'db>) -> Self {
        Self::PropertyGetter(ty)
    }

    const fn property_setter(ty: Type<'db>) -> Self {
        Self::PropertySetter(ty)
    }

    const fn ty(self) -> Type<'db> {
        match self {
            Self::Annotation(annotation) => annotation.ty,
            Self::PropertyGetter(ty) | Self::PropertySetter(ty) => ty,
            Self::Descriptor { descriptor, .. } => descriptor.ty,
        }
    }

    const fn with_ty(self, ty: Type<'db>) -> Self {
        match self {
            Self::Annotation(annotation) => {
                Self::Annotation(ProtocolAnnotation { ty, ..annotation })
            }
            Self::PropertyGetter(_) => Self::PropertyGetter(ty),
            Self::PropertySetter(_) => Self::PropertySetter(ty),
            Self::Descriptor {
                descriptor,
                receiver,
                access,
            } => Self::Descriptor {
                descriptor: ProtocolAnnotation { ty, ..descriptor },
                receiver,
                access,
            },
        }
    }

    fn annotation(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ProtocolAnnotation<'db>> {
        self.annotation_in_context(
            db,
            env,
            &ObservedType::root(self.ty()),
            &RelationContext::default(),
        )
    }

    fn annotation_in_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        observed: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<ProtocolAnnotation<'db>> {
        match self {
            Self::Annotation(annotation) => Some(annotation),
            Self::PropertyGetter(getter) => property_get_member_type(db, env, getter),
            Self::PropertySetter(setter) => property_set_member_type(db, env, setter),
            Self::Descriptor {
                descriptor,
                receiver,
                access,
            } => {
                let descriptor_value = observed.unchanged_or_unresolved(descriptor.ty);
                let receiver = observed.unchanged_or_unresolved(receiver);
                let ty = match access {
                    ProtocolDescriptorAccess::Get => {
                        let owner = receiver.child_at(
                            db,
                            env,
                            receiver.ty.to_meta_type(db, env),
                            ObservationEdge::MetaType,
                        );
                        bind_descriptor(
                            db,
                            env,
                            &descriptor_value,
                            Some(&receiver),
                            &owner,
                            context,
                        )?
                        .0
                        .value?
                        .ty
                    }
                    ProtocolDescriptorAccess::Set => {
                        match descriptor_setter_domain(
                            db,
                            env,
                            &descriptor_value,
                            &receiver,
                            context,
                        ) {
                            DescriptorSetterDomain::Known(domain) => domain,
                            DescriptorSetterDomain::Missing | DescriptorSetterDomain::Deferred => {
                                return None;
                            }
                        }
                    }
                };
                Some(ProtocolAnnotation { ty, ..descriptor })
            }
        }
    }

    /// Observe a descriptor only after replaying substitutions that can select its overload.
    /// Materialization changes its exposed result, not the receiver domain used for lookup.
    fn annotation_with_operations(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operations: ProtocolInterfaceOperations<'db>,
        observed: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<ProtocolAnnotation<'db>> {
        if let Self::Descriptor {
            descriptor,
            mut receiver,
            access: ProtocolDescriptorAccess::Get,
        } = self
        {
            let descriptor_value = observed.unchanged_or_unresolved(descriptor.ty);
            let getter = lookup_member(
                db,
                env,
                &descriptor_value,
                &descriptor_value,
                "__get__",
                MemberLookupPolicy::REQUIRE_CONCRETE | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                context,
            )?
            .value?;
            let mut callables = getter.ty.try_upcast_to_callable_in_context(
                db,
                env,
                UpcastPolicy::default(),
                getter.clone(),
                context.clone(),
            )?;
            for operation in operations.operations(db) {
                let mut visitor = ApplyTypeMappingVisitor::new(env)
                    .with_normalization(TypeNormalization::Structural);
                if let RecursiveOperation::Materialize(kind, bounds) = operation {
                    visitor.materialize_typevar_bounds_and_defaults = *bounds;
                    callables = callables.map(|callable| {
                        let signatures = CallableSignature::from_overloads(
                            callable.signatures(db).iter().map(|signature| {
                                let result = signature.return_ty.materialize(db, *kind, &visitor);
                                signature.clone().with_return_type(result)
                            }),
                        );
                        callable.with_signatures(db, signatures)
                    });
                } else {
                    operation.with_mapping(|mapping| {
                        receiver = receiver.apply_type_mapping_impl(
                            db,
                            &mapping,
                            TypeContext::default(),
                            &visitor,
                        );
                        callables = callables.clone().map(|callable| {
                            callable.apply_type_mapping_impl(
                                db,
                                &mapping,
                                TypeContext::default(),
                                &visitor,
                            )
                        });
                    });
                }
            }
            let arguments = CallArguments::positional([receiver, receiver.to_meta_type(db, env)]);
            let callable = getter.unchanged_or_unresolved(callables.to_type(db, env));
            let constraints = ConstraintSetBuilder::with_relation_context(context.clone());
            let ty = match Type::bindings_observed(db, env, callable, context.clone())
                .match_parameters(db, env, &constraints, &arguments)
                .check_types(
                    db,
                    env,
                    &constraints,
                    &arguments,
                    TypeContext::default(),
                    &[],
                ) {
                Ok(bindings) => bindings.observed_return_type(db, env, context).ty,
                Err(error) => error.1.observed_return_type(db, env, context).ty,
            };
            return Some(ProtocolAnnotation { ty, ..descriptor });
        }
        let annotation = self.annotation_in_context(db, env, observed, context)?;
        Some(ProtocolAnnotation {
            ty: operations.apply(db, env, annotation.ty, false),
            ..annotation
        })
    }

    fn resolve(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        self.annotation(db, env).map(|annotation| annotation.ty)
    }

    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        if let Self::Descriptor {
            descriptor,
            receiver,
            access,
        } = self
        {
            let receiver = if let Self::Descriptor {
                receiver: previous_receiver,
                ..
            } = previous
            {
                receiver.cycle_normalized(db, env, previous_receiver, cycle)
            } else {
                receiver.recursive_type_normalized(db, env, cycle)
            };
            return Self::Descriptor {
                descriptor: ProtocolAnnotation {
                    ty: descriptor
                        .ty
                        .cycle_normalized(db, env, previous.ty(), cycle),
                    ..descriptor
                },
                receiver,
                access,
            };
        }
        let ty = self.ty().cycle_normalized(db, env, previous.ty(), cycle);
        self.with_ty(ty)
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if let Self::Descriptor {
            descriptor,
            receiver,
            access,
        } = self
        {
            return Some(Self::Descriptor {
                descriptor: descriptor.recursive_type_normalized_impl(db, env, div, nested)?,
                receiver: receiver.recursive_type_normalized_impl(db, env, div, nested)?,
                access,
            });
        }
        let ty = if nested {
            self.ty()
                .recursive_type_normalized_impl(db, env, div, true)?
        } else {
            self.ty()
                .recursive_type_normalized_impl(db, env, div, true)
                .unwrap_or(div)
        };
        Some(self.with_ty(ty))
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if let Self::Descriptor {
            descriptor,
            receiver,
            access,
        } = self
        {
            return Self::Descriptor {
                descriptor: descriptor.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                receiver: receiver.apply_type_mapping_impl(
                    db,
                    type_mapping,
                    TypeContext::default(),
                    visitor,
                ),
                access,
            };
        }
        let ty = self
            .ty()
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        self.with_ty(ty)
    }
}

/// Describes instance or class-based access to a protocol member.
#[derive(Debug, Copy, Clone)]
struct ProtocolMemberAccess<'a, 'db> {
    declaration: &'a ProtocolMemberData<'db>,
    mode: ProtocolMemberAccessMode,
    materialization: Option<MaterializationKind>,
    operations: Option<ProtocolInterfaceOperations<'db>>,
}

impl<'a, 'db> ProtocolMemberAccess<'a, 'db> {
    fn read(self) -> Option<ProtocolMemberReadAccess<'a, 'db>> {
        let supported = match self.declaration.kind {
            ProtocolMemberKind::Method(..) => true,
            ProtocolMemberKind::Property { read, .. } => {
                self.mode == ProtocolMemberAccessMode::Instance && read.is_some()
            }
            ProtocolMemberKind::Attribute { .. } => {
                self.mode == ProtocolMemberAccessMode::Instance
                    || self
                        .declaration
                        .qualifiers
                        .contains(TypeQualifiers::CLASS_VAR)
            }
        };
        supported.then_some(ProtocolMemberReadAccess { access: self })
    }

    fn materialize_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Type<'db> {
        match self.operations {
            Some(operations) => operations.apply(db, env, ty, false),
            None => self
                .materialization
                .map_or(ty, |kind| ty.materialization(db, env, kind)),
        }
    }

    fn write(self) -> Option<ProtocolMemberWriteAccess<'db>> {
        let write = match self.declaration.kind {
            ProtocolMemberKind::Method(..) => return None,
            ProtocolMemberKind::Property { write, .. }
                if self.mode == ProtocolMemberAccessMode::Instance =>
            {
                write?
            }
            ProtocolMemberKind::Attribute { write, .. } => {
                let is_class_var = self
                    .declaration
                    .qualifiers
                    .contains(TypeQualifiers::CLASS_VAR);
                let is_final = self.declaration.qualifiers.contains(TypeQualifiers::FINAL);
                if is_final || is_class_var != (self.mode == ProtocolMemberAccessMode::Class) {
                    return None;
                }
                ProtocolMemberWrite::from_type(ProtocolPropertyType::Annotation(write))
            }
            ProtocolMemberKind::Property { .. } => return None,
        };
        Some(ProtocolMemberWriteAccess {
            declaration: write,
            materialization: self.materialization.map(MaterializationKind::flip),
            operations: self.operations,
        })
    }

    fn variances(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = (Type<'db>, TypeVarVariance)> {
        self.read()
            .and_then(|read| read.result_type(db, env, None))
            .map(|ty| (ty, TypeVarVariance::Covariant))
            .into_iter()
            .chain(
                self.write()
                    .and_then(|write| {
                        write
                            .requirement(db, env, None)
                            .and_then(|requirement| requirement.accepted_type())
                    })
                    .map(|ty| (ty, TypeVarVariance::Contravariant)),
            )
    }

    /// The exposed types affected by materialization. Descriptor identity and write presence
    /// are fixed by the declaration, so they need not be resolved to compare its two views.
    fn materialized_types(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> (Option<Type<'db>>, Option<Type<'db>>) {
        (
            self.read().and_then(|read| read.result_type(db, env, None)),
            self.write().and_then(|write| {
                write
                    .requirement(db, env, None)
                    .and_then(|requirement| requirement.accepted_type())
            }),
        )
    }
}

fn walk_protocol_member_access<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    access: ProtocolMemberAccess<'_, 'db>,
    self_type: Option<Type<'db>>,
    visitor: &V,
) {
    let env = visitor.program_environment();
    let read_ty = access
        .read()
        .and_then(|read| read.result_type(db, env, self_type));
    if let Some(read_ty) = read_ty {
        visitor.visit_type(db, read_ty);
    } else if access.mode == ProtocolMemberAccessMode::Instance
        && let ProtocolMemberKind::Property {
            read: Some(read), ..
        } = access.declaration.kind
    {
        // Fall back to the accessor callable when its read type cannot be extracted.
        visitor.visit_type(db, read.ty());
    }

    let Some(write) = access.write() else {
        return;
    };
    let requirement = write.requirement(db, env, self_type);
    let write_ty = requirement
        .as_ref()
        .and_then(ProtocolMemberWriteRequirement::accepted_type);
    if let Some(write_ty) = write_ty {
        visitor.visit_type(db, write_ty);
    } else if let Some(domain) = write.declaration.domain() {
        // Apply the same accessor fallback when the write type cannot be extracted.
        visitor.visit_type(db, domain.ty());
    }
    if let Some(ProtocolMemberWriteRequirement::Descriptor { descriptor_ty, .. }) = requirement {
        visitor.visit_type(db, descriptor_ty);
    }
}

/// A supported read operation, resolved only when its result type is needed.
#[derive(Debug, Copy, Clone)]
struct ProtocolMemberReadAccess<'a, 'db> {
    access: ProtocolMemberAccess<'a, 'db>,
}

impl<'db> ProtocolMemberReadAccess<'_, 'db> {
    fn result_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        self.result_type_with_receiver(db, env, None, self_type)
    }

    fn result_type_with_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver_type: Option<Type<'db>>,
        self_type: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let raw = match self.access.declaration.kind {
            ProtocolMemberKind::Method(ty, _) => ty,
            ProtocolMemberKind::Property { read, .. } => read?.ty(),
            ProtocolMemberKind::Attribute { read, .. } => read.ty,
        };
        self.result_type_in_context(
            db,
            env,
            receiver_type,
            self_type,
            &ObservedType::root(raw),
            &RelationContext::default(),
        )
    }

    fn result_type_in_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver_type: Option<Type<'db>>,
        self_type: Option<Type<'db>>,
        observed: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<Type<'db>> {
        let annotation = match self.access.declaration.kind {
            ProtocolMemberKind::Method(ty, kind) => {
                let ty = if let Type::Callable(callable) = ty
                    && (kind == ProtocolMethodKind::Class
                        || (kind == ProtocolMethodKind::Instance
                            && self.access.mode == ProtocolMemberAccessMode::Instance))
                {
                    let bound =
                        protocol_bind_self(db, env.program(db), callable, receiver_type, None);
                    Type::Callable(self_type.map_or(bound, |self_type| {
                        let receiver_type = if kind == ProtocolMethodKind::Class {
                            self_type.to_meta_type(db, env)
                        } else {
                            self_type
                        };
                        bound.apply_self_with_receiver(db, env, receiver_type, self_type)
                    }))
                } else {
                    ty
                };
                ProtocolAnnotation {
                    ty,
                    self_binding_context: self
                        .access
                        .declaration
                        .definition
                        .map(BindingContext::Definition),
                }
            }
            ProtocolMemberKind::Property { read, .. } => {
                let read = read?;
                if let Some(operations) = self.access.operations {
                    let annotation =
                        read.annotation_with_operations(db, env, operations, observed, context)?;
                    return Some(self_type.map_or(annotation.ty, |self_type| {
                        annotation.bind_self(db, env, self_type)
                    }));
                }
                read.annotation_in_context(db, env, observed, context)?
            }
            ProtocolMemberKind::Attribute { read, .. } => read,
        };
        let annotation = ProtocolAnnotation {
            ty: self.access.materialize_type(db, env, annotation.ty),
            ..annotation
        };
        Some(self_type.map_or(annotation.ty, |self_type| {
            annotation.bind_self(db, env, self_type)
        }))
    }
}

/// A write operation retains the setter declaration while materializing only its accepted values.
#[derive(Debug, Copy, Clone)]
struct ProtocolMemberWriteAccess<'db> {
    declaration: ProtocolMemberWrite<'db>,
    materialization: Option<MaterializationKind>,
    operations: Option<ProtocolInterfaceOperations<'db>>,
}

impl<'db> ProtocolMemberWriteAccess<'db> {
    fn resolve_value(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        value: ProtocolPropertyType<'db>,
        self_type: Option<Type<'db>>,
        observed: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<Type<'db>> {
        let annotation = value.annotation_in_context(db, env, observed, context)?;
        let annotation = ProtocolAnnotation {
            ty: match self.operations {
                Some(operations) => operations.apply(db, env, annotation.ty, true),
                None => self.materialization.map_or(annotation.ty, |kind| {
                    annotation.ty.materialization(db, env, kind)
                }),
            },
            ..annotation
        };
        Some(self_type.map_or(annotation.ty, |self_type| {
            annotation.bind_self(db, env, self_type)
        }))
    }

    /// Resolve the complete write requirement. Type-only queries can omit the receiver;
    /// assignment checking supplies one for `Self` substitution.
    fn requirement(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Option<Type<'db>>,
    ) -> Option<ProtocolMemberWriteRequirement<'db>> {
        let raw = self
            .declaration
            .domain()
            .or_else(|| self.declaration.descriptor_type())?;
        self.requirement_in_context(
            db,
            env,
            self_type,
            &ObservedType::root(raw.ty()),
            &RelationContext::default(),
        )
    }

    fn requirement_in_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Option<Type<'db>>,
        observed: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<ProtocolMemberWriteRequirement<'db>> {
        match self.declaration {
            ProtocolMemberWrite::Type(annotation) => {
                Some(ProtocolMemberWriteRequirement::AssignableTo(
                    self.resolve_value(db, env, annotation, self_type, observed, context)?,
                ))
            }
            ProtocolMemberWrite::Descriptor { descriptor, domain } => {
                let annotation = descriptor.annotation_in_context(db, env, observed, context)?;
                let descriptor_ty = self_type.map_or(annotation.ty, |self_type| {
                    annotation.bind_self(db, env, self_type)
                });
                Some(ProtocolMemberWriteRequirement::Descriptor {
                    descriptor_ty,
                    domain: domain.and_then(|domain| {
                        self.resolve_value(db, env, domain, self_type, observed, context)
                    }),
                })
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ProtocolMemberAccessMode {
    Instance,
    Class,
}

fn cycle_normalized_optional_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    current: Option<ProtocolPropertyType<'db>>,
    previous: Option<ProtocolPropertyType<'db>>,
    cycle: &salsa::Cycle,
) -> Option<ProtocolPropertyType<'db>> {
    match (current, previous) {
        (Some(current), Some(previous)) => Some(current.cycle_normalized(db, env, previous, cycle)),
        (Some(current), None) => {
            Some(current.with_ty(current.ty().recursive_type_normalized(db, env, cycle)))
        }
        (None, _) => None,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ProtocolMemberData<'db> {
    kind: ProtocolMemberKind<'db>,
    qualifiers: TypeQualifiers,
    definition: Option<Definition<'db>>,
    bound_on_class: bool,
}

impl<'db> ProtocolMemberData<'db> {
    fn method(
        db: &'db dyn Db,
        callable: CallableType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Self {
        let (method_kind, callable) = if callable.is_classmethod_like(db) {
            (ProtocolMethodKind::Class, callable)
        } else if callable.is_staticmethod_like(db) {
            (ProtocolMethodKind::Static, callable.into_regular(db))
        } else {
            (ProtocolMethodKind::Instance, callable)
        };

        Self {
            kind: ProtocolMemberKind::Method(Type::Callable(callable), method_kind),
            qualifiers: TypeQualifiers::default(),
            definition,
            bound_on_class: true,
        }
    }

    fn property(
        read: Option<ProtocolPropertyType<'db>>,
        write: Option<ProtocolMemberWrite<'db>>,
        definition: Option<Definition<'db>>,
    ) -> Self {
        Self {
            kind: ProtocolMemberKind::Property { read, write },
            qualifiers: TypeQualifiers::default(),
            definition,
            bound_on_class: true,
        }
    }

    fn attribute(
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
        definition: Option<Definition<'db>>,
    ) -> Self {
        let annotation = ProtocolAnnotation {
            ty,
            self_binding_context: definition.map(BindingContext::Definition),
        };
        Self {
            kind: ProtocolMemberKind::Attribute {
                read: annotation,
                write: annotation,
            },
            qualifiers,
            definition,
            bound_on_class: false,
        }
    }

    fn cycle_normalized(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: &Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        Self {
            kind: self.kind.cycle_normalized(db, env, previous.kind, cycle),
            qualifiers: self.qualifiers,
            definition: self.definition,
            bound_on_class: self.bound_on_class,
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            kind: self
                .kind
                .recursive_type_normalized_impl(db, env, div, nested)?,
            qualifiers: self.qualifiers,
            definition: self.definition,
            bound_on_class: self.bound_on_class,
        })
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let kind = self
            .kind
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        // A class-bound attribute can become a method when a type argument supplies
        // a function. Classify the substituted type before binding its receiver.
        if let ProtocolMemberKind::Attribute { read, .. } = kind {
            match read.ty {
                Type::FunctionLiteral(function)
                    if self.bound_on_class
                        || function.is_staticmethod(db)
                        || function.is_classmethod(db) =>
                {
                    return Self::method(db, function.into_callable_type(db), self.definition);
                }
                Type::Callable(callable) if self.bound_on_class && callable.is_method_like(db) => {
                    return Self::method(db, callable, self.definition);
                }
                _ => {}
            }
        }
        Self {
            kind,
            qualifiers: self.qualifiers,
            definition: self.definition,
            bound_on_class: self.bound_on_class,
        }
    }

    fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        _visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for member_type in self.kind.member_types() {
            member_type.find_legacy_typevars(db, env, binding_context, typevars);
        }
    }

    fn display<'a, 'env>(
        &'a self,
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
    ) -> impl std::fmt::Display + 'a
    where
        'env: 'a,
    {
        std::fmt::from_fn(move |f| match self.kind {
            ProtocolMemberKind::Method(member, _) => {
                write!(f, "MethodMember(`{}`)", member.display(db, env))
            }
            ProtocolMemberKind::Property { read, write } => {
                let mut d = f.debug_struct("PropertyMember");
                if let Some(read) = read.and_then(|read| read.resolve(db, env)) {
                    d.field("read", &format_args!("`{}`", read.display(db, env)));
                }
                if let Some(write) = write.and_then(|write| write.display_type(db, env)) {
                    d.field("write", &format_args!("`{}`", write.display(db, env)));
                }
                d.finish()
            }
            ProtocolMemberKind::Attribute { read, write } => {
                f.write_str("AttributeMember(")?;
                write!(f, "`{}`", read.ty.display(db, env))?;
                if read != write {
                    write!(f, "; write `{}`", write.ty.display(db, env))?;
                }
                if self.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                    f.write_str("; ClassVar")?;
                }
                f.write_char(')')
            }
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMemberKind<'db> {
    Method(Type<'db>, ProtocolMethodKind),
    Property {
        read: Option<ProtocolPropertyType<'db>>,
        write: Option<ProtocolMemberWrite<'db>>,
    },
    /// Reads and writes initially share an annotation. Materializing substituted type
    /// arguments can change their types in opposite directions while retaining the field.
    Attribute {
        read: ProtocolAnnotation<'db>,
        write: ProtocolAnnotation<'db>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ProtocolMethodKind {
    Instance,
    Class,
    Static,
}

impl<'db> ProtocolMemberKind<'db> {
    fn member_types(self) -> impl Iterator<Item = Type<'db>> {
        match self {
            Self::Method(method, _) => [Some(method), None, None],
            Self::Property { read, write } => [
                read.map(ProtocolPropertyType::ty),
                write
                    .and_then(ProtocolMemberWrite::domain)
                    .map(ProtocolPropertyType::ty),
                write
                    .and_then(ProtocolMemberWrite::descriptor_type)
                    .map(ProtocolPropertyType::ty),
            ],
            Self::Attribute { read, write } => [Some(read.ty), Some(write.ty), None],
        }
        .into_iter()
        .flatten()
    }

    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match (self, previous) {
            (Self::Method(current, kind), Self::Method(previous, _)) => {
                let (Type::Callable(current_callable), Type::Callable(previous_callable)) =
                    (current, previous)
                else {
                    return Self::Method(current.cycle_normalized(db, env, previous, cycle), kind);
                };
                debug_assert_eq!(current_callable.kind(db), previous_callable.kind(db));
                let signatures = current_callable.signatures(db).cycle_normalized(
                    db,
                    env,
                    previous_callable.signatures(db),
                    cycle,
                );
                Self::Method(
                    Type::Callable(current_callable.with_signatures(db, signatures)),
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
                read: cycle_normalized_optional_type(db, env, current_read, previous_read, cycle),
                write: match (current_write, previous_write) {
                    (Some(current), Some(previous)) => {
                        Some(current.cycle_normalized(db, env, previous, cycle))
                    }
                    (Some(current), None) => {
                        Some(current.cycle_normalized_without_previous(db, env, cycle))
                    }
                    (None, _) => None,
                },
            },
            (
                Self::Attribute { read, write },
                Self::Attribute {
                    read: previous_read,
                    write: previous_write,
                },
            ) => Self::Attribute {
                read: read.cycle_normalized(db, env, previous_read, cycle),
                write: write.cycle_normalized(db, env, previous_write, cycle),
            },
            (current, _) => current,
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(match self {
            Self::Method(member, kind) => {
                let ty = if nested {
                    member.recursive_type_normalized_impl(db, env, div, true)?
                } else {
                    member
                        .recursive_type_normalized_impl(db, env, div, true)
                        .unwrap_or(div)
                };
                Self::Method(ty, kind)
            }
            Self::Property { read, write } => Self::Property {
                read: match read {
                    Some(read) => Some(read.recursive_type_normalized_impl(db, env, div, nested)?),
                    None => None,
                },
                write: match write {
                    Some(write) => {
                        Some(write.recursive_type_normalized_impl(db, env, div, nested)?)
                    }
                    None => None,
                },
            },
            Self::Attribute { read, write } => Self::Attribute {
                read: read.recursive_type_normalized_impl(db, env, div, nested)?,
                write: write.recursive_type_normalized_impl(db, env, div, nested)?,
            },
        })
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
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
            Self::Attribute { read, write } => Self::Attribute {
                read: read.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                write: write.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor),
            },
        }
    }
}

/// A single member of a protocol interface.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProtocolMember<'a, 'db> {
    name: &'a str,
    data: &'a ProtocolMemberData<'db>,
    materialization: Option<MaterializationKind>,
    operations: Option<ProtocolInterfaceOperations<'db>>,
}

/// Orders protocol members so that finite constraints are established before recursive relations.
///
/// The declaration order is significant because the derived ordering is used when comparing
/// protocol interfaces.
#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum StructuralMemberPriority {
    /// A non-recursive member with at most one callable signature.
    Simple,
    /// A non-recursive callable member with multiple overloads.
    FiniteOverload,
    /// A member that contains a protocol or recursive alias, or whose finiteness is unknown.
    Recursive,
}

fn walk_protocol_member<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    member: &ProtocolMember<'_, 'db>,
    visitor: &V,
) {
    if member.materialization.is_some() || member.operations.is_some() {
        for mode in [
            ProtocolMemberAccessMode::Instance,
            ProtocolMemberAccessMode::Class,
        ] {
            walk_protocol_member_access(db, member.access(mode), None, visitor);
        }
    } else {
        for ty in member.data.kind.member_types() {
            visitor.visit_type(db, ty);
        }
    }
}

impl<'a, 'db> ProtocolMember<'a, 'db> {
    pub(super) fn name(&self) -> &'a str {
        self.name
    }

    fn qualifiers(&self) -> TypeQualifiers {
        self.data.qualifiers
    }

    /// Returns whether an instance declaration conflicts with a required writable class variable.
    ///
    /// An unannotated assignment preserves an inherited `ClassVar`; an explicit instance
    /// annotation does not:
    ///
    /// ```python
    /// from typing import ClassVar
    ///
    /// class Base:
    ///     value: ClassVar[int]
    ///
    /// class Valid(Base):
    ///     value = 1
    ///
    /// class Invalid(Base):
    ///     value: int = 1
    /// ```
    ///
    /// Inspect declarations before descriptor binding, and ignore synthesized members without
    /// source provenance.
    pub(super) fn has_incompatible_class_variable_declaration(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        let qualifiers = self.qualifiers();
        qualifiers.contains(TypeQualifiers::CLASS_VAR)
            && !qualifiers.contains(TypeQualifiers::FINAL)
            && ty
                .nominal_class(db, env)
                .or_else(|| {
                    if !is_class_object_type(ty) {
                        return None;
                    }

                    ty.to_meta_type(db, env)
                        .to_instance_approximation(db, env)?
                        .nominal_class(db, env)
                })
                .is_some_and(|class| {
                    effective_superclass_variable_kind(db, class, Name::new(self.name))
                        == Some(VariableKind::Instance)
                        && [
                            class
                                .class_member(db, env, self.name, MemberLookupPolicy::default())
                                .place,
                            class.instance_member(db, env, self.name).place,
                        ]
                        .into_iter()
                        .any(|place| {
                            matches!(
                                place,
                                Place::Defined(defined) if defined.provenance != Provenance::Unknown
                            )
                        })
                })
    }

    pub(super) fn is_method(&self) -> bool {
        matches!(self.data.kind, ProtocolMemberKind::Method(..))
    }

    /// Returns whether this member has a form supported by
    /// `protocol_materialization_is_noop_with_type_parameters`.
    ///
    /// That proof inspects `P[T]` once and treats recursive specializations of `P` as leaves after
    /// checking their arguments. This check limits the member binding and accessor resolution it
    /// needs to account for:
    ///
    /// - Ordinary properties must have resolvable getter return types and setter value types.
    /// - Instance methods must have at least one signature, and every overload must have a
    ///   positional receiver. The walker binds inferred receivers. Explicit receivers must be
    ///   direct, unmaterialized specializations of `class_origin`, so the proof can handle them
    ///   using the same rule as other recursive references to `P`.
    ///
    /// Arbitrary descriptor access can select an overload based on the specialized receiver;
    /// inspecting `P[T]` alone does not establish the result for every specialization.
    /// Attributes, arbitrary descriptors, static methods, class methods, and other receiver forms
    /// are conservatively excluded from this proof. They may still be unchanged by materialization;
    /// the caller can use concrete interface inspection or structural comparison instead.
    /// This check does not establish that the supported member types or type arguments are static.
    pub(super) fn supports_type_parameter_materialization_proof(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class_origin: ClassLiteral<'db>,
    ) -> bool {
        if let ProtocolMemberKind::Property { read, write } = self.data.kind {
            // The proof supports resolved ordinary property accessors, excluding descriptors.
            return matches!(
                (read, write),
                (
                    None | Some(ProtocolPropertyType::PropertyGetter(_)),
                    None | Some(ProtocolMemberWrite::Type(
                        ProtocolPropertyType::PropertySetter(_)
                    ))
                )
            ) && read.is_none_or(|getter| getter.resolve(db, env).is_some())
                && write.is_none_or(|setter| {
                    setter
                        .domain()
                        .is_some_and(|setter| setter.resolve(db, env).is_some())
                });
        }
        let ProtocolMemberKind::Method(Type::Callable(callable), ProtocolMethodKind::Instance) =
            self.data.kind
        else {
            return false;
        };
        callable.signatures(db).iter().next().is_some()
            && callable.signatures(db).iter().all(|signature| {
                signature.has_implicit_positional_receiver_annotation()
                    || (signature.has_explicit_positional_receiver_annotation()
                        && signature.parameters().get(0).is_some_and(|parameter| {
                            parameter
                                .annotated_type()
                                .as_protocol_instance(db)
                                .is_some_and(|protocol| {
                                    protocol.materialization_kind(db).is_none()
                                        && protocol.class_origin(db).is_some_and(|class| {
                                            class.class_literal(db) == class_origin
                                        })
                                })
                        }))
            })
    }

    /// Returns whether an instance method has an explicit positional receiver annotation.
    pub(super) fn has_explicit_receiver_annotation(&self, db: &'db dyn Db) -> bool {
        match self.data.kind {
            ProtocolMemberKind::Method(Type::Callable(callable), ProtocolMethodKind::Instance) => {
                callable
                    .signatures(db)
                    .iter()
                    .any(Signature::has_explicit_positional_receiver_annotation)
            }
            _ => false,
        }
    }

    /// Returns the priority for structurally comparing this member.
    ///
    /// Simple finite members are cheapest, followed by finite overloads. Recursive members and
    /// aliases that contain a protocol or are themselves recursive are compared last.
    pub(super) fn structural_member_priority(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> StructuralMemberPriority {
        let is_recursive_type = |ty| {
            any_over_type_expanding_aliases(db, env, ty, |nested| {
                matches!(nested, Type::ProtocolInstance(_))
                    || matches!(nested, Type::Recursive(recursive) if recursive.protocol_origin(db).is_some())
            })
        };

        let ProtocolMemberKind::Method(callable, _) = self.data.kind else {
            let values = match self.data.kind {
                ProtocolMemberKind::Attribute { read, write } => [
                    Some(ProtocolPropertyType::Annotation(read)),
                    Some(ProtocolPropertyType::Annotation(write)),
                    None,
                ],
                ProtocolMemberKind::Property { read, write } => [
                    read,
                    write.and_then(ProtocolMemberWrite::domain),
                    write.and_then(ProtocolMemberWrite::descriptor_type),
                ],
                ProtocolMemberKind::Method(..) => [None, None, None],
            };
            let is_finite = values.into_iter().flatten().all(|value| {
                value
                    .resolve(db, env)
                    .is_some_and(|ty| !is_recursive_type(ty))
            });
            return if is_finite {
                StructuralMemberPriority::Simple
            } else {
                StructuralMemberPriority::Recursive
            };
        };
        let Type::Callable(callable) = callable else {
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

    fn read_observation_edge(&self, mode: ProtocolMemberAccessMode) -> ObservationEdge {
        ObservationEdge::ProtocolMemberRead {
            name: Name::new(self.name),
            class_access: mode == ProtocolMemberAccessMode::Class,
        }
    }

    fn write_observation_edge(&self, mode: ProtocolMemberAccessMode) -> ObservationEdge {
        ObservationEdge::ProtocolMemberWrite {
            name: Name::new(self.name),
            class_access: mode == ProtocolMemberAccessMode::Class,
        }
    }

    fn access(&self, mode: ProtocolMemberAccessMode) -> ProtocolMemberAccess<'a, 'db> {
        ProtocolMemberAccess {
            declaration: self.data,
            mode,
            materialization: self.materialization,
            operations: self.operations,
        }
    }

    /// Returns the access that a candidate value must provide for this member.
    ///
    /// A module-level callable can satisfy an ordinary or static method through direct member
    /// access. A class object can likewise satisfy a class, static, or ordinary instance method;
    /// special instance methods instead use special-method lookup through the meta-type. Neither
    /// case needs a separate class-side check for the same member.
    fn implementation_access(
        &self,
        ty: Type<'db>,
        mode: ProtocolMemberAccessMode,
    ) -> Option<ProtocolMemberAccess<'a, 'db>> {
        if mode == ProtocolMemberAccessMode::Class
            && (matches!(
                (ty, self.data.kind),
                (
                    Type::ModuleLiteral(_),
                    ProtocolMemberKind::Method(
                        _,
                        ProtocolMethodKind::Instance | ProtocolMethodKind::Static
                    )
                )
            ) || (is_class_object_type(ty) && self.is_method()))
        {
            None
        } else {
            Some(self.access(mode))
        }
    }
}

fn property_get_member_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    getter: Type<'db>,
) -> Option<ProtocolAnnotation<'db>> {
    let mut get_types = Vec::new();
    let mut definition = None;
    for callable in &getter.try_upcast_to_callable(db, env)? {
        for signature in callable.signatures(db) {
            get_types.push(signature.return_ty);
            definition = definition.or(signature.definition());
        }
    }
    Some(ProtocolAnnotation {
        ty: UnionType::from_elements(db, env, get_types),
        self_binding_context: definition.map(BindingContext::Definition),
    })
}

fn property_set_member_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    setter: Type<'db>,
) -> Option<ProtocolAnnotation<'db>> {
    let (ty, definition) = property_setter_value_type(db, env, setter)?;
    Some(ProtocolAnnotation {
        ty,
        self_binding_context: definition.map(BindingContext::Definition),
    })
}

/// Derive the observable instance capabilities of a descriptor-decorated protocol member.
fn descriptor_decorated_protocol_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    descriptor_ty: Type<'db>,
    protocol: ClassType<'db>,
    definition: Option<Definition<'db>>,
) -> Option<ProtocolMemberData<'db>> {
    let descriptor_ty = descriptor_ty.resolve_type_alias(db);

    // Applying a generic descriptor decorator to a method that refers to an enclosing type
    // variable can currently materialize that variable as `Unknown`. Reducing the descriptor to
    // its `__get__` result would then erase the remaining descriptor structure and weaken the
    // protocol member to a bare `Unknown`.
    if super::visitor::any_over_type(db, env, descriptor_ty, false, |ty| ty.is_unknown()) {
        return None;
    }

    let Place::Defined(DefinedPlace {
        definedness: Definedness::AlwaysDefined,
        ..
    }) = descriptor_ty
        .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
    else {
        return None;
    };

    let receiver_ty = Type::instance(db, env, protocol);
    let descriptor = ProtocolAnnotation {
        ty: descriptor_ty,
        self_binding_context: definition.map(BindingContext::Definition),
    };
    let read = Some(ProtocolPropertyType::Descriptor {
        descriptor,
        receiver: receiver_ty,
        access: ProtocolDescriptorAccess::Get,
    });
    let write = descriptor_ty
        .class_member_with_policy(db, env, "__set__", MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
        .is_definitely_bound()
        .then_some(ProtocolMemberWrite::Descriptor {
            descriptor: ProtocolPropertyType::Annotation(descriptor),
            domain: Some(ProtocolPropertyType::Descriptor {
                descriptor,
                receiver: receiver_ty,
                access: ProtocolDescriptorAccess::Set,
            }),
        });

    Some(ProtocolMemberData::property(read, write, definition))
}

fn is_class_object_type(ty: Type<'_>) -> bool {
    matches!(
        ty,
        Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
    )
}

/// A member expression selected from a static declaration, before class substitution or binding.
struct NominalMemberDeclaration<'db> {
    owner: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    place: DefinedPlace<'db>,
    bound_on_class: bool,
}

impl<'db> ClassType<'db> {
    fn nominal_member_declaration(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Option<NominalMemberDeclaration<'db>> {
        for base in self.iter_mro(db) {
            let class = match base {
                ClassBase::Class(class) => class,
                ClassBase::Generic | ClassBase::Protocol => continue,
                _ => return None,
            };
            let (owner, specialization) = class.static_class_literal(db)?;
            let member = class_member(db, owner.body_scope(db), name);
            let member = if member.is_undefined() {
                ClassType::NonGeneric(owner.into()).own_instance_member(db, env, name)
            } else {
                member
            };
            if let Place::Defined(place) = member.inner.place {
                let bound_on_class = ClassType::NonGeneric(owner.into())
                    .own_class_member(db, env, None, name)
                    .inner
                    .place
                    .is_definitely_bound();
                return Some(NominalMemberDeclaration {
                    owner,
                    specialization,
                    place,
                    bound_on_class,
                });
            }
        }
        None
    }

    /// Certify that substituting the schema's parameters cannot change member selection.
    /// A custom getter is admitted only when one fixed signature accepts every instance and
    /// owner, independently of its type arguments. Receiver-selected overloads remain excluded.
    pub(super) fn has_uniform_protocol_members(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> bool {
        let Some(target_class) = target.class_origin(db) else {
            return false;
        };
        let class_access = is_class_object_type(source);
        if ![self, *target_class].into_iter().all(|class| uniform_member_mro(db, class))
            // Custom metaclasses can change which descriptor wins before the class's own
            // declaration is reached. Their lookup plans need a separate uniformity proof.
            || (class_access
                && self.metaclass(db) != super::KnownClass::Type.to_class_literal(db, env))
        {
            return false;
        }
        target.interface(db).members(db).all(|member| {
            if class_access
                && !self
                    .metaclass_instance_type(db, env)
                    .class_member(db, env, member.name)
                    .place
                    .is_undefined()
            {
                return false;
            }
            let Some(source) = self.nominal_member_declaration(db, env, member.name) else {
                return false;
            };
            let Some(target) = target_class.nominal_member_declaration(db, env, member.name) else {
                return false;
            };
            // Getter selection alone is not a proof about a descriptor's write contract.
            let getter_only = [
                ProtocolMemberAccessMode::Instance,
                ProtocolMemberAccessMode::Class,
            ]
            .into_iter()
            .all(|mode| member.access(mode).write().is_none());
            [(source, true), (target, false)]
                .into_iter()
                .all(|(declaration, is_source)| {
                    if declaration.place.definedness != Definedness::AlwaysDefined {
                        return false;
                    }
                    match declaration.place.ty {
                        ty @ (Type::FunctionLiteral(_) | Type::Callable(_)) => {
                            uniform_method_signatures(db, ty)
                        }
                        Type::PropertyInstance(property) => {
                            property
                                .getter(db)
                                .is_none_or(|getter| uniform_method_signatures(db, getter))
                                && property
                                    .setter(db)
                                    .is_none_or(|setter| uniform_method_signatures(db, setter))
                        }
                        ty => {
                            // Annotation-only instance fields do not invoke a descriptor. A
                            // class-bound T could acquire __get__ after substitution, so only an
                            // explicit, uniform descriptor declaration is eligible here.
                            (declaration.place.origin.is_declared() && !declaration.bound_on_class)
                                || (is_source
                                    && declaration.bound_on_class
                                    && getter_only
                                    && uniform_descriptor_getter(db, env, ty))
                        }
                    }
                })
        })
    }
}

fn uniform_member_mro<'db>(db: &'db dyn Db, class: ClassType<'db>) -> bool {
    class.iter_mro(db).all(|base| match base {
        ClassBase::Class(class) => class.static_class_literal(db).is_some(),
        ClassBase::Generic | ClassBase::Protocol => true,
        _ => false,
    })
}

fn uniform_method_signatures<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let callable = match ty {
        Type::FunctionLiteral(function) => function.into_callable_type(db),
        Type::Callable(callable) if callable.is_method_like(db) => callable,
        _ => return false,
    };
    callable.is_staticmethod_like(db)
        || (!callable.signatures(db).overloads.is_empty()
            && callable
                .signatures(db)
                .iter()
                .all(Signature::has_implicit_positional_receiver_annotation))
}

/// Establish unconditional applicability, rather than selecting an overload using a rigid
/// placeholder. The instance argument can be any object (including None); the owner is a class.
fn uniform_descriptor_getter<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    descriptor: Type<'db>,
) -> bool {
    let Some(instance) = descriptor.as_nominal_instance() else {
        return false;
    };
    let class = instance.class(db, env);
    if !uniform_member_mro(db, class) {
        return false;
    }
    let Some((declaration, specialization)) = class.static_class_literal(db) else {
        return false;
    };
    if specialization
        .is_some_and(|specialization| specialization.materialization_kind(db).is_some())
        || declaration.generic_context(db).is_some_and(|context| {
            context.variables(db).any(|parameter| {
                parameter.is_paramspec(db)
                    || parameter.is_typevartuple(db)
                    || parameter.typevar(db).has_declared_domain(db)
                    || parameter.typevar(db).default_type(db, env).is_some()
            })
        })
    {
        return false;
    }
    let Some(getter) = class.nominal_member_declaration(db, env, "__get__") else {
        return false;
    };
    if getter.place.definedness != Definedness::AlwaysDefined || !getter.bound_on_class {
        return false;
    }
    let callable = match getter.place.ty {
        Type::FunctionLiteral(function) => function.into_callable_type(db),
        Type::Callable(callable) => callable,
        _ => return false,
    };
    if !callable.is_method_like(db)
        || callable.is_classmethod_like(db)
        || callable.is_staticmethod_like(db)
    {
        return false;
    }
    let [signature] = callable.signatures(db).overloads.as_slice() else {
        return false;
    };
    let [receiver, instance, owner] = signature.parameters().iter().as_slice() else {
        return false;
    };
    // The signature contains the implicit Self binder and can inherit the descriptor's
    // class parameters. Neither is a type parameter selected by this call. Reject other
    // method binders: their inference could affect which result this getter returns.
    signature.generic_context.is_none_or(|context| {
        context.variables(db).all(|parameter| {
            parameter.typevar(db).is_self(db)
                || getter
                    .owner
                    .generic_context(db)
                    .is_some_and(|owner| owner.contains(db, parameter.identity(db)))
        })
    }) && signature.has_implicit_positional_receiver_annotation()
        && receiver.is_positional()
        && instance.is_positional()
        && owner.is_positional()
        && accepts_every_descriptor_argument(db, env, instance.annotated_type(), false)
        && accepts_every_descriptor_argument(db, env, owner.annotated_type(), true)
}

fn accepts_every_descriptor_argument<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    annotation: Type<'db>,
    owner: bool,
) -> bool {
    match annotation {
        Type::Dynamic(_) => true,
        Type::Union(union) => union
            .elements(db)
            .iter()
            .any(|&element| accepts_every_descriptor_argument(db, env, element, owner)),
        _ => {
            annotation == Type::object()
                || (owner && annotation == super::KnownClass::Type.to_instance(db, env))
        }
    }
}

/// Runtime bindings applied after observing a protocol member's callable expression.
#[derive(Clone, Copy)]
struct ProtocolCallableBinding<'db> {
    callable: CallableType<'db>,
    receiver: Type<'db>,
    self_type: Type<'db>,
}

impl<'db> ProtocolCallableBinding<'db> {
    fn self_binding(self) -> CallableSelfBinding<'db> {
        CallableSelfBinding {
            receiver: self.receiver,
            self_type: self.self_type,
        }
    }

    fn bound_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        Type::Callable(protocol_apply_self_with_receiver(
            db,
            env.program(db),
            self.callable,
            self.receiver,
            self.self_type,
        ))
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Resolve the candidate member with the search rules required by its protocol declaration.
    fn protocol_member_read_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        access: ProtocolMemberAccessMode,
        demand: MemberLookupDemand,
        observed: &ObservedType<'db>,
    ) -> Option<ObservedMember<'db>> {
        if access == ProtocolMemberAccessMode::Instance
            && member.is_method()
            && member.name == "__call__"
        {
            return Some(ObservedMember {
                result: Place::bound(ty).into(),
                value: Some(observed.unchanged_or_unresolved(ty)),
            });
        }
        let special = access == ProtocolMemberAccessMode::Instance
            && member.is_instance_method()
            && !matches!(ty, Type::ModuleLiteral(_))
            && (!is_class_object_type(ty) || member.uses_special_method_lookup());
        let receiver = if special {
            observed.unchanged_or_unresolved(ty)
        } else {
            observed.clone()
        };
        let policy = if special {
            MemberLookupPolicy::NO_INSTANCE_FALLBACK
        } else {
            MemberLookupPolicy::default()
        };
        lookup_member_with_options(
            db,
            self.env,
            &receiver,
            &receiver,
            member.name,
            MemberLookupOptions { policy, demand },
            &self.context(),
        )
    }

    /// Compare the value produced by the selected member operation with the protocol declaration.
    fn with_protocol_member_operands(
        &self,
        db: &'db dyn Db,
        observed_source: &ObservedType<'db>,
        source: Type<'db>,
        target: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        access: ProtocolMemberAccessMode,
    ) -> Self {
        let target = self.operands().target.child_at(
            db,
            self.env,
            target,
            member.read_observation_edge(access),
        );
        self.with_operands(ObservedTypePair::new(
            observed_source.unchanged_or_unresolved(source),
            target,
        ))
    }

    /// Writes reverse the two member domains while retaining their declaration owners.
    fn with_protocol_write_operands(
        &self,
        db: &'db dyn Db,
        instance: Type<'db>,
        value: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        access: ProtocolMemberAccessMode,
        observed_receiver: &ObservedType<'db>,
    ) -> Self {
        let edge = member.write_observation_edge(access);
        let value = self
            .operands()
            .target
            .child_at(db, self.env, value, edge.clone());
        // Materialized protocol receivers already expose separate read and write domains.
        // Project the selected write requirement before falling back to a nominal declaration.
        if let Some(destination) = observed_receiver.project(db, self.env, edge.clone()) {
            return self.with_operands(ObservedTypePair::new(value, destination));
        }
        let mut destination = self.operands().source.unresolved();
        let class = match instance {
            Type::NominalInstance(instance) => Some(instance.class(db, self.env)),
            Type::ClassLiteral(class) => Some(ClassType::NonGeneric(class)),
            Type::GenericAlias(alias) => Some(ClassType::Generic(alias)),
            _ => None,
        };
        if let Some(declaration) =
            class.and_then(|class| class.nominal_member_declaration(db, self.env, member.name))
        {
            let raw = match declaration.place.ty {
                Type::PropertyInstance(property) => property
                    .setter(db)
                    .and_then(|setter| {
                        ProtocolPropertyType::property_setter(setter).annotation(db, self.env)
                    })
                    .map_or(declaration.place.ty, |annotation| annotation.ty),
                ty => ty,
            };
            destination = self.operands().source.declaration_member(
                db,
                self.env,
                declaration.owner,
                edge,
                raw,
                declaration.specialization,
            );
        }
        self.with_operands(ObservedTypePair::new(value, destination))
    }

    /// Select the runtime receiver of an ordinary protocol requirement. Class access
    /// is evaluated on the meta-type of the candidate's instance view.
    fn protocol_access_receiver(
        &self,
        db: &'db dyn Db,
        receiver_ty: Type<'db>,
        access: ProtocolMemberAccessMode,
    ) -> ObservedType<'db> {
        match access {
            ProtocolMemberAccessMode::Class => self.operands().source.child_at(
                db,
                self.env,
                receiver_ty,
                ObservationEdge::MetaType,
            ),
            ProtocolMemberAccessMode::Instance => {
                self.operands().source.unchanged_or_unresolved(receiver_ty)
            }
        }
    }

    fn check_protocol_member_read(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        member: &ProtocolMember<'_, 'db>,
        required: ProtocolMemberAccess<'_, 'db>,
        observed_receiver: &ObservedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        // A universal read contract needs evidence of presence only. Prove universality with
        // the same relation and target occurrence as the real read; constraints on an inferable
        // target are not enough to weaken the demand for this operation.
        let protocol_self_binding_ty = ty.literal_fallback_instance(db, env).unwrap_or(ty);
        // Select the declaration occurrence before evaluating a getter. Applicability can
        // itself ask whether the receiver implements this protocol.
        let required_observed = self
            .operands()
            .target
            .project(db, env, member.read_observation_edge(required.mode))
            .unwrap_or_else(|| self.operands().target.unresolved());
        let required_ty = required.read().and_then(|read| {
            read.result_type_in_context(
                db,
                env,
                None,
                (!member.is_method()).then_some(protocol_self_binding_ty),
                &required_observed,
                &self.context(),
            )
        });
        let demand = if !member.is_method()
            && required_ty.is_some_and(|required_ty| {
                let upper_bound = ObservedType::root(Type::object());
                self.with_protocol_member_operands(
                    db,
                    &upper_bound,
                    upper_bound.ty,
                    required_ty,
                    member,
                    required.mode,
                )
                .check_child_pair(db, upper_bound.ty, required_ty)
                .is_trivially_always_satisfied()
            }) {
            MemberLookupDemand::Presence
        } else {
            MemberLookupDemand::Value
        };
        let Some(attribute) = self.protocol_member_read_type(
            db,
            ty,
            member,
            required.mode,
            demand,
            observed_receiver,
        ) else {
            return ConstraintSet::incomplete(self.constraints);
        };
        if !attribute.place(db).place.is_definitely_bound() {
            return self.never();
        }
        if demand == MemberLookupDemand::Presence {
            return self.always();
        }
        let Some(attribute_observed) = attribute.value else {
            return self.never();
        };
        let attribute_type = attribute_observed.ty;

        // `Self` in a protocol member names the value satisfying the protocol. `Self` in a
        // method on a class object names instances of that class: a `@classmethod` returning
        // `Self` returns `Factory`, not `type[Factory]`. Keep the bindings separate so a method
        // that returns an instance cannot satisfy a protocol that promises the class object.
        if !member.is_method() {
            return required_ty.when_some_and(db, self.constraints, |required_ty| {
                let result = self
                    .with_protocol_member_operands(
                        db,
                        &attribute_observed,
                        attribute_type,
                        required_ty,
                        member,
                        required.mode,
                    )
                    .check_child_pair(db, attribute_type, required_ty);
                if let Some(context) = self.report_context()
                    && result.is_never_satisfied(db, env, self.inferable)
                {
                    context.push(ErrorContext::ProtocolMemberReadTypeIncompatible {
                        source: attribute_type,
                        target: required_ty,
                    });
                }
                result
            });
        }

        let implementation_self_binding_ty = ty
            .to_instance_approximation(db, env)
            .or_else(|| ty.literal_fallback_instance(db, env))
            .unwrap_or(ty);
        let (implementation_receiver_binding_ty, protocol_receiver_binding_ty) =
            if member.is_class_method() {
                (
                    implementation_self_binding_ty.to_meta_type(db, env),
                    protocol_self_binding_ty.to_meta_type(db, env),
                )
            } else {
                (implementation_self_binding_ty, protocol_self_binding_ty)
            };

        let Some(Type::Callable(required_callable)) = required_ty else {
            return self.never();
        };
        if required.mode == ProtocolMemberAccessMode::Instance {
            attribute_type
                .try_upcast_to_callable_in_context(
                    db,
                    env,
                    UpcastPolicy::from(self.relation),
                    self.with_protocol_member_operands(
                        db,
                        &attribute_observed,
                        attribute_type,
                        Type::Callable(required_callable),
                        member,
                        required.mode,
                    )
                    .operands()
                    .source
                    .unchanged_or_unresolved(attribute_type),
                    self.context(),
                )
                .when_some_and(db, self.constraints, |callables| {
                    callables.iter().when_all(db, self.constraints, |callable| {
                        let source = ProtocolCallableBinding {
                            callable: *callable,
                            receiver: implementation_receiver_binding_ty,
                            self_type: implementation_self_binding_ty,
                        };
                        let target = ProtocolCallableBinding {
                            callable: required_callable,
                            receiver: protocol_receiver_binding_ty,
                            self_type: protocol_self_binding_ty,
                        };
                        self.with_protocol_member_operands(
                            db,
                            &attribute_observed,
                            Type::Callable(*callable),
                            Type::Callable(required_callable),
                            member,
                            required.mode,
                        )
                        .with_callable_self_bindings(
                            db,
                            Some(source.self_binding()),
                            Some(target.self_binding()),
                        )
                        .check_child_pair(
                            db,
                            source.bound_type(db, env),
                            target.bound_type(db, env),
                        )
                    })
                })
        } else if member.is_instance_method() {
            attribute_type
                .try_upcast_to_callable_in_context(
                    db,
                    env,
                    UpcastPolicy::from(self.relation),
                    self.with_protocol_member_operands(
                        db,
                        &attribute_observed,
                        attribute_type,
                        Type::Callable(required_callable),
                        member,
                        required.mode,
                    )
                    .operands()
                    .source
                    .unchanged_or_unresolved(attribute_type),
                    self.context(),
                )
                .when_some_and(db, self.constraints, |callables| {
                    callables.iter().when_all(db, self.constraints, |callable| {
                        if callable.is_function_like(db) {
                            // Require a positional receiver before binding: a zero-argument static
                            // method otherwise loses no parameters while the protocol loses `self`.
                            let signatures = CallableSignature::from_overloads(
                                callable
                                    .signatures(db)
                                    .iter()
                                    .filter(|signature| {
                                        let parameters = signature.parameters();
                                        parameters.get_positional(0).is_some()
                                            || parameters.variadic().is_some()
                                    })
                                    .map(|signature| {
                                        signature.bind_self(
                                            db,
                                            env,
                                            Some(implementation_self_binding_ty),
                                        )
                                    }),
                            );
                            if signatures.overloads.is_empty() {
                                return self.never();
                            }
                            let source = Type::Callable(callable.with_signatures(db, signatures));
                            let target = Type::Callable(protocol_bind_self(
                                db,
                                env.program(db),
                                required_callable,
                                Some(protocol_self_binding_ty),
                                Some(protocol_self_binding_ty),
                            ));
                            self.with_protocol_member_operands(
                                db,
                                &attribute_observed,
                                source,
                                target,
                                member,
                                required.mode,
                            )
                            .check_child_pair(db, source, target)
                        } else {
                            self.with_protocol_member_operands(
                                db,
                                &attribute_observed,
                                Type::Callable(*callable),
                                Type::Callable(required_callable),
                                member,
                                required.mode,
                            )
                            .check_child_pair(
                                db,
                                Type::Callable(*callable),
                                Type::Callable(required_callable),
                            )
                        }
                    })
                })
        } else {
            let target = Type::Callable(protocol_apply_self_with_receiver(
                db,
                env.program(db),
                required_callable,
                protocol_receiver_binding_ty,
                protocol_self_binding_ty,
            ));
            self.with_protocol_member_operands(
                db,
                &attribute_observed,
                attribute_type,
                target,
                member,
                required.mode,
            )
            .check_child_pair(db, attribute_type, target)
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
        required: Option<ProtocolMemberAccess<'_, 'db>>,
        observed_receiver: &ObservedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let Some(required) = required else {
            return self.always();
        };
        if required.mode == ProtocolMemberAccessMode::Class
            && member.has_incompatible_class_variable_declaration(db, self.env, ty)
        {
            if let Some(context) = self.report_context() {
                context.push(ErrorContext::ProtocolMemberClassVarMismatch {
                    member_name: member.name.into(),
                    ty,
                });
            }
            return self.never();
        }

        if required.mode == ProtocolMemberAccessMode::Class
            && member.is_instance_method()
            && required.read().is_some()
        {
            if member.name == "__call__" {
                return self.always();
            }
            return match self.protocol_member_read_type(
                db,
                ty,
                member,
                ProtocolMemberAccessMode::Class,
                MemberLookupDemand::Presence,
                observed_receiver,
            ) {
                Some(member) => ConstraintSet::from_bool(
                    self.constraints,
                    member.place(db).place.is_definitely_bound(),
                ),
                None => ConstraintSet::incomplete(self.constraints),
            };
        }

        let read_result = if required.read().is_some() {
            self.check_protocol_member_read(db, ty, member, required, observed_receiver)
        } else {
            self.always()
        };

        read_result.and(db, self.constraints, || {
            required.write().map_or_else(
                || self.always(),
                |write| {
                    let env = self.env;
                    let fallback_ty = ty.literal_fallback_instance(db, env).unwrap_or(ty);
                    let receiver_ty = if required.mode == ProtocolMemberAccessMode::Instance
                        && matches!(ty, Type::LiteralValue(_))
                    {
                        fallback_ty
                    } else {
                        receiver_ty
                    };
                    let declaration = self
                        .operands()
                        .target
                        .project(db, env, member.write_observation_edge(required.mode))
                        .unwrap_or_else(|| self.operands().target.unresolved());
                    write
                        .requirement_in_context(
                            db,
                            env,
                            Some(fallback_ty),
                            &declaration,
                            &self.context(),
                        )
                        .map(|requirement| {
                            // TODO: Check if using `Unknown` here is correct
                            requirement.accepted_type().unwrap_or_else(Type::unknown)
                        })
                        .when_some_and(db, self.constraints, |write_ty| {
                            let result = self
                                .with_protocol_write_operands(
                                    db,
                                    ty,
                                    write_ty,
                                    member,
                                    required.mode,
                                    observed_receiver,
                                )
                                .check_attribute_write(db, receiver_ty, member.name, write_ty);
                            if let Some(context) = self.report_context()
                                && result.is_never_satisfied(db, env, self.inferable)
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
        let env = self.env;
        let instance_access = member.implementation_access(ty, ProtocolMemberAccessMode::Instance);
        if let Some(context) = self.report_context() {
            if member.has_incompatible_class_variable_declaration(db, env, ty) {
                context.push(ErrorContext::ProtocolMemberClassVarMismatch {
                    member_name: member.name.into(),
                    ty,
                });
                context.push(ErrorContext::ProtocolMemberIncompatible {
                    member_name: member.name.into(),
                });
                return self.never();
            }

            let missing = |receiver_ty, access| {
                self.protocol_member_read_type(
                    db,
                    ty,
                    member,
                    access,
                    MemberLookupDemand::Presence,
                    &self.protocol_access_receiver(db, receiver_ty, access),
                )
                .map(|member| !member.place(db).place.is_definitely_bound())
            };
            let instance_read_missing = if instance_access
                .and_then(ProtocolMemberAccess::read)
                .is_some()
            {
                let Some(missing) = missing(ty, ProtocolMemberAccessMode::Instance) else {
                    return ConstraintSet::incomplete(self.constraints);
                };
                missing
            } else {
                false
            };
            let class_access = member.implementation_access(ty, ProtocolMemberAccessMode::Class);
            let class_read_missing = if class_access.and_then(ProtocolMemberAccess::read).is_some()
                && !(member.is_instance_method() && member.name == "__call__")
            {
                let Some(missing) =
                    missing(ty.to_meta_type(db, env), ProtocolMemberAccessMode::Class)
                else {
                    return ConstraintSet::incomplete(self.constraints);
                };
                missing
            } else {
                false
            };
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
                instance_access,
                &self.operands().source,
            )
            .and(db, self.constraints, || {
                let class_access =
                    member.implementation_access(ty, ProtocolMemberAccessMode::Class);
                self.type_satisfies_protocol_member_access(
                    db,
                    ty,
                    ty.to_meta_type(db, env),
                    member,
                    class_access,
                    &self.protocol_access_receiver(
                        db,
                        ty.to_meta_type(db, env),
                        ProtocolMemberAccessMode::Class,
                    ),
                )
            });
        if let Some(context) = self.report_context()
            && result.is_never_satisfied(db, env, self.inferable)
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
        let env = self.env;
        protocol
            .interface(db)
            .members(db)
            .when_all(db, self.constraints, |member| {
                let required = member.access(ProtocolMemberAccessMode::Class);
                if required.read().is_none() && required.write().is_none() {
                    return self.always();
                }

                let result = if member.is_method() {
                    self.check_protocol_member_read(
                        db,
                        instance_ty,
                        &member,
                        required,
                        &self.operands().source.unchanged_or_unresolved(meta_ty),
                    )
                } else {
                    self.type_satisfies_protocol_member_access(
                        db,
                        instance_ty,
                        meta_ty,
                        &member,
                        Some(required),
                        &self.operands().source.unchanged_or_unresolved(meta_ty),
                    )
                };

                if let Some(context) = self.report_context()
                    && result.is_never_satisfied(db, env, self.inferable)
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
        let env = self.env;
        let source = source_member.access(access);

        if access == ProtocolMemberAccessMode::Class
            && source_member.is_method()
            && target_member.is_instance_method()
        {
            // The instance-side check is authoritative for an ordinary method's signature. Class
            // access only establishes that the source member is also present on the class.
            return ConstraintSet::from_bool(self.constraints, source.read().is_some());
        }
        let target = target_member.access(access);

        let read_result = if target.read().is_none() {
            self.always()
        } else if source.read().is_none() {
            self.never()
        } else {
            let bind_read = |access: ProtocolMemberAccess<'_, 'db>,
                             member: &ProtocolMember<'_, 'db>,
                             receiver: &ObservedType<'db>| {
                let declaration = receiver
                    .project(db, env, member.read_observation_edge(access.mode))
                    .unwrap_or_else(|| receiver.unresolved());
                let unbound = access.read()?.result_type_in_context(
                    db,
                    env,
                    None,
                    None,
                    &declaration,
                    &self.context(),
                )?;
                if member.is_method()
                    && let Type::Callable(callable) = unbound
                {
                    let binding = ProtocolCallableBinding {
                        callable,
                        receiver: source_type,
                        self_type: source_type,
                    };
                    Some((unbound, binding.bound_type(db, env), Some(binding)))
                } else {
                    let bound = access.read()?.result_type_in_context(
                        db,
                        env,
                        None,
                        Some(source_type),
                        &declaration,
                        &self.context(),
                    )?;
                    Some((unbound, bound, None))
                }
            };
            let (
                Some((source_unbound, source, source_binding)),
                Some((target_unbound, target, target_binding)),
            ) = (
                bind_read(source, source_member, &self.operands().source),
                bind_read(target, target_member, &self.operands().target),
            )
            else {
                return self.never();
            };
            let mut checker = self
                .with_child_operands_at(
                    db,
                    source_unbound,
                    target_unbound,
                    source_member.read_observation_edge(access),
                    target_member.read_observation_edge(access),
                )
                .with_callable_self_bindings(
                    db,
                    source_binding.map(ProtocolCallableBinding::self_binding),
                    target_binding.map(ProtocolCallableBinding::self_binding),
                );
            if source_binding.is_none() {
                let mapping = TypeMapping::BindSelf(SelfBinding::new(
                    db,
                    env,
                    source_type,
                    source_member.definition().map(BindingContext::Definition),
                ));
                checker = checker.with_operand_mappings(db, Some(&mapping), None);
            }
            if target_binding.is_none() {
                let mapping = TypeMapping::BindSelf(SelfBinding::new(
                    db,
                    env,
                    source_type,
                    target_member.definition().map(BindingContext::Definition),
                ));
                checker = checker.with_operand_mappings(db, None, Some(&mapping));
            }
            let result = checker.check_child_pair(db, source, target);
            if let Some(context) = self.report_context()
                && !target_member.is_method()
                && result.is_never_satisfied(db, env, self.inferable)
            {
                context.push(ErrorContext::ProtocolMemberReadTypeIncompatible { source, target });
            }
            result
        };

        read_result.and(db, self.constraints, || {
            match (source.write(), target.write()) {
                (_, None) => self.always(),
                (None, Some(_)) => {
                    if let Some(context) = self.report_context() {
                        context.push(ErrorContext::ProtocolMemberNotWritable);
                    }
                    self.never()
                }
                (Some(source), Some(target)) => {
                    let source_observed = self
                        .operands()
                        .source
                        .project(db, env, source_member.write_observation_edge(access))
                        .unwrap_or_else(|| self.operands().source.unresolved());
                    let target_observed = self
                        .operands()
                        .target
                        .project(db, env, target_member.write_observation_edge(access))
                        .unwrap_or_else(|| self.operands().target.unresolved());
                    let (Some(target), Some(source)) = (
                        target
                            .requirement_in_context(
                                db,
                                env,
                                Some(source_type),
                                &target_observed,
                                &self.context(),
                            )
                            .map(|requirement| {
                                // TODO: Check if using `Unknown` here is correct
                                requirement.accepted_type().unwrap_or_else(Type::unknown)
                            }),
                        source
                            .requirement_in_context(
                                db,
                                env,
                                Some(source_type),
                                &source_observed,
                                &self.context(),
                            )
                            .map(|requirement| {
                                // TODO: Check if using `Unknown` here is correct
                                requirement.accepted_type().unwrap_or_else(Type::unknown)
                            }),
                    ) else {
                        return self.never();
                    };
                    let result = self.reversed().check_child_pair_at(
                        db,
                        target,
                        source,
                        target_member.write_observation_edge(access),
                        source_member.write_observation_edge(access),
                    );
                    if let Some(context) = self.report_context()
                        && result.is_never_satisfied(db, env, self.inferable)
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
        source: ProtocolInterfaceView<'db>,
        target: ProtocolInterfaceView<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if source.member_count(db) < target.member_count(db)
            && !self.is_context_collection_enabled()
            && source.member_count(db) < non_object_protocol_member_count(db, target.interface)
        {
            return self.never();
        }

        let env = self.env;
        target
            .members(db)
            .sorted_by_cached_key(|member| member.structural_member_priority(db, env))
            .when_all(db, self.constraints, |target_member| {
                let source_member = source.member_by_name(db, target_member.name);

                if source_member.is_none()
                    && source.includes_member_or_object_fallback(db, env, target_member.name)
                {
                    return self.type_satisfies_protocol_member(db, source_type, &target_member);
                }

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
                    && result.is_never_satisfied(db, env, self.inferable)
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
        let env = self.env;
        if member
            .access(ProtocolMemberAccessMode::Instance)
            .write()
            .is_none()
        {
            return self.never();
        }

        let Place::Defined(DefinedPlace {
            ty: Type::PropertyInstance(actual_property),
            definedness: Definedness::AlwaysDefined,
            ..
        }) = ty.class_member(db, env, member.name()).place
        else {
            return self.never();
        };

        let missing = actual_property.setter(db).is_none();
        if missing && let Some(context) = self.report_context() {
            context.push(ErrorContext::ProtocolMemberNotWritable);
        }
        ConstraintSet::from_bool(self.constraints, missing)
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
        let env = self.env;
        let access = member.access(ProtocolMemberAccessMode::Instance);
        let result = if !member.is_method() {
            access
                .read()
                .and_then(|read| read.result_type(db, env, None))
                .when_some_and(db, self.constraints, |read_ty| {
                    let result = self.check_child_pair_at(
                        db,
                        ty,
                        read_ty,
                        ObservationEdge::Identity,
                        member.read_observation_edge(ProtocolMemberAccessMode::Instance),
                    );
                    if let Some(context) = self.report_context()
                        && result.is_always_satisfied(db, env, self.inferable)
                    {
                        context.push(ErrorContext::DisjointTypes {
                            left: ty,
                            right: read_ty,
                        });
                    }
                    result
                })
        } else {
            let Some(Type::Callable(method)) = access
                .read()
                .and_then(|read| read.result_type(db, env, None))
            else {
                return self.never();
            };
            let receiver_checker = self.as_relation_checker(TypeRelation::Assignability);
            let Some(method_signatures) =
                callable_disjointness_signatures(db, &receiver_checker, method)
            else {
                return self.never();
            };

            let Some(callables) = ty.try_upcast_to_callable_in_context(
                db,
                env,
                UpcastPolicy::Sound,
                self.operands().source.unchanged_or_unresolved(ty),
                self.context(),
            ) else {
                return self.never();
            };

            callables.iter().when_all(db, self.constraints, |callable| {
                let Some(callable_signatures) =
                    callable_disjointness_signatures(db, &receiver_checker, *callable)
                else {
                    return self.never();
                };

                let return_checker = self.reversed().with_child_operands_at(
                    db,
                    Type::Callable(method),
                    Type::Callable(*callable),
                    member.read_observation_edge(ProtocolMemberAccessMode::Instance),
                    ObservationEdge::Identity,
                );
                // Disjointness distributes over unions. Compare the overload return arms
                // directly so that recursive return types do not require canonicalizing an
                // intermediate union merely to distribute it again.
                method_signatures.iter().when_all(
                    db,
                    self.constraints,
                    |(method_index, method_signature)| {
                        callable_signatures.iter().when_all(
                            db,
                            self.constraints,
                            |(callable_index, callable_signature)| {
                                let result = return_checker.check_child_pair_at(
                                    db,
                                    method_signature.return_ty,
                                    callable_signature.return_ty,
                                    ObservationEdge::CallableReturn {
                                        overload: *method_index,
                                    },
                                    ObservationEdge::CallableReturn {
                                        overload: *callable_index,
                                    },
                                );
                                if let Some(context) = self.report_context()
                                    && result.is_always_satisfied(db, env, self.inferable)
                                {
                                    context.push(ErrorContext::DisjointReturnTypes {
                                        left: method_signature.return_ty,
                                        right: callable_signature.return_ty,
                                    });
                                }
                                result
                            },
                        )
                    },
                )
            })
        };
        if let Some(context) = self.report_context()
            && !result.is_always_satisfied(db, env, self.inferable)
        {
            context.take();
        }
        result
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
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        if let Some(specialization) = specialization {
            // Inherited members may use different specializations, so each substitution
            // needs its own traversal cache. Interface preparation records requirements
            // without comparing recursive applications while rebuilding their types.
            let visitor =
                ApplyTypeMappingVisitor::new(env).with_normalization(TypeNormalization::Structural);
            self.ty = self
                .ty
                .apply_specialization_with_visitor(db, specialization, &visitor);
        }
        self
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
                    property
                        .getter(db)
                        .map(ProtocolPropertyType::property_getter),
                    property
                        .setter(db)
                        .map(ProtocolPropertyType::property_setter),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(member) = member.resolve(db, visitor.program_environment()) {
                        visitor.visit_type(db, member);
                    }
                }
            }
            Type::FunctionLiteral(function) => {
                for signature in function.signature(db) {
                    // The inferred receiver describes binding to this declaration, rather than a
                    // recursive requirement. Explicit receiver annotations can relate independent
                    // method parameters to the protocol and must remain part of the flow graph.
                    let skip_receiver =
                        usize::from(signature.has_implicit_positional_receiver_annotation());
                    for parameter in signature.parameters().iter().skip(skip_receiver) {
                        visitor.visit_type(db, parameter.annotated_type());
                    }
                    for (receiver, annotation) in signature.receiver_relations() {
                        visitor.visit_type(db, receiver);
                        visitor.visit_type(db, annotation);
                    }
                    visitor.visit_type(db, signature.return_ty);
                }
            }
            _ => visitor.visit_type(db, self.ty),
        }
    }
}

/// Cache `object` member names so missing protocol members can be rejected without member lookup.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn object_member_names<'db>(db: &'db dyn Db, program: Program<'db>) -> FxHashSet<Name> {
    let env = ProgramEnvironment::from_program(program);
    let Some((object, _)) = ClassType::object(db, &env).static_class_literal(db) else {
        return FxHashSet::default();
    };

    let mut names = place_table(db, object.body_scope(db))
        .symbols()
        .map(|symbol| symbol.name().clone())
        .collect::<FxHashSet<_>>();
    names.shrink_to_fit();
    names
}

/// Count protocol requirements that cannot be supplied by inherited `object` members.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn non_object_protocol_member_count<'db>(
    db: &'db dyn Db,
    interface: ProtocolInterface<'db>,
) -> usize {
    let inherited_member_count = object_member_names(db, interface.program(db))
        .iter()
        .filter(|name| {
            !matches!(name.as_str(), "__hash__" | "__dict__") && interface.includes_member(db, name)
        })
        .count();
    interface.member_count(db) - inherited_member_count
}

/// Check variance dependencies by definition, so expanding specializations such as `P[list[T]]`
/// do not produce an unbounded number of queries. A recursive dependency is supported unless
/// some member in the cycle has an unsupported type; variance itself is inferred by a separate
/// fixed-point computation starting from bivariance.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, _, _| true,
    heap_size=ruff_memory_usage::heap_size,
)]
fn supports_protocol_variance_inference<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> bool {
    let Some(protocol) = class.identity_specialization(db).into_protocol_class(db) else {
        return false;
    };
    let interface = protocol.interface(db);
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    if interface.members(db).any(|member| {
        matches!(
            member.data.kind,
            ProtocolMemberKind::Property {
                write: Some(ProtocolMemberWrite::Descriptor { domain, .. }),
                ..
            } if domain.is_none_or(|domain| domain.resolve(db, &env).is_none())
        )
    }) {
        return false;
    }

    let supports_type = |ty| {
        !any_over_type_expanding_aliases(db, &env, ty, |nested| match nested {
            Type::ProtocolInstance(protocol) => protocol
                .class_origin(db)
                .is_none_or(|class| !class.supports_variance_inference(db)),
            Type::Recursive(recursive) => recursive
                .protocol_origin(db)
                .is_some_and(|class| !class.supports_variance_inference(db)),
            _ => false,
        })
    };
    interface.variance_types(db, &env).all(|(ty, _)| {
        if let Type::Callable(callable) = ty {
            // Bound receivers constrain when a method can be called, but they are not input
            // or output positions in variance inference. Match `Signature::variance_of`.
            callable.signatures(db).iter().all(|signature| {
                signature
                    .parameters()
                    .iter()
                    .map(Parameter::annotated_type)
                    .chain(std::iter::once(signature.return_ty))
                    .all(supports_type)
            })
        } else {
            supports_type(ty)
        }
    })
}

/// Inner Salsa query for [`ProtocolClass::interface`].
#[salsa::tracked(
    returns(copy),
    cycle_initial=protocol_interface_cycle_initial,
    cycle_fn=proto_interface_cycle_recover,
    heap_size=ruff_memory_usage::heap_size,
)]
fn cached_protocol_interface<'db>(
    db: &'db dyn Db,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    let env = ProgramEnvironment::from_file(class.class_literal(db).program_file(db));
    let mut members = BTreeMap::default();

    ProtocolClass(class).for_each_member_candidate(db, &env, |name, candidate, specialization| {
        if members.contains_key(name) {
            return;
        }

        let specialization =
            specialization.map(|specialization| specialization.with_typevar_bounds(db));
        let ProtocolMemberCandidate {
            ty,
            qualifiers,
            definition,
            bound_on_class,
        } = candidate;

        let mut member = match ty {
            Type::PropertyInstance(property) => ProtocolMemberData::property(
                property
                    .getter(db)
                    .map(ProtocolPropertyType::property_getter),
                property
                    .setter(db)
                    .map(ProtocolPropertyType::property_setter)
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
                    descriptor_decorated_protocol_member(db, &env, ty, class, definition)
                {
                    descriptor
                } else {
                    ProtocolMemberData::attribute(ty, qualifiers, definition)
                }
            }
            _ => ProtocolMemberData::attribute(ty, qualifiers, definition),
        };

        member.bound_on_class = bound_on_class.is_yes();
        if let Some(specialization) = specialization {
            // A mutable member has opposite read and write positions. Substitute its
            // requirements after classifying the declaration so those positions remain
            // distinct when the specialization includes a materialization.
            let mapping = ApplySpecialization::specialization(specialization);
            let mapping = match specialization.materialization_kind(db) {
                Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                    specialization: mapping,
                    materialization_kind,
                },
                None => TypeMapping::ApplySpecialization(mapping),
            };
            member = member.apply_type_mapping_impl(
                db,
                &mapping,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::new(&env)
                    .with_normalization(TypeNormalization::Structural),
            );
        }

        members.insert(name.clone(), member);
    });

    ProtocolInterface::new(db, env.program(db), members)
}

fn protocol_interface_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    ProtocolInterface::empty(
        db,
        &ProgramEnvironment::from_file(class.class_literal(db).program_file(db)),
    )
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn proto_interface_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &ProtocolInterface<'db>,
    value: ProtocolInterface<'db>,
    class: ClassType<'db>,
) -> ProtocolInterface<'db> {
    let env = ProgramEnvironment::from_file(class.class_literal(db).program_file(db));
    value.cycle_normalized(db, &env, *previous, cycle)
}

/// Bind `self` unless this is a `Callable[P, R]` dunder, and *also* discard the functionlike-ness
/// of the callable.
///
/// This additional upcasting is required in order for protocols with `__call__` method
/// members to be considered assignable to `Callable` types, since the `Callable` supertype
/// of the `__call__` method will be function-like but a `Callable` type is not.
///
/// Protocol interfaces can be prepared before their receiver is known, so we do not use
/// [`CallableType::bind_self`] here. Preserve all overloads and record receiver constraints for
/// later compatibility checks instead of specializing or filtering signatures here.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn protocol_bind_self<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    callable: CallableType<'db>,
    receiver_type: Option<Type<'db>>,
    self_type: Option<Type<'db>>,
) -> CallableType<'db> {
    if callable.is_dunder_paramspec(db) {
        return callable.into_regular(db);
    }

    let env = ProgramEnvironment::from_program(program);
    callable
        .with_signatures(
            db,
            callable
                .signatures(db)
                .bind_self_with_receiver(db, &env, receiver_type, self_type),
        )
        .into_regular(db)
}

/// Cache receiver and `Self` binding only for protocol-member compatibility checks.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|db, _, _, _, _, _| CallableType::bottom(db),
    heap_size=ruff_memory_usage::heap_size
)]
fn protocol_apply_self_with_receiver<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    callable: CallableType<'db>,
    receiver_type: Type<'db>,
    self_type: Type<'db>,
) -> CallableType<'db> {
    let env = ProgramEnvironment::from_program(program);

    callable.apply_self_with_receiver(db, &env, receiver_type, self_type)
}

/// Returns the available signatures whose return types can establish disjointness.
///
/// Return-type disjointness is a pragmatic approximation for method members: a callable returning
/// `Never` could satisfy otherwise-incompatible signatures, so it must not establish disjointness.
/// An impossible receiver removes an overload; uncertain applicability cannot prove disjointness.
fn callable_disjointness_signatures<'db>(
    db: &'db dyn Db,
    checker: &TypeRelationChecker<'_, '_, 'db>,
    callable: CallableType<'db>,
) -> Option<SmallVec<[(usize, &'db Signature<'db>); 1]>> {
    let mut available = SmallVec::new();
    for (index, signature) in callable.signatures(db).iter().enumerate() {
        let receiver = signature.receiver_constraints_when_satisfied(db, checker);
        if receiver.is_never_satisfied(db, checker.env, checker.inferable) {
            continue;
        }
        if !receiver.is_always_satisfied(db, checker.env, checker.inferable)
            || signature.return_ty.resolve_type_alias(db).is_never()
        {
            return None;
        }
        available.push((index, signature));
    }
    (!available.is_empty()).then_some(available)
}

/// Protocol compatibility can only succeed if every required member is present.
///
/// Check that necessary condition up front so we can avoid expensive per-member type
/// comparisons and generic protocol solving when the actual type is plainly missing a member.
pub(super) fn has_all_protocol_members_defined<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
) -> bool {
    let target_interface = protocol.interface(db);

    match ty.as_protocol_instance(db) {
        Some(source_protocol) => {
            let source_interface = source_protocol.interface(db);

            (source_interface.member_count(db) >= target_interface.member_count(db)
                || source_interface.member_count(db)
                    >= non_object_protocol_member_count(db, target_interface.interface))
                && target_interface.members(db).all(|member| {
                    source_interface.includes_member_or_object_fallback(db, env, member.name())
                })
        }
        None => target_interface.members(db).all(|member| {
            ty.member_lookup_with_policy(
                db,
                env,
                member.name(),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
            .is_definitely_bound()
                || ty
                    .member(db, env, member.name())
                    .place
                    .is_definitely_bound()
        }),
    }
}
