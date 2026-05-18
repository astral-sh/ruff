use super::context::InferContext;
use super::{Signature, Type, TypeContext};
use crate::Db;
use crate::types::call::bind::BindingError;
use crate::types::{MemberLookupPolicy, PropertyInstanceType};
use ruff_python_ast as ast;

mod arguments;
pub(crate) mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{Binding, Bindings, CallableBinding, MatchedArgument};

impl<'db> Type<'db> {
    /// Memoize the pure return-type part of binary dunder resolution so repeated identical
    /// expressions don't re-run overload selection at every call site.
    pub(crate) fn try_call_bin_op_return_type(
        db: &'db dyn Db,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        #[salsa::tracked(cycle_initial=|_, _, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
        fn try_call_bin_op_return_type_impl<'db>(
            db: &'db dyn Db,
            left_ty: Type<'db>,
            op: ast::Operator,
            right_ty: Type<'db>,
        ) -> Option<Type<'db>> {
            Type::try_call_bin_op(db, left_ty, op, right_ty)
                .ok()
                .map(|bindings| bindings.return_type(db))
        }

        try_call_bin_op_return_type_impl(db, left_ty, op, right_ty)
    }

    pub(crate) fn try_call_bin_op(
        db: &'db dyn Db,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Result<Bindings<'db>, CallBinOpError> {
        Self::try_call_bin_op_with_policy(db, left_ty, op, right_ty, MemberLookupPolicy::default())
    }

    pub(crate) fn try_call_bin_op_with_policy(
        db: &'db dyn Db,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallBinOpError> {
        // We either want to call lhs.__op__ or rhs.__rop__. The full decision tree from
        // the Python spec [1] is:
        //
        //   - If rhs is a (proper) subclass of lhs, and it provides a different
        //     implementation of __rop__, use that.
        //   - Otherwise, if lhs implements __op__, use that.
        //   - Otherwise, if lhs and rhs are different types, and rhs implements __rop__,
        //     use that.
        //
        // [1] https://docs.python.org/3/reference/datamodel.html#object.__radd__

        // Technically we don't have to check left_ty != right_ty here, since if the types
        // are the same, they will trivially have the same implementation of the reflected
        // dunder, and so we'll fail the inner check. But the type equality check will be
        // faster for the common case, and allow us to skip the (two) class member lookups.
        let left_class = left_ty.to_meta_type(db);
        let right_class = right_ty.to_meta_type(db);
        if left_ty != right_ty && right_ty.is_subtype_of(db, left_ty) {
            let reflected_dunder = op.reflected_dunder();
            let rhs_reflected = right_class.member(db, reflected_dunder).place;
            // TODO: if `rhs_reflected` is possibly unbound, we should union the two possible
            // Bindings together
            if !rhs_reflected.is_undefined()
                && rhs_reflected != left_class.member(db, reflected_dunder).place
            {
                return Ok(right_ty
                    .try_call_dunder_with_policy(
                        db,
                        reflected_dunder,
                        &mut CallArguments::positional([left_ty]),
                        TypeContext::default(),
                        policy,
                    )
                    .or_else(|_| {
                        left_ty.try_call_dunder_with_policy(
                            db,
                            op.dunder(),
                            &mut CallArguments::positional([right_ty]),
                            TypeContext::default(),
                            policy,
                        )
                    })?);
            }
        }

        let call_on_left_instance = left_ty.try_call_dunder_with_policy(
            db,
            op.dunder(),
            &mut CallArguments::positional([right_ty]),
            TypeContext::default(),
            policy,
        );

        call_on_left_instance.or_else(|_| {
            if left_ty == right_ty {
                Err(CallBinOpError::NotSupported)
            } else {
                Ok(right_ty.try_call_dunder_with_policy(
                    db,
                    op.reflected_dunder(),
                    &mut CallArguments::positional([left_ty]),
                    TypeContext::default(),
                    policy,
                )?)
            }
        })
    }
}

/// Wraps a [`Bindings`] for an unsuccessful call with information about why the call was
/// unsuccessful.
///
/// The bindings are boxed so that we do not pass around large `Err` variants on the stack.
#[derive(Debug)]
pub(crate) struct CallError<'db>(pub(crate) CallErrorKind, pub(crate) Box<Bindings<'db>>);

impl<'db> CallError<'db> {
    pub(crate) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.1.return_type(db)
    }

    /// Returns `Some(property)` if the call error was caused by an attempt to set a property
    /// that has no setter, and `None` otherwise.
    pub(crate) fn as_attempt_to_set_property_with_no_setter(
        &self,
    ) -> Option<PropertyInstanceType<'db>> {
        if self.0 != CallErrorKind::BindingError {
            return None;
        }
        self.1
            .iter_flat()
            .flatten()
            .flat_map(bind::Binding::errors)
            .find_map(|error| match error {
                BindingError::PropertyHasNoSetter(property) => Some(*property),
                _ => None,
            })
    }

    /// Returns `Some(property)` if the call error was caused by an attempt to delete a property
    /// that has no deleter, and `None` otherwise.
    pub(crate) fn as_attempt_to_delete_property_with_no_deleter(
        &self,
    ) -> Option<PropertyInstanceType<'db>> {
        if self.0 != CallErrorKind::BindingError {
            return None;
        }
        self.1
            .iter_flat()
            .flatten()
            .flat_map(bind::Binding::errors)
            .find_map(|error| match error {
                BindingError::PropertyHasNoDeleter(property) => Some(*property),
                _ => None,
            })
    }
}

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
    PossiblyUnbound {
        // Describes the places where the dunder was indeed defined.
        bindings: Box<Bindings<'db>>,

        // Lists the types on which the dunder was undefined (e.g., the specific
        // members of a union on which the dunder was missing). `None` means
        // that the call path does not track where the dunder may be unbound.
        unbound_on: Option<Box<[Type<'db>]>>,
    },

    /// The dunder method with the specified name is missing.
    MethodNotAvailable,
}

impl<'db> CallDunderError<'db> {
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::MethodNotAvailable | Self::CallError(CallErrorKind::NotCallable, _) => None,
            Self::CallError(_, bindings) => Some(bindings.return_type(db)),
            Self::PossiblyUnbound { bindings, .. } => Some(bindings.return_type(db)),
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

#[derive(Debug)]
pub(crate) enum CallBinOpError {
    /// The dunder attribute exists but it can't be called with the given arguments.
    ///
    /// This includes non-callable dunder attributes that are possibly unbound.
    CallError,

    NotSupported,
}

impl From<CallDunderError<'_>> for CallBinOpError {
    fn from(value: CallDunderError<'_>) -> Self {
        match value {
            CallDunderError::CallError(_, _) => Self::CallError,
            CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound { .. } => {
                CallBinOpError::NotSupported
            }
        }
    }
}
