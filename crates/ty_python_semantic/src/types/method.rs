use itertools::Either;
use ruff_python_ast::name::Name;

use crate::{
    Db, Program, ProgramEnvironment,
    types::{
        ApplyTypeMappingVisitor, CallableType, CallableTypes, InternedType, KnownClass,
        LiteralValueType, LiteralValueTypeKind, Parameter, Parameters, PropertyInstanceType,
        Signature, StringLiteralType, Type, TypeContext, TypeFormType, TypeMapping, UnionType,
        callable::CallableTypeKind, constraints::ConstraintSet, function::FunctionType,
        known_instance::InternedConstraintSet, relation::TypeRelationChecker,
        signatures::CallableSignature, visitor,
    },
};

/// This type represents bound method objects that are created when a method is accessed
/// on an instance of a class. For example, the expression `Path("a.txt").touch` creates
/// a bound method object that represents the `Path.touch` method which is bound to the
/// instance `Path("a.txt")`.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct BoundMethodType<'db> {
    /// The callable being bound. Retaining the unbound payload separately from the receiver
    /// preserves both its signature and its identity, when a function definition is available.
    #[returns(copy)]
    pub(crate) func: Type<'db>,
    /// Synthesized functions need not have a definition from which to obtain a program.
    #[returns(copy)]
    pub(super) program: Program<'db>,
    /// Class method binding captures a class object but substitutes its instance type for `Self`.
    #[returns(copy)]
    pub(super) class_method: bool,
    #[returns(copy)]
    receiver: BoundMethodReceiver<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundMethodType<'_> {}

/// The captured receiver and the type used to check the method's first parameter.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum BoundMethodReceiver<'db> {
    /// The captured receiver is also used to check the signature.
    Instance(Type<'db>),
    /// Looking up `x.method` for `T: (A, B)` checks each alternative separately.
    /// In the `A` alternative, the signature receives `A`, but `__self__` retains `T`.
    Constrained {
        receiver: Type<'db>,
        constraint: Type<'db>,
    },
}

impl<'db> BoundMethodReceiver<'db> {
    fn constrained(receiver: Type<'db>, constraint: Type<'db>) -> Self {
        // Specialization can make the captured receiver equal to its constraint. Use the
        // same representation as direct binding so equivalent bound methods stay identical.
        if receiver == constraint {
            Self::Instance(receiver)
        } else {
            Self::Constrained {
                receiver,
                constraint,
            }
        }
    }

    fn self_instance(self) -> Type<'db> {
        match self {
            Self::Instance(receiver) | Self::Constrained { receiver, .. } => receiver,
        }
    }

    fn signature_receiver(self) -> Type<'db> {
        match self {
            Self::Instance(receiver) => receiver,
            Self::Constrained { constraint, .. } => constraint,
        }
    }

    fn map(self, mut f: impl FnMut(Type<'db>) -> Type<'db>) -> Self {
        match self {
            Self::Instance(receiver) => Self::Instance(f(receiver)),
            Self::Constrained {
                receiver,
                constraint,
            } => Self::constrained(f(receiver), f(constraint)),
        }
    }

    fn try_map(self, mut f: impl FnMut(Type<'db>) -> Option<Type<'db>>) -> Option<Self> {
        Some(match self {
            Self::Instance(receiver) => Self::Instance(f(receiver)?),
            Self::Constrained {
                receiver,
                constraint,
            } => Self::constrained(f(receiver)?, f(constraint)?),
        })
    }
}

pub(super) fn walk_bound_method_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method: BoundMethodType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, method.func(db));
    visitor.visit_type(db, method.self_instance(db));
    visitor.visit_type(db, method.signature_receiver(db));
}

#[salsa::tracked]
impl<'db> BoundMethodType<'db> {
    pub(crate) fn from_callable(
        db: &'db dyn Db,
        func: Type<'db>,
        program: Program<'db>,
        receiver: Type<'db>,
    ) -> Self {
        Self::new_internal(
            db,
            func.underlying_function(db),
            program,
            func.is_classmethod(db),
            BoundMethodReceiver::Instance(receiver),
        )
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        // A bound method retains its function identity even when its receiver is promoted.
        let func = match self.func(db) {
            Type::FunctionLiteral(function) => Type::FunctionLiteral(
                function.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
            func => func.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        };
        Self::new_internal(
            db,
            func,
            self.program(db),
            self.class_method(db),
            self.receiver(db)
                .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
        )
    }

    pub(crate) fn new(db: &'db dyn Db, function: FunctionType<'db>, receiver: Type<'db>) -> Self {
        Self::from_callable(
            db,
            Type::FunctionLiteral(function),
            function.program_file(db).program(db),
            receiver,
        )
    }

    /// The captured receiver, exposed through the bound method's `__self__` attribute.
    pub(crate) fn self_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.receiver(db).self_instance()
    }

    /// The receiver used to check the signature for this member-lookup alternative.
    pub(super) fn signature_receiver(self, db: &'db dyn Db) -> Type<'db> {
        self.receiver(db).signature_receiver()
    }

    /// Returns the underlying Python function, when the bound callable has a function definition.
    pub(crate) fn function(self, db: &'db dyn Db) -> Option<FunctionType<'db>> {
        self.func(db).as_function_literal()
    }

    pub(super) fn with_func(self, db: &'db dyn Db, func: Type<'db>) -> Self {
        Self::new_internal(
            db,
            func,
            self.program(db),
            self.class_method(db),
            self.receiver(db),
        )
    }

    pub(crate) fn unbound_signatures(self, db: &'db dyn Db) -> &'db CallableSignature<'db> {
        match self.func(db) {
            Type::FunctionLiteral(function) => function.signature(db),
            Type::Callable(callable) => callable.signatures(db),
            _ => CallableType::unknown(db).signatures(db),
        }
    }

    /// Returns the type that replaces any `typing.Self` annotations in the bound method signature.
    /// This is normally the bound-instance type. Classmethod binding and a `type[Self]`
    /// receiver annotation instead use an instance of the captured class.
    pub(crate) fn typing_self_type(self, db: &'db dyn Db) -> Type<'db> {
        let mut self_instance = self.self_instance(db);
        let is_class_method = self.class_method(db);
        // Extracting a classmethod's `__func__` removes its descriptor behavior, but its
        // `type[Self]` receiver annotation still relates `Self` to an instance of the class.
        let has_class_self_receiver = is_class_method
            || self
                .unbound_signatures(db)
                .overloads
                .iter()
                .filter_map(|signature| signature.parameters().get(0))
                .filter(|parameter| parameter.is_positional())
                .any(|parameter| {
                    matches!(
                        parameter.annotated_type().resolve_type_alias(db),
                        Type::SubclassOf(subclass)
                            if subclass.into_type_var().is_some_and(|typevar| typevar.typevar(db).is_self(db))
                    )
                });
        if has_class_self_receiver {
            let env = ProgramEnvironment::from_program(self.program(db));
            self_instance = self_instance
                .to_instance_approximation(db, &env)
                .unwrap_or_else(|| {
                    // Constructor callables can already carry an instance as their `Self`
                    // substitution, even when the function's receiver is `type[Self]`.
                    if is_class_method {
                        Type::unknown()
                    } else {
                        self_instance
                    }
                });
        }
        self_instance
    }

    pub(crate) fn map_self_type(
        self,
        db: &'db dyn Db,
        f: impl FnMut(Type<'db>) -> Type<'db>,
    ) -> Self {
        Self::new_internal(
            db,
            self.func(db),
            self.program(db),
            self.class_method(db),
            self.receiver(db).map(f),
        )
    }

    pub(crate) fn with_constrained_receiver(
        self,
        db: &'db dyn Db,
        receiver: Type<'db>,
        constraint: Type<'db>,
    ) -> Self {
        Self::new_internal(
            db,
            self.func(db),
            self.program(db),
            self.class_method(db),
            BoundMethodReceiver::constrained(receiver, constraint),
        )
    }

    pub(crate) fn callables(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<CallableTypes<'db>> {
        match self.func(db) {
            Type::FunctionLiteral(_) | Type::Callable(_) => {
                Some(CallableTypes::one(self.into_callable_type(db)))
            }
            func => func.try_upcast_to_callable(db, env).map(|callables| {
                callables
                    .map(|callable| callable.bind_self(db, env, Some(self.signature_receiver(db))))
            }),
        }
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|db, _, _| CallableType::bottom(db),
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        let env = ProgramEnvironment::from_program(self.program(db));
        let typing_self_type = self.typing_self_type(db);
        let receiver_type = self.signature_receiver(db);

        self.callable_with_signatures(
            db,
            self.unbound_signatures(db)
                .bind_method(db, &env, receiver_type, typing_self_type),
        )
    }

    /// Converts this bound method into a callable using separate runtime-receiver and `Self` types.
    pub(crate) fn into_callable_type_with_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver_type: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> CallableType<'db> {
        self.callable_with_signatures(
            db,
            self.unbound_signatures(db)
                .bind_method(db, env, receiver_type, typing_self_type),
        )
    }

    fn callable_with_signatures(
        self,
        db: &'db dyn Db,
        signatures: CallableSignature<'db>,
    ) -> CallableType<'db> {
        match self.func(db) {
            Type::Callable(callable) => callable.with_signatures(db, signatures).into_regular(db),
            _ => CallableType::new(db, signatures, CallableTypeKind::Regular),
        }
    }

    /// Shares the signatures retained in the method's interned callable.
    pub(crate) fn bound_signatures(self, db: &'db dyn Db) -> &'db CallableSignature<'db> {
        self.into_callable_type(db).signatures(db)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new_internal(
            db,
            self.func(db)
                .recursive_type_normalized_impl(db, env, div, nested)?,
            self.program(db),
            self.class_method(db),
            self.receiver(db)
                .try_map(|ty| ty.recursive_type_normalized_impl(db, env, div, true))?,
        ))
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_bound_method_pair(
        &self,
        db: &'db dyn Db,
        source: BoundMethodType<'db>,
        target: BoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // The receiver exposed by `__self__` is an already-captured value, so it is covariant.
        // However, `Self` can also appear in the remaining parameters, where binding the
        // receiver must still preserve ordinary callable contravariance.
        self.check_type_pair(db, source.func(db), target.func(db))
            .and(db, self.constraints, || {
                self.check_type_pair(db, source.self_instance(db), target.self_instance(db))
            })
            .and(db, self.constraints, || {
                let (Some(source), Some(target)) = (
                    source.callables(db, self.env),
                    target.callables(db, self.env),
                ) else {
                    return self.never();
                };
                self.check_type_pair(
                    db,
                    source.into_type(db, self.env),
                    target.into_type(db, self.env),
                )
            })
    }
}

/// Represents a specific instance of a bound method type for a builtin class.
///
/// Unlike bound methods of user-defined classes, these are not generally instances
/// of `types.BoundMethodType` at runtime.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum KnownBoundMethodType<'db> {
    /// Method wrapper for `some_function.__get__`
    FunctionTypeDunderGet(InternedType<'db>),
    /// Native `__call__` wrapper for a function, bound method, or staticmethod descriptor.
    /// Retains the original callable so its receiver and signature are checked when called.
    DunderCall(InternedType<'db>),
    /// Native `types.MethodType.__get__`, which preserves the captured receiver.
    MethodTypeDunderGet(BoundMethodType<'db>),
    /// Method wrapper for `some_property.__get__`
    PropertyDunderGet(PropertyInstanceType<'db>),
    /// Method wrapper for `some_property.__set__`
    PropertyDunderSet(PropertyInstanceType<'db>),
    /// Method wrapper for `some_property.__delete__`
    PropertyDunderDelete(PropertyInstanceType<'db>),
    /// Method wrapper for `str.startswith`.
    /// We treat this method specially because we want to be able to infer precise Boolean
    /// literal return types if the instance and the prefix are both string literals, and
    /// this allows us to understand statically known branches for common tests such as
    /// `if sys.platform.startswith("freebsd")`.
    StrStartswith(StringLiteralType<'db>),

    // ConstraintSet methods
    ConstraintSetLowerBound,
    ConstraintSetUpperBound,
    ConstraintSetEquality,
    ConstraintSetRange,
    ConstraintSetAlways,
    ConstraintSetNever,
    ConstraintSetImpliesSubtypeOf(InternedConstraintSet<'db>),
    ConstraintSetSatisfies(InternedConstraintSet<'db>),
    ConstraintSetExists(InternedConstraintSet<'db>),
    ConstraintSetForAll(InternedConstraintSet<'db>),
    ConstraintSetSolutionsFor(InternedConstraintSet<'db>),
    ConstraintSetSolutions(InternedConstraintSet<'db>),
    ConstraintSetWithDetailedDisplay(InternedConstraintSet<'db>),
}

pub(super) fn walk_method_wrapper_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method_wrapper: KnownBoundMethodType<'db>,
    visitor: &V,
) {
    match method_wrapper {
        KnownBoundMethodType::FunctionTypeDunderGet(function)
        | KnownBoundMethodType::DunderCall(function) => {
            visitor.visit_type(db, function.inner(db));
        }
        KnownBoundMethodType::MethodTypeDunderGet(method) => {
            visitor.visit_type(db, Type::BoundMethod(method));
        }
        KnownBoundMethodType::PropertyDunderGet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        KnownBoundMethodType::PropertyDunderSet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        KnownBoundMethodType::PropertyDunderDelete(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        KnownBoundMethodType::StrStartswith(string_literal) => {
            visitor.visit_type(
                db,
                LiteralValueType::promotable(LiteralValueTypeKind::String(string_literal)).into(),
            );
        }
        KnownBoundMethodType::ConstraintSetLowerBound
        | KnownBoundMethodType::ConstraintSetUpperBound
        | KnownBoundMethodType::ConstraintSetEquality
        | KnownBoundMethodType::ConstraintSetRange
        | KnownBoundMethodType::ConstraintSetAlways
        | KnownBoundMethodType::ConstraintSetNever
        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
        | KnownBoundMethodType::ConstraintSetSatisfies(_)
        | KnownBoundMethodType::ConstraintSetExists(_)
        | KnownBoundMethodType::ConstraintSetForAll(_)
        | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
        | KnownBoundMethodType::ConstraintSetSolutions(_)
        | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {}
    }
}

impl<'db> KnownBoundMethodType<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(function) => Some(
                KnownBoundMethodType::FunctionTypeDunderGet(InternedType::new(
                    db,
                    function
                        .inner(db)
                        .recursive_type_normalized_impl(db, env, div, nested)?,
                )),
            ),
            KnownBoundMethodType::DunderCall(callable) => {
                Some(KnownBoundMethodType::DunderCall(InternedType::new(
                    db,
                    callable
                        .inner(db)
                        .recursive_type_normalized_impl(db, env, div, nested)?,
                )))
            }
            KnownBoundMethodType::MethodTypeDunderGet(method) => {
                Some(KnownBoundMethodType::MethodTypeDunderGet(
                    method.recursive_type_normalized_impl(db, env, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderGet(property) => {
                Some(KnownBoundMethodType::PropertyDunderGet(
                    property.recursive_type_normalized_impl(db, env, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderSet(property) => {
                Some(KnownBoundMethodType::PropertyDunderSet(
                    property.recursive_type_normalized_impl(db, env, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderDelete(property) => {
                Some(KnownBoundMethodType::PropertyDunderDelete(
                    property.recursive_type_normalized_impl(db, env, div, nested)?,
                ))
            }
            KnownBoundMethodType::StrStartswith(_)
            | KnownBoundMethodType::ConstraintSetLowerBound
            | KnownBoundMethodType::ConstraintSetUpperBound
            | KnownBoundMethodType::ConstraintSetEquality
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetExists(_)
            | KnownBoundMethodType::ConstraintSetForAll(_)
            | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
            | KnownBoundMethodType::ConstraintSetSolutions(_)
            | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => Some(self),
        }
    }

    /// Return the [`KnownClass`] that inhabitants of this type are instances of at runtime
    pub(super) fn class(self) -> KnownClass {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::DunderCall(_)
            | KnownBoundMethodType::MethodTypeDunderGet(_)
            | KnownBoundMethodType::PropertyDunderGet(_)
            | KnownBoundMethodType::PropertyDunderSet(_)
            | KnownBoundMethodType::PropertyDunderDelete(_) => KnownClass::MethodWrapperType,
            KnownBoundMethodType::StrStartswith(_) => KnownClass::BuiltinFunctionType,
            KnownBoundMethodType::ConstraintSetLowerBound
            | KnownBoundMethodType::ConstraintSetUpperBound
            | KnownBoundMethodType::ConstraintSetEquality
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetExists(_)
            | KnownBoundMethodType::ConstraintSetForAll(_)
            | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
            | KnownBoundMethodType::ConstraintSetSolutions(_)
            | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {
                KnownClass::ConstraintSet
            }
        }
    }

    /// Return the call signatures, preserving union alternatives of the captured callable.
    pub(super) fn callables(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<CallableTypes<'db>> {
        let object_type_form = || TypeFormType::from_type_expression(db, Type::object());

        let signatures = match self {
            KnownBoundMethodType::DunderCall(callable) => {
                return callable
                    .inner(db)
                    .try_upcast_to_callable(db, env)
                    .map(|callables| callables.map(|callable| callable.into_regular(db)));
            }
            // Here, we dynamically model the overloaded function signature of `types.FunctionType.__get__`.
            // This is required because we need to return more precise types than what the signature in
            // typeshed provides:
            //
            // ```py
            // class FunctionType:
            //     # ...
            //     @overload
            //     def __get__(self, instance: None, owner: type, /) -> FunctionType: ...
            //     @overload
            //     def __get__(self, instance: object, owner: type | None = None, /) -> MethodType: ...
            // ```
            //
            // For `builtins.property.__get__`, we use the same signature. The return types are not
            // specified yet, they will be dynamically added in `Bindings::evaluate_known_cases`.
            // Python 3.13's native `types.MethodType.__get__` accepts the same arguments but returns
            // the existing bound method. As with function descriptors, these overloads currently
            // also accept `None` without an owner, although that combination fails at runtime.
            //
            // TODO: Consider merging these synthesized signatures with the ones in
            // [`WrapperDescriptorKind::signatures`], since this one is just that signature
            // with the `self` parameters removed.
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::PropertyDunderGet(_)
            | KnownBoundMethodType::MethodTypeDunderGet(_) => Either::Left(
                [
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::none(db, env)),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(KnownClass::Type.to_instance(db, env)),
                        ]),
                        match self {
                            KnownBoundMethodType::MethodTypeDunderGet(method) => {
                                Type::BoundMethod(method)
                            }
                            _ => Type::unknown(),
                        },
                    ),
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    db,
                                    env,
                                    KnownClass::Type.to_instance(db, env),
                                    Type::none(db, env),
                                ))
                                .with_default_type(Type::none(db, env)),
                        ]),
                        match self {
                            KnownBoundMethodType::MethodTypeDunderGet(method) => {
                                Type::BoundMethod(method)
                            }
                            _ => Type::unknown(),
                        },
                    ),
                ]
                .into_iter(),
            ),
            KnownBoundMethodType::PropertyDunderSet(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object()),
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(Type::object()),
                    ]),
                    Type::unknown(),
                )))
            }
            KnownBoundMethodType::PropertyDunderDelete(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "instance",
                    )))
                    .with_annotated_type(Type::object())]),
                    Type::unknown(),
                )))
            }
            KnownBoundMethodType::StrStartswith(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("prefix")))
                            .with_annotated_type(UnionType::from_two_elements(
                                db,
                                env,
                                KnownClass::Str.to_instance(db, env),
                                Type::homogeneous_tuple(
                                    db,
                                    env,
                                    KnownClass::Str.to_instance(db, env),
                                ),
                            )),
                        Parameter::positional_only(Some(Name::new_static("start")))
                            .with_annotated_type(UnionType::from_two_elements(
                                db,
                                env,
                                KnownClass::SupportsIndex.to_instance(db, env),
                                Type::none(db, env),
                            ))
                            .with_default_type(Type::none(db, env)),
                        Parameter::positional_only(Some(Name::new_static("end")))
                            .with_annotated_type(UnionType::from_two_elements(
                                db,
                                env,
                                KnownClass::SupportsIndex.to_instance(db, env),
                                Type::none(db, env),
                            ))
                            .with_default_type(Type::none(db, env)),
                    ]),
                    KnownClass::Bool.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetLowerBound => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("lower_bound")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("typevar")))
                            .with_annotated_type(object_type_form()),
                    ]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetUpperBound => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("typevar")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("upper_bound")))
                            .with_annotated_type(object_type_form()),
                    ]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetEquality => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("typevar")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(object_type_form()),
                    ]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetRange => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("lower_bound")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("typevar")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("upper_bound")))
                            .with_annotated_type(object_type_form()),
                    ]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("ty")))
                            .with_annotated_type(object_type_form()),
                        Parameter::positional_only(Some(Name::new_static("of")))
                            .with_annotated_type(object_type_form()),
                    ]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfies(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "other",
                    )))
                    .with_annotated_type(KnownClass::ConstraintSet.to_instance(db, env))]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetExists(_)
            | KnownBoundMethodType::ConstraintSetForAll(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "typevars",
                    )))
                    .with_annotated_type(TypeFormType::from_type_expression(
                        db,
                        Type::homogeneous_tuple(db, env, Type::object()),
                    ))]),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }

            KnownBoundMethodType::ConstraintSetSolutionsFor(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("typevar")))
                            .with_annotated_type(object_type_form()),
                        Parameter::keyword_only(Name::new_static("inferable")).with_annotated_type(
                            TypeFormType::from_type_expression(
                                db,
                                Type::homogeneous_tuple(db, env, Type::object()),
                            ),
                        ),
                    ]),
                    UnionType::from_two_elements(
                        db,
                        env,
                        Type::homogeneous_tuple(
                            db,
                            env,
                            KnownClass::ConstraintSetSolution.to_instance(db, env),
                        ),
                        Type::none(db, env),
                    ),
                )))
            }

            KnownBoundMethodType::ConstraintSetSolutions(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::keyword_only(Name::new_static("inferable"))
                        .with_annotated_type(TypeFormType::from_type_expression(
                            db,
                            Type::homogeneous_tuple(db, env, Type::object()),
                        ))]),
                    UnionType::from_two_elements(
                        db,
                        env,
                        Type::homogeneous_tuple(
                            db,
                            env,
                            KnownClass::ConstraintSetSolution.to_instance(db, env),
                        ),
                        Type::none(db, env),
                    ),
                )))
            }

            KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    KnownClass::ConstraintSet.to_instance(db, env),
                )))
            }
        };
        Some(CallableTypes::one(CallableType::new(
            db,
            CallableSignature::from_overloads(signatures),
            CallableTypeKind::Regular,
        )))
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_known_bound_method_pair(
        &self,
        db: &'db dyn Db,
        source: KnownBoundMethodType<'db>,
        target: KnownBoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match (source, target) {
            (
                KnownBoundMethodType::FunctionTypeDunderGet(source_function),
                KnownBoundMethodType::FunctionTypeDunderGet(target_function),
            ) => self.check_type_pair(db, source_function.inner(db), target_function.inner(db)),

            (
                KnownBoundMethodType::DunderCall(source_callable),
                KnownBoundMethodType::DunderCall(target_callable),
            ) => self.check_type_pair(db, source_callable.inner(db), target_callable.inner(db)),

            (
                KnownBoundMethodType::MethodTypeDunderGet(source_method),
                KnownBoundMethodType::MethodTypeDunderGet(target_method),
            ) => self.check_bound_method_pair(db, source_method, target_method),

            (
                KnownBoundMethodType::PropertyDunderGet(source_property),
                KnownBoundMethodType::PropertyDunderGet(target_property),
            )
            | (
                KnownBoundMethodType::PropertyDunderSet(source_property),
                KnownBoundMethodType::PropertyDunderSet(target_property),
            )
            | (
                KnownBoundMethodType::PropertyDunderDelete(source_property),
                KnownBoundMethodType::PropertyDunderDelete(target_property),
            ) => self.check_property_instance_pair(db, source_property, target_property),

            (KnownBoundMethodType::StrStartswith(_), KnownBoundMethodType::StrStartswith(_)) => {
                ConstraintSet::from_bool(self.constraints, source == target)
            }

            (
                KnownBoundMethodType::ConstraintSetLowerBound,
                KnownBoundMethodType::ConstraintSetLowerBound,
            )
            | (
                KnownBoundMethodType::ConstraintSetUpperBound,
                KnownBoundMethodType::ConstraintSetUpperBound,
            )
            | (
                KnownBoundMethodType::ConstraintSetEquality,
                KnownBoundMethodType::ConstraintSetEquality,
            )
            | (
                KnownBoundMethodType::ConstraintSetRange,
                KnownBoundMethodType::ConstraintSetRange,
            )
            | (
                KnownBoundMethodType::ConstraintSetAlways,
                KnownBoundMethodType::ConstraintSetAlways,
            )
            | (
                KnownBoundMethodType::ConstraintSetNever,
                KnownBoundMethodType::ConstraintSetNever,
            )
            | (
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_),
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfies(_),
                KnownBoundMethodType::ConstraintSetSatisfies(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetExists(_),
                KnownBoundMethodType::ConstraintSetExists(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetForAll(_),
                KnownBoundMethodType::ConstraintSetForAll(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSolutionsFor(_),
                KnownBoundMethodType::ConstraintSetSolutionsFor(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSolutions(_),
                KnownBoundMethodType::ConstraintSetSolutions(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
                KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            ) => self.always(),

            (
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::DunderCall(_)
                | KnownBoundMethodType::MethodTypeDunderGet(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::DunderCall(_)
                | KnownBoundMethodType::MethodTypeDunderGet(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            ) => self.never(),
        }
    }
}

/// Represents a specific instance of `types.WrapperDescriptorType`
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize)]
pub enum WrapperDescriptorKind {
    /// `FunctionType.__get__`
    FunctionTypeDunderGet,
    /// `property.__get__`
    PropertyDunderGet,
    /// `property.__set__`
    PropertyDunderSet,
    /// `property.__delete__`
    PropertyDunderDelete,
}

impl WrapperDescriptorKind {
    pub(super) fn signatures<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = Signature<'db>> {
        /// Similar to what we do in [`KnownBoundMethod::signatures`],
        /// here we also model `types.FunctionType.__get__` (or builtins.property.__get__),
        /// but now we consider a call to this as a function, i.e. we also expect the `self`
        /// argument to be passed in.
        ///
        /// TODO: Consider merging these synthesized signatures with the ones in
        /// [`KnownBoundMethod::signatures`], since that one is just this signature
        /// with the `self` parameters removed.
        fn dunder_get_signatures<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            class: KnownClass,
        ) -> [Signature<'db>; 2] {
            let type_instance = KnownClass::Type.to_instance(db, env);
            let none = Type::none(db, env);
            let descriptor = class.to_instance(db, env);
            [
                Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(descriptor),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(none),
                        Parameter::positional_only(Some(Name::new_static("owner")))
                            .with_annotated_type(type_instance),
                    ]),
                    Type::unknown(),
                ),
                Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(descriptor),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object()),
                        Parameter::positional_only(Some(Name::new_static("owner")))
                            .with_annotated_type(UnionType::from_two_elements(
                                db,
                                env,
                                type_instance,
                                none,
                            ))
                            .with_default_type(none),
                    ]),
                    Type::unknown(),
                ),
            ]
        }

        match self {
            WrapperDescriptorKind::FunctionTypeDunderGet => {
                Either::Left(dunder_get_signatures(db, env, KnownClass::FunctionType).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderGet => {
                Either::Left(dunder_get_signatures(db, env, KnownClass::Property).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderSet => {
                let object = Type::object();
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(KnownClass::Property.to_instance(db, env)),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(object),
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(object),
                    ]),
                    Type::unknown(),
                )))
            }
            WrapperDescriptorKind::PropertyDunderDelete => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(KnownClass::Property.to_instance(db, env)),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object()),
                    ]),
                    Type::unknown(),
                )))
            }
        }
    }
}
