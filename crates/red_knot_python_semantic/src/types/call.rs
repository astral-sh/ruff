use super::context::InferContext;
use super::{Signature, Type};
use crate::types::UnionType;
use crate::Db;

mod arguments;
mod bind;

pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{bind_call, CallBinding};

/// A successfully bound call where all arguments are valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallOutcome<'db> {
    /// The call resolves to exactly one binding.
    Single(CallBinding<'db>),

    /// The call resolves to multiple bindings.
    Union(Box<[CallBinding<'db>]>),
}

impl<'db> CallOutcome<'db> {
    /// Tries calling each union element using the provided `call` function.
    ///
    /// Returns `Ok` if all variants are callable according to the callback and `Err` otherwise.
    pub(super) fn try_call<F>(
        db: &'db dyn Db,
        union: UnionType<'db>,
        call: F,
    ) -> Result<Self, CallError<'db>>
    where
        F: Fn(Type<'db>) -> Result<Self, CallError<'db>>,
    {
        let elements = union.elements(db);
        let mut bindings = Vec::with_capacity(elements.len());
        let mut not_callable = Vec::new();

        for element in elements {
            match call(*element) {
                Ok(CallOutcome::Single(binding)) => bindings.push(binding),
                Ok(CallOutcome::Union(inner_bindings)) => {
                    bindings.extend(inner_bindings);
                }
                Err(error) => match error {
                    CallError::NotCallable { not_callable_ty } => {
                        not_callable.push(NotCallableVariant::new(not_callable_ty, None));
                    }
                    CallError::NotCallableVariants {
                        called_ty: _,
                        callable: inner_callable,
                        not_callable: inner_not_callable,
                    } => {
                        not_callable.extend(inner_not_callable);
                        bindings.extend(inner_callable);
                    }
                    // Should this be OK, or an error? We can't make it not_callable
                    // because calling it would actually work because it ignores the
                    // possibly unboundness.
                    CallError::PossiblyUnboundDunderCall { outcome, .. } => match *outcome {
                        CallOutcome::Union(inner_bindings) => {
                            bindings.extend(inner_bindings);
                        }
                        CallOutcome::Single(binding) => {
                            bindings.push(binding);
                        }
                    },
                    CallError::BindingError { binding } => {
                        not_callable.push(NotCallableVariant::new(
                            binding.callable_type(),
                            Some(binding),
                        ));
                    }
                },
            }
        }

        if not_callable.is_empty() {
            Ok(CallOutcome::Union(bindings.into()))
        } else {
            Err(CallError::NotCallableVariants {
                not_callable: not_callable.into(),
                callable: bindings.into(),
                called_ty: union,
            })
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
    /// can't be called with the given arguments or isn't callable at all.
    NotCallableVariants {
        /// The variants that can't be called with the given arguments.
        not_callable: Box<[NotCallableVariant<'db>]>,

        /// The variants that can be called with the given arguments.
        callable: Box<[CallBinding<'db>]>,

        /// The union type that we tried calling.
        called_ty: UnionType<'db>,
    },

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
            CallError::NotCallableVariants {
                not_callable,
                callable,
                ..
            } => {
                // If all variants are not callable, return `None`
                if callable.is_empty()
                    && not_callable.iter().all(|binding| binding.binding.is_none())
                {
                    None
                } else {
                    // If some variants are callable, and some are not, return the union of the return types of the callable variants
                    // combined with `Type::Unknown`
                    Some(UnionType::from_elements(
                        db,
                        callable.iter().map(bind::CallBinding::return_type).chain(
                            not_callable
                                .iter()
                                .map(NotCallableVariant::unwrap_return_type),
                        ),
                    ))
                }
            }
            Self::PossiblyUnboundDunderCall { outcome, .. } => Some(outcome.return_type(db)),
            Self::BindingError { binding } => Some(binding.return_type()),
        }
    }

    /// Returns the return type of the call or a fallback that
    /// represents the best guess of the return type (e.g. the actual return type even if the
    /// dunder is possibly unbound).
    ///
    /// If the type is not callable, returns `Type::Unknown`.
    pub(super) fn unwrap_return_type(&self, db: &'db dyn Db) -> Type<'db> {
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
            Self::NotCallableVariants { called_ty, .. } => Type::Union(*called_ty),
            Self::PossiblyUnboundDunderCall { called_type, .. } => *called_type,
            Self::BindingError { binding } => binding.callable_type(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NotCallableVariant<'db> {
    /// The resolved binding for that variant or `None` if the variant is not callable.
    binding: Option<CallBinding<'db>>,

    /// The variant's type that is not callable.
    not_callable: Type<'db>,
}

impl<'db> NotCallableVariant<'db> {
    pub(super) fn new(ty: Type<'db>, binding: Option<CallBinding<'db>>) -> Self {
        Self {
            not_callable: ty,
            binding,
        }
    }

    pub(super) fn return_type(&self) -> Option<Type<'db>> {
        self.binding.as_ref().map(CallBinding::return_type)
    }

    pub(super) fn unwrap_return_type(&self) -> Type<'db> {
        self.return_type().unwrap_or(Type::unknown())
    }

    pub(super) fn not_callable_type(&self) -> Type<'db> {
        self.not_callable
    }
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

    pub(super) fn unwrap_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.return_type(db).unwrap_or(Type::unknown())
    }
}

impl<'db> From<CallError<'db>> for CallDunderError<'db> {
    fn from(error: CallError<'db>) -> Self {
        Self::Call(error)
    }
}
