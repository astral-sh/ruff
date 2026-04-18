use super::context::InferContext;
use super::{Signature, Type, TypeContext};
use crate::Db;
use crate::types::call::bind::BindingError;
use crate::types::{MemberLookupPolicy, PropertyInstanceType};
use ruff_python_ast::{self as ast, name::Name};

mod arguments;
pub(crate) mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{Binding, Bindings, CallableBinding, MatchedArgument};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum SimpleCallArguments<'db> {
    None,
    Unary(Type<'db>),
    Ternary(Type<'db>, Type<'db>, Type<'db>),
}

impl<'db> SimpleCallArguments<'db> {
    fn into_call_arguments(self) -> CallArguments<'static, 'db> {
        match self {
            Self::None => CallArguments::none(),
            Self::Unary(arg) => CallArguments::positional([arg]),
            Self::Ternary(arg1, arg2, arg3) => CallArguments::positional([arg1, arg2, arg3]),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct CallBindingSummary<'db> {
    callable_type: Type<'db>,
    return_type: Option<Type<'db>>,
    is_single: bool,
}

impl<'db> CallBindingSummary<'db> {
    fn from_bindings(bindings: &Bindings<'db>, return_type: Option<Type<'db>>) -> Self {
        Self {
            callable_type: bindings.callable_type(),
            return_type,
            is_single: bindings.is_single(),
        }
    }

    pub(crate) fn callable_type(self) -> Type<'db> {
        self.callable_type
    }

    pub(crate) fn return_type(self) -> Option<Type<'db>> {
        self.return_type
    }

    pub(crate) fn fallback_return_type(self) -> Type<'db> {
        self.return_type.unwrap_or(Type::unknown())
    }

    pub(crate) fn is_single(self) -> bool {
        self.is_single
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum CallDunderOutcome<'db> {
    ReturnType(Type<'db>),
    CallError(CallErrorKind, CallBindingSummary<'db>),
    PossiblyUnbound(CallBindingSummary<'db>),
    MethodNotAvailable,
    Cycle,
}

impl<'db> CallDunderOutcome<'db> {
    pub(super) fn from_error(db: &'db dyn Db, error: CallDunderError<'db>) -> Self {
        match error {
            CallDunderError::CallError(kind, bindings) => Self::CallError(
                kind,
                CallBindingSummary::from_bindings(
                    &bindings,
                    (kind != CallErrorKind::NotCallable).then(|| bindings.return_type(db)),
                ),
            ),
            CallDunderError::PossiblyUnbound(bindings) => Self::PossiblyUnbound(
                CallBindingSummary::from_bindings(&bindings, Some(bindings.return_type(db))),
            ),
            CallDunderError::MethodNotAvailable => Self::MethodNotAvailable,
        }
    }

    fn from_result(db: &'db dyn Db, result: Result<Bindings<'db>, CallDunderError<'db>>) -> Self {
        match result {
            Ok(bindings) => Self::ReturnType(bindings.return_type(db)),
            Err(error) => Self::from_error(db, error),
        }
    }

    pub(crate) fn successful_return_type(self) -> Option<Type<'db>> {
        match self {
            Self::ReturnType(return_type) => Some(return_type),
            Self::CallError(_, _)
            | Self::PossiblyUnbound(_)
            | Self::MethodNotAvailable
            | Self::Cycle => None,
        }
    }

    pub(crate) fn return_type(self) -> Option<Type<'db>> {
        match self {
            Self::ReturnType(return_type) => Some(return_type),
            Self::CallError(_, summary) | Self::PossiblyUnbound(summary) => summary.return_type(),
            Self::MethodNotAvailable | Self::Cycle => None,
        }
    }
}

impl<'db> Type<'db> {
    pub(crate) fn dunder_call_outcome(
        self,
        db: &'db dyn Db,
        name: &'static str,
        arguments: SimpleCallArguments<'db>,
        tcx: TypeContext<'db>,
    ) -> CallDunderOutcome<'db> {
        self.dunder_call_outcome_with_policy(
            db,
            name,
            arguments,
            tcx,
            MemberLookupPolicy::default(),
        )
    }

    pub(crate) fn dunder_call_outcome_with_policy(
        self,
        db: &'db dyn Db,
        name: &'static str,
        arguments: SimpleCallArguments<'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> CallDunderOutcome<'db> {
        fn dunder_call_outcome_cycle<'db>(
            _db: &'db dyn Db,
            _id: salsa::Id,
            _receiver: Type<'db>,
            _name: Name,
            _arguments: SimpleCallArguments<'db>,
            _tcx: TypeContext<'db>,
            _policy: MemberLookupPolicy,
        ) -> CallDunderOutcome<'db> {
            CallDunderOutcome::Cycle
        }

        #[salsa::tracked(
            cycle_result=dunder_call_outcome_cycle,
            heap_size=ruff_memory_usage::heap_size
        )]
        #[allow(
            clippy::needless_pass_by_value,
            reason = "Salsa tracked query inputs must be owned"
        )]
        fn dunder_call_outcome_impl<'db>(
            db: &'db dyn Db,
            receiver: Type<'db>,
            name: Name,
            arguments: SimpleCallArguments<'db>,
            tcx: TypeContext<'db>,
            policy: MemberLookupPolicy,
        ) -> CallDunderOutcome<'db> {
            let mut arguments = arguments.into_call_arguments();
            CallDunderOutcome::from_result(
                db,
                receiver.try_call_dunder_with_policy(
                    db,
                    name.as_str(),
                    &mut arguments,
                    tcx,
                    policy,
                ),
            )
        }

        dunder_call_outcome_impl(db, self, Name::new_static(name), arguments, tcx, policy)
    }

    /// Memoize the pure return-type part of binary dunder resolution so repeated identical
    /// expressions don't re-run overload selection at every call site.
    pub(crate) fn try_call_bin_op_return_type(
        db: &'db dyn Db,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        #[salsa::tracked]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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
            CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound(_) => {
                CallBinOpError::NotSupported
            }
        }
    }
}
