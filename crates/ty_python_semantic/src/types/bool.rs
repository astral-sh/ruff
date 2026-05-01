use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_text_size::{Ranged, TextRange};

use crate::{
    Db,
    types::{
        CallArguments, CallDunderError, ClassType, CycleDetector, KnownClass, KnownInstanceType,
        LiteralValueTypeKind, SubclassOfInner, Type, TypeContext, TypeVarBoundOrConstraints,
        UnionType, call::CallErrorKind, constraints::ConstraintSetBuilder, context::InferContext,
        diagnostic::UNSUPPORTED_BOOL_CONVERSION, typed_dict::TypedDictField,
    },
};
use ty_python_core::Truthiness;

impl<'db> Type<'db> {
    /// Resolves the boolean value of the type and falls back to [`Truthiness::Ambiguous`] if the type doesn't implement `__bool__` correctly.
    ///
    /// This method should only be used outside type checking or when evaluating if a type
    /// is truthy or falsy in a context where Python doesn't make an implicit `bool` call.
    /// Use [`try_bool`](Self::try_bool) for type checking or implicit `bool` calls.
    pub(crate) fn bool(&self, db: &'db dyn Db) -> Truthiness {
        self.try_bool_impl(db, true, &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)))
            .unwrap_or_else(|err| err.fallback_truthiness())
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    ///
    /// Returns an error if the type doesn't implement `__bool__` correctly.
    pub(crate) fn try_bool(&self, db: &'db dyn Db) -> Result<Truthiness, BoolError<'db>> {
        self.try_bool_impl(db, false, &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)))
    }

    /// Resolves the boolean value of a type.
    ///
    /// Setting `allow_short_circuit` to `true` allows the implementation to
    /// early return if the bool value of any union variant is `Truthiness::Ambiguous`.
    /// Early returning shows a 1-2% perf improvement on our benchmarks because
    /// `bool` (which doesn't care about errors) is used heavily when evaluating statically known branches.
    ///
    /// An alternative to this flag is to implement a trait similar to Rust's `Try` trait.
    /// The advantage of that is that it would allow collecting the errors as well. However,
    /// it is significantly more complex and duplicating the logic into `bool` without the error
    /// handling didn't show any significant performance difference to when using the `allow_short_circuit` flag.
    #[inline]
    fn try_bool_impl(
        &self,
        db: &'db dyn Db,
        allow_short_circuit: bool,
        visitor: &TryBoolVisitor<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        let type_to_truthiness = |ty: Type<'db>| {
            match ty.as_literal_value_kind() {
                Some(LiteralValueTypeKind::Bool(bool_val)) => Truthiness::from(bool_val),
                Some(LiteralValueTypeKind::Int(int_val)) => Truthiness::from(int_val.as_i64() != 0),
                // anything else is handled lower down
                _ => Truthiness::Ambiguous,
            }
        };

        let try_dunders = || {
            match self.try_call_dunder(
                db,
                "__bool__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(outcome) => {
                    let return_type = outcome.return_type(db);
                    if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type,
                            not_boolable_type: *self,
                        });
                    }
                    Ok(type_to_truthiness(return_type))
                }

                Err(CallDunderError::PossiblyUnbound {
                    bindings: outcome, ..
                }) => {
                    let return_type = outcome.return_type(db);
                    if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type: outcome.return_type(db),
                            not_boolable_type: *self,
                        });
                    }

                    // Don't trust possibly missing `__bool__` method.
                    Ok(Truthiness::Ambiguous)
                }

                Err(CallDunderError::MethodNotAvailable) => {
                    // We only consider `__len__` for tuples and `@final` types,
                    // since `__bool__` takes precedence
                    // and a subclass could add a `__bool__` method.
                    //
                    // TODO: with regards to tuple types, we intend to emit a diagnostic
                    // if a tuple subclass defines a `__bool__` method with a return type
                    // that is inconsistent with the tuple's length. Otherwise, the special
                    // handling for tuples here isn't sound.
                    if let Some(instance) = self.as_nominal_instance() {
                        if let Some(tuple_spec) = instance.tuple_spec(db) {
                            Ok(tuple_spec.truthiness())
                        } else if instance.class(db).is_final(db) {
                            match self.try_call_dunder(
                                db,
                                "__len__",
                                CallArguments::none(),
                                TypeContext::default(),
                            ) {
                                Ok(outcome) => {
                                    let return_type = outcome.return_type(db);
                                    if return_type.is_assignable_to(
                                        db,
                                        KnownClass::SupportsIndex.to_instance(db),
                                    ) {
                                        Ok(type_to_truthiness(return_type))
                                    } else {
                                        // TODO: should report a diagnostic similar to if return type of `__bool__`
                                        // is not assignable to `bool`
                                        Ok(Truthiness::Ambiguous)
                                    }
                                }
                                // if a `@final` type does not define `__bool__` or `__len__`, it is always truthy
                                Err(CallDunderError::MethodNotAvailable) => {
                                    Ok(Truthiness::AlwaysTrue)
                                }
                                // TODO: errors during a `__len__` call (if `__len__` exists) should be reported
                                // as diagnostics similar to errors during a `__bool__` call (when `__bool__` exists)
                                Err(_) => Ok(Truthiness::Ambiguous),
                            }
                        } else {
                            Ok(Truthiness::Ambiguous)
                        }
                    } else {
                        Ok(Truthiness::Ambiguous)
                    }
                }

                Err(CallDunderError::CallError(CallErrorKind::BindingError, bindings)) => {
                    Err(BoolError::IncorrectArguments {
                        truthiness: type_to_truthiness(bindings.return_type(db)),
                        not_boolable_type: *self,
                    })
                }

                Err(CallDunderError::CallError(CallErrorKind::NotCallable, _)) => {
                    Err(BoolError::NotCallable {
                        not_boolable_type: *self,
                    })
                }

                Err(CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _)) => {
                    Err(BoolError::Other {
                        not_boolable_type: *self,
                    })
                }
            }
        };

        let try_union = |union: UnionType<'db>| {
            let mut truthiness = None;
            let mut all_not_callable = true;
            let mut has_errors = false;

            for element in union.elements(db) {
                let element_truthiness =
                    match element.try_bool_impl(db, allow_short_circuit, visitor) {
                        Ok(truthiness) => truthiness,
                        Err(err) => {
                            has_errors = true;
                            all_not_callable &= matches!(err, BoolError::NotCallable { .. });
                            err.fallback_truthiness()
                        }
                    };

                truthiness.get_or_insert(element_truthiness);

                if Some(element_truthiness) != truthiness {
                    truthiness = Some(Truthiness::Ambiguous);

                    if allow_short_circuit {
                        return Ok(Truthiness::Ambiguous);
                    }
                }
            }

            if has_errors {
                if all_not_callable {
                    return Err(BoolError::NotCallable {
                        not_boolable_type: *self,
                    });
                }
                return Err(BoolError::Union {
                    union,
                    truthiness: truthiness.unwrap_or(Truthiness::Ambiguous),
                });
            }
            Ok(truthiness.unwrap_or(Truthiness::Ambiguous))
        };

        let truthiness = match self {
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::Callable(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDictTop => Truthiness::Ambiguous,

            Type::TypedDict(td) => {
                if td.items(db).values().any(TypedDictField::is_required) {
                    Truthiness::AlwaysTrue
                } else {
                    // We can potentially infer empty typeddicts as always falsy if they're `closed=True`,
                    // but as of 22-01-26 we don't yet support PEP 728.
                    Truthiness::Ambiguous
                }
            }

            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked_set)) => {
                let constraints = ConstraintSetBuilder::new();
                let tracked_set = constraints.load(db, tracked_set.constraints(db));
                Truthiness::from(tracked_set.is_always_satisfied(db))
            }

            Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::PropertyInstance(_)
            | Type::BoundSuper(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::AlwaysTruthy => Truthiness::AlwaysTrue,

            Type::AlwaysFalsy => Truthiness::AlwaysFalse,

            Type::ClassLiteral(class) => {
                class
                    .metaclass_instance_type(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)?
            }
            Type::GenericAlias(alias) => ClassType::from(*alias)
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit, visitor)?,

            Type::SubclassOf(subclass_of_ty) => {
                match subclass_of_ty.subclass_of().with_transposed_type_var(db) {
                    SubclassOfInner::Dynamic(_) => Truthiness::Ambiguous,
                    SubclassOfInner::Class(class) => {
                        Type::from(class).try_bool_impl(db, allow_short_circuit, visitor)?
                    }
                    SubclassOfInner::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar)
                        .try_bool_impl(db, allow_short_circuit, visitor)?,
                }
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Truthiness::Ambiguous,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.try_bool_impl(db, allow_short_circuit, visitor)?
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .as_type(db)
                        .try_bool_impl(db, allow_short_circuit, visitor)?,
                }
            }

            Type::NominalInstance(instance) => instance
                .known_class(db)
                .and_then(KnownClass::bool)
                .map(Ok)
                .unwrap_or_else(try_dunders)?,

            Type::ProtocolInstance(_) => try_dunders()?,

            Type::Union(union) => try_union(*union)?,

            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::LiteralString => Truthiness::Ambiguous,
                LiteralValueTypeKind::Enum(enum_type) => enum_type
                    .enum_class_instance(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)?,

                LiteralValueTypeKind::Int(num) => Truthiness::from(num.as_i64() != 0),
                LiteralValueTypeKind::Bool(bool) => Truthiness::from(bool),
                LiteralValueTypeKind::String(str) => Truthiness::from(!str.value(db).is_empty()),
                LiteralValueTypeKind::Bytes(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            },

            Type::TypeAlias(alias) => visitor.visit(*self, || {
                alias
                    .value_type(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)
            })?,
            Type::NewTypeInstance(newtype) => {
                newtype
                    .concrete_base_type(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)?
            }
        };

        Ok(truthiness)
    }
}

/// A [`CycleDetector`] that is used in `try_bool` methods.
pub(crate) type TryBoolVisitor<'db> =
    CycleDetector<TryBool, Type<'db>, Result<Truthiness, BoolError<'db>>>;
pub(crate) struct TryBool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoolError<'db> {
    /// The type has a `__bool__` attribute but it can't be called.
    NotCallable { not_boolable_type: Type<'db> },

    /// The type has a callable `__bool__` attribute, but it isn't callable
    /// with the given arguments.
    IncorrectArguments {
        not_boolable_type: Type<'db>,
        truthiness: Truthiness,
    },

    /// The type has a `__bool__` method, is callable with the given arguments,
    /// but the return type isn't assignable to `bool`.
    IncorrectReturnType {
        not_boolable_type: Type<'db>,
        return_type: Type<'db>,
    },

    /// A union type doesn't implement `__bool__` correctly.
    Union {
        union: UnionType<'db>,
        truthiness: Truthiness,
    },

    /// Any other reason why the type can't be converted to a bool.
    /// E.g. because calling `__bool__` returns in a union type and not all variants support `__bool__` or
    /// because `__bool__` points to a type that has a possibly missing `__call__` method.
    Other { not_boolable_type: Type<'db> },
}

impl<'db> BoolError<'db> {
    pub(super) fn fallback_truthiness(&self) -> Truthiness {
        match self {
            BoolError::NotCallable { .. }
            | BoolError::IncorrectReturnType { .. }
            | BoolError::Other { .. } => Truthiness::Ambiguous,
            BoolError::IncorrectArguments { truthiness, .. }
            | BoolError::Union { truthiness, .. } => *truthiness,
        }
    }

    fn not_boolable_type(&self) -> Type<'db> {
        match self {
            BoolError::NotCallable {
                not_boolable_type, ..
            }
            | BoolError::IncorrectArguments {
                not_boolable_type, ..
            }
            | BoolError::Other { not_boolable_type }
            | BoolError::IncorrectReturnType {
                not_boolable_type, ..
            } => *not_boolable_type,
            BoolError::Union { union, .. } => Type::Union(*union),
        }
    }

    pub(super) fn report_diagnostic(&self, context: &InferContext, condition: impl Ranged) {
        self.report_diagnostic_impl(context, condition.range());
    }

    fn report_diagnostic_impl(&self, context: &InferContext, condition: TextRange) {
        let Some(builder) = context.report_lint(&UNSUPPORTED_BOOL_CONVERSION, condition) else {
            return;
        };
        match self {
            Self::IncorrectArguments {
                not_boolable_type, ..
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`",
                    not_boolable_type.display(context.db())
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "`__bool__` methods must only have a `self` parameter",
                );
                if let Some((func_span, parameter_span)) = not_boolable_type
                    .member(context.db(), "__bool__")
                    .into_lookup_result(context.db())
                    .ok()
                    .and_then(|quals| quals.inner_type().parameter_span(context.db(), None))
                {
                    sub.annotate(
                        Annotation::primary(parameter_span).message("Incorrect parameters"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::IncorrectReturnType {
                not_boolable_type,
                return_type,
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{not_boolable}`",
                    not_boolable = not_boolable_type.display(context.db()),
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`{return_type}` is not assignable to `bool`",
                        return_type = return_type.display(context.db()),
                    ),
                );
                if let Some((func_span, return_type_span)) = not_boolable_type
                    .member(context.db(), "__bool__")
                    .into_lookup_result(context.db())
                    .ok()
                    .and_then(|quals| quals.inner_type().function_spans(context.db()))
                    .and_then(|spans| Some((spans.name, spans.return_type?)))
                {
                    sub.annotate(
                        Annotation::primary(return_type_span).message("Incorrect return type"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::NotCallable { not_boolable_type } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`",
                    not_boolable_type.display(context.db())
                ));
                let sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`__bool__` on `{}` must be callable",
                        not_boolable_type.display(context.db())
                    ),
                );
                // TODO: It would be nice to create an annotation here for
                // where `__bool__` is defined. At time of writing, I couldn't
                // figure out a straight-forward way of doing this. ---AG
                diag.sub(sub);
            }
            Self::Union { union, .. } => {
                let first_error = union
                    .elements(context.db())
                    .iter()
                    .find_map(|element| element.try_bool(context.db()).err())
                    .unwrap();

                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for union `{}` \
                     because `{}` doesn't implement `__bool__` correctly",
                    Type::Union(*union).display(context.db()),
                    first_error.not_boolable_type().display(context.db()),
                ));
            }

            Self::Other { not_boolable_type } => {
                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`; \
                     it incorrectly implements `__bool__`",
                    not_boolable_type.display(context.db())
                ));
            }
        }
    }
}
