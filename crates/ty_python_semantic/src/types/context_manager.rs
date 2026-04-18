use crate::{
    Db,
    types::{
        CallDunderOutcome, SimpleCallArguments, Type, TypeContext, context::InferContext,
        diagnostic::INVALID_CONTEXT_MANAGER,
    },
};
use ruff_python_ast as ast;
use ty_python_core::EvaluationMode;

impl<'db> Type<'db> {
    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    pub(super) fn enter(self, db: &'db dyn Db) -> Type<'db> {
        self.try_enter_with_mode(db, EvaluationMode::Sync)
            .unwrap_or_else(|err| err.fallback_enter_type(db))
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    pub(super) fn aenter(self, db: &'db dyn Db) -> Type<'db> {
        self.try_enter_with_mode(db, EvaluationMode::Async)
            .unwrap_or_else(|err| err.fallback_enter_type(db))
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
        mode: EvaluationMode,
    ) -> Result<Type<'db>, Box<ContextManagerError<'db>>> {
        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let enter = self.dunder_call_outcome(
            db,
            enter_method,
            SimpleCallArguments::None,
            TypeContext::default(),
        );
        let exit = self.dunder_call_outcome(
            db,
            exit_method,
            SimpleCallArguments::Ternary(Type::none(db), Type::none(db), Type::none(db)),
            TypeContext::default(),
        );

        // TODO: Make use of Protocols when we support it (the manager be assignable to `contextlib.AbstractContextManager`).
        match (enter, exit) {
            (CallDunderOutcome::ReturnType(ty), CallDunderOutcome::ReturnType(_)) => {
                Ok(if mode.is_async() {
                    ty.try_await(db).unwrap_or(Type::unknown())
                } else {
                    ty
                })
            }
            (CallDunderOutcome::ReturnType(ty), exit_error) => {
                Err(Box::new(ContextManagerError::Exit {
                    enter_return_type: if mode.is_async() {
                        ty.try_await(db).unwrap_or(Type::unknown())
                    } else {
                        ty
                    },
                    exit_error,
                    mode,
                }))
            }
            // TODO: Use the `exit_ty` to determine if any raised exception is suppressed.
            (enter_error, CallDunderOutcome::ReturnType(_)) => {
                Err(Box::new(ContextManagerError::Enter(enter_error, mode)))
            }
            (enter_error, exit_error) => Err(Box::new(ContextManagerError::EnterAndExit {
                enter_error,
                exit_error,
                mode,
            })),
        }
    }
}

/// Error returned if a type is not (or may not be) a context manager.
#[derive(Debug)]
pub(super) enum ContextManagerError<'db> {
    Enter(CallDunderOutcome<'db>, EvaluationMode),
    Exit {
        enter_return_type: Type<'db>,
        exit_error: CallDunderOutcome<'db>,
        mode: EvaluationMode,
    },
    EnterAndExit {
        enter_error: CallDunderOutcome<'db>,
        exit_error: CallDunderOutcome<'db>,
        mode: EvaluationMode,
    },
}

impl<'db> ContextManagerError<'db> {
    pub(super) fn fallback_enter_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.enter_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the `__enter__` or `__aenter__` return type if it is known,
    /// or `None` if the type never has a callable `__enter__` or `__aenter__` attribute
    fn enter_type(&self, _db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Exit {
                enter_return_type,
                exit_error: _,
                mode: _,
            } => Some(*enter_return_type),
            Self::Enter(enter_error, _)
            | Self::EnterAndExit {
                enter_error,
                exit_error: _,
                mode: _,
            } => enter_error.return_type(),
        }
    }

    pub(super) fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        let Some(builder) = context.report_lint(&INVALID_CONTEXT_MANAGER, context_expression_node)
        else {
            return;
        };

        let mode = match self {
            Self::Exit { mode, .. } | Self::Enter(_, mode) | Self::EnterAndExit { mode, .. } => {
                *mode
            }
        };

        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let format_call_dunder_error = |call_dunder_error: &CallDunderOutcome<'db>, name: &str| {
            match call_dunder_error {
                CallDunderOutcome::MethodNotAvailable => format!("it does not implement `{name}`"),
                CallDunderOutcome::PossiblyUnbound(_) => {
                    format!("the method `{name}` may be missing")
                }
                // TODO: Use more specific error messages for the different error cases.
                //  E.g. hint toward the union variant that doesn't correctly implement enter,
                //  distinguish between a not callable `__enter__` attribute and a wrong signature.
                CallDunderOutcome::CallError(_, _) => {
                    format!("it does not correctly implement `{name}`")
                }
                CallDunderOutcome::ReturnType(_) => {
                    format!("it has an inconsistent `{name}` implementation")
                }
            }
        };

        let format_call_dunder_errors = |error_a: &CallDunderOutcome<'db>,
                                         name_a: &str,
                                         error_b: &CallDunderOutcome<'db>,
                                         name_b: &str| {
            match (error_a, error_b) {
                (CallDunderOutcome::PossiblyUnbound(_), CallDunderOutcome::PossiblyUnbound(_)) => {
                    format!("the methods `{name_a}` and `{name_b}` are possibly missing")
                }
                (CallDunderOutcome::MethodNotAvailable, CallDunderOutcome::MethodNotAvailable) => {
                    format!("it does not implement `{name_a}` and `{name_b}`")
                }
                (CallDunderOutcome::CallError(_, _), CallDunderOutcome::CallError(_, _)) => {
                    format!("it does not correctly implement `{name_a}` or `{name_b}`")
                }
                (_, _) => format!(
                    "{format_a}, and {format_b}",
                    format_a = format_call_dunder_error(error_a, name_a),
                    format_b = format_call_dunder_error(error_b, name_b)
                ),
            }
        };

        let db = context.db();

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
        };

        // Suggest using `async with` if only async methods are available in a sync context,
        // or suggest using `with` if only sync methods are available in an async context.
        let with_kw = match mode {
            EvaluationMode::Sync => "with",
            EvaluationMode::Async => "async with",
        };

        let mut diag = builder.into_diagnostic(format_args!(
            "Object of type `{}` cannot be used with `{}` because {}",
            context_expression_type.display(db),
            with_kw,
            formatted_errors,
        ));

        let (alt_mode, alt_enter_method, alt_exit_method, alt_with_kw) = match mode {
            EvaluationMode::Sync => ("async", "__aenter__", "__aexit__", "async with"),
            EvaluationMode::Async => ("sync", "__enter__", "__exit__", "with"),
        };

        let alt_enter = context_expression_type.dunder_call_outcome(
            db,
            alt_enter_method,
            SimpleCallArguments::None,
            TypeContext::default(),
        );
        let alt_exit = context_expression_type.dunder_call_outcome(
            db,
            alt_exit_method,
            SimpleCallArguments::Ternary(Type::unknown(), Type::unknown(), Type::unknown()),
            TypeContext::default(),
        );

        if matches!(
            alt_enter,
            CallDunderOutcome::ReturnType(_) | CallDunderOutcome::CallError(..)
        ) && matches!(
            alt_exit,
            CallDunderOutcome::ReturnType(_) | CallDunderOutcome::CallError(..)
        ) {
            diag.info(format_args!(
                "Objects of type `{}` can be used as {} context managers",
                context_expression_type.display(db),
                alt_mode
            ));
            diag.info(format!("Consider using `{alt_with_kw}` here"));
        }
    }
}
