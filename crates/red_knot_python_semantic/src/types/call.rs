use super::context::InferContext;
use super::diagnostic::{CALL_NON_CALLABLE, TYPE_ASSERTION_FAILURE};
use super::{Severity, Signature, Type, TypeArrayDisplay, UnionBuilder};
use crate::types::diagnostic::STATIC_ASSERT_ERROR;
use crate::Db;
use ruff_db::diagnostic::DiagnosticId;
use ruff_python_ast as ast;

mod arguments;
mod bind;

pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{bind_call, CallBinding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StaticAssertionErrorKind<'db> {
    ArgumentIsFalse,
    ArgumentIsFalsy(Type<'db>),
    ArgumentTruthinessIsAmbiguous(Type<'db>),
    CustomError(&'db str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallOutcome<'db> {
    Callable {
        binding: CallBinding<'db>,
    },
    RevealType {
        binding: CallBinding<'db>,
        revealed_ty: Type<'db>,
    },
    NotCallable {
        not_callable_ty: Type<'db>,
    },
    Union {
        called_ty: Type<'db>,
        outcomes: Box<[CallOutcome<'db>]>,
    },
    PossiblyUnboundDunderCall {
        called_ty: Type<'db>,
        call_outcome: Box<CallOutcome<'db>>,
    },
    StaticAssertionError {
        binding: CallBinding<'db>,
        error_kind: StaticAssertionErrorKind<'db>,
    },
    AssertType {
        binding: CallBinding<'db>,
        asserted_ty: Type<'db>,
    },
}

impl<'db> CallOutcome<'db> {
    /// Create a new `CallOutcome::Callable` with given binding.
    pub(super) fn callable(binding: CallBinding<'db>) -> CallOutcome<'db> {
        CallOutcome::Callable { binding }
    }

    /// Create a new `CallOutcome::NotCallable` with given not-callable type.
    pub(super) fn not_callable(not_callable_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::NotCallable { not_callable_ty }
    }

    /// Create a new `CallOutcome::RevealType` with given revealed and return types.
    pub(super) fn revealed(binding: CallBinding<'db>, revealed_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::RevealType {
            binding,
            revealed_ty,
        }
    }

    /// Create a new `CallOutcome::Union` with given wrapped outcomes.
    pub(super) fn union(
        called_ty: Type<'db>,
        outcomes: impl IntoIterator<Item = CallOutcome<'db>>,
    ) -> CallOutcome<'db> {
        CallOutcome::Union {
            called_ty,
            outcomes: outcomes.into_iter().collect(),
        }
    }

    /// Create a new `CallOutcome::AssertType` with given asserted and return types.
    pub(super) fn asserted(binding: CallBinding<'db>, asserted_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::AssertType {
            binding,
            asserted_ty,
        }
    }

    /// Get the return type of the call, or `None` if not callable.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Callable { binding } => Some(binding.return_type()),
            Self::RevealType {
                binding,
                revealed_ty: _,
            } => Some(binding.return_type()),
            Self::NotCallable { not_callable_ty: _ } => None,
            Self::Union {
                outcomes,
                called_ty: _,
            } => outcomes
                .iter()
                // If all outcomes are NotCallable, we return None; if some outcomes are callable
                // and some are not, we return a union including Unknown.
                .fold(None, |acc, outcome| {
                    let ty = outcome.return_type(db);
                    match (acc, ty) {
                        (None, None) => None,
                        (None, Some(ty)) => Some(UnionBuilder::new(db).add(ty)),
                        (Some(builder), ty) => Some(builder.add(ty.unwrap_or(Type::unknown()))),
                    }
                })
                .map(UnionBuilder::build),
            Self::PossiblyUnboundDunderCall { call_outcome, .. } => call_outcome.return_type(db),
            Self::StaticAssertionError { .. } => Some(Type::none(db)),
            Self::AssertType {
                binding,
                asserted_ty: _,
            } => Some(binding.return_type()),
        }
    }

    /// Get the return type of the call, emitting default diagnostics if needed.
    pub(super) fn unwrap_with_diagnostic(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
    ) -> Type<'db> {
        match self.return_type_result(context, node) {
            Ok(return_ty) => return_ty,
            Err(NotCallableError::Type {
                not_callable_ty,
                return_ty,
            }) => {
                context.report_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable",
                        not_callable_ty.display(context.db())
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElement {
                not_callable_ty,
                called_ty,
                return_ty,
            }) => {
                context.report_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (due to union element `{}`)",
                        called_ty.display(context.db()),
                        not_callable_ty.display(context.db()),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElements {
                not_callable_tys,
                called_ty,
                return_ty,
            }) => {
                context.report_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (due to union elements {})",
                        called_ty.display(context.db()),
                        not_callable_tys.display(context.db()),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: called_ty,
                return_ty,
            }) => {
                context.report_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (possibly unbound `__call__` method)",
                        called_ty.display(context.db())
                    ),
                );
                return_ty
            }
        }
    }

    /// Get the return type of the call as a result.
    pub(super) fn return_type_result(
        &self,
        context: &InferContext<'db>,
        node: ast::AnyNodeRef,
    ) -> Result<Type<'db>, NotCallableError<'db>> {
        // TODO should this method emit diagnostics directly, or just return results that allow the
        // caller to decide about emitting diagnostics? Currently it emits binding diagnostics, but
        // only non-callable diagnostics in the union case, which is inconsistent.
        match self {
            Self::Callable { binding } => {
                binding.report_diagnostics(context, node);
                Ok(binding.return_type())
            }
            Self::RevealType {
                binding,
                revealed_ty,
            } => {
                binding.report_diagnostics(context, node);
                context.report_diagnostic(
                    node,
                    DiagnosticId::RevealedType,
                    Severity::Info,
                    format_args!("Revealed type is `{}`", revealed_ty.display(context.db())),
                );
                Ok(binding.return_type())
            }
            Self::NotCallable { not_callable_ty } => Err(NotCallableError::Type {
                not_callable_ty: *not_callable_ty,
                return_ty: Type::unknown(),
            }),
            Self::PossiblyUnboundDunderCall {
                called_ty,
                call_outcome,
            } => Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: *called_ty,
                return_ty: call_outcome
                    .return_type(context.db())
                    .unwrap_or(Type::unknown()),
            }),
            Self::Union {
                outcomes,
                called_ty,
            } => {
                let mut not_callable = vec![];
                let mut union_builder = UnionBuilder::new(context.db());
                let mut revealed = false;
                for outcome in outcomes {
                    let return_ty = match outcome {
                        Self::NotCallable { not_callable_ty } => {
                            not_callable.push(*not_callable_ty);
                            Type::unknown()
                        }
                        Self::RevealType {
                            binding,
                            revealed_ty: _,
                        } => {
                            if revealed {
                                binding.return_type()
                            } else {
                                revealed = true;
                                outcome.unwrap_with_diagnostic(context, node)
                            }
                        }
                        _ => outcome.unwrap_with_diagnostic(context, node),
                    };
                    union_builder = union_builder.add(return_ty);
                }
                let return_ty = union_builder.build();
                match not_callable[..] {
                    [] => Ok(return_ty),
                    [elem] => Err(NotCallableError::UnionElement {
                        not_callable_ty: elem,
                        called_ty: *called_ty,
                        return_ty,
                    }),
                    _ if not_callable.len() == outcomes.len() => Err(NotCallableError::Type {
                        not_callable_ty: *called_ty,
                        return_ty,
                    }),
                    _ => Err(NotCallableError::UnionElements {
                        not_callable_tys: not_callable.into_boxed_slice(),
                        called_ty: *called_ty,
                        return_ty,
                    }),
                }
            }
            Self::StaticAssertionError {
                binding,
                error_kind,
            } => {
                binding.report_diagnostics(context, node);

                match error_kind {
                    StaticAssertionErrorKind::ArgumentIsFalse => {
                        context.report_lint(
                            &STATIC_ASSERT_ERROR,
                            node,
                            format_args!("Static assertion error: argument evaluates to `False`"),
                        );
                    }
                    StaticAssertionErrorKind::ArgumentIsFalsy(parameter_ty) => {
                        context.report_lint(
                            &STATIC_ASSERT_ERROR,
                            node,
                            format_args!(
                                "Static assertion error: argument of type `{parameter_ty}` is statically known to be falsy",
                                parameter_ty=parameter_ty.display(context.db())
                            ),
                        );
                    }
                    StaticAssertionErrorKind::ArgumentTruthinessIsAmbiguous(parameter_ty) => {
                        context.report_lint(
                            &STATIC_ASSERT_ERROR,
                            node,
                            format_args!(
                                "Static assertion error: argument of type `{parameter_ty}` has an ambiguous static truthiness",
                                parameter_ty=parameter_ty.display(context.db())
                            ),
                        );
                    }
                    StaticAssertionErrorKind::CustomError(message) => {
                        context.report_lint(
                            &STATIC_ASSERT_ERROR,
                            node,
                            format_args!("Static assertion error: {message}"),
                        );
                    }
                }

                Ok(Type::unknown())
            }
            Self::AssertType {
                binding,
                asserted_ty,
            } => {
                let [actual_ty, _asserted] = binding.parameter_types() else {
                    return Ok(binding.return_type());
                };

                if !actual_ty.is_gradual_equivalent_to(context.db(), *asserted_ty) {
                    context.report_lint(
                        &TYPE_ASSERTION_FAILURE,
                        node,
                        format_args!(
                            "Actual type `{}` is not the same as asserted type `{}`",
                            actual_ty.display(context.db()),
                            asserted_ty.display(context.db()),
                        ),
                    );
                }

                Ok(binding.return_type())
            }
        }
    }
}

pub(super) enum CallDunderResult<'db> {
    CallOutcome(CallOutcome<'db>),
    PossiblyUnbound(CallOutcome<'db>),
    MethodNotAvailable,
}

impl<'db> CallDunderResult<'db> {
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::CallOutcome(outcome) => outcome.return_type(db),
            Self::PossiblyUnbound { .. } => None,
            Self::MethodNotAvailable => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NotCallableError<'db> {
    /// The type is not callable.
    Type {
        not_callable_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    /// A single union element is not callable.
    UnionElement {
        not_callable_ty: Type<'db>,
        called_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    /// Multiple (but not all) union elements are not callable.
    UnionElements {
        not_callable_tys: Box<[Type<'db>]>,
        called_ty: Type<'db>,
        return_ty: Type<'db>,
    },
    PossiblyUnboundDunderCall {
        callable_ty: Type<'db>,
        return_ty: Type<'db>,
    },
}

impl<'db> NotCallableError<'db> {
    /// The return type that should be used when a call is not callable.
    pub(super) fn return_type(&self) -> Type<'db> {
        match self {
            Self::Type { return_ty, .. } => *return_ty,
            Self::UnionElement { return_ty, .. } => *return_ty,
            Self::UnionElements { return_ty, .. } => *return_ty,
            Self::PossiblyUnboundDunderCall { return_ty, .. } => *return_ty,
        }
    }

    /// The resolved type that was not callable.
    ///
    /// For unions, returns the union type itself, which may contain a mix of callable and
    /// non-callable types.
    pub(super) fn called_type(&self) -> Type<'db> {
        match self {
            Self::Type {
                not_callable_ty, ..
            } => *not_callable_ty,
            Self::UnionElement { called_ty, .. } => *called_ty,
            Self::UnionElements { called_ty, .. } => *called_ty,
            Self::PossiblyUnboundDunderCall {
                callable_ty: called_ty,
                ..
            } => *called_ty,
        }
    }
}
