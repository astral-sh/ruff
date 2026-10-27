use crate::Db;
use crate::ProgramEnvironment;
use crate::{
    FxOrderSet, Program,
    types::{
        Bindings, CallArguments, CallDunderError, KnownClass, MemberLookupPolicy, Type,
        TypeContext, call::CallErrorKind, context::InferContext,
        diagnostic::INVALID_CONTEXT_MANAGER,
    },
};
use ruff_python_ast as ast;
use ty_python_core::EvaluationMode;

impl<'db> Type<'db> {
    /// Returns whether this context manager can suppress an exception raised inside its suite.
    ///
    /// Following the [typing specification], only exit methods returning exactly `bool` or
    /// `Literal[True]` are considered suppressing; `bool | None` and `Any` are not. This
    /// intentionally differs from runtime truthiness: non-suppressing context managers are
    /// commonly annotated as returning `bool | None`, so treating every potentially truthy return
    /// type as suppressing would incorrectly preserve exception paths for ordinary managers.
    /// Asynchronous exit results are awaited before applying this rule.
    ///
    /// [typing specification]: https://typing.python.org/en/latest/spec/exceptions.html#context-managers
    ///
    /// Suppression is cached by manager type because the same predicate can be evaluated repeatedly
    /// for different bindings and context managers. Each alternative in a union is classified
    /// separately: if any possible manager can suppress exceptions, the union can suppress
    /// exceptions too. Exceptional-exit overloads are also classified independently. Merging the
    /// return types of different manager alternatives or overloads could incorrectly classify a
    /// suppressing exit alongside a non-suppressing exit as returning `bool | None`.
    ///
    /// Python passes `(None, None, None)` to an exit method when a suite completes normally and
    /// passes the exception type, value, and traceback when it raises. Consequently, overloads
    /// whose first two arguments cannot accept an exception type and instance cannot describe an
    /// exceptional exit and must not affect the suppression result:
    ///
    /// ```python
    /// @overload
    /// def __exit__(self, typ: None, value: None, tb: None) -> None: ...
    ///
    /// @overload
    /// def __exit__(
    ///     self,
    ///     typ: type[BaseException],
    ///     value: BaseException,
    ///     tb: TracebackType | None,
    /// ) -> Literal[True]: ...
    /// ```
    ///
    /// This manager can suppress exceptions despite its normal-exit overload returning `None`.
    /// Suppression preserves any state from before an operation that raises:
    ///
    /// ```python
    /// from contextlib import suppress
    ///
    /// value = None
    /// with suppress(ValueError):
    ///     value = int("invalid")
    /// reveal_type(value)  # int | None
    /// ```
    pub(crate) fn can_suppress_exceptions(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: EvaluationMode,
    ) -> bool {
        #[salsa::tracked(
            returns(copy),
            cycle_initial = |_, _, _, _, _| false,
            heap_size = ruff_memory_usage::heap_size
        )]
        fn can_suppress_exceptions_impl<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            manager: Type<'db>,
            is_async: bool,
        ) -> bool {
            if let Some(union) = manager.as_union_like(db) {
                return union
                    .elements(db)
                    .iter()
                    .any(|&element| can_suppress_exceptions_impl(db, program, element, is_async));
            }

            let env = ProgramEnvironment::from_program(program);
            let method = if is_async { "__aexit__" } else { "__exit__" };
            let Some(callables) = manager
                .member_lookup_with_policy(
                    db,
                    &env,
                    method,
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
                .ignore_possibly_undefined()
                .and_then(|exit| exit.try_upcast_to_callable(db, &env))
            else {
                return false;
            };

            let exception_type = KnownClass::BaseException.to_subclass_of(db, &env);
            let exception_instance = KnownClass::BaseException.to_instance(db, &env);
            for signature in callables
                .iter()
                .flat_map(|callable| callable.signatures(db))
            {
                if signature
                    .parameters()
                    .get_positional(0)
                    .is_some_and(|parameter| {
                        parameter
                            .annotated_type()
                            .is_disjoint_from(db, &env, exception_type)
                    })
                    || signature
                        .parameters()
                        .get_positional(1)
                        .is_some_and(|parameter| {
                            parameter.annotated_type().is_disjoint_from(
                                db,
                                &env,
                                exception_instance,
                            )
                        })
                {
                    continue;
                }

                let return_type = if is_async {
                    let Ok(awaited) = signature.return_ty.try_await(db, &env) else {
                        continue;
                    };
                    awaited
                } else {
                    signature.return_ty
                };

                if return_type.is_equivalent_to(db, &env, KnownClass::Bool.to_instance(db, &env))
                    || return_type.is_equivalent_to(db, &env, Type::bool_literal(true))
                {
                    return true;
                }
            }

            false
        }

        can_suppress_exceptions_impl(db, env.program(db), self, mode.is_async())
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    pub(super) fn enter(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.try_enter_with_mode(db, env, EvaluationMode::Sync)
            .unwrap_or_else(|err| err.fallback_enter_type(db, env))
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    pub(super) fn aenter(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.try_enter_with_mode(db, env, EvaluationMode::Async)
            .unwrap_or_else(|err| err.fallback_enter_type(db, env))
    }

    /// Given the type of an object that is used as a context manager (i.e. in a `with` statement),
    /// return the return type of its `__enter__` or `__aenter__` method, which is bound to any potential targets.
    ///
    /// E.g., for the following `with` statement, given the type of `x`, infer the type of `y`:
    /// ```python
    /// with x as y:
    ///     pass
    /// ```
    pub(super) fn try_enter_with_mode(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: EvaluationMode,
    ) -> Result<Type<'db>, ContextManagerError<'db>> {
        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let enter = self.try_call_dunder(
            db,
            env,
            enter_method,
            CallArguments::none(),
            TypeContext::default(),
        );
        let exit = self.try_call_dunder(
            db,
            env,
            exit_method,
            CallArguments::positional([
                Type::none(db, env),
                Type::none(db, env),
                Type::none(db, env),
            ]),
            TypeContext::default(),
        );

        let awaited_enter_type = if mode.is_async() {
            let return_type = |call: &Result<Bindings<'db>, CallDunderError<'db>>| match call {
                Ok(bindings) => Some(bindings.return_type(db, env)),
                Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                    Some(bindings.return_type(db, env))
                }
                Err(CallDunderError::MethodNotAvailable | CallDunderError::CallError(..)) => None,
            };

            let enter_return_type = return_type(&enter);
            let exit_return_type = return_type(&exit);
            let awaited_enter_type =
                enter_return_type.and_then(|return_type| return_type.try_await(db, env).ok());
            let awaited_exit_type =
                exit_return_type.and_then(|return_type| return_type.try_await(db, env).ok());
            let non_awaitable_enter = enter_return_type.filter(|_| awaited_enter_type.is_none());
            let non_awaitable_exit = exit_return_type.filter(|_| awaited_exit_type.is_none());

            if let Some(non_awaitable) =
                NonAwaitableMethods::from_parts(non_awaitable_enter, non_awaitable_exit)
            {
                return Err(ContextManagerError::NotAwaitable {
                    enter_return_type: awaited_enter_type.unwrap_or(Type::unknown()),
                    non_awaitable,
                    enter_error: enter.err().map(Box::new),
                    exit_error: exit.err().map(Box::new),
                });
            }

            awaited_enter_type
        } else {
            None
        };

        // TODO: Make use of Protocols when we support it (the manager be assignable to `contextlib.AbstractContextManager`).
        match (enter, exit) {
            (Ok(enter), Ok(_)) => {
                let return_type = enter.return_type(db, env);
                Ok(if mode.is_async() {
                    awaited_enter_type.unwrap_or(Type::unknown())
                } else {
                    return_type
                })
            }
            (Ok(enter), Err(exit_error)) => {
                let return_type = enter.return_type(db, env);
                Err(ContextManagerError::Exit {
                    enter_return_type: if mode.is_async() {
                        awaited_enter_type.unwrap_or(Type::unknown())
                    } else {
                        return_type
                    },
                    exit_error,
                    mode,
                })
            }
            // TODO: Use the `exit_ty` to determine if any raised exception is suppressed.
            (Err(enter_error), Ok(_)) => Err(ContextManagerError::Enter(enter_error, mode)),
            (Err(enter_error), Err(exit_error)) => Err(ContextManagerError::EnterAndExit {
                enter_error,
                exit_error,
                mode,
            }),
        }
    }
}

/// Error returned if a type is not (or may not be) a context manager.
#[derive(Debug)]
pub(super) enum ContextManagerError<'db> {
    Enter(CallDunderError<'db>, EvaluationMode),
    Exit {
        enter_return_type: Type<'db>,
        exit_error: CallDunderError<'db>,
        mode: EvaluationMode,
    },
    EnterAndExit {
        enter_error: CallDunderError<'db>,
        exit_error: CallDunderError<'db>,
        mode: EvaluationMode,
    },
    /// At least one async context-manager method returns a non-awaitable, possibly in addition to
    /// a missing or invalid method.
    NotAwaitable {
        /// The type bound to the `as` target, already awaited when `__aenter__` allowed it.
        enter_return_type: Type<'db>,
        non_awaitable: NonAwaitableMethods<'db>,
        enter_error: Option<Box<CallDunderError<'db>>>,
        exit_error: Option<Box<CallDunderError<'db>>>,
    },
}

/// Which of `__aenter__` and `__aexit__` returned a value that cannot be awaited, and what each
/// of them returned.
///
/// At least one method must be at fault for the enclosing error to exist, which is why this is an
/// enum rather than a pair of `Option`s or a collection that could be empty.
#[derive(Debug)]
pub(super) enum NonAwaitableMethods<'db> {
    Enter(Type<'db>),
    Exit(Type<'db>),
    Both { enter: Type<'db>, exit: Type<'db> },
}

impl<'db> NonAwaitableMethods<'db> {
    /// Builds the error description from whichever methods are at fault, or `None` if both
    /// returned awaitables and there is nothing to report.
    fn from_parts(enter: Option<Type<'db>>, exit: Option<Type<'db>>) -> Option<Self> {
        match (enter, exit) {
            (Some(enter), Some(exit)) => Some(Self::Both { enter, exit }),
            (Some(enter), None) => Some(Self::Enter(enter)),
            (None, Some(exit)) => Some(Self::Exit(exit)),
            (None, None) => None,
        }
    }

    /// The offending return types, paired with the name of the method that returned each one.
    fn named_return_types(
        &self,
        enter_method: &'static str,
        exit_method: &'static str,
    ) -> Vec<(&'static str, Type<'db>)> {
        match self {
            Self::Enter(enter) => vec![(enter_method, *enter)],
            Self::Exit(exit) => vec![(exit_method, *exit)],
            Self::Both { enter, exit } => vec![(enter_method, *enter), (exit_method, *exit)],
        }
    }

    const fn is_both(&self) -> bool {
        matches!(self, Self::Both { .. })
    }
}

impl<'db> ContextManagerError<'db> {
    pub(super) fn fallback_enter_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.enter_type(db, env).unwrap_or(Type::unknown())
    }

    /// Returns the `__enter__` or `__aenter__` return type if it is known,
    /// or `None` if the type never has a callable `__enter__` or `__aenter__` attribute
    fn enter_type(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        match self {
            Self::Exit {
                enter_return_type,
                exit_error: _,
                mode: _,
            }
            | Self::NotAwaitable {
                enter_return_type, ..
            } => Some(*enter_return_type),
            Self::Enter(enter_error, mode)
            | Self::EnterAndExit {
                enter_error,
                exit_error: _,
                mode,
            } => match enter_error {
                CallDunderError::PossiblyUnbound { bindings, .. } => {
                    let return_type = bindings.return_type(db, env);
                    Some(if mode.is_async() {
                        return_type.try_await(db, env).unwrap_or(Type::unknown())
                    } else {
                        return_type
                    })
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, _, _) => None,
                CallDunderError::CallError(_, bindings, _) => Some(bindings.return_type(db, env)),
                CallDunderError::MethodNotAvailable => None,
            },
        }
    }

    pub(super) fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        fn unbound_on<'db>(error: &CallDunderError<'db>) -> FxOrderSet<Type<'db>> {
            match error {
                CallDunderError::PossiblyUnbound {
                    unbound_on: Some(unbound_on),
                    ..
                } => unbound_on.iter().copied().collect(),
                _ => FxOrderSet::default(),
            }
        }
        let db = context.db();

        let Some(builder) = context.report_lint(&INVALID_CONTEXT_MANAGER, context_expression_node)
        else {
            return;
        };

        let mode = match self {
            Self::Exit { mode, .. } | Self::Enter(_, mode) | Self::EnterAndExit { mode, .. } => {
                *mode
            }
            // `NotAwaitable` is only ever constructed for `async with`.
            Self::NotAwaitable { .. } => EvaluationMode::Async,
        };

        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let format_call_dunder_error = |call_dunder_error: &CallDunderError<'db>, name: &str| {
            match call_dunder_error {
                CallDunderError::MethodNotAvailable => format!("it does not implement `{name}`"),
                CallDunderError::PossiblyUnbound { .. } => {
                    format!("the method `{name}` may be missing")
                }
                // TODO: Use more specific error messages for the different error cases.
                //  E.g. distinguish between a not callable `__enter__` attribute and a wrong signature.
                CallDunderError::CallError(_, _, _) => {
                    format!("it does not correctly implement `{name}`")
                }
            }
        };

        let format_call_dunder_errors = |error_a: &CallDunderError<'db>,
                                         name_a: &str,
                                         error_b: &CallDunderError<'db>,
                                         name_b: &str| {
            match (error_a, error_b) {
                (
                    CallDunderError::PossiblyUnbound { .. },
                    CallDunderError::PossiblyUnbound { .. },
                ) => format!("the methods `{name_a}` and `{name_b}` are possibly missing"),
                (CallDunderError::MethodNotAvailable, CallDunderError::MethodNotAvailable) => {
                    format!("it does not implement `{name_a}` and `{name_b}`")
                }
                (CallDunderError::CallError(_, _, _), CallDunderError::CallError(_, _, _)) => {
                    format!("it does not correctly implement `{name_a}` or `{name_b}`")
                }
                (_, _) => format!(
                    "{format_a}, and {format_b}",
                    format_a = format_call_dunder_error(error_a, name_a),
                    format_b = format_call_dunder_error(error_b, name_b)
                ),
            }
        };

        let env = context.program_environment();

        let formatted_errors = match self {
            Self::Exit {
                enter_return_type: _,
                exit_error,
                mode: _,
            } => format_call_dunder_error(exit_error, exit_method),
            Self::Enter(enter_error, _) => format_call_dunder_error(enter_error, enter_method),
            Self::EnterAndExit {
                enter_error,
                exit_error,
                mode: _,
            } => format_call_dunder_errors(enter_error, enter_method, exit_error, exit_method),
            Self::NotAwaitable {
                non_awaitable,
                enter_error,
                exit_error,
                ..
            } => {
                let methods = non_awaitable
                    .named_return_types(enter_method, exit_method)
                    .iter()
                    .map(|(name, _)| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(" and ");
                let await_error = if non_awaitable.is_both() {
                    format!("{methods} do not return awaitables")
                } else {
                    format!("{methods} does not return an awaitable")
                };

                match (enter_error.as_deref(), exit_error.as_deref()) {
                    (
                        Some(CallDunderError::PossiblyUnbound { .. }),
                        Some(CallDunderError::PossiblyUnbound { .. }),
                    ) if non_awaitable.is_both() => {
                        format!(
                            "`{enter_method}` and `{exit_method}` may be missing or return non-awaitables"
                        )
                    }
                    (Some(enter_error), Some(exit_error)) => format!(
                        "{}, and {await_error}",
                        format_call_dunder_errors(
                            enter_error,
                            enter_method,
                            exit_error,
                            exit_method
                        )
                    ),
                    (Some(enter_error), None) => format!(
                        "{}, and {await_error}",
                        format_call_dunder_error(enter_error, enter_method)
                    ),
                    (None, Some(exit_error)) => format!(
                        "{}, and {await_error}",
                        format_call_dunder_error(exit_error, exit_method)
                    ),
                    (None, None) => await_error,
                }
            }
        };

        // Suggest using `async with` if only async methods are available in a sync context,
        // or suggest using `with` if only sync methods are available in an async context.
        let with_kw = match mode {
            EvaluationMode::Sync => "with",
            EvaluationMode::Async => "async with",
        };

        let mut diag = builder.into_diagnostic(format_args!(
            "Object of type `{}` cannot be used with `{}` because {}",
            context_expression_type.display(db, env),
            with_kw,
            formatted_errors,
        ));

        match self {
            Self::Exit { exit_error, .. } => {
                let exit_unbound_on = unbound_on(exit_error);
                for ty in &exit_unbound_on {
                    diag.info(format_args!(
                        "`{}` does not implement `{exit_method}`",
                        ty.display(db, env)
                    ));
                }
            }
            Self::Enter(enter_error, _) => {
                let enter_unbound_on = unbound_on(enter_error);
                for ty in &enter_unbound_on {
                    diag.info(format_args!(
                        "`{}` does not implement `{enter_method}`",
                        ty.display(db, env)
                    ));
                }
            }
            Self::EnterAndExit {
                enter_error,
                exit_error,
                ..
            } => {
                let enter_unbound_on = unbound_on(enter_error);
                let exit_unbound_on = unbound_on(exit_error);

                for ty in &enter_unbound_on {
                    if exit_unbound_on.contains(ty) {
                        diag.info(format_args!(
                            "`{}` does not implement `{enter_method}` or `{exit_method}`",
                            ty.display(db, env)
                        ));
                    } else {
                        diag.info(format_args!(
                            "`{}` does not implement `{enter_method}`",
                            ty.display(db, env)
                        ));
                    }
                }

                for ty in &exit_unbound_on {
                    if !enter_unbound_on.contains(ty) {
                        diag.info(format_args!(
                            "`{}` does not implement `{exit_method}`",
                            ty.display(db, env)
                        ));
                    }
                }
            }
            Self::NotAwaitable {
                non_awaitable,
                enter_error,
                exit_error,
                ..
            } => {
                let enter_unbound_on = enter_error
                    .as_deref()
                    .map_or_else(FxOrderSet::default, unbound_on);
                let exit_unbound_on = exit_error
                    .as_deref()
                    .map_or_else(FxOrderSet::default, unbound_on);

                for ty in &enter_unbound_on {
                    if exit_unbound_on.contains(ty) {
                        diag.info(format_args!(
                            "`{}` does not implement `{enter_method}` or `{exit_method}`",
                            ty.display(db, env)
                        ));
                    } else {
                        diag.info(format_args!(
                            "`{}` does not implement `{enter_method}`",
                            ty.display(db, env)
                        ));
                    }
                }

                for ty in &exit_unbound_on {
                    if !enter_unbound_on.contains(ty) {
                        diag.info(format_args!(
                            "`{}` does not implement `{exit_method}`",
                            ty.display(db, env)
                        ));
                    }
                }

                for (method, return_type) in
                    non_awaitable.named_return_types(enter_method, exit_method)
                {
                    diag.info(format_args!(
                        "`{method}` returns `{}`, which is not awaitable",
                        return_type.display(db, env)
                    ));
                }
                if non_awaitable.is_both() {
                    diag.info("Consider declaring the methods with `async def`");
                } else {
                    diag.info("Consider declaring the method with `async def`");
                }
            }
        }

        // Do not suggest switching between `with` and `async with` for a non-awaitable return.
        if matches!(self, Self::NotAwaitable { .. }) {
            return;
        }

        let (alt_mode, alt_enter_method, alt_exit_method, alt_with_kw) = match mode {
            EvaluationMode::Sync => ("async", "__aenter__", "__aexit__", "async with"),
            EvaluationMode::Async => ("sync", "__enter__", "__exit__", "with"),
        };

        let alt_enter = context_expression_type.try_call_dunder(
            db,
            env,
            alt_enter_method,
            CallArguments::none(),
            TypeContext::default(),
        );
        let alt_exit = context_expression_type.try_call_dunder(
            db,
            env,
            alt_exit_method,
            CallArguments::positional([Type::unknown(), Type::unknown(), Type::unknown()]),
            TypeContext::default(),
        );

        if (alt_enter.is_ok() || matches!(alt_enter, Err(CallDunderError::CallError(..))))
            && (alt_exit.is_ok() || matches!(alt_exit, Err(CallDunderError::CallError(..))))
        {
            diag.info(format_args!(
                "Objects of type `{}` can be used as {} context managers",
                context_expression_type.display(db, env),
                alt_mode
            ));
            diag.info(format!("Consider using `{alt_with_kw}` here"));
        }
    }
}
