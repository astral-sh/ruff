use super::context::InferContext;
use super::{CallableSignature, Signature, Signatures, Type};
use crate::Db;

mod arguments;
mod bind;
pub(super) use arguments::{Argument, CallArgumentTypes, CallArguments};
pub(super) use bind::Bindings;

/// Wraps a [`Bindings`] for an unsuccessful call with information about why the call was
/// unsuccessful.
///
/// The bindings are boxed so that we do not pass around large `Err` variants on the stack.
#[derive(Debug)]
pub(crate) struct CallError<'db>(pub(crate) CallErrorKind, pub(crate) Box<Bindings<'db>>);

/// The reason why calling a type failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallErrorKind {
    /// The type is not callable. For a union type, _none_ of the union elements are callable.
    NotCallable,

    /// The type is not callable with the given arguments.
    ///
    /// `BindingError` takes precedence over `PossiblyNotCallable`: for a union type, there might
    /// be some union elements that are not callable at all, but the call arguments are not
    /// compatible with at least one of the callable elements.
    BindingError,

    /// Not all of the elements of a union type are callable, but the call arguments are compatible
    /// with all of the callable elements.
    PossiblyNotCallable,
}

#[derive(Debug)]
pub(super) enum CallDunderError<'db> {
    /// The dunder attribute exists but it can't be called with the given arguments.
    ///
    /// This includes non-callable dunder attributes that are possibly unbound.
    CallError(CallErrorKind, Box<Bindings<'db>>),

    /// The type has the specified dunder method and it is callable
    /// with the specified arguments without any binding errors
    /// but it is possibly unbound.
    PossiblyUnbound(Box<Bindings<'db>>),

    /// The dunder method with the specified name is missing.
    MethodNotAvailable,
}

impl<'db> CallDunderError<'db> {
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::MethodNotAvailable | Self::CallError(CallErrorKind::NotCallable, _) => None,
            Self::CallError(_, bindings) => Some(bindings.return_type(db)),
            Self::PossiblyUnbound(bindings) => Some(bindings.return_type(db)),
        }
    }

    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }
}

impl<'db> From<CallError<'db>> for CallDunderError<'db> {
    fn from(CallError(kind, bindings): CallError<'db>) -> Self {
        Self::CallError(kind, bindings)
    }
}
