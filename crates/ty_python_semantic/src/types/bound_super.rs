//! Logic for inferring `super()`, `super(x)` and `super(x, y)` calls.

use itertools::{Either, Itertools};
use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast::AnyNodeRef;

use crate::{
    Db, DisplaySettings,
    place::{Place, PlaceAndQualifiers},
    types::{
        BoundTypeVarInstance, ClassBase, ClassType, DivergentType, DynamicType,
        IntersectionBuilder, KnownClass, MemberLookupPolicy, SpecialFormType, SubclassOfInner,
        SubclassOfType, Type, TypeVarBoundOrConstraints, UnionBuilder,
        constraints::ConstraintSet,
        context::InferContext,
        diagnostic::{INVALID_SUPER_ARGUMENT, UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS},
        relation::EquivalenceChecker,
        todo_type,
        typevar::{TypeVarConstraints, TypeVarInstance},
        visitor,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeVarOwnerContext<'db> {
    Bare(BoundTypeVarInstance<'db>),
    SubclassOf(BoundTypeVarInstance<'db>),
}

impl<'db> TypeVarOwnerContext<'db> {
    fn typevar(self, db: &'db dyn Db) -> TypeVarInstance<'db> {
        match self {
            TypeVarOwnerContext::Bare(bound_typevar)
            | TypeVarOwnerContext::SubclassOf(bound_typevar) => bound_typevar.typevar(db),
        }
    }

    fn has_implicit_upper_bound(self, db: &'db dyn Db) -> bool {
        self.typevar(db).bound_or_constraints(db).is_none()
    }

    /// The bound or constraints of this typevar, as a type (i.e. constraints are unioned), wrapped
    /// in `SubclassOf` if this is a `SubclassOf` context. `object` if no bound/constraints.
    /// Used for error messages.
    fn bound_or_constraints_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeVarOwnerContext::Bare(typevar) => typevar
                .typevar(db)
                .require_bound_or_constraints(db)
                .as_type(db),
            TypeVarOwnerContext::SubclassOf(typevar) => SubclassOfType::try_from_instance(
                db,
                typevar
                    .typevar(db)
                    .require_bound_or_constraints(db)
                    .as_type(db),
            )
            .unwrap_or_else(SubclassOfType::subclass_of_unknown),
        }
    }
}

/// Enumeration of ways in which a `super()` call can cause us to emit a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundSuperError<'db> {
    /// The second argument to `super()` (which may have been implicitly provided by
    /// the Python interpreter) has an abstract or structural type.
    /// It's impossible to determine whether a `Callable` type or a synthesized protocol
    /// type is an instance or subclass of the pivot class, so these are rejected.
    AbstractOwnerType {
        owner_type: Type<'db>,
        pivot_class: Type<'db>,
        /// If `owner_type` is a type variable, this contains the type variable instance
        typevar_context: Option<TypeVarOwnerContext<'db>>,
    },
    /// The first argument to `super()` (which may have been implicitly provided by
    /// the Python interpreter) is not a valid class type.
    InvalidPivotClassType { pivot_class: Type<'db> },
    /// The second argument to `super()` was not a subclass or instance of the first argument.
    /// (Note that both arguments may have been implicitly provided by the Python interpreter.)
    FailingConditionCheck {
        pivot_class: Type<'db>,
        owner: Type<'db>,
        /// If `owner_type` is a type variable, this contains the type variable instance
        typevar_context: Option<TypeVarOwnerContext<'db>>,
    },
    /// It was a single-argument `super()` call, but we were unable to determine
    /// the implicit arguments provided by the Python interpreter.
    UnavailableImplicitArguments,
}

impl<'db> BoundSuperError<'db> {
    pub(super) fn report_diagnostic(&self, context: &'db InferContext<'db, '_>, node: AnyNodeRef) {
        match self {
            BoundSuperError::AbstractOwnerType {
                owner_type,
                pivot_class,
                typevar_context,
            } => {
                if let Some(builder) = context.report_lint(&INVALID_SUPER_ARGUMENT, node) {
                    if let Some(typevar_context) = typevar_context {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "`{owner}` is a type variable with an abstract/structural type as \
                            its bounds or constraints, in `super({pivot_class}, {owner})` call",
                            pivot_class = pivot_class.display(context.db()),
                            owner = owner_type.display(context.db()),
                        ));
                        Self::describe_typevar(context.db(), &mut diagnostic, *typevar_context);
                    } else {
                        builder.into_diagnostic(format_args!(
                            "`{owner}` is an abstract/structural type in \
                            `super({pivot_class}, {owner})` call",
                            pivot_class = pivot_class.display(context.db()),
                            owner = owner_type.display(context.db()),
                        ));
                    }
                }
            }
            BoundSuperError::InvalidPivotClassType { pivot_class } => {
                if let Some(builder) = context.report_lint(&INVALID_SUPER_ARGUMENT, node) {
                    match pivot_class {
                        Type::GenericAlias(alias) => {
                            builder.into_diagnostic(format_args!(
                                "`types.GenericAlias` instance `{}` is not a valid class",
                                alias.display_with(context.db(), DisplaySettings::default()),
                            ));
                        }
                        _ => {
                            let mut diagnostic =
                                builder.into_diagnostic("Argument is not a valid class");
                            diagnostic.set_primary_message(format_args!(
                                "Argument has type `{}`",
                                pivot_class.display(context.db())
                            ));
                            diagnostic.set_concise_message(format_args!(
                                "`{}` is not a valid class",
                                pivot_class.display(context.db()),
                            ));
                        }
                    }
                }
            }
            BoundSuperError::FailingConditionCheck {
                pivot_class,
                owner,
                typevar_context,
            } => {
                if let Some(builder) = context.report_lint(&INVALID_SUPER_ARGUMENT, node) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "`{owner}` is not an instance or subclass of \
                        `{pivot_class}` in `super({pivot_class}, {owner})` call",
                        pivot_class = pivot_class.display(context.db()),
                        owner = owner.display(context.db()),
                    ));
                    if let Some(typevar_context) = typevar_context {
                        Self::describe_typevar(context.db(), &mut diagnostic, *typevar_context);
                        diagnostic.info(format_args!(
                            "`{bounds_or_constraints}` is not an instance or subclass of `{pivot_class}`",
                            bounds_or_constraints =
                                typevar_context.bound_or_constraints_type(context.db()).display(context.db()),
                            pivot_class = pivot_class.display(context.db()),
                        ));
                        let typevar = typevar_context.typevar(context.db());
                        if typevar_context.has_implicit_upper_bound(context.db()) {
                            diagnostic.help(format_args!(
                                "Consider adding an upper bound to type variable `{}`",
                                typevar.name(context.db())
                            ));
                        }
                    }
                }
            }
            BoundSuperError::UnavailableImplicitArguments => {
                if let Some(builder) =
                    context.report_lint(&UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS, node)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot determine implicit arguments for 'super()' in this context",
                    ));
                }
            }
        }
    }

    /// Add an `info`-level diagnostic describing the bounds or constraints,
    /// and return the type variable's upper bound or the union of its constraints.
    fn describe_typevar(
        db: &'db dyn Db,
        diagnostic: &mut Diagnostic,
        type_var_context: TypeVarOwnerContext<'db>,
    ) -> Type<'db> {
        let type_var = type_var_context.typevar(db);
        match type_var_context.typevar(db).bound_or_constraints(db) {
            None => {
                diagnostic.info(format_args!(
                    "Type variable `{}` has `object` as its implicit upper bound",
                    type_var.name(db),
                ));
                Type::object()
            }
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                diagnostic.info(format_args!(
                    "Type variable `{}` has upper bound `{}`",
                    type_var.name(db),
                    bound.display(db)
                ));
                bound
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                diagnostic.info(format_args!(
                    "Type variable `{}` has constraints `{}`",
                    type_var.name(db),
                    constraints
                        .elements(db)
                        .iter()
                        .map(|c| c.display(db))
                        .join(", ")
                ));
                constraints.as_type(db)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
enum DescriptorReceiverKind {
    /// Bind descriptors as if `super()` were owned by a class object, i.e. via
    /// `__get__(None, owner)`.
    Class,
    /// Bind descriptors as if `super()` were owned by an instance, i.e. via
    /// `__get__(owner, type(owner))`.
    Instance,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
pub struct ResolvedSuperOwner<'db> {
    /// The resolved second `super()` argument, used when binding descriptors after
    /// attribute lookup. If `receiver` is [`DescriptorReceiverKind::Instance`], this
    /// is passed as the first argument to `__get__` in a `__get__(owner, type(owner))`
    /// call; if `receiver` is [`DescriptorReceiverKind::Class`], it is passed as the
    /// second argument to `__get__` in a `__get__(None, owner)` call.
    owner_type: Type<'db>,
    /// The class whose MRO is searched for attributes after the pivot class.
    lookup_anchor: ClassType<'db>,
    /// The descriptor-binding mode used after attribute lookup.
    receiver: DescriptorReceiverKind,
}

impl<'db> ResolvedSuperOwner<'db> {
    const fn new(
        owner_type: Type<'db>,
        lookup_anchor: ClassType<'db>,
        receiver: DescriptorReceiverKind,
    ) -> Self {
        Self {
            owner_type,
            lookup_anchor,
            receiver,
        }
    }

    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            owner_type: self
                .owner_type
                .recursive_type_normalized_impl(db, div, nested)?,
            lookup_anchor: self
                .lookup_anchor
                .recursive_type_normalized_impl(db, div, nested)?,
            receiver: self.receiver,
        })
    }

    fn descriptor_binding(&self, db: &'db dyn Db) -> (Option<Type<'db>>, Type<'db>) {
        match self.receiver {
            DescriptorReceiverKind::Class => (None, self.owner_type),
            DescriptorReceiverKind::Instance => {
                (Some(self.owner_type), self.owner_type.to_meta_type(db))
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
pub enum SuperOwnerKind<'db> {
    Dynamic(DynamicType<'db>),
    Divergent(DivergentType),
    Resolved(ResolvedSuperOwner<'db>),
}

impl<'db> SuperOwnerKind<'db> {
    fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => {
                Some(SuperOwnerKind::Dynamic(dynamic.recursive_type_normalized()))
            }
            SuperOwnerKind::Divergent(_) => Some(self.clone()),
            SuperOwnerKind::Resolved(resolved_owner) => Some(SuperOwnerKind::Resolved(
                resolved_owner.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }

    fn iter_mro(&self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => {
                Either::Left(ClassBase::Dynamic(*dynamic).mro(db, None))
            }
            SuperOwnerKind::Divergent(divergent) => {
                Either::Left(ClassBase::Divergent(*divergent).mro(db, None))
            }
            SuperOwnerKind::Resolved(resolved_owner) => {
                Either::Right(resolved_owner.lookup_anchor.iter_mro(db))
            }
        }
    }

    /// Returns the type representation of this owner.
    pub(super) fn owner_type(&self) -> Type<'db> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => Type::Dynamic(*dynamic),
            SuperOwnerKind::Divergent(divergent) => Type::Divergent(*divergent),
            SuperOwnerKind::Resolved(resolved_owner) => resolved_owner.owner_type,
        }
    }

    fn descriptor_binding(self, db: &'db dyn Db) -> Option<(Option<Type<'db>>, Type<'db>)> {
        match self {
            SuperOwnerKind::Dynamic(_) | SuperOwnerKind::Divergent(_) => None,
            SuperOwnerKind::Resolved(resolved_owner) => Some(resolved_owner.descriptor_binding(db)),
        }
    }
}

/// Represent a bound super object like `super(PivotClass, owner)`
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct BoundSuperType<'db> {
    pub pivot_class: ClassBase<'db>,
    pub owner: SuperOwnerKind<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundSuperType<'_> {}

pub(super) fn walk_bound_super_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bound_super: BoundSuperType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, Type::from(bound_super.pivot_class(db)));
    match bound_super.owner(db) {
        SuperOwnerKind::Dynamic(dynamic) => visitor.visit_type(db, Type::Dynamic(dynamic)),
        SuperOwnerKind::Divergent(divergent) => visitor.visit_type(db, Type::Divergent(divergent)),
        SuperOwnerKind::Resolved(resolved_owner) => {
            visitor.visit_type(db, resolved_owner.owner_type);
            visitor.visit_type(db, Type::from(resolved_owner.lookup_anchor));
        }
    }
}

impl<'db> BoundSuperType<'db> {
    fn mro_contains_pivot(
        db: &'db dyn Db,
        class: ClassType<'db>,
        pivot_class: ClassBase<'db>,
    ) -> bool {
        match pivot_class {
            ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
            ClassBase::Class(pivot_class) => {
                let pivot_class = pivot_class.class_literal(db);
                class.iter_mro(db).any(|superclass| match superclass {
                    ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
                    ClassBase::Class(superclass) => superclass.class_literal(db) == pivot_class,
                    ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict => false,
                })
            }
            special_form @ (ClassBase::Generic | ClassBase::Protocol) => {
                class.iter_mro(db).any(|superclass| match superclass {
                    ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
                    _ => superclass == special_form,
                })
            }
            // typing.TypedDict never stays in a runtime class' MRO
            ClassBase::TypedDict => false,
        }
    }

    fn validate_resolved_super_owner(
        db: &'db dyn Db,
        pivot_class: ClassBase<'db>,
        pivot_class_type: Type<'db>,
        owner_for_error: Type<'db>,
        owner: ResolvedSuperOwner<'db>,
        metaclass_owner: Option<ResolvedSuperOwner<'db>>,
        typevar_context: Option<TypeVarOwnerContext<'db>>,
    ) -> Result<ResolvedSuperOwner<'db>, BoundSuperError<'db>> {
        [Some(owner), metaclass_owner]
            .into_iter()
            .flatten()
            .find(|candidate| Self::mro_contains_pivot(db, candidate.lookup_anchor, pivot_class))
            .ok_or(BoundSuperError::FailingConditionCheck {
                pivot_class: pivot_class_type,
                owner: owner_for_error,
                typevar_context,
            })
    }

    fn resolve_class_super_owner(
        db: &'db dyn Db,
        pivot_class: ClassBase<'db>,
        pivot_class_type: Type<'db>,
        owner_for_error: Type<'db>,
        owner_display_type: Type<'db>,
        owner_class: ClassType<'db>,
        typevar_context: Option<TypeVarOwnerContext<'db>>,
    ) -> Result<ResolvedSuperOwner<'db>, BoundSuperError<'db>> {
        Self::validate_resolved_super_owner(
            db,
            pivot_class,
            pivot_class_type,
            owner_for_error,
            ResolvedSuperOwner::new(
                owner_display_type,
                owner_class,
                DescriptorReceiverKind::Class,
            ),
            owner_class
                .metaclass(db)
                .to_class_type(db)
                .map(|metaclass| {
                    ResolvedSuperOwner::new(
                        owner_display_type,
                        metaclass,
                        DescriptorReceiverKind::Instance,
                    )
                }),
            typevar_context,
        )
    }

    fn resolve_instance_super_owner(
        db: &'db dyn Db,
        pivot_class: ClassBase<'db>,
        pivot_class_type: Type<'db>,
        owner_type: Type<'db>,
        owner_class: ClassType<'db>,
        typevar_context: Option<TypeVarOwnerContext<'db>>,
    ) -> Result<ResolvedSuperOwner<'db>, BoundSuperError<'db>> {
        Self::validate_resolved_super_owner(
            db,
            pivot_class,
            pivot_class_type,
            owner_type,
            ResolvedSuperOwner::new(owner_type, owner_class, DescriptorReceiverKind::Instance),
            None,
            typevar_context,
        )
    }

    fn build_from_owner(
        db: &'db dyn Db,
        pivot_class: ClassBase<'db>,
        owner: SuperOwnerKind<'db>,
    ) -> Type<'db> {
        Type::BoundSuper(BoundSuperType::new(db, pivot_class, owner))
    }

    /// Attempts to build a `Type::BoundSuper` based on the given `pivot_class` and `owner`.
    ///
    /// This mimics the behavior of Python's built-in `super(pivot, owner)` at runtime.
    /// - `super(pivot, owner_class)` is valid only if `issubclass(owner_class, pivot)`
    /// - `super(pivot, owner_instance)` is valid only if `isinstance(owner_instance, pivot)`
    ///
    /// However, the checking is skipped when any of the arguments is a dynamic type.
    pub(super) fn build(
        db: &'db dyn Db,
        pivot_class_type: Type<'db>,
        owner_type: Type<'db>,
    ) -> Result<Type<'db>, BoundSuperError<'db>> {
        let delegate_to =
            |type_to_delegate_to| BoundSuperType::build(db, pivot_class_type, type_to_delegate_to);

        // Delegate but rewrite errors to preserve TypeVar context.
        let delegate_with_error_mapped =
            |type_to_delegate_to, error_context: Option<TypeVarOwnerContext<'db>>| {
                delegate_to(type_to_delegate_to).map_err(|err| match err {
                    BoundSuperError::AbstractOwnerType {
                        owner_type: _,
                        pivot_class: _,
                        typevar_context: _,
                    } => BoundSuperError::AbstractOwnerType {
                        owner_type,
                        pivot_class: pivot_class_type,
                        typevar_context: error_context,
                    },
                    BoundSuperError::FailingConditionCheck {
                        pivot_class,
                        owner: _,
                        typevar_context: _,
                    } => BoundSuperError::FailingConditionCheck {
                        pivot_class,
                        owner: owner_type,
                        typevar_context: error_context,
                    },
                    BoundSuperError::InvalidPivotClassType { pivot_class } => {
                        BoundSuperError::InvalidPivotClassType { pivot_class }
                    }
                    BoundSuperError::UnavailableImplicitArguments => {
                        BoundSuperError::UnavailableImplicitArguments
                    }
                })
            };

        // We don't use `ClassBase::try_from_type` here because:
        // - There are objects that may validly be present in a class's bases list
        //   but are not valid as pivot classes, e.g. `typing.ChainMap`
        // - There are objects that are not valid in a class's bases list
        //   but are valid as pivot classes, e.g. unsubscripted `typing.Generic`
        let pivot_class = match pivot_class_type {
            Type::ClassLiteral(class) => ClassBase::Class(ClassType::NonGeneric(class)),
            Type::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                SubclassOfInner::Dynamic(dynamic) => ClassBase::Dynamic(dynamic),
                _ => match subclass_of.subclass_of().into_class(db) {
                    Some(class) => ClassBase::Class(class),
                    None => {
                        return Err(BoundSuperError::InvalidPivotClassType {
                            pivot_class: pivot_class_type,
                        });
                    }
                },
            },
            Type::SpecialForm(SpecialFormType::Protocol) => ClassBase::Protocol,
            Type::SpecialForm(SpecialFormType::Generic) => ClassBase::Generic,
            Type::SpecialForm(SpecialFormType::TypedDict) => ClassBase::TypedDict,
            Type::Dynamic(dynamic) => ClassBase::Dynamic(dynamic),
            Type::Divergent(divergent) => ClassBase::Divergent(divergent),
            _ => {
                return Err(BoundSuperError::InvalidPivotClassType {
                    pivot_class: pivot_class_type,
                });
            }
        };

        // Helper to build a union of bound-super instances for constrained TypeVars.
        // Each constraint must be a subclass of the pivot class.
        let build_constrained_union = |constraints: TypeVarConstraints<'db>,
                                       typevar: TypeVarOwnerContext<'db>|
         -> Result<Type<'db>, BoundSuperError<'db>> {
            let mut builder = UnionBuilder::new(db);
            for constraint in constraints.elements(db) {
                let class = match constraint {
                    Type::NominalInstance(instance) => Some(instance.class(db)),
                    _ => constraint.to_class_type(db),
                };
                match class {
                    Some(class) => {
                        let owner = match typevar {
                            TypeVarOwnerContext::Bare(_) => {
                                SuperOwnerKind::Resolved(Self::resolve_instance_super_owner(
                                    db,
                                    pivot_class,
                                    pivot_class_type,
                                    owner_type,
                                    class,
                                    Some(typevar),
                                )?)
                            }
                            TypeVarOwnerContext::SubclassOf(_) => {
                                SuperOwnerKind::Resolved(Self::resolve_class_super_owner(
                                    db,
                                    pivot_class,
                                    pivot_class_type,
                                    owner_type,
                                    owner_type,
                                    class,
                                    Some(typevar),
                                )?)
                            }
                        };
                        builder = builder.add(Self::build_from_owner(db, pivot_class, owner));
                    }
                    None => {
                        // Delegate to the constraint to get better error messages
                        // if the constraint is incompatible with the pivot class.
                        builder = builder.add(delegate_to(*constraint)?);
                    }
                }
            }
            Ok(builder.build())
        };

        let owner = match owner_type {
            Type::Never => SuperOwnerKind::Dynamic(DynamicType::Unknown),
            Type::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic),
            Type::Divergent(divergent) => SuperOwnerKind::Divergent(divergent),
            Type::ClassLiteral(class) => SuperOwnerKind::Resolved(Self::resolve_class_super_owner(
                db,
                pivot_class,
                pivot_class_type,
                owner_type,
                Type::from(ClassType::NonGeneric(class)),
                ClassType::NonGeneric(class),
                None,
            )?),
            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Class(class) => {
                    SuperOwnerKind::Resolved(Self::resolve_class_super_owner(
                        db,
                        pivot_class,
                        pivot_class_type,
                        owner_type,
                        Type::from(class),
                        class,
                        None,
                    )?)
                }
                SubclassOfInner::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic),
                SubclassOfInner::TypeVar(bound_typevar) => {
                    let typevar = bound_typevar.typevar(db);
                    match typevar.bound_or_constraints(db) {
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            let class = match bound {
                                Type::NominalInstance(instance) => Some(instance.class(db)),
                                Type::ProtocolInstance(protocol) => protocol
                                    .to_nominal_instance()
                                    .map(|instance| instance.class(db)),
                                _ => None,
                            };
                            if let Some(class) = class {
                                SuperOwnerKind::Resolved(Self::resolve_class_super_owner(
                                    db,
                                    pivot_class,
                                    pivot_class_type,
                                    owner_type,
                                    owner_type,
                                    class,
                                    Some(TypeVarOwnerContext::SubclassOf(bound_typevar)),
                                )?)
                            } else {
                                let subclass_of = SubclassOfType::try_from_instance(db, bound)
                                    .unwrap_or_else(SubclassOfType::subclass_of_unknown);
                                return delegate_with_error_mapped(
                                    subclass_of,
                                    Some(TypeVarOwnerContext::SubclassOf(bound_typevar)),
                                );
                            }
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                            return build_constrained_union(
                                constraints,
                                TypeVarOwnerContext::SubclassOf(bound_typevar),
                            );
                        }
                        None => {
                            // No bound means the implicit upper bound is `object`.
                            SuperOwnerKind::Resolved(Self::resolve_class_super_owner(
                                db,
                                pivot_class,
                                pivot_class_type,
                                owner_type,
                                owner_type,
                                ClassType::object(db),
                                Some(TypeVarOwnerContext::SubclassOf(bound_typevar)),
                            )?)
                        }
                    }
                }
            },
            Type::NominalInstance(instance) => {
                SuperOwnerKind::Resolved(Self::resolve_instance_super_owner(
                    db,
                    pivot_class,
                    pivot_class_type,
                    owner_type,
                    instance.class(db),
                    None,
                )?)
            }

            Type::ProtocolInstance(protocol) => {
                if let Some(nominal_instance) = protocol.to_nominal_instance() {
                    SuperOwnerKind::Resolved(Self::resolve_instance_super_owner(
                        db,
                        pivot_class,
                        pivot_class_type,
                        owner_type,
                        nominal_instance.class(db),
                        None,
                    )?)
                } else {
                    return Err(BoundSuperError::AbstractOwnerType {
                        owner_type,
                        pivot_class: pivot_class_type,
                        typevar_context: None,
                    });
                }
            }

            Type::Union(union) => {
                return Ok(union
                    .elements(db)
                    .iter()
                    .try_fold(UnionBuilder::new(db), |builder, element| {
                        delegate_to(*element).map(|ty| builder.add(ty))
                    })?
                    .build());
            }
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db);
                let mut one_good_element_found = false;
                for positive in intersection.positive(db) {
                    if let Ok(good_element) = delegate_to(*positive) {
                        one_good_element_found = true;
                        builder = builder.add_positive(good_element);
                    }
                }
                if !one_good_element_found {
                    return Err(BoundSuperError::AbstractOwnerType {
                        owner_type,
                        pivot_class: pivot_class_type,
                        typevar_context: None,
                    });
                }
                for negative in intersection.negative(db) {
                    if let Ok(good_element) = delegate_to(*negative) {
                        builder = builder.add_negative(good_element);
                    }
                }
                return Ok(builder.build());
            }
            Type::TypeAlias(alias) => {
                return delegate_to(alias.value_type(db));
            }
            Type::TypeVar(bound_typevar) => {
                let typevar = bound_typevar.typevar(db);
                match typevar.bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        let class = match bound {
                            Type::NominalInstance(instance) => Some(instance.class(db)),
                            Type::ProtocolInstance(protocol) => protocol
                                .to_nominal_instance()
                                .map(|instance| instance.class(db)),
                            _ => None,
                        };
                        if let Some(class) = class {
                            SuperOwnerKind::Resolved(Self::resolve_instance_super_owner(
                                db,
                                pivot_class,
                                pivot_class_type,
                                owner_type,
                                class,
                                Some(TypeVarOwnerContext::Bare(bound_typevar)),
                            )?)
                        } else {
                            return delegate_with_error_mapped(
                                bound,
                                Some(TypeVarOwnerContext::Bare(bound_typevar)),
                            );
                        }
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        return build_constrained_union(
                            constraints,
                            TypeVarOwnerContext::Bare(bound_typevar),
                        );
                    }
                    None => {
                        // No bound means the implicit upper bound is `object`.
                        SuperOwnerKind::Resolved(Self::resolve_instance_super_owner(
                            db,
                            pivot_class,
                            pivot_class_type,
                            owner_type,
                            ClassType::object(db),
                            Some(TypeVarOwnerContext::Bare(bound_typevar)),
                        )?)
                    }
                }
            }
            Type::TypeIs(_) | Type::TypeGuard(_) => {
                return delegate_to(KnownClass::Bool.to_instance(db));
            }
            Type::LiteralValue(literal) => return delegate_to(literal.fallback_instance(db)),
            Type::SpecialForm(special_form) => {
                return delegate_to(special_form.instance_fallback(db));
            }
            Type::KnownInstance(instance) => {
                return delegate_to(instance.instance_fallback(db));
            }
            Type::FunctionLiteral(_) | Type::DataclassDecorator(_) => {
                return delegate_to(KnownClass::FunctionType.to_instance(db));
            }
            Type::WrapperDescriptor(_) => {
                return delegate_to(KnownClass::WrapperDescriptorType.to_instance(db));
            }
            Type::KnownBoundMethod(method) => {
                return delegate_to(method.class().to_instance(db));
            }
            Type::BoundMethod(_) => return delegate_to(KnownClass::MethodType.to_instance(db)),
            Type::ModuleLiteral(_) => {
                return delegate_to(KnownClass::ModuleType.to_instance(db));
            }
            Type::GenericAlias(_) => return delegate_to(KnownClass::GenericAlias.to_instance(db)),
            Type::PropertyInstance(_) => return delegate_to(KnownClass::Property.to_instance(db)),
            Type::BoundSuper(_) => return delegate_to(KnownClass::Super.to_instance(db)),
            Type::TypedDict(td) => {
                // In general it isn't sound to upcast a `TypedDict` to a `dict`,
                // but here it seems like it's probably sound?
                let mut key_builder = UnionBuilder::new(db);
                let mut value_builder = UnionBuilder::new(db);
                for (name, field) in td.items(db) {
                    key_builder = key_builder.add(Type::string_literal(db, name));
                    value_builder = value_builder.add(field.declared_ty);
                }
                return delegate_to(
                    KnownClass::Dict
                        .to_specialized_instance(db, &[key_builder.build(), value_builder.build()]),
                );
            }
            Type::NewTypeInstance(newtype) => {
                return delegate_to(newtype.concrete_base_type(db));
            }
            Type::Callable(callable) if callable.is_function_like(db) => {
                return delegate_to(KnownClass::FunctionType.to_instance(db));
            }
            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::Callable(_)
            | Type::TypedDictTop
            | Type::DataclassTransformer(_) => {
                return Err(BoundSuperError::AbstractOwnerType {
                    owner_type,
                    pivot_class: pivot_class_type,
                    typevar_context: None,
                });
            }
        };

        Ok(Self::build_from_owner(db, pivot_class, owner))
    }

    /// Skips elements in the MRO up to and including the pivot class.
    ///
    /// If the pivot class is a dynamic type, its MRO can't be determined,
    /// so we fall back to using the MRO of `DynamicType::Unknown`.
    fn skip_until_after_pivot(
        self,
        db: &'db dyn Db,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> {
        let Some(pivot_class) = self.pivot_class(db).into_class() else {
            return Either::Left(ClassBase::Dynamic(DynamicType::Unknown).mro(db, None));
        };

        let mut pivot_found = false;

        Either::Right(mro_iter.skip_while(move |superclass| {
            if pivot_found {
                false
            } else if Some(pivot_class) == superclass.into_class() {
                pivot_found = true;
                true
            } else {
                true
            }
        }))
    }

    /// Tries to call `__get__` on the attribute.
    /// The arguments passed to `__get__` depend on whether the owner is an instance or a class.
    /// See the `CPython` implementation for reference:
    /// <https://github.com/python/cpython/blob/3b3720f1a26ab34377542b48eb6a6565f78ff892/Objects/typeobject.c#L11690-L11693>
    pub(super) fn try_call_dunder_get_on_attribute(
        self,
        db: &'db dyn Db,
        attribute: PlaceAndQualifiers<'db>,
    ) -> Option<PlaceAndQualifiers<'db>> {
        let (instance, owner) = self.owner(db).descriptor_binding(db)?;
        Some(Type::try_call_dunder_get_on_attribute(db, attribute, instance, owner).0)
    }

    /// Similar to `Type::find_name_in_mro_with_policy`, but performs lookup starting *after* the
    /// pivot class in the MRO, based on the `owner` type instead of the `super` type.
    pub(super) fn find_name_in_mro_after_pivot(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let owner = self.owner(db);
        let class = match &owner {
            SuperOwnerKind::Dynamic(dynamic) => {
                return Type::Dynamic(*dynamic)
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Calling `find_name_in_mro` on dynamic type should return `Some`");
            }
            SuperOwnerKind::Divergent(_) => {
                return Type::unknown()
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Calling `find_name_in_mro` on Unknown should return `Some`");
            }
            SuperOwnerKind::Resolved(resolved_owner) => resolved_owner.lookup_anchor,
        };

        let class_literal = class.class_literal(db);
        // TODO properly support super() with generic types
        // * requires a fix for https://github.com/astral-sh/ruff/issues/17432
        // * also requires understanding how we should handle cases like this:
        //  ```python
        //  b_int: B[int]
        //  b_unknown: B
        //
        //  super(B, b_int)
        //  super(B[int], b_unknown)
        //  ```
        match class_literal.generic_context(db) {
            Some(_) => Place::bound(todo_type!("super in generic class")).into(),
            None => class_literal.class_member_from_mro(
                db,
                name,
                policy,
                self.skip_until_after_pivot(db, owner.iter_mro(db)),
            ),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.pivot_class(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.owner(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }
}

impl<'c, 'db> EquivalenceChecker<'_, 'c, 'db> {
    /// Check whether two `BoundSuperType`s are equivalent by recursing into
    /// their fields.
    ///
    /// This method is necessary because [`super::relation::TypeRelationChecker::check_type_pair`]
    /// should only return an always-satisfied constraint set for two
    /// `Type::BoundSuper` types if the two types are exactly equivalent. But
    /// `TypeRelationChecker::check_type_pair` cannot simply delegate to
    /// [`EquivalenceChecker::check_type_pair`] for this case, because
    /// `EquivalenceChecker::check_type_pair` itself delegates back to
    /// `TypeRelationChecker::check_type_pair`, which would cause an infinite loop.
    pub(super) fn check_bound_super_pair(
        &self,
        db: &'db dyn Db,
        left: BoundSuperType<'db>,
        right: BoundSuperType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let mut class_equivalence = match (left.pivot_class(db), right.pivot_class(db)) {
            (ClassBase::Class(left), ClassBase::Class(right)) => {
                self.check_type_pair(db, Type::from(left), Type::from(right))
            }

            (ClassBase::Class(_), _) => self.never(),

            // A `Divergent` type is only equivalent to itself
            (ClassBase::Divergent(l), ClassBase::Divergent(r)) => {
                ConstraintSet::from_bool(self.constraints, l == r)
            }
            (ClassBase::Divergent(_), _) | (_, ClassBase::Divergent(_)) => self.never(),
            (ClassBase::Dynamic(_), ClassBase::Dynamic(_)) => self.always(),
            (ClassBase::Dynamic(_), _) => self.never(),

            (ClassBase::Generic, ClassBase::Generic) => self.always(),
            (ClassBase::Generic, _) => self.never(),

            (ClassBase::Protocol, ClassBase::Protocol) => self.always(),
            (ClassBase::Protocol, _) => self.never(),

            (ClassBase::TypedDict, ClassBase::TypedDict) => self.always(),
            (ClassBase::TypedDict, _) => self.never(),
        };
        if class_equivalence.is_never_satisfied(db) {
            return self.never();
        }
        let owner_equivalence = match (left.owner(db), right.owner(db)) {
            (SuperOwnerKind::Resolved(left), SuperOwnerKind::Resolved(right)) => self
                .check_type_pair(db, left.owner_type, right.owner_type)
                .and(db, self.constraints, || {
                    self.check_type_pair(
                        db,
                        Type::from(left.lookup_anchor),
                        Type::from(right.lookup_anchor),
                    )
                })
                .and(db, self.constraints, || {
                    ConstraintSet::from_bool(self.constraints, left.receiver == right.receiver)
                }),
            (SuperOwnerKind::Resolved(_), _) => self.never(),

            // A `Divergent` type is only equivalent to itself
            (SuperOwnerKind::Divergent(l), SuperOwnerKind::Divergent(r)) => {
                ConstraintSet::from_bool(self.constraints, l == r)
            }
            (SuperOwnerKind::Divergent(_), _) | (_, SuperOwnerKind::Divergent(_)) => self.never(),
            (SuperOwnerKind::Dynamic(_), SuperOwnerKind::Dynamic(_)) => self.always(),
            (SuperOwnerKind::Dynamic(_), _) => self.never(),
        };
        class_equivalence.intersect(db, self.constraints, owner_equivalence)
    }
}
