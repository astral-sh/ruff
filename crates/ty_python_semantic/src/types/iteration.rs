use crate::{
    Db,
    types::{
        AwaitError, Bindings, CallArguments, CallDunderError, KnownClass, LintDiagnosticGuard,
        LintDiagnosticGuardBuilder, LiteralValueTypeKind, Type, TypeContext,
        TypeVarBoundOrConstraints, UnionType,
        call::CallErrorKind,
        context::InferContext,
        diagnostic::NOT_ITERABLE,
        todo_type,
        tuple::{TupleSpec, TupleSpecBuilder},
    },
};
use ruff_python_ast as ast;
use std::borrow::Cow;
use ty_python_core::EvaluationMode;

/// Extract the element types from an expression with a statically known fixed-length iteration.
///
/// List and tuple literals are expanded directly so we preserve precise element types, including
/// recursively unpacking starred elements whose iterables are also fixed-length.
pub(crate) fn extract_fixed_length_iterable_element_types<'db>(
    db: &'db dyn Db,
    iterable: &ast::Expr,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> Option<Box<[Type<'db>]>> {
    fn extend_fixed_length_iterable<'db>(
        db: &'db dyn Db,
        iterable: &ast::Expr,
        expression_type: &mut impl FnMut(&ast::Expr) -> Type<'db>,
        element_types: &mut Vec<Type<'db>>,
    ) -> Option<()> {
        let elements = match iterable {
            ast::Expr::List(list) => Some(&list.elts),
            ast::Expr::Tuple(tuple) => Some(&tuple.elts),
            _ => None,
        };

        if let Some(elements) = elements {
            for element in elements {
                if let ast::Expr::Starred(starred) = element {
                    extend_fixed_length_iterable(
                        db,
                        starred.value.as_ref(),
                        expression_type,
                        element_types,
                    )?;
                } else {
                    element_types.push(expression_type(element));
                }
            }
            return Some(());
        }

        let iterable_type = expression_type(iterable);
        let spec = iterable_type.try_iterate(db).ok()?;
        let tuple = spec.as_fixed_length()?;
        element_types.extend(tuple.all_elements().iter().copied());
        Some(())
    }

    let mut element_types = Vec::new();
    extend_fixed_length_iterable(db, iterable, &mut expression_type, &mut element_types)?;
    Some(element_types.into_boxed_slice())
}

impl<'db> Type<'db> {
    /// Returns a tuple spec describing the elements that are produced when iterating over `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_iterate`](Self::try_iterate) instead.
    pub(super) fn iterate(self, db: &'db dyn Db) -> Cow<'db, TupleSpec<'db>> {
        self.try_iterate(db)
            .unwrap_or_else(|err| Cow::Owned(TupleSpec::homogeneous(err.fallback_element_type(db))))
    }

    /// Given the type of an object that is iterated over in some way,
    /// return a tuple spec describing the type of objects that are yielded by that iteration.
    ///
    /// E.g., for the following call, given the type of `x`, infer the types of the values that are
    /// splatted into `y`'s positional arguments:
    /// ```python
    /// y(*x)
    /// ```
    pub(super) fn try_iterate(
        self,
        db: &'db dyn Db,
    ) -> Result<Cow<'db, TupleSpec<'db>>, IterationError<'db>> {
        self.try_iterate_with_mode(db, EvaluationMode::Sync)
    }

    pub(super) fn try_iterate_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Result<Cow<'db, TupleSpec<'db>>, IterationError<'db>> {
        fn non_async_special_case<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
        ) -> Option<Cow<'db, TupleSpec<'db>>> {
            // We will not infer precise heterogeneous tuple specs for literals with lengths above this threshold.
            // The threshold here is somewhat arbitrary and conservative; it could be increased if needed.
            // However, it's probably very rare to need heterogeneous unpacking inference for long string literals
            // or bytes literals, and creating long heterogeneous tuple specs has a performance cost.
            const MAX_TUPLE_LENGTH: usize = 128;

            match ty {
                Type::NominalInstance(nominal) => nominal.tuple_spec(db),
                Type::NewTypeInstance(newtype) => non_async_special_case(db, newtype.concrete_base_type(db)),
                Type::GenericAlias(alias) if alias.origin(db).is_tuple(db) => {
                    Some(Cow::Owned(TupleSpec::homogeneous(todo_type!(
                        "*tuple[] annotations"
                    ))))
                }
                Type::LiteralValue(literal) => match literal.kind() {
                    LiteralValueTypeKind::Bytes(bytes) => {
                        let bytes_literal = bytes.value(db);
                        let spec = if bytes_literal.len() < MAX_TUPLE_LENGTH {
                            TupleSpec::heterogeneous(
                                bytes_literal
                                    .iter()
                                    .map(|b| Type::int_literal( i64::from(*b))),
                            )
                        } else {
                            TupleSpec::homogeneous(KnownClass::Int.to_instance(db))
                        };
                        Some(Cow::Owned(spec))
                    },
                    LiteralValueTypeKind::String(string_literal_ty) => {
                        let string_literal = string_literal_ty.value(db);
                        let spec = if string_literal.len() < MAX_TUPLE_LENGTH {
                            TupleSpec::heterogeneous(
                                string_literal
                                    .chars()
                                    .map(|c| Type::string_literal(db, &c.to_string())),
                            )
                        } else {
                            TupleSpec::homogeneous(Type::literal_string())
                        };
                        Some(Cow::Owned(spec))
                    }
                    // N.B. This special case isn't strictly necessary, it's just an obvious optimization
                    LiteralValueTypeKind::LiteralString => {
                        Some(Cow::Owned(TupleSpec::homogeneous(ty)))
                    }
                    _ => None
                }
                Type::Never => {
                    // The dunder logic below would have us return `tuple[Never, ...]`, which eagerly
                    // simplifies to `tuple[()]`. That will will cause us to emit false positives if we
                    // index into the tuple. Using `tuple[Unknown, ...]` avoids these false positives.
                    // TODO: Consider removing this special case, and instead hide the indexing
                    // diagnostic in unreachable code.
                    Some(Cow::Owned(TupleSpec::homogeneous(Type::unknown())))
                }
                Type::TypeAlias(alias) => {
                    non_async_special_case(db, alias.value_type(db))
                }
                Type::TypeVar(tvar) => match tvar.typevar(db).bound_or_constraints(db)? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        non_async_special_case(db, bound)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => non_async_special_case(db, constraints.as_type(db)),
                },
                Type::Union(union) => {
                    let elements = union.elements(db);
                    if elements.len() < MAX_TUPLE_LENGTH {
                        let mut elements_iter = elements.iter();
                        let first_element_spec = elements_iter.next()?.try_iterate_with_mode(db, EvaluationMode::Sync).ok()?;
                        let mut builder = TupleSpecBuilder::from(&*first_element_spec);
                        for element in elements_iter {
                            builder = builder.union(db, &*element.try_iterate_with_mode(db, EvaluationMode::Sync).ok()?);
                        }
                        Some(Cow::Owned(builder.build()))
                    } else {
                        None
                    }
                }
                Type::Intersection(intersection) => {
                    // For intersections containing TypeVars with union bounds, we need to
                    // flatten the TypeVars first. This distributes the intersection over
                    // the union and simplifies, e.g.:
                    // `T & tuple[object, ...]` where `T: tuple[int, ...] | list[str]`
                    // becomes `(tuple[int, ...] & tuple[object, ...]) | (list[str] & tuple[object, ...])`
                    // which simplifies to `tuple[int, ...] | Never` = `tuple[int, ...]`
                    //
                    // After flattening, the result may be:
                    // - An intersection (if no union-bound typevars, or they didn't simplify).
                    // - A union of intersections (if distribution happened).
                    // - A simpler type (if it fully simplified).
                    //
                    // We then iterate over the flattened type.
                    let flattened = ty.flatten_typevars(db);

                    // If flattening didn't change anything, iterate the intersection directly.
                    if flattened == ty {
                        let mut specs_iter = intersection.positive_elements_or_object(db).filter_map(
                            |element| element.try_iterate_with_mode(db, EvaluationMode::Sync).ok(),
                        );
                        let first_spec = specs_iter.next()?;
                        let mut builder = TupleSpecBuilder::from(&*first_spec);
                        for spec in specs_iter {
                            // Two tuples cannot have incompatible specs unless the tuples themselves
                            // are disjoint. `IntersectionBuilder` eagerly simplifies such
                            // intersections to `Never`, so this should always return `Some`.
                            let Some(intersected) = builder.intersect(db, &spec) else {
                                return Some(Cow::Owned(TupleSpec::homogeneous(Type::unknown())));
                            };
                            builder = intersected;
                        }
                        return Some(Cow::Owned(builder.build()));
                    }

                    // Flattening changed the type; recursively iterate the flattened result.
                    flattened.try_iterate(db).ok()
                }
                // N.B. This special case isn't strictly necessary, it's just an obvious optimization
                Type::Dynamic(_) => Some(Cow::Owned(TupleSpec::homogeneous(ty))),
                Type::Divergent(_) => Some(Cow::Owned(TupleSpec::homogeneous(ty))),

                Type::FunctionLiteral(_)
                | Type::GenericAlias(_)
                | Type::BoundMethod(_)
                | Type::KnownBoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_)
                | Type::ModuleLiteral(_)
                // We could infer a precise tuple spec for enum classes with members,
                // but it's not clear whether that's worth the added complexity:
                // you'd have to check that `EnumMeta.__iter__` is not overridden for it to be sound
                // (enums can have `EnumMeta` subclasses as their metaclasses).
                | Type::ClassLiteral(_)
                | Type::SubclassOf(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypedDict(_) => None
            }
        }

        if mode.is_async() {
            if let Type::Intersection(_) = self {
                let flattened = self.flatten_typevars(db);
                if flattened != self {
                    return flattened.try_iterate_with_mode(db, mode);
                }
            }

            let try_call_dunder_anext_on_iterator = |iterator: Type<'db>| -> Result<
                Result<Type<'db>, AwaitError<'db>>,
                CallDunderError<'db>,
            > {
                iterator
                    .try_call_dunder(
                        db,
                        "__anext__",
                        CallArguments::none(),
                        TypeContext::default(),
                    )
                    .map(|dunder_anext_outcome| dunder_anext_outcome.return_type(db).try_await(db))
            };

            return match self.try_call_dunder(
                db,
                "__aiter__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(dunder_aiter_bindings) => {
                    let iterator = dunder_aiter_bindings.return_type(db);
                    match try_call_dunder_anext_on_iterator(iterator) {
                        Ok(Ok(result)) => Ok(Cow::Owned(TupleSpec::homogeneous(result))),
                        Ok(Err(AwaitError::InvalidReturnType(..))) => {
                            Err(IterationError::UnboundAiterError)
                        } // TODO: __anext__ is bound, but is not properly awaitable
                        Err(dunder_anext_error) | Ok(Err(AwaitError::Call(dunder_anext_error))) => {
                            Err(IterationError::IterReturnsInvalidIterator {
                                iterator,
                                dunder_error: dunder_anext_error,
                                mode,
                            })
                        }
                    }
                }
                Err(CallDunderError::PossiblyUnbound {
                    bindings: dunder_aiter_bindings,
                    ..
                }) => {
                    let iterator = dunder_aiter_bindings.return_type(db);
                    match try_call_dunder_anext_on_iterator(iterator) {
                        Ok(_) => Err(IterationError::IterCallError {
                            kind: CallErrorKind::PossiblyNotCallable,
                            bindings: dunder_aiter_bindings,
                            mode,
                        }),
                        Err(dunder_anext_error) => {
                            Err(IterationError::IterReturnsInvalidIterator {
                                iterator,
                                dunder_error: dunder_anext_error,
                                mode,
                            })
                        }
                    }
                }
                Err(CallDunderError::CallError(kind, bindings)) => {
                    Err(IterationError::IterCallError {
                        kind,
                        bindings,
                        mode,
                    })
                }
                Err(CallDunderError::MethodNotAvailable) => Err(IterationError::UnboundAiterError),
            };
        }

        if let Some(special_case) = non_async_special_case(db, self) {
            return Ok(special_case);
        }

        let try_call_dunder_getitem = || {
            self.try_call_dunder(
                db,
                "__getitem__",
                CallArguments::positional([KnownClass::Int.to_instance(db)]),
                TypeContext::default(),
            )
            .map(|dunder_getitem_outcome| dunder_getitem_outcome.return_type(db))
        };

        let try_call_dunder_next_on_iterator = |iterator: Type<'db>| {
            iterator
                .try_call_dunder(
                    db,
                    "__next__",
                    CallArguments::none(),
                    TypeContext::default(),
                )
                .map(|dunder_next_outcome| dunder_next_outcome.return_type(db))
        };

        let dunder_iter_result = self
            .try_call_dunder(
                db,
                "__iter__",
                CallArguments::none(),
                TypeContext::default(),
            )
            .map(|dunder_iter_outcome| dunder_iter_outcome.return_type(db));

        match dunder_iter_result {
            Ok(iterator) => {
                // `__iter__` is definitely bound and calling it succeeds.
                // See what calling `__next__` on the object returned by `__iter__` gives us...
                try_call_dunder_next_on_iterator(iterator)
                    .map(|ty| Cow::Owned(TupleSpec::homogeneous(ty)))
                    .map_err(
                        |dunder_next_error| IterationError::IterReturnsInvalidIterator {
                            iterator,
                            dunder_error: dunder_next_error,
                            mode,
                        },
                    )
            }

            // `__iter__` is possibly unbound...
            Err(CallDunderError::PossiblyUnbound {
                bindings: dunder_iter_outcome,
                unbound_on: unbound_on_iter,
            }) => {
                let iterator = dunder_iter_outcome.return_type(db);

                match try_call_dunder_next_on_iterator(iterator) {
                    Ok(dunder_next_return) => {
                        try_call_dunder_getitem()
                            .map(|dunder_getitem_return_type| {
                                // If `__iter__` is possibly unbound,
                                // but it returns an object that has a bound and valid `__next__` method,
                                // *and* the object has a bound and valid `__getitem__` method,
                                // we infer a union of the type returned by the `__next__` method
                                // and the type returned by the `__getitem__` method.
                                //
                                // No diagnostic is emitted; iteration will always succeed!
                                Cow::Owned(TupleSpec::homogeneous(UnionType::from_two_elements(
                                    db,
                                    dunder_next_return,
                                    dunder_getitem_return_type,
                                )))
                            })
                            .map_err(|dunder_getitem_error| {
                                IterationError::PossiblyUnboundIterAndGetitemError {
                                    dunder_next_return,
                                    unbound_on_iter,
                                    dunder_getitem_error,
                                }
                            })
                    }

                    Err(dunder_next_error) => Err(IterationError::IterReturnsInvalidIterator {
                        iterator,
                        dunder_error: dunder_next_error,
                        mode,
                    }),
                }
            }

            // `__iter__` is definitely bound but it can't be called with the expected arguments
            Err(CallDunderError::CallError(kind, bindings)) => Err(IterationError::IterCallError {
                kind,
                bindings,
                mode,
            }),

            // There's no `__iter__` method. Try `__getitem__` instead...
            Err(CallDunderError::MethodNotAvailable) => try_call_dunder_getitem()
                .map(|ty| Cow::Owned(TupleSpec::homogeneous(ty)))
                .map_err(
                    |dunder_getitem_error| IterationError::UnboundIterAndGetitemError {
                        dunder_getitem_error,
                    },
                ),
        }
    }
}

/// Error returned if a type is not (or may not be) iterable.
#[derive(Debug)]
pub(super) enum IterationError<'db> {
    /// The object being iterated over has a bound `__(a)iter__` method,
    /// but calling it with the expected arguments results in an error.
    IterCallError {
        kind: CallErrorKind,
        bindings: Box<Bindings<'db>>,
        mode: EvaluationMode,
    },

    /// The object being iterated over has a bound `__(a)iter__` method that can be called
    /// with the expected types, but it returns an object that is not a valid iterator.
    IterReturnsInvalidIterator {
        /// The type of the object returned by the `__(a)iter__` method.
        iterator: Type<'db>,
        /// The error we encountered when we tried to call `__(a)next__` on the type
        /// returned by `__(a)iter__`
        dunder_error: CallDunderError<'db>,
        /// Whether this is a synchronous or an asynchronous iterator.
        mode: EvaluationMode,
    },

    /// The object being iterated over has a bound `__iter__` method that returns a
    /// valid iterator. However, the `__iter__` method is possibly unbound, and there
    /// either isn't a `__getitem__` method to fall back to, or calling the `__getitem__`
    /// method returns some kind of error.
    PossiblyUnboundIterAndGetitemError {
        /// The type of the object returned by the `__next__` method on the iterator.
        /// (The iterator being the type returned by the `__iter__` method on the iterable.)
        dunder_next_return: Type<'db>,
        /// For union types, the elements where `__iter__` was completely undefined.
        /// Used to emit per-element info sub-diagnostics identifying the problematic members.
        /// When this is omitted, it is because we don't care to track where exactly the methods were unbound.
        unbound_on_iter: Option<Box<[Type<'db>]>>,
        /// The error we encountered when we tried to call `__getitem__` on the iterable.
        dunder_getitem_error: CallDunderError<'db>,
    },

    /// The object being iterated over doesn't have an `__iter__` method.
    /// It also either doesn't have a `__getitem__` method to fall back to,
    /// or calling the `__getitem__` method returns some kind of error.
    UnboundIterAndGetitemError {
        dunder_getitem_error: CallDunderError<'db>,
    },

    /// The asynchronous iterable has no `__aiter__` method.
    UnboundAiterError,
}

impl<'db> IterationError<'db> {
    pub(super) fn fallback_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.element_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the element type if it is known, or `None` if the type is never iterable.
    fn element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        let return_type = |result: Result<Bindings<'db>, CallDunderError<'db>>| {
            result
                .map(|outcome| Some(outcome.return_type(db)))
                .unwrap_or_else(|call_error| call_error.return_type(db))
        };

        match self {
            Self::IterReturnsInvalidIterator {
                dunder_error, mode, ..
            } => dunder_error.return_type(db).and_then(|ty| {
                if mode.is_async() {
                    ty.try_await(db).ok()
                } else {
                    Some(ty)
                }
            }),

            Self::IterCallError {
                kind: _,
                bindings: dunder_iter_bindings,
                mode,
            } => {
                if mode.is_async() {
                    return_type(dunder_iter_bindings.return_type(db).try_call_dunder(
                        db,
                        "__anext__",
                        CallArguments::none(),
                        TypeContext::default(),
                    ))
                    .and_then(|ty| ty.try_await(db).ok())
                } else {
                    return_type(dunder_iter_bindings.return_type(db).try_call_dunder(
                        db,
                        "__next__",
                        CallArguments::none(),
                        TypeContext::default(),
                    ))
                }
            }

            Self::PossiblyUnboundIterAndGetitemError {
                dunder_next_return,
                unbound_on_iter: _,
                dunder_getitem_error,
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => Some(*dunder_next_return),
                CallDunderError::PossiblyUnbound {
                    bindings: dunder_getitem_outcome,
                    ..
                } => Some(UnionType::from_two_elements(
                    db,
                    *dunder_next_return,
                    dunder_getitem_outcome.return_type(db),
                )),
                CallDunderError::CallError(CallErrorKind::NotCallable, _) => {
                    Some(*dunder_next_return)
                }
                CallDunderError::CallError(_, dunder_getitem_bindings) => {
                    let dunder_getitem_return = dunder_getitem_bindings.return_type(db);
                    Some(UnionType::from_two_elements(
                        db,
                        *dunder_next_return,
                        dunder_getitem_return,
                    ))
                }
            },

            Self::UnboundIterAndGetitemError {
                dunder_getitem_error,
            } => dunder_getitem_error.return_type(db),

            Self::UnboundAiterError => None,
        }
    }

    /// Does this error concern a synchronous or asynchronous iterable?
    fn mode(&self) -> EvaluationMode {
        match self {
            Self::IterCallError { mode, .. } => *mode,
            Self::IterReturnsInvalidIterator { mode, .. } => *mode,
            Self::PossiblyUnboundIterAndGetitemError { .. }
            | Self::UnboundIterAndGetitemError { .. } => EvaluationMode::Sync,
            Self::UnboundAiterError => EvaluationMode::Async,
        }
    }

    /// Reports the diagnostic for this error.
    pub(super) fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        iterable_type: Type<'db>,
        iterable_node: ast::AnyNodeRef,
    ) {
        /// A little helper type for emitting a diagnostic
        /// based on the variant of iteration error.
        struct Reporter<'a> {
            db: &'a dyn Db,
            builder: LintDiagnosticGuardBuilder<'a, 'a>,
            iterable_type: Type<'a>,
            mode: EvaluationMode,
        }

        impl<'a> Reporter<'a> {
            /// Emit a diagnostic that is certain that `iterable_type` is not iterable.
            ///
            /// `because` should explain why `iterable_type` is not iterable.
            #[expect(clippy::wrong_self_convention)]
            fn is_not(self, because: impl std::fmt::Display) -> LintDiagnosticGuard<'a, 'a> {
                let mut diag = self.builder.into_diagnostic(format_args!(
                    "Object of type `{iterable_type}` is not {maybe_async}iterable",
                    iterable_type = self.iterable_type.display(self.db),
                    maybe_async = if self.mode.is_async() { "async-" } else { "" }
                ));
                diag.info(because);
                diag
            }

            /// Emit a diagnostic that is uncertain that `iterable_type` is not iterable.
            ///
            /// `because` should explain why `iterable_type` is likely not iterable.
            fn may_not(self, because: impl std::fmt::Display) -> LintDiagnosticGuard<'a, 'a> {
                let mut diag = self.builder.into_diagnostic(format_args!(
                    "Object of type `{iterable_type}` may not be {maybe_async}iterable",
                    iterable_type = self.iterable_type.display(self.db),
                    maybe_async = if self.mode.is_async() { "async-" } else { "" }
                ));
                diag.info(because);
                diag
            }
        }

        let Some(builder) = context.report_lint(&NOT_ITERABLE, iterable_node) else {
            return;
        };
        let db = context.db();
        let mode = self.mode();
        let reporter = Reporter {
            db,
            builder,
            iterable_type,
            mode,
        };

        // TODO: for all of these error variants, the "explanation" for the diagnostic
        // (everything after the "because") should really be presented as a "help:", "note",
        // or similar, rather than as part of the same sentence as the error message.
        match self {
            Self::IterCallError {
                kind,
                bindings,
                mode,
            } => {
                let method = if mode.is_async() {
                    "__aiter__"
                } else {
                    "__iter__"
                };

                match kind {
                    CallErrorKind::NotCallable => {
                        reporter.is_not(format_args!(
                        "Its `{method}` attribute has type `{dunder_iter_type}`, which is not callable",
                        dunder_iter_type = bindings.callable_type().display(db),
                    ));
                    }
                    CallErrorKind::PossiblyNotCallable => {
                        reporter.may_not(format_args!(
                            "Its `{method}` attribute (with type `{dunder_iter_type}`) \
                             may not be callable",
                            dunder_iter_type = bindings.callable_type().display(db),
                        ));
                    }
                    CallErrorKind::BindingError => {
                        if bindings.is_single() {
                            reporter
                                .is_not(format_args!(
                                    "Its `{method}` method has an invalid signature"
                                ))
                                .info(format_args!("Expected signature `def {method}(self): ...`"));
                        } else {
                            let mut diag = reporter.may_not(format_args!(
                                "Its `{method}` method may have an invalid signature"
                            ));
                            diag.info(format_args!(
                                "Type of `{method}` is `{dunder_iter_type}`",
                                dunder_iter_type = bindings.callable_type().display(db),
                            ));
                            diag.info(format_args!(
                                "Expected signature for `{method}` is `def {method}(self): ...`",
                            ));
                        }
                    }
                }
            }

            Self::IterReturnsInvalidIterator {
                iterator,
                dunder_error: dunder_next_error,
                mode,
            } => {
                let dunder_iter_name = if mode.is_async() {
                    "__aiter__"
                } else {
                    "__iter__"
                };
                let dunder_next_name = if mode.is_async() {
                    "__anext__"
                } else {
                    "__next__"
                };
                match dunder_next_error {
                    CallDunderError::MethodNotAvailable => {
                        reporter.is_not(format_args!(
                        "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                         which has no `{dunder_next_name}` method",
                        iterator_type = iterator.display(db),
                    ));
                    }
                    CallDunderError::PossiblyUnbound { .. } => {
                        reporter.may_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which may not have a `{dunder_next_name}` method",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::NotCallable, _) => {
                        reporter.is_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which has a `{dunder_next_name}` attribute that is not callable",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _) => {
                        reporter.may_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which has a `{dunder_next_name}` attribute that may not be callable",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                        if bindings.is_single() =>
                    {
                        reporter
                            .is_not(format_args!(
                                "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                                which has an invalid `{dunder_next_name}` method",
                                iterator_type = iterator.display(db),
                            ))
                            .info(format_args!("Expected signature for `{dunder_next_name}` is `def {dunder_next_name}(self): ...`"));
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, _) => {
                        reporter
                            .may_not(format_args!(
                                "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                                which may have an invalid `{dunder_next_name}` method",
                                iterator_type = iterator.display(db),
                            ))
                            .info(format_args!("Expected signature for `{dunder_next_name}` is `def {dunder_next_name}(self): ...`"));
                    }
                }
            }

            Self::PossiblyUnboundIterAndGetitemError {
                unbound_on_iter,
                dunder_getitem_error,
                ..
            } => {
                let mut diag = match dunder_getitem_error {
                    CallDunderError::MethodNotAvailable => reporter.may_not(
                        "It may not have an `__iter__` method \
                         and it doesn't have a `__getitem__` method",
                    ),
                    CallDunderError::PossiblyUnbound { .. } => reporter
                        .may_not("It may not have an `__iter__` method or a `__getitem__` method"),
                    CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => reporter
                        .may_not(format_args!(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                             which is not callable",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        )),
                    CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings)
                        if bindings.is_single() =>
                    {
                        reporter.may_not(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` attribute may not be callable",
                        )
                    }
                    CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                        reporter.may_not(format_args!(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` attribute (with type `{dunder_getitem_type}`) \
                             may not be callable",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        ))
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                        if bindings.is_single() =>
                    {
                        let mut diag = reporter.may_not(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` method has an incorrect signature \
                             for the old-style iteration protocol",
                        );
                        diag.info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                        diag
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, bindings) => {
                        let mut diag = reporter.may_not(format_args!(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` method (with type `{dunder_getitem_type}`) \
                             may have an incorrect signature for the old-style iteration protocol",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        ));
                        diag.info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                        diag
                    }
                };
                if let Some(unbound_on) = unbound_on_iter.as_deref() {
                    for ty in unbound_on.iter().copied() {
                        diag.info(format_args!(
                            "`{}` does not implement `__iter__`",
                            ty.display(db)
                        ));
                    }
                }
            }

            Self::UnboundIterAndGetitemError {
                dunder_getitem_error,
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => {
                    reporter
                        .is_not("It doesn't have an `__iter__` method or a `__getitem__` method");
                }
                CallDunderError::PossiblyUnbound { .. } => {
                    reporter.is_not(
                        "It has no `__iter__` method and it may not have a `__getitem__` method",
                    );
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => {
                    reporter.is_not(format_args!(
                        "It has no `__iter__` method and \
                         its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                         which is not callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings)
                    if bindings.is_single() =>
                {
                    reporter.may_not(
                        "It has no `__iter__` method and its `__getitem__` attribute \
                         may not be callable",
                    );
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                    reporter.may_not(
                        "It has no `__iter__` method and its `__getitem__` attribute is invalid",
                    ).info(format_args!(
                        "`__getitem__` has type `{dunder_getitem_type}`, which is not callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                    if bindings.is_single() =>
                {
                    reporter
                        .is_not(
                            "It has no `__iter__` method and \
                             its `__getitem__` method has an incorrect signature \
                             for the old-style iteration protocol",
                        )
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) => {
                    reporter
                        .may_not(format_args!(
                            "It has no `__iter__` method and \
                             its `__getitem__` method (with type `{dunder_getitem_type}`) \
                             may have an incorrect signature for the old-style iteration protocol",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        ))
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
            },

            IterationError::UnboundAiterError => {
                reporter.is_not("It has no `__aiter__` method");
            }
        }
    }
}
