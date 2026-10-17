use crate::Db;
use crate::ProgramEnvironment;
use crate::types::typevar::TypeVarSet;
use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_text_size::{Ranged, TextRange};

use crate::types::{
    CallArguments, CallDunderError, ClassType, CycleKind, KnownClass, KnownInstanceType,
    LiteralValueTypeKind, PropertyInstanceClass, SubclassOfInner, Type, TypeContext, TypeQuery,
    TypeQueryContext, TypeVarBoundOrConstraints, UnionType, call::CallErrorKind,
    constraints::ConstraintSetBuilder, context::InferContext,
    diagnostic::UNSUPPORTED_BOOL_CONVERSION, typed_dict::TypedDictField,
};
use ty_python_core::Truthiness;

impl<'db> Type<'db> {
    /// Resolves the boolean value of the type and falls back to [`Truthiness::Ambiguous`] if the type doesn't implement `__bool__` correctly.
    ///
    /// This method should only be used outside type checking or when evaluating if a type
    /// is truthy or falsy in a context where Python doesn't make an implicit `bool` call.
    /// Use [`try_bool`](Self::try_bool) for type checking or implicit `bool` calls.
    ///
    /// Uninhabited types return [`Truthiness::Uninhabited`]: neither boolean outcome is possible.
    /// This describes the value type; a compound expression can also fail to complete because
    /// of its operands, even when its inferred result type is inhabited.
    pub(crate) fn bool(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Truthiness {
        TypeQueryContext::with_root(
            env,
            TryBool {
                allow_short_circuit: true,
            },
            |root| root.visit(db, *self),
        )
        .unwrap_or_else(|err| err.fallback_truthiness())
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    ///
    /// Returns an error if the type doesn't implement `__bool__` correctly.
    pub(crate) fn try_bool(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        TypeQueryContext::with_root(
            env,
            TryBool {
                allow_short_circuit: false,
            },
            |root| root.visit(db, *self),
        )
    }
}

struct TryBool {
    allow_short_circuit: bool,
}

impl<'db> TypeQuery<'db> for TryBool {
    type Output = Result<Truthiness, BoolError<'db>>;

    fn on_cycle(
        &self,
        _traversal: &TypeQueryContext<'_, 'db, Self>,
        _ty: Type<'db>,
        kind: CycleKind,
        _state: (),
    ) -> Self::Output {
        Ok(match kind {
            // An exact back-edge adds no alternatives to the enclosing union. A different
            // specialization can introduce values with either truthiness.
            CycleKind::Exact => Truthiness::Uninhabited,
            CycleKind::MatchingIdentity => Truthiness::Ambiguous,
        })
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
    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &TypeQueryContext<'_, 'db, Self>,
        ty: Type<'db>,
        _state: (),
    ) -> Self::Output {
        let env = traversal.environment();
        let allow_short_circuit = self.allow_short_circuit;
        let ty = &ty;
        let type_to_truthiness = |ty: Type<'db>| {
            match ty.as_literal_value_kind() {
                Some(LiteralValueTypeKind::Bool(bool_val)) => Truthiness::from(bool_val),
                Some(LiteralValueTypeKind::Int(int_val)) => Truthiness::from(int_val.as_i64() != 0),
                // anything else is handled lower down
                _ => Truthiness::Ambiguous,
            }
        };

        let try_dunders = || {
            match ty.try_call_dunder(
                db,
                env,
                "__bool__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(outcome) => {
                    let return_type = outcome.return_type(db, env);
                    if !return_type.is_assignable_to(db, env, KnownClass::Bool.to_instance(db, env))
                    {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type,
                            not_boolable_type: *ty,
                        });
                    }
                    Ok(type_to_truthiness(return_type))
                }

                Err(CallDunderError::PossiblyUnbound {
                    bindings: outcome, ..
                }) => {
                    let return_type = outcome.return_type(db, env);
                    if !return_type.is_assignable_to(db, env, KnownClass::Bool.to_instance(db, env))
                    {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type: outcome.return_type(db, env),
                            not_boolable_type: *ty,
                        });
                    }

                    // Don't trust possibly missing `__bool__` method.
                    Ok(Truthiness::Ambiguous)
                }

                // TODO: with regards to tuple types, we intend to emit a diagnostic
                // if a tuple subclass defines a `__bool__` method with a return type
                // that is inconsistent with the tuple's length. Otherwise, the special
                // handling for tuples here isn't sound.
                Err(CallDunderError::MethodNotAvailable)
                    if let Type::NominalInstance(instance) = ty
                        && let Some(tuple_spec) = instance.tuple_spec(db, env) =>
                {
                    Ok(tuple_spec.truthiness())
                }

                // We only consider `__len__` for tuples and `@final` types,
                // since `__bool__` takes precedence
                // and a subclass could add a `__bool__` method.
                Err(CallDunderError::MethodNotAvailable)
                    if let Type::NominalInstance(instance) = ty
                        && instance.class(db, env).is_final(db) =>
                {
                    match ty.try_call_dunder(
                        db,
                        env,
                        "__len__",
                        CallArguments::none(),
                        TypeContext::default(),
                    ) {
                        Ok(outcome) => {
                            let return_type = outcome.return_type(db, env);
                            if return_type.is_assignable_to(
                                db,
                                env,
                                KnownClass::SupportsIndex.to_instance(db, env),
                            ) {
                                Ok(type_to_truthiness(return_type))
                            } else {
                                // TODO: should report a diagnostic similar to if return type of `__bool__`
                                // is not assignable to `bool`
                                Ok(Truthiness::Ambiguous)
                            }
                        }
                        // if a `@final` type does not define `__bool__` or `__len__`, it is always truthy
                        Err(CallDunderError::MethodNotAvailable) => Ok(Truthiness::AlwaysTrue),
                        // TODO: errors during a `__len__` call (if `__len__` exists) should be reported
                        // as diagnostics similar to errors during a `__bool__` call (when `__bool__` exists)
                        Err(_) => Ok(Truthiness::Ambiguous),
                    }
                }

                Err(CallDunderError::MethodNotAvailable) => Ok(Truthiness::Ambiguous),

                Err(CallDunderError::CallError(CallErrorKind::BindingError, bindings, _)) => {
                    Err(BoolError::IncorrectArguments {
                        truthiness: type_to_truthiness(bindings.return_type(db, env)),
                        not_boolable_type: *ty,
                    })
                }

                Err(CallDunderError::CallError(CallErrorKind::NotCallable, _, _)) => {
                    Err(BoolError::NotCallable {
                        not_boolable_type: *ty,
                    })
                }

                Err(CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _, _)) => {
                    Err(BoolError::Other {
                        not_boolable_type: *ty,
                    })
                }
            }
        };

        let try_union = |union: UnionType<'db>| {
            let mut truthiness = Truthiness::Uninhabited;
            let mut all_not_callable = true;
            let mut has_errors = false;

            for element in union.elements(db) {
                let element_truthiness = match traversal.visit(db, *element) {
                    Ok(truthiness) => truthiness,
                    Err(err) => {
                        has_errors = true;
                        all_not_callable &= matches!(err, BoolError::NotCallable { .. });
                        err.fallback_truthiness()
                    }
                };

                truthiness = truthiness.union(element_truthiness);
                if allow_short_circuit && truthiness.is_ambiguous() {
                    return Ok(Truthiness::Ambiguous);
                }
            }

            if has_errors {
                if all_not_callable {
                    return Err(BoolError::NotCallable {
                        not_boolable_type: *ty,
                    });
                }
                return Err(BoolError::Union { union, truthiness });
            }
            Ok(truthiness)
        };

        let truthiness = match ty {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Callable(callable) if callable.runtime_class(db).is_some() => {
                Truthiness::AlwaysTrue
            }

            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Callable(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypeAlias(_)
            | Type::Recursive(_) => Truthiness::Ambiguous,

            Type::Never => Truthiness::Uninhabited,

            Type::TypedDict(td) => {
                if td.items(db).values().any(TypedDictField::is_required) {
                    Truthiness::AlwaysTrue
                } else if td.openness(db).is_closed()
                    && td.items(db).values().all(|field| !field.may_be_present(db))
                {
                    Truthiness::AlwaysFalse
                } else {
                    Truthiness::Ambiguous
                }
            }

            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked_set)) => {
                let constraints = ConstraintSetBuilder::new();
                let tracked_set = constraints.load(db, env, tracked_set.constraints(db));
                Truthiness::from(tracked_set.is_always_satisfied(db, env, TypeVarSet::None))
            }

            Type::KnownInstance(KnownInstanceType::Range { is_non_empty }) => {
                Truthiness::from(*is_non_empty)
            }

            Type::PropertyInstance(property)
                if let PropertyInstanceClass::Subclass(class) = property.instance_class(db) =>
            {
                traversal.visit(db, Type::instance(db, env, class))?
            }

            Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::BoundSuper(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::AlwaysTruthy => Truthiness::AlwaysTrue,

            Type::AlwaysFalsy => Truthiness::AlwaysFalse,

            Type::ClassLiteral(class) => {
                traversal.visit(db, class.metaclass_instance_type(db, env))?
            }
            Type::GenericAlias(alias) => {
                traversal.visit(db, ClassType::from(*alias).metaclass_instance_type(db, env))?
            }

            Type::SubclassOf(subclass_of_ty) => {
                match subclass_of_ty
                    .subclass_of()
                    .with_transposed_type_var(db, env)
                {
                    SubclassOfInner::Dynamic(_) => Truthiness::Ambiguous,
                    SubclassOfInner::Class(class) => traversal.visit(db, Type::from(class))?,
                    SubclassOfInner::Protocol(_) => Truthiness::Ambiguous,
                    SubclassOfInner::TypeVar(bound_typevar) => {
                        traversal.visit(db, Type::TypeVar(bound_typevar))?
                    }
                }
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db, env) {
                    None => Truthiness::Ambiguous,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        traversal.visit(db, bound)?
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        traversal.visit(db, constraints.as_type(db, env))?
                    }
                }
            }

            Type::NominalInstance(instance) => instance
                .known_class(db)
                .and_then(KnownClass::bool)
                .map(Ok)
                .unwrap_or_else(try_dunders)?,

            Type::ProtocolInstance(_) => try_dunders()?,

            Type::Union(union) => try_union(*union)?,

            Type::Intersection(intersection) => {
                if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                    traversal.visit(db, alternatives)?
                } else {
                    // TODO
                    Truthiness::Ambiguous
                }
            }

            Type::EnumComplement(complement) => {
                traversal.visit(db, complement.remaining_literal_union(db, env))?
            }

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::LiteralString => Truthiness::Ambiguous,
                LiteralValueTypeKind::Enum(enum_type) => {
                    traversal.visit(db, enum_type.enum_class_instance(db, env))?
                }

                LiteralValueTypeKind::Int(num) => Truthiness::from(num.as_i64() != 0),
                LiteralValueTypeKind::Bool(bool) => Truthiness::from(bool),
                LiteralValueTypeKind::String(str) => Truthiness::from(!str.value(db).is_empty()),
                LiteralValueTypeKind::Bytes(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            },

            Type::NewTypeInstance(newtype) => {
                traversal.visit(db, newtype.concrete_base_type(db))?
            }
        };

        Ok(truthiness)
    }
}

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
        let db = context.db();
        let Some(builder) = context.report_lint(&UNSUPPORTED_BOOL_CONVERSION, condition) else {
            return;
        };
        let env = context.program_environment();
        match self {
            Self::IncorrectArguments {
                not_boolable_type, ..
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`",
                    not_boolable_type.display(db, env)
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "`__bool__` methods must only have a `self` parameter",
                );
                if let Some((func_span, parameter_span)) = not_boolable_type
                    .member(db, env, "__bool__")
                    .into_lookup_result(db, env)
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
                    not_boolable = not_boolable_type.display(db, env),
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`{return_type}` is not assignable to `bool`",
                        return_type = return_type.display(db, env),
                    ),
                );
                if let Some((func_span, return_type_span)) = not_boolable_type
                    .member(db, env, "__bool__")
                    .into_lookup_result(db, env)
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
                    not_boolable_type.display(db, env)
                ));
                let sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`__bool__` on `{}` must be callable",
                        not_boolable_type.display(db, env)
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
                    .find_map(|element| element.try_bool(db, env).err())
                    .unwrap();

                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for union `{}` \
                     because `{}` doesn't implement `__bool__` correctly",
                    Type::Union(*union).display(db, env),
                    first_error.not_boolable_type().display(db, env),
                ));
            }

            Self::Other { not_boolable_type } => {
                builder.into_diagnostic(format_args!(
                    "Boolean conversion is not supported for type `{}`; \
                     it incorrectly implements `__bool__`",
                    not_boolable_type.display(db, env)
                ));
            }
        }
    }
}
