use super::diagnostic::{TypeCheckDiagnosticsBuilder, CALL_NON_CALLABLE};
use super::{Severity, Type, TypeArrayDisplay, UnionBuilder};
use crate::Db;
use ruff_db::diagnostic::DiagnosticId;
use ruff_python_ast as ast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallOutcome<'db> {
    Callable {
        return_ty: Type<'db>,
    },
    RevealType {
        return_ty: Type<'db>,
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
}

impl<'db> CallOutcome<'db> {
    /// Create a new `CallOutcome::Callable` with given return type.
    pub(super) fn callable(return_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::Callable { return_ty }
    }

    /// Create a new `CallOutcome::NotCallable` with given not-callable type.
    pub(super) fn not_callable(not_callable_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::NotCallable { not_callable_ty }
    }

    /// Create a new `CallOutcome::RevealType` with given revealed and return types.
    pub(super) fn revealed(return_ty: Type<'db>, revealed_ty: Type<'db>) -> CallOutcome<'db> {
        CallOutcome::RevealType {
            return_ty,
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

    /// Get the return type of the call, or `None` if not callable.
    pub(super) fn return_ty(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Callable { return_ty } => Some(*return_ty),
            Self::RevealType {
                return_ty,
                revealed_ty: _,
            } => Some(*return_ty),
            Self::NotCallable { not_callable_ty: _ } => None,
            Self::Union {
                outcomes,
                called_ty: _,
            } => outcomes
                .iter()
                // If all outcomes are NotCallable, we return None; if some outcomes are callable
                // and some are not, we return a union including Unknown.
                .fold(None, |acc, outcome| {
                    let ty = outcome.return_ty(db);
                    match (acc, ty) {
                        (None, None) => None,
                        (None, Some(ty)) => Some(UnionBuilder::new(db).add(ty)),
                        (Some(builder), ty) => Some(builder.add(ty.unwrap_or(Type::Unknown))),
                    }
                })
                .map(UnionBuilder::build),
            Self::PossiblyUnboundDunderCall { call_outcome, .. } => call_outcome.return_ty(db),
        }
    }

    /// Get the return type of the call, emitting default diagnostics if needed.
    pub(super) fn unwrap_with_diagnostic<'a>(
        &self,
        db: &'db dyn Db,
        node: ast::AnyNodeRef,
        diagnostics: &'a mut TypeCheckDiagnosticsBuilder<'db>,
    ) -> Type<'db> {
        match self.return_ty_result(db, node, diagnostics) {
            Ok(return_ty) => return_ty,
            Err(NotCallableError::Type {
                not_callable_ty,
                return_ty,
            }) => {
                diagnostics.add_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable",
                        not_callable_ty.display(db)
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElement {
                not_callable_ty,
                called_ty,
                return_ty,
            }) => {
                diagnostics.add_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (due to union element `{}`)",
                        called_ty.display(db),
                        not_callable_ty.display(db),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::UnionElements {
                not_callable_tys,
                called_ty,
                return_ty,
            }) => {
                diagnostics.add_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (due to union elements {})",
                        called_ty.display(db),
                        not_callable_tys.display(db),
                    ),
                );
                return_ty
            }
            Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: called_ty,
                return_ty,
            }) => {
                diagnostics.add_lint(
                    &CALL_NON_CALLABLE,
                    node,
                    format_args!(
                        "Object of type `{}` is not callable (possibly unbound `__call__` method)",
                        called_ty.display(db)
                    ),
                );
                return_ty
            }
        }
    }

    /// Get the return type of the call as a result.
    pub(super) fn return_ty_result<'a>(
        &self,
        db: &'db dyn Db,
        node: ast::AnyNodeRef,
        diagnostics: &'a mut TypeCheckDiagnosticsBuilder<'db>,
    ) -> Result<Type<'db>, NotCallableError<'db>> {
        match self {
            Self::Callable { return_ty } => Ok(*return_ty),
            Self::RevealType {
                return_ty,
                revealed_ty,
            } => {
                diagnostics.add(
                    node,
                    DiagnosticId::RevealedType,
                    Severity::Info,
                    format_args!("Revealed type is `{}`", revealed_ty.display(db)),
                );
                Ok(*return_ty)
            }
            Self::NotCallable { not_callable_ty } => Err(NotCallableError::Type {
                not_callable_ty: *not_callable_ty,
                return_ty: Type::Unknown,
            }),
            Self::PossiblyUnboundDunderCall {
                called_ty,
                call_outcome,
            } => Err(NotCallableError::PossiblyUnboundDunderCall {
                callable_ty: *called_ty,
                return_ty: call_outcome.return_ty(db).unwrap_or(Type::Unknown),
            }),
            Self::Union {
                outcomes,
                called_ty,
            } => {
                let mut not_callable = vec![];
                let mut union_builder = UnionBuilder::new(db);
                let mut revealed = false;
                for outcome in outcomes {
                    let return_ty = match outcome {
                        Self::NotCallable { not_callable_ty } => {
                            not_callable.push(*not_callable_ty);
                            Type::Unknown
                        }
                        Self::RevealType {
                            return_ty,
                            revealed_ty: _,
                        } => {
                            if revealed {
                                *return_ty
                            } else {
                                revealed = true;
                                outcome.unwrap_with_diagnostic(db, node, diagnostics)
                            }
                        }
                        _ => outcome.unwrap_with_diagnostic(db, node, diagnostics),
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
        }
    }
}

pub(super) enum CallDunderResult<'db> {
    CallOutcome(CallOutcome<'db>),
    PossiblyUnbound(CallOutcome<'db>),
    MethodNotAvailable,
}

impl<'db> CallDunderResult<'db> {
    pub(super) fn return_ty(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::CallOutcome(outcome) => outcome.return_ty(db),
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
    pub(super) fn return_ty(&self) -> Type<'db> {
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
    pub(super) fn called_ty(&self) -> Type<'db> {
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
