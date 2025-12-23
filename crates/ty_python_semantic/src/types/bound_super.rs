//! Logic for inferring `super()`, `super(x)` and `super(x, y)` calls.

use itertools::{Either, Itertools};
use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast::AnyNodeRef;

use crate::{
    Db, DisplaySettings,
    place::{Place, PlaceAndQualifiers},
    types::{
        ClassBase, ClassType, DynamicType, IntersectionBuilder, KnownClass, MemberLookupPolicy,
        NominalInstanceType, NormalizedVisitor, SpecialFormType, SubclassOfInner, Type,
        TypeVarBoundOrConstraints, TypeVarInstance, UnionBuilder,
        context::InferContext,
        diagnostic::{INVALID_SUPER_ARGUMENT, UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS},
        todo_type, visitor,
    },
};

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
        typevar_context: Option<TypeVarInstance<'db>>,
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
        typevar_context: Option<TypeVarInstance<'db>>,
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
                        let bound_or_constraints_union =
                            Self::describe_typevar(context.db(), &mut diagnostic, *typevar_context);
                        diagnostic.info(format_args!(
                            "`{bounds_or_constraints}` is not an instance or subclass of `{pivot_class}`",
                            bounds_or_constraints =
                                bound_or_constraints_union.display(context.db()),
                            pivot_class = pivot_class.display(context.db()),
                        ));
                        if typevar_context.bound_or_constraints(context.db()).is_none()
                            && !typevar_context.kind(context.db()).is_self()
                        {
                            diagnostic.help(format_args!(
                                "Consider adding an upper bound to type variable `{}`",
                                typevar_context.name(context.db())
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
        type_var: TypeVarInstance<'db>,
    ) -> Type<'db> {
        match type_var.bound_or_constraints(db) {
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
            None => {
                diagnostic.info(format_args!(
                    "Type variable `{}` has `object` as its implicit upper bound",
                    type_var.name(db)
                ));
                Type::object()
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::Update)]
pub enum SuperOwnerKind<'db> {
    Dynamic(DynamicType<'db>),
    Class(ClassType<'db>),
    Instance(NominalInstanceType<'db>),
}

impl<'db> SuperOwnerKind<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic.normalized()),
            SuperOwnerKind::Class(class) => {
                SuperOwnerKind::Class(class.normalized_impl(db, visitor))
            }
            SuperOwnerKind::Instance(instance) => instance
                .normalized_impl(db, visitor)
                .as_nominal_instance()
                .map(Self::Instance)
                .unwrap_or(Self::Dynamic(DynamicType::Any)),
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => {
                Some(SuperOwnerKind::Dynamic(dynamic.recursive_type_normalized()))
            }
            SuperOwnerKind::Class(class) => Some(SuperOwnerKind::Class(
                class.recursive_type_normalized_impl(db, div, nested)?,
            )),
            SuperOwnerKind::Instance(instance) => Some(SuperOwnerKind::Instance(
                instance.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }

    fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => {
                Either::Left(ClassBase::Dynamic(dynamic).mro(db, None))
            }
            SuperOwnerKind::Class(class) => Either::Right(class.iter_mro(db)),
            SuperOwnerKind::Instance(instance) => Either::Right(instance.class(db).iter_mro(db)),
        }
    }

    fn into_class(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            SuperOwnerKind::Dynamic(_) => None,
            SuperOwnerKind::Class(class) => Some(class),
            SuperOwnerKind::Instance(instance) => Some(instance.class(db)),
        }
    }
}

impl<'db> From<SuperOwnerKind<'db>> for Type<'db> {
    fn from(owner: SuperOwnerKind<'db>) -> Self {
        match owner {
            SuperOwnerKind::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SuperOwnerKind::Class(class) => class.into(),
            SuperOwnerKind::Instance(instance) => instance.into(),
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
    visitor.visit_type(db, Type::from(bound_super.owner(db)));
}

impl<'db> BoundSuperType<'db> {
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

        let delegate_with_error_mapped =
            |type_to_delegate_to, error_context: Option<TypeVarInstance<'db>>| {
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

        let owner = match owner_type {
            Type::Never => SuperOwnerKind::Dynamic(DynamicType::Unknown),
            Type::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic),
            Type::ClassLiteral(class) => SuperOwnerKind::Class(ClassType::NonGeneric(class)),
            Type::SubclassOf(subclass_of_type) => {
                match subclass_of_type.subclass_of().with_transposed_type_var(db) {
                    SubclassOfInner::Class(class) => SuperOwnerKind::Class(class),
                    SubclassOfInner::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic),
                    SubclassOfInner::TypeVar(bound_typevar) => {
                        return delegate_to(Type::TypeVar(bound_typevar));
                    }
                }
            }
            Type::NominalInstance(instance) => SuperOwnerKind::Instance(instance),

            Type::ProtocolInstance(protocol) => {
                if let Some(nominal_instance) = protocol.as_nominal_type() {
                    SuperOwnerKind::Instance(nominal_instance)
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
                return delegate_with_error_mapped(alias.value_type(db), None);
            }
            Type::TypeVar(type_var) => {
                let type_var = type_var.typevar(db);
                return match type_var.bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        delegate_with_error_mapped(bound, Some(type_var))
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        delegate_with_error_mapped(constraints.as_type(db), Some(type_var))
                    }
                    None => delegate_with_error_mapped(Type::object(), Some(type_var)),
                };
            }
            Type::BooleanLiteral(_) | Type::TypeIs(_) => {
                return delegate_to(KnownClass::Bool.to_instance(db));
            }
            Type::IntLiteral(_) => return delegate_to(KnownClass::Int.to_instance(db)),
            Type::StringLiteral(_) | Type::LiteralString => {
                return delegate_to(KnownClass::Str.to_instance(db));
            }
            Type::BytesLiteral(_) => {
                return delegate_to(KnownClass::Bytes.to_instance(db));
            }
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
            Type::EnumLiteral(enum_literal_type) => {
                return delegate_to(enum_literal_type.enum_class_instance(db));
            }
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
                        .to_specialized_instance(db, [key_builder.build(), value_builder.build()]),
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
            | Type::DataclassTransformer(_) => {
                return Err(BoundSuperError::AbstractOwnerType {
                    owner_type,
                    pivot_class: pivot_class_type,
                    typevar_context: None,
                });
            }
        };

        // We don't use `Classbase::try_from_type` here because:
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
            _ => {
                return Err(BoundSuperError::InvalidPivotClassType {
                    pivot_class: pivot_class_type,
                });
            }
        };

        if let Some(pivot_class) = pivot_class.into_class()
            && let Some(owner_class) = owner.into_class(db)
        {
            let pivot_class = pivot_class.class_literal(db).0;
            if !owner_class.iter_mro(db).any(|superclass| match superclass {
                ClassBase::Dynamic(_) => true,
                ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict => false,
                ClassBase::Class(superclass) => superclass.class_literal(db).0 == pivot_class,
            }) {
                return Err(BoundSuperError::FailingConditionCheck {
                    pivot_class: pivot_class_type,
                    owner: owner_type,
                    typevar_context: None,
                });
            }
        }

        Ok(Type::BoundSuper(BoundSuperType::new(
            db,
            pivot_class,
            owner,
        )))
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
        let owner = self.owner(db);

        match owner {
            // If the owner is a dynamic type, we can't tell whether it's a class or an instance.
            // Also, invoking a descriptor on a dynamic attribute is meaningless, so we don't handle this.
            SuperOwnerKind::Dynamic(_) => None,
            SuperOwnerKind::Class(_) => Some(
                Type::try_call_dunder_get_on_attribute(
                    db,
                    attribute,
                    Type::none(db),
                    Type::from(owner),
                )
                .0,
            ),
            SuperOwnerKind::Instance(_) => {
                let owner = Type::from(owner);
                Some(
                    Type::try_call_dunder_get_on_attribute(
                        db,
                        attribute,
                        owner,
                        owner.to_meta_type(db),
                    )
                    .0,
                )
            }
        }
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
        let class = match owner {
            SuperOwnerKind::Dynamic(dynamic) => {
                return Type::Dynamic(dynamic)
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Calling `find_name_in_mro` on dynamic type should return `Some`");
            }
            SuperOwnerKind::Class(class) => class,
            SuperOwnerKind::Instance(instance) => instance.class(db),
        };

        let (class_literal, _) = class.class_literal(db);
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

    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.pivot_class(db).normalized_impl(db, visitor),
            self.owner(db).normalized_impl(db, visitor),
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
            self.pivot_class(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.owner(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }
}
