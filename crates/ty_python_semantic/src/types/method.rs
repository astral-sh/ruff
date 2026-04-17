use itertools::Either;
use ruff_python_ast::name::Name;

use crate::{
    Db,
    types::{
        CallableType, KnownClass, LiteralValueType, LiteralValueTypeKind, Parameter, Parameters,
        PropertyInstanceType, Signature, StringLiteralType, Type, UnionType,
        callable::CallableTypeKind, constraints::ConstraintSet, function::FunctionType,
        known_instance::InternedConstraintSet, relation::TypeRelationChecker,
        signatures::CallableSignature, visitor,
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
    pub(crate) function: FunctionType<'db>,
    /// The instance on which this method has been called. Corresponds to the `__self__`
    /// attribute on a bound method object
    pub(super) self_instance: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundMethodType<'_> {}

pub(super) fn walk_bound_method_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method: BoundMethodType<'db>,
    visitor: &V,
) {
    visitor.visit_function_type(db, method.function(db));
    visitor.visit_type(db, method.self_instance(db));
}

fn into_callable_type_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _self: BoundMethodType<'db>,
) -> CallableType<'db> {
    CallableType::bottom(db)
}

#[salsa::tracked]
impl<'db> BoundMethodType<'db> {
    /// Returns the type that replaces any `typing.Self` annotations in the bound method signature.
    /// This is normally the bound-instance type (the type of `self` or `cls`), but if the bound method is
    /// a `@classmethod`, then it should be an instance of that bound-instance type.
    pub(crate) fn typing_self_type(self, db: &'db dyn Db) -> Type<'db> {
        let mut self_instance = self.self_instance(db);
        if self.function(db).is_classmethod(db) {
            self_instance = self_instance.to_instance(db).unwrap_or_else(Type::unknown);
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

    #[salsa::tracked(cycle_initial=into_callable_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        CallableType::new(
            db,
            self.bound_signatures(db),
            CallableTypeKind::FunctionLike,
        )
    }

    #[salsa::tracked(cycle_initial=|_, _, _| CallableSignature::bottom(), heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn bound_signatures(self, db: &'db dyn Db) -> CallableSignature<'db> {
        let function_signature = self.function(db).signature(db);
        let typing_self_type = self.typing_self_type(db);

        let [signature] = function_signature.overloads.as_slice() else {
            let self_instance = self.self_instance(db);
            let bind_all_overloads = || {
                CallableSignature::from_overloads(
                    function_signature
                        .overloads
                        .iter()
                        .map(|signature| signature.bind_self(db, Some(typing_self_type))),
                )
            };

            // A gradual receiver can satisfy any receiver-specific overload, so filtering cannot
            // safely discard candidates.
            if self_instance.has_dynamic(db) {
                return bind_all_overloads();
            }

            let mut applicable_overloads = function_signature
                .overloads
                .iter()
                .filter(|signature| signature.can_bind_self_to(db, self_instance))
                .map(|signature| signature.bind_self(db, Some(typing_self_type)))
                .peekable();

            // If no overload accepts the bound receiver, keep the full overload set so a later call
            // still reports the existing invalid-`self` diagnostic instead of becoming non-callable.
            return if applicable_overloads.peek().is_none() {
                bind_all_overloads()
            } else {
                CallableSignature::from_overloads(applicable_overloads)
            };
        };

        CallableSignature::single(signature.bind_self(db, Some(typing_self_type)))
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.function(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.self_instance(db)
                .recursive_type_normalized_impl(db, div, true)?,
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
        // A bound method is a typically a subtype of itself. However, we must explicitly verify
        // the subtyping of the underlying function signatures (since they might be specialized
        // differently), and of the bound self parameter (taking care that parameters, including a
        // bound self parameter, are contravariant.)
        self.check_function_pair(db, source.function(db), target.function(db))
            .and(db, self.constraints, || {
                self.check_type_pair(db, target.self_instance(db), source.self_instance(db))
            })
    }
}

/// Represents a specific instance of a bound method type for a builtin class.
///
/// Unlike bound methods of user-defined classes, these are not generally instances
/// of `types.BoundMethodType` at runtime.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
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
    ConstraintSetSatisfiedByAllTypeVars(InternedConstraintSet<'db>),
}

pub(super) fn walk_method_wrapper_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method_wrapper: KnownBoundMethodType<'db>,
    visitor: &V,
) {
    match method_wrapper {
        KnownBoundMethodType::FunctionTypeDunderGet(function) => {
            visitor.visit_function_type(db, function);
        }
        KnownBoundMethodType::FunctionTypeDunderCall(function) => {
            visitor.visit_function_type(db, function);
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
        KnownBoundMethodType::ConstraintSetRange
        | KnownBoundMethodType::ConstraintSetAlways
        | KnownBoundMethodType::ConstraintSetNever
        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
        | KnownBoundMethodType::ConstraintSetSatisfies(_)
        | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {}
    }
}

impl<'db> KnownBoundMethodType<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderGet(
                    function.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::FunctionTypeDunderCall(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderCall(
                    function.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderGet(property) => {
                Some(KnownBoundMethodType::PropertyDunderGet(
                    property.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderSet(property) => {
                Some(KnownBoundMethodType::PropertyDunderSet(
                    property.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderDelete(property) => {
                Some(KnownBoundMethodType::PropertyDunderDelete(
                    property.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::StrStartswith(_)
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => Some(self),
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
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {
                KnownClass::ConstraintSet
            }
        }
    }

    /// Return the signatures of this bound method type.
    ///
    /// If the bound method type is overloaded, it may have multiple signatures.
    pub(super) fn signatures(self, db: &'db dyn Db) -> impl Iterator<Item = Signature<'db>> {
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
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::none(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(KnownClass::Type.to_instance(db)),
                            ],
                        ),
                        Type::unknown(),
                    ),
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::object()),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        KnownClass::Type.to_instance(db),
                                        Type::none(db),
                                    ))
                                    .with_default_type(Type::none(db)),
                            ],
                        ),
                        Type::unknown(),
                    ),
                ]
                .into_iter(),
            )),
            KnownBoundMethodType::FunctionTypeDunderCall(function) => Either::Left(Either::Right(
                function.signature(db).overloads.iter().cloned(),
            )),
            KnownBoundMethodType::PropertyDunderSet(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("value")))
                                .with_annotated_type(Type::object()),
                        ],
                    ),
                    Type::unknown(),
                )))
            }
            KnownBoundMethodType::PropertyDunderDelete(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                        ],
                    ),
                    Type::unknown(),
                )))
            }
            KnownBoundMethodType::StrStartswith(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("prefix")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    db,
                                    KnownClass::Str.to_instance(db),
                                    Type::homogeneous_tuple(db, KnownClass::Str.to_instance(db)),
                                )),
                            Parameter::positional_only(Some(Name::new_static("start")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    db,
                                    KnownClass::SupportsIndex.to_instance(db),
                                    Type::none(db),
                                ))
                                .with_default_type(Type::none(db)),
                            Parameter::positional_only(Some(Name::new_static("end")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    db,
                                    KnownClass::SupportsIndex.to_instance(db),
                                    Type::none(db),
                                ))
                                .with_default_type(Type::none(db)),
                        ],
                    ),
                    KnownClass::Bool.to_instance(db),
                )))
            }

            KnownBoundMethodType::ConstraintSetRange => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("lower_bound")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("typevar")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("upper_bound")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ],
                    ),
                    KnownClass::ConstraintSet.to_instance(db),
                )))
            }

            KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    KnownClass::ConstraintSet.to_instance(db),
                )))
            }

            KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("ty")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("of")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ],
                    ),
                    KnownClass::ConstraintSet.to_instance(db),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfies(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("other")))
                            .with_annotated_type(KnownClass::ConstraintSet.to_instance(db))],
                    ),
                    KnownClass::ConstraintSet.to_instance(db),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::keyword_only(Name::new_static("inferable"))
                            .type_form()
                            .with_annotated_type(UnionType::from_two_elements(
                                db,
                                Type::homogeneous_tuple(db, Type::any()),
                                Type::none(db),
                            ))
                            .with_default_type(Type::none(db))],
                    ),
                    KnownClass::Bool.to_instance(db),
                )))
            }
        }
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
            ) => self.check_function_pair(db, source_function, target_function),

            (
                KnownBoundMethodType::FunctionTypeDunderCall(source_function),
                KnownBoundMethodType::FunctionTypeDunderCall(target_function),
            ) => self.check_function_pair(db, source_function, target_function),

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
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
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
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
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
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
            ) => self.never(),
        }
    }
}

/// Represents a specific instance of `types.WrapperDescriptorType`
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
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
    pub(super) fn signatures(self, db: &dyn Db) -> impl Iterator<Item = Signature<'_>> {
        /// Similar to what we do in [`KnownBoundMethod::signatures`],
        /// here we also model `types.FunctionType.__get__` (or builtins.property.__get__),
        /// but now we consider a call to this as a function, i.e. we also expect the `self`
        /// argument to be passed in.
        ///
        /// TODO: Consider merging these synthesized signatures with the ones in
        /// [`KnownBoundMethod::signatures`], since that one is just this signature
        /// with the `self` parameters removed.
        fn dunder_get_signatures(db: &dyn Db, class: KnownClass) -> [Signature<'_>; 2] {
            let type_instance = KnownClass::Type.to_instance(db);
            let none = Type::none(db);
            let descriptor = class.to_instance(db);
            [
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(descriptor),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(none),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(type_instance),
                        ],
                    ),
                    Type::unknown(),
                ),
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(descriptor),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(UnionType::from_two_elements(
                                    db,
                                    type_instance,
                                    none,
                                ))
                                .with_default_type(none),
                        ],
                    ),
                    Type::unknown(),
                ),
            ]
        }

        match self {
            WrapperDescriptorKind::FunctionTypeDunderGet => {
                Either::Left(dunder_get_signatures(db, KnownClass::FunctionType).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderGet => {
                Either::Left(dunder_get_signatures(db, KnownClass::Property).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderSet => {
                let object = Type::object();
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(KnownClass::Property.to_instance(db)),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(object),
                            Parameter::positional_only(Some(Name::new_static("value")))
                                .with_annotated_type(object),
                        ],
                    ),
                    Type::unknown(),
                )))
            }
            WrapperDescriptorKind::PropertyDunderDelete => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(KnownClass::Property.to_instance(db)),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                        ],
                    ),
                    Type::unknown(),
                )))
            }
        }
    }
}
