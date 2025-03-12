use super::context::InferContext;
use super::{CallableSignature, Signature, Signatures, Type};
use crate::Db;

mod arguments;
mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::Bindings;

/// The reason why calling a type failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallError<T> {
    /// The type is not callable.
    NotCallable(T),

    /// The type is callable but not with the given arguments.
    BindingError(T),

    /// The type is possibly not callable, but there are no binding errors in the situations where
    /// it is callable.
    PossiblyNotCallable(T),
}

impl<'db> CallError<Bindings<'db>> {
    pub(super) fn bindings(&self) -> &Bindings<'db> {
        match self {
            CallError::NotCallable(bindings)
            | CallError::BindingError(bindings)
            | CallError::PossiblyNotCallable(bindings) => bindings,
        }
    }

    /// Returns a fallback return type to use that best approximates the return type of the call.
    ///
    /// Returns `None` if the type isn't callable.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        let bindings = self.bindings();
        if bindings.is_not_callable() {
            return None;
        }
        Some(bindings.return_type(db))
    }

    /// Returns the return type of the call or a fallback that
    /// represents the best guess of the return type (e.g. the actual return type even if the
    /// dunder is possibly unbound).
    ///
    /// If the type is not callable, returns `Type::Unknown`.
    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.bindings().return_type(db)
    }

    /// The resolved type that was not callable.
    ///
    /// For unions, returns the union type itself, which may contain a mix of callable and
    /// non-callable types.
    pub(super) fn called_type(&self) -> Type<'db> {
        self.bindings().ty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallDunderError<'db> {
    /// The dunder attribute exists but it can't be called with the given arguments.
    ///
    /// This includes non-callable dunder attributes that are possibly unbound.
    Call(CallError<Bindings<'db>>),

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
            Self::Call(error) => error.return_type(db),
            Self::PossiblyUnbound(call_outcome) => Some(call_outcome.return_type(db)),
            Self::MethodNotAvailable => None,
        }
    }

    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }
}

impl<'db> From<CallError<Bindings<'db>>> for CallDunderError<'db> {
    fn from(error: CallError<Bindings<'db>>) -> Self {
        Self::Call(error)
    }
}
