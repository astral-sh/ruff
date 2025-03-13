use super::context::InferContext;
use super::{CallableSignature, Signature, Signatures, Type};
use crate::Db;

mod arguments;
mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::Bindings;

/// The reason why calling a type failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallError {
    /// The type is not callable.
    NotCallable,

    /// The type is callable but not with the given arguments.
    BindingError,

    /// The type is possibly not callable, but there are no binding errors in the situations where
    /// it is callable.
    PossiblyNotCallable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallDunderError<'db> {
    /// The dunder attribute exists but it can't be called with the given arguments.
    ///
    /// This includes non-callable dunder attributes that are possibly unbound.
    Call(Bindings<'db>, CallError),

    /// The type has the specified dunder method and it is callable
    /// with the specified arguments without any binding errors
    /// but it is possibly unbound.
    PossiblyUnbound(Bindings<'db>),

    /// The dunder method with the specified name is missing.
    MethodNotAvailable,
}

impl<'db> CallDunderError<'db> {
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::MethodNotAvailable | Self::Call(_, CallError::NotCallable) => None,
            Self::Call(bindings, _) | Self::PossiblyUnbound(bindings) => {
                Some(bindings.return_type(db))
            }
        }
    }

    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }
}
