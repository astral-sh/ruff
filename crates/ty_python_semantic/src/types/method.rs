use crate::SemanticContext;
use itertools::Either;
use ruff_python_ast::name::Name;

use crate::{
    Db,
    types::{
        CallableType, KnownClass, LiteralValueType, LiteralValueTypeKind, Parameter, Parameters,
        PropertyInstanceType, Signature, StringLiteralType, Type, TypeFormType, UnionType,
        callable::{CallableFunctionProvenance, CallableTypeKind},
        constraints::ConstraintSet,
        function::FunctionType,
        known_instance::InternedConstraintSet,
        relation::TypeRelationChecker,
        signatures::CallableSignature,
        visitor,
    },
};

/// This type represents bound method objects that are created when a method is accessed
/// on an instance of a class. For example, the expression `Path("a.txt").touch` creates
/// a bound method object that represents the `Path.touch` method which is bound to the
/// instance `Path("a.txt")`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct BoundMethodType<'db> {
    /// The function that is being bound. Corresponds to the `__func__` attribute on a
    /// bound method object
    #[returns(copy)]
    pub(crate) function: FunctionType<'db>,
    /// The instance on which this method has been called. Corresponds to the `__self__`
    /// attribute on a bound method object
    #[returns(copy)]
    pub(super) self_instance: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundMethodType<'_> {}

pub(super) fn walk_bound_method_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    method: BoundMethodType<'db>,
    visitor: &V,
) {
    let db = ctx.db();
    visitor.visit_function_type(ctx, method.function(db));
    visitor.visit_type(ctx, method.self_instance(db));
}

#[salsa::tracked]
impl<'db> BoundMethodType<'db> {
    /// Returns the type that replaces any `typing.Self` annotations in the bound method signature.
    /// This is normally the bound-instance type (the type of `self` or `cls`), but if the bound method is
    /// a `@classmethod`, then it should be an instance of that bound-instance type.
    pub(crate) fn typing_self_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        let mut self_instance = self.self_instance(db);
        if self.function(db).is_classmethod(ctx) {
            self_instance = self_instance
                .to_instance_approximation(ctx)
                .unwrap_or_else(Type::unknown);
        }
        self_instance
    }

    pub(crate) fn map_self_type(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> Self {
        Self::new(db, self.function(db), f(self.self_instance(db)))
    }

    pub(crate) fn into_callable_type(self, ctx: &SemanticContext<'db>) -> CallableType<'db> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.function(db).program(db));
        self.into_callable_type_inner(db)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|db, _, _| CallableType::bottom(db),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn into_callable_type_inner(self, db: &'db dyn Db) -> CallableType<'db> {
        let function = self.function(db);
        let ctx = SemanticContext::from_file(db, function.python_file(db));

        CallableType::new(
            db,
            self.bound_signatures_inner(db),
            CallableTypeKind::FunctionLike,
            CallableFunctionProvenance::from_function_return_annotation(
                function.has_explicit_return_annotation(&ctx),
            ),
        )
    }

    /// Converts this bound method into a callable using separate runtime-receiver and `Self` types.
    pub(crate) fn into_callable_type_with_receiver(
        self,
        ctx: &SemanticContext<'db>,
        receiver_type: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> CallableType<'db> {
        let db = ctx.db();
        let function = self.function(db);

        CallableType::new(
            db,
            self.bound_signatures_with_receiver(ctx, receiver_type, typing_self_type),
            CallableTypeKind::FunctionLike,
            CallableFunctionProvenance::from_function_return_annotation(
                function.has_explicit_return_annotation(ctx),
            ),
        )
    }

    pub(crate) fn bound_signatures(
        self,
        ctx: &SemanticContext<'db>,
    ) -> &'db CallableSignature<'db> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.function(db).program(db));
        self.bound_signatures_inner(db)
    }

    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _| CallableSignature::bottom(), heap_size=ruff_memory_usage::heap_size)]
    fn bound_signatures_inner(self, db: &'db dyn Db) -> CallableSignature<'db> {
        let function = self.function(db);
        let ctx = SemanticContext::from_file(db, function.python_file(db));
        let typing_self_type = self.typing_self_type(&ctx);
        let receiver_type = self.self_instance(db);

        self.bound_signatures_with_receiver(&ctx, receiver_type, typing_self_type)
    }

    fn bound_signatures_with_receiver(
        self,
        ctx: &SemanticContext<'db>,
        receiver_type: Type<'db>,
        typing_self_type: Type<'db>,
    ) -> CallableSignature<'db> {
        let db = ctx.db();
        let function_signature = self.function(db).signature(ctx);

        let [signature] = function_signature.overloads.as_slice() else {
            if !function_signature
                .overloads
                .iter()
                .any(Signature::has_explicit_positional_receiver_annotation)
            {
                return CallableSignature::from_overloads(function_signature.overloads.iter().map(
                    |signature| {
                        signature.bind_self_with_receiver(
                            ctx,
                            Some(receiver_type),
                            Some(typing_self_type),
                        )
                    },
                ));
            }

            return CallableSignature::from_overloads(
                function_signature
                    .overloads
                    .iter()
                    .filter(|signature| signature.can_bind_self_to(ctx, receiver_type))
                    .map(|signature| {
                        signature.bind_self_with_receiver(
                            ctx,
                            Some(receiver_type),
                            Some(typing_self_type),
                        )
                    }),
            );
        };

        CallableSignature::single(signature.bind_self_with_receiver(
            ctx,
            Some(receiver_type),
            Some(typing_self_type),
        ))
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        ctx: &SemanticContext<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let db = ctx.db();
        Some(Self::new(
            db,
            self.function(db)
                .recursive_type_normalized_impl(ctx, div, nested)?,
            self.self_instance(db)
                .recursive_type_normalized_impl(ctx, div, true)?,
        ))
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_bound_method_pair(
        &self,
        ctx: &SemanticContext<'db>,
        source: BoundMethodType<'db>,
        target: BoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let db = ctx.db();
        // A bound method is a typically a subtype of itself. However, we must explicitly verify
        // the subtyping of the underlying function signatures (since they might be specialized
        // differently), and of the bound self parameter (taking care that parameters, including a
        // bound self parameter, are contravariant.)
        self.check_function_pair(ctx, source.function(db), target.function(db))
            .and(ctx, self.constraints, || {
                self.check_type_pair(ctx, target.self_instance(db), source.self_instance(db))
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
    FunctionTypeDunderGet(FunctionType<'db>),
    /// Method wrapper for `some_function.__call__`
    FunctionTypeDunderCall(FunctionType<'db>),
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
    ConstraintSetRange,
    ConstraintSetAlways,
    ConstraintSetNever,
    ConstraintSetImpliesSubtypeOf(InternedConstraintSet<'db>),
    ConstraintSetSatisfies(InternedConstraintSet<'db>),
    ConstraintSetForAll(InternedConstraintSet<'db>),
    ConstraintSetSatisfiedByAllTypeVars(InternedConstraintSet<'db>),
    ConstraintSetSolutionsFor(InternedConstraintSet<'db>),
    ConstraintSetSolutions(InternedConstraintSet<'db>),
    ConstraintSetWithDetailedDisplay(InternedConstraintSet<'db>),
}

pub(super) fn walk_method_wrapper_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    method_wrapper: KnownBoundMethodType<'db>,
    visitor: &V,
) {
    match method_wrapper {
        KnownBoundMethodType::FunctionTypeDunderGet(function) => {
            visitor.visit_function_type(ctx, function);
        }
        KnownBoundMethodType::FunctionTypeDunderCall(function) => {
            visitor.visit_function_type(ctx, function);
        }
        KnownBoundMethodType::PropertyDunderGet(property) => {
            visitor.visit_property_instance_type(ctx, property);
        }
        KnownBoundMethodType::PropertyDunderSet(property) => {
            visitor.visit_property_instance_type(ctx, property);
        }
        KnownBoundMethodType::PropertyDunderDelete(property) => {
            visitor.visit_property_instance_type(ctx, property);
        }
        KnownBoundMethodType::StrStartswith(string_literal) => {
            visitor.visit_type(
                ctx,
                LiteralValueType::promotable(LiteralValueTypeKind::String(string_literal)).into(),
            );
        }
        KnownBoundMethodType::ConstraintSetRange
        | KnownBoundMethodType::ConstraintSetAlways
        | KnownBoundMethodType::ConstraintSetNever
        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
        | KnownBoundMethodType::ConstraintSetSatisfies(_)
        | KnownBoundMethodType::ConstraintSetForAll(_)
        | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
        | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
        | KnownBoundMethodType::ConstraintSetSolutions(_)
        | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {}
    }
}

impl<'db> KnownBoundMethodType<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        ctx: &SemanticContext<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderGet(
                    function.recursive_type_normalized_impl(ctx, div, nested)?,
                ))
            }
            KnownBoundMethodType::FunctionTypeDunderCall(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderCall(
                    function.recursive_type_normalized_impl(ctx, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderGet(property) => {
                Some(KnownBoundMethodType::PropertyDunderGet(
                    property.recursive_type_normalized_impl(ctx, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderSet(property) => {
                Some(KnownBoundMethodType::PropertyDunderSet(
                    property.recursive_type_normalized_impl(ctx, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderDelete(property) => {
                Some(KnownBoundMethodType::PropertyDunderDelete(
                    property.recursive_type_normalized_impl(ctx, div, nested)?,
                ))
            }
            KnownBoundMethodType::StrStartswith(_)
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetForAll(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
            | KnownBoundMethodType::ConstraintSetSolutions(_)
            | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => Some(self),
        }
    }

    /// Return the [`KnownClass`] that inhabitants of this type are instances of at runtime
    pub(super) fn class(self) -> KnownClass {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::FunctionTypeDunderCall(_)
            | KnownBoundMethodType::PropertyDunderGet(_)
            | KnownBoundMethodType::PropertyDunderSet(_)
            | KnownBoundMethodType::PropertyDunderDelete(_) => KnownClass::MethodWrapperType,
            KnownBoundMethodType::StrStartswith(_) => KnownClass::BuiltinFunctionType,
            KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetForAll(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
            | KnownBoundMethodType::ConstraintSetSolutions(_)
            | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {
                KnownClass::ConstraintSet
            }
        }
    }

    /// Return the signatures of this bound method type.
    ///
    /// If the bound method type is overloaded, it may have multiple signatures.
    pub(super) fn signatures(
        self,
        ctx: &SemanticContext<'db>,
    ) -> impl Iterator<Item = Signature<'db>> {
        let db = ctx.db();
        let object_type_form = || TypeFormType::from_type_expression(db, Type::object());

        match self {
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
            //
            // TODO: Consider merging these synthesized signatures with the ones in
            // [`WrapperDescriptorKind::signatures`], since this one is just that signature
            // with the `self` parameters removed.
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::PropertyDunderGet(_) => Either::Left(Either::Left(
                [
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::none(ctx)),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(KnownClass::Type.to_instance(ctx)),
                        ]),
                        Type::unknown(),
                    ),
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    ctx,
                                    KnownClass::Type.to_instance(ctx),
                                    Type::none(ctx),
                                ))
                                .with_default_type(Type::none(ctx)),
                        ]),
                        Type::unknown(),
                    ),
                ]
                .into_iter(),
            )),
            KnownBoundMethodType::FunctionTypeDunderCall(function) => Either::Left(Either::Right(
                function.signature(ctx).overloads.iter().cloned(),
            )),
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
                                ctx,
                                KnownClass::Str.to_instance(ctx),
                                Type::homogeneous_tuple(db, KnownClass::Str.to_instance(ctx)),
                            )),
                        Parameter::positional_only(Some(Name::new_static("start")))
                            .with_annotated_type(UnionType::from_two_elements(
                                ctx,
                                KnownClass::SupportsIndex.to_instance(ctx),
                                Type::none(ctx),
                            ))
                            .with_default_type(Type::none(ctx)),
                        Parameter::positional_only(Some(Name::new_static("end")))
                            .with_annotated_type(UnionType::from_two_elements(
                                ctx,
                                KnownClass::SupportsIndex.to_instance(ctx),
                                Type::none(ctx),
                            ))
                            .with_default_type(Type::none(ctx)),
                    ]),
                    KnownClass::Bool.to_instance(ctx),
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
                    KnownClass::ConstraintSet.to_instance(ctx),
                )))
            }

            KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    KnownClass::ConstraintSet.to_instance(ctx),
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
                    KnownClass::ConstraintSet.to_instance(ctx),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfies(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "other",
                    )))
                    .with_annotated_type(KnownClass::ConstraintSet.to_instance(ctx))]),
                    KnownClass::ConstraintSet.to_instance(ctx),
                )))
            }

            KnownBoundMethodType::ConstraintSetForAll(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "typevars",
                    )))
                    .with_annotated_type(TypeFormType::from_type_expression(
                        db,
                        Type::homogeneous_tuple(db, Type::object()),
                    ))]),
                    KnownClass::ConstraintSet.to_instance(ctx),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::keyword_only(Name::new_static("inferable"))
                        .with_annotated_type(UnionType::from_two_elements(
                            ctx,
                            TypeFormType::from_type_expression(
                                db,
                                Type::homogeneous_tuple(db, Type::object()),
                            ),
                            Type::none(ctx),
                        ))
                        .with_default_type(Type::none(ctx))]),
                    KnownClass::Bool.to_instance(ctx),
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
                                Type::homogeneous_tuple(db, Type::object()),
                            ),
                        ),
                    ]),
                    UnionType::from_two_elements(
                        ctx,
                        Type::homogeneous_tuple(
                            db,
                            KnownClass::ConstraintSetSolution.to_instance(ctx),
                        ),
                        Type::none(ctx),
                    ),
                )))
            }

            KnownBoundMethodType::ConstraintSetSolutions(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([Parameter::keyword_only(Name::new_static("inferable"))
                        .with_annotated_type(TypeFormType::from_type_expression(
                            db,
                            Type::homogeneous_tuple(db, Type::object()),
                        ))]),
                    UnionType::from_two_elements(
                        ctx,
                        Type::homogeneous_tuple(
                            db,
                            KnownClass::ConstraintSetSolution.to_instance(ctx),
                        ),
                        Type::none(ctx),
                    ),
                )))
            }

            KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    KnownClass::ConstraintSet.to_instance(ctx),
                )))
            }
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_known_bound_method_pair(
        &self,
        ctx: &SemanticContext<'db>,
        source: KnownBoundMethodType<'db>,
        target: KnownBoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match (source, target) {
            (
                KnownBoundMethodType::FunctionTypeDunderGet(source_function),
                KnownBoundMethodType::FunctionTypeDunderGet(target_function),
            ) => self.check_function_pair(ctx, source_function, target_function),

            (
                KnownBoundMethodType::FunctionTypeDunderCall(source_function),
                KnownBoundMethodType::FunctionTypeDunderCall(target_function),
            ) => self.check_function_pair(ctx, source_function, target_function),

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
            ) => self.check_property_instance_pair(ctx, source_property, target_property),

            (KnownBoundMethodType::StrStartswith(_), KnownBoundMethodType::StrStartswith(_)) => {
                ConstraintSet::from_bool(self.constraints, source == target)
            }

            (
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
                KnownBoundMethodType::ConstraintSetForAll(_),
                KnownBoundMethodType::ConstraintSetForAll(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
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
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
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
        ctx: &SemanticContext<'db>,
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
            ctx: &SemanticContext<'db>,
            class: KnownClass,
        ) -> [Signature<'db>; 2] {
            let type_instance = KnownClass::Type.to_instance(ctx);
            let none = Type::none(ctx);
            let descriptor = class.to_instance(ctx);
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
                                ctx,
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
                Either::Left(dunder_get_signatures(ctx, KnownClass::FunctionType).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderGet => {
                Either::Left(dunder_get_signatures(ctx, KnownClass::Property).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderSet => {
                let object = Type::object();
                Either::Right(std::iter::once(Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(KnownClass::Property.to_instance(ctx)),
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
                            .with_annotated_type(KnownClass::Property.to_instance(ctx)),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object()),
                    ]),
                    Type::unknown(),
                )))
            }
        }
    }
}
