use super::context::InferContext;
use super::{Signature, Type};
use crate::types::UnionType;
use crate::Db;

mod arguments;
mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{bind_call, CallBinding};

/// A successfully bound call where all arguments are valid.
///
/// It's guaranteed that the wrapped bindings have no errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallOutcome<'db> {
    /// The call resolves to exactly one binding.
    Single(CallBinding<'db>),

    /// The call resolves to multiple bindings.
    Union(Box<[CallBinding<'db>]>),
}

impl<'db> CallOutcome<'db> {
    /// Calls each union element using the provided `call` function.
    ///
    /// Returns `Ok` if all variants can be called without error according to the callback and `Err` otherwise.
    pub(super) fn try_call_union<F>(
        db: &'db dyn Db,
        union: UnionType<'db>,
        call: F,
    ) -> Result<Self, CallError<'db>>
    where
        F: Fn(Type<'db>) -> Result<Self, CallError<'db>>,
    {
        let elements = union.elements(db);
        let mut bindings = Vec::with_capacity(elements.len());
        let mut errors = Vec::new();
        let mut not_callable = true;

        for element in elements {
            match call(*element) {
                Ok(CallOutcome::Single(binding)) => bindings.push(binding),
                Ok(CallOutcome::Union(inner_bindings)) => {
                    bindings.extend(inner_bindings);
                }
                Err(error) => {
                    not_callable |= error.is_not_callable();
                    errors.push(error);
                }
            }
        }

        if errors.is_empty() {
            Ok(CallOutcome::Union(bindings.into()))
        } else if bindings.is_empty() && not_callable {
            Err(CallError::NotCallable {
                not_callable_ty: Type::Union(union),
            })
        } else {
            Err(CallError::Union(UnionCallError {
                errors: errors.into(),
                bindings: bindings.into(),
                called_ty: Type::Union(union),
            }))
        }
    }

    /// The type returned by this call.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Single(binding) => binding.return_type(),
            Self::Union(bindings) => {
                UnionType::from_elements(db, bindings.iter().map(bind::CallBinding::return_type))
            }
        }
    }

    pub(super) fn bindings(&self) -> &[CallBinding<'db>] {
        match self {
            Self::Single(binding) => std::slice::from_ref(binding),
            Self::Union(bindings) => bindings,
        }
    }
}

/// The reason why calling a type failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallError<'db> {
    /// The type is not callable.
    NotCallable {
        /// The type that can't be called.
        not_callable_ty: Type<'db>,
    },

    /// A call to a union failed because at least one variant
    /// can't be called with the given arguments.
    ///
    /// A union where all variants are not callable is represented as a `NotCallable` error.
    Union(UnionCallError<'db>),

    /// The type has a `__call__` method but it isn't always bound.
    PossiblyUnboundDunderCall {
        called_type: Type<'db>,
        outcome: Box<CallOutcome<'db>>,
    },

    /// The type is callable but not with the given arguments.
    BindingError { binding: CallBinding<'db> },
}

impl<'db> CallError<'db> {
    /// Returns a fallback return type to use that best approximates the return type of the call.
    ///
    /// Returns `None` if the type isn't callable.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            CallError::NotCallable { .. } => None,
            // If some variants are callable, and some are not, return the union of the return types of the callable variants
            // combined with `Type::Unknown`
            CallError::Union(UnionCallError {
                bindings, errors, ..
            }) => Some(UnionType::from_elements(
                db,
                bindings
                    .iter()
                    .map(CallBinding::return_type)
                    .chain(errors.iter().map(|err| err.fallback_return_type(db))),
            )),
            Self::PossiblyUnboundDunderCall { outcome, .. } => Some(outcome.return_type(db)),
            Self::BindingError { binding } => Some(binding.return_type()),
        }
    }

    /// Returns the return type of the call or a fallback that
    /// represents the best guess of the return type (e.g. the actual return type even if the
    /// dunder is possibly unbound).
    ///
    /// If the type is not callable, returns `Type::Unknown`.
    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }

    /// The resolved type that was not callable.
    ///
    /// For unions, returns the union type itself, which may contain a mix of callable and
    /// non-callable types.
    pub(super) fn called_type(&self) -> Type<'db> {
        match self {
            Self::NotCallable {
                not_callable_ty, ..
            } => *not_callable_ty,
            Self::Union(UnionCallError { called_ty, .. }) => *called_ty,
            Self::PossiblyUnboundDunderCall { called_type, .. } => *called_type,
            Self::BindingError { binding } => binding.callable_type(),
        }
    }

    pub(super) const fn is_not_callable(&self) -> bool {
        matches!(self, Self::NotCallable { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnionCallError<'db> {
    /// The variants that can't be called with the given arguments.
    pub(super) errors: Box<[CallError<'db>]>,

    /// The bindings for the callable variants (that have no binding errors).
    pub(super) bindings: Box<[CallBinding<'db>]>,

    /// The union type that we tried calling.
    pub(super) called_ty: Type<'db>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallDunderError<'db> {
    /// The dunder attribute exists but it can't be called with the given arguments.
    ///
    /// This includes non-callable dunder attributes that are possibly unbound.
    Call(CallError<'db>),

    /// The type has the specified dunder method and it is callable
    /// with the specified arguments without any binding errors
    /// but it is possibly unbound.
    PossiblyUnbound(CallOutcome<'db>),

    /// The dunder method with the specified name is missing.
    MethodNotAvailable,
}

impl<'db> CallDunderError<'db> {
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Call(error) => error.return_type(db),
            Self::PossiblyUnbound(_) => None,
            Self::MethodNotAvailable => None,
        }
    }

    pub(super) fn fallback_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }
}

impl<'db> From<CallError<'db>> for CallDunderError<'db> {
    fn from(error: CallError<'db>) -> Self {
        Self::Call(error)
    }
}
