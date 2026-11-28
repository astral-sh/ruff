use super::context::InferContext;
use super::{ClassType, Signature, Type, TypeContext, UnionType};
use crate::Db;
use crate::place::Provenance;
use crate::types::call::bind::BindingError;
use crate::types::function::OverloadLiteral;
use crate::types::{MemberLookupPolicy, PropertyInstanceType};
use crate::{Program, ProgramEnvironment};
use ruff_python_ast as ast;

mod ambiguous;
mod arguments;
pub(crate) mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{
    Binding, Bindings, CallDiagnosticOverride, CallableBinding, MatchedArgument,
};

/// The return type and deprecations retained from operator resolution.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct OperatorResult<'db> {
    pub(crate) return_type: Type<'db>,
    /// Deprecated implementations or overloads selected for the operation.
    pub(crate) deprecated_functions: Box<[OverloadLiteral<'db>]>,
}

impl<'db> OperatorResult<'db> {
    pub(super) fn from_bindings(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> Self {
        Self::from_bindings_and_return_type(db, bindings, bindings.return_type(db, env))
    }

    fn from_bindings_and_return_type(
        db: &'db dyn Db,
        bindings: &Bindings<'db>,
        return_type: Type<'db>,
    ) -> Self {
        Self {
            return_type,
            deprecated_functions: bindings
                .deprecated_functions(db)
                .map(|(_, function)| function)
                .collect(),
        }
    }
}

/// The methods called by a binary operation and its result after `NotImplemented` fallback.
///
/// Method bindings retain their original return types for IDE navigation. Arithmetic syntax
/// uses a separate result after `NotImplemented` fallback.
pub(crate) struct BinaryOperatorCall<'db> {
    pub(super) bindings: Bindings<'db>,
    pub(super) return_type: Type<'db>,
}

impl<'db> BinaryOperatorCall<'db> {
    fn union(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, other: Self) -> Self {
        let Self {
            bindings: left,
            return_type: left_type,
        } = self;
        let Self {
            bindings: right,
            return_type: right_type,
        } = other;
        let callable_type =
            UnionType::from_two_elements(db, env, left.callable_type(), right.callable_type());
        Self {
            bindings: Bindings::from_union(callable_type, [left, right]),
            return_type: UnionType::from_two_elements(db, env, left_type, right_type),
        }
    }

    pub(super) fn into_result(self, db: &'db dyn Db) -> OperatorResult<'db> {
        let Self {
            bindings,
            return_type,
        } = self;
        OperatorResult::from_bindings_and_return_type(db, &bindings, return_type)
    }
}

#[expect(
    clippy::large_enum_variant,
    reason = "Boxing adds an allocation to each successful call"
)]
enum OperatorCallOutcome<'db> {
    Called {
        call: BinaryOperatorCall<'db>,
        /// The method can return `NotImplemented` or may not be defined at runtime.
        may_fall_through: bool,
    },
    Unavailable,
    Blocked,
}

impl<'db> OperatorCallOutcome<'db> {
    fn into_error(self, db: &'db dyn Db) -> CallBinOpError<'db> {
        match self {
            Self::Called {
                call,
                may_fall_through: _,
            } => CallBinOpError::from_call(db, call),
            Self::Unavailable => CallBinOpError::default(),
            Self::Blocked => CallBinOpError::default(),
        }
    }
}

fn operator_call_outcome<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    call: Result<Bindings<'db>, CallDunderError<'db>>,
) -> OperatorCallOutcome<'db> {
    let (bindings, possibly_unbound) = match call {
        Ok(bindings) => (bindings, false),
        Err(error) => match error {
            CallDunderError::PossiblyUnbound {
                bindings,
                unbound_on: _,
            } => (*bindings, true),
            CallDunderError::MethodNotAvailable => return OperatorCallOutcome::Unavailable,
            CallDunderError::CallError(kind, _, _) => match kind {
                // Calling a non-callable method can raise before reflection, even if other
                // possible method types are callable.
                CallErrorKind::NotCallable => return OperatorCallOutcome::Blocked,
                CallErrorKind::PossiblyNotCallable => return OperatorCallOutcome::Blocked,
                // Keep parameter-based fallback for the existing annotation convention.
                CallErrorKind::BindingError => return OperatorCallOutcome::Unavailable,
            },
        },
    };
    let (return_type, may_decline) = bindings.return_type(db, env).split_not_implemented(db, env);
    OperatorCallOutcome::Called {
        call: BinaryOperatorCall {
            bindings,
            return_type,
        },
        may_fall_through: may_decline || possibly_unbound,
    }
}

fn resolve_ordered_operator_calls<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    first: OperatorCallOutcome<'db>,
    fallback: impl FnOnce() -> OperatorCallOutcome<'db>,
) -> Result<BinaryOperatorCall<'db>, CallBinOpError<'db>> {
    let first = match first {
        OperatorCallOutcome::Called {
            call,
            may_fall_through,
        } => {
            if !may_fall_through {
                return Ok(call);
            }
            Some(call)
        }
        OperatorCallOutcome::Unavailable => None,
        OperatorCallOutcome::Blocked => return Err(CallBinOpError::default()),
    };
    match fallback() {
        OperatorCallOutcome::Called {
            call: second,
            may_fall_through,
        } => {
            let call = match first {
                Some(first) => first.union(db, env, second),
                None => second,
            };
            if may_fall_through {
                Err(CallBinOpError::from_call(db, call))
            } else {
                Ok(call)
            }
        }
        OperatorCallOutcome::Unavailable => Err(match first {
            Some(call) => CallBinOpError::from_call(db, call),
            None => CallBinOpError::default(),
        }),
        OperatorCallOutcome::Blocked => Err(match first {
            Some(call) => CallBinOpError::from_call(db, call),
            None => CallBinOpError::default(),
        }),
    }
}

fn resolve_possible_operator_calls<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    first: OperatorCallOutcome<'db>,
    second: OperatorCallOutcome<'db>,
) -> Result<BinaryOperatorCall<'db>, CallBinOpError<'db>> {
    let (call, may_fall_through) = match first {
        OperatorCallOutcome::Blocked => return Err(second.into_error(db)),
        OperatorCallOutcome::Unavailable => match second {
            OperatorCallOutcome::Called {
                call,
                may_fall_through,
            } => (call, may_fall_through),
            OperatorCallOutcome::Unavailable => return Err(CallBinOpError::default()),
            OperatorCallOutcome::Blocked => return Err(CallBinOpError::default()),
        },
        OperatorCallOutcome::Called {
            call: first,
            may_fall_through: first_declines,
        } => match second {
            OperatorCallOutcome::Called {
                call: second,
                may_fall_through: second_declines,
            } => (
                first.union(db, env, second),
                first_declines && second_declines,
            ),
            OperatorCallOutcome::Unavailable => (first, first_declines),
            OperatorCallOutcome::Blocked => return Err(CallBinOpError::from_call(db, first)),
        },
    };
    if may_fall_through {
        Err(CallBinOpError::from_call(db, call))
    } else {
        Ok(call)
    }
}

/// Whether the right operand's reflected method has priority based on the possible runtime
/// classes of both operands.
///
/// `Possibly` requires preserving both dispatch results because the static types admit runtime
/// pairs for which either method has priority.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum ReflectedMethodPriority {
    Never,
    Possibly,
    Definitely,
}

/// Returns whether every inhabitant of `ty` has the same nominal runtime class.
///
/// This is intentionally conservative: a false negative only widens a binary operation's result,
/// while a false positive could discard a valid normal-method result.
fn has_exact_runtime_class<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    match ty {
        Type::ClassLiteral(_) | Type::LiteralValue(_) => true,
        Type::NominalInstance(instance) => instance.class(db, env).is_final(db),
        Type::TypeAlias(alias) => has_exact_runtime_class(db, env, alias.value_type(db)),
        _ => false,
    }
}

/// Returns the nominal runtime class used to dispatch binary operators.
///
/// Instances dispatch through their nominal class, while class objects dispatch through their
/// metaclass.
fn operator_dispatch_class<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Option<ClassType<'db>> {
    match ty {
        Type::ClassLiteral(class) => class.metaclass(db).to_class_type(db),
        _ => ty.nominal_class(db, env),
    }
}

/// Classifies reflected-method priority from the operands' static types.
///
/// For example, an integer literal has exact runtime class `int`, so an `IntFlag` operand is
/// definitely a strict subclass. By contrast, an inhabitant of `Base` may itself have runtime
/// class `Child`, making reflected priority for `Base + Child` only possible.
///
/// ```python
/// class Base: ...
/// class Child(Base): ...
///
/// left: Base
/// right: Child
/// left + right
/// ```
fn reflected_method_priority<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) -> ReflectedMethodPriority {
    if left_ty == right_ty {
        return ReflectedMethodPriority::Never;
    }

    // Positive intersection components retain their nominal subclass relationships, even
    // when a negative component prevents a subtype check against the full intersection.
    // Any component can establish possible or definite priority for the reflected method.
    match (left_ty, right_ty) {
        (Type::Intersection(intersection), _) => {
            return intersection
                .positive_elements_or_object(db)
                .map(|left| reflected_method_priority(db, env, left, right_ty))
                .max()
                .unwrap_or(ReflectedMethodPriority::Never);
        }
        (_, Type::Intersection(intersection)) => {
            return intersection
                .positive_elements_or_object(db)
                .map(|right| reflected_method_priority(db, env, left_ty, right))
                .max()
                .unwrap_or(ReflectedMethodPriority::Never);
        }
        _ => {}
    }

    if let (Some(left_class), Some(right_class)) = (
        operator_dispatch_class(db, env, left_ty),
        operator_dispatch_class(db, env, right_ty),
    ) && left_class.class_literal(db) != right_class.class_literal(db)
        && right_class.is_subtype_of_class_literal(db, left_class.class_literal(db))
    {
        if has_exact_runtime_class(db, env, left_ty) {
            ReflectedMethodPriority::Definitely
        } else {
            ReflectedMethodPriority::Possibly
        }
    } else if right_ty.is_subtype_of(db, env, left_ty) {
        ReflectedMethodPriority::Possibly
    } else {
        ReflectedMethodPriority::Never
    }
}

impl<'db> Type<'db> {
    /// Separate operator result values from a request to try another method.
    pub(super) fn split_not_implemented(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> (Self, bool) {
        let expanded = self.expand_top_level_aliases(db, env);
        let completing = match expanded {
            Type::Union(union) => union.filter(db, |ty| !ty.is_notimplemented(db)),
            ty => {
                if ty.is_notimplemented(db) {
                    Type::Never
                } else {
                    ty
                }
            }
        };
        if completing == expanded {
            (self, false)
        } else {
            (completing, true)
        }
    }

    /// Return the result of dispatching a rich comparison method between two operands.
    ///
    /// A strict subclass on the right takes precedence over the normal method on the left.
    /// The caller remains responsible for operator-specific fallbacks such as identity-based
    /// equality when neither comparison method is available.
    pub(super) fn try_call_rich_comparison_dunder(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        dunder: &'static str,
        reflected_dunder: &'static str,
        policy: MemberLookupPolicy,
    ) -> Option<Type<'db>> {
        let call_dunder = |name, receiver: Type<'db>, argument: Type<'db>| {
            let mut arguments = CallArguments::positional([argument]);
            if let Type::Intersection(intersection) = receiver {
                return intersection
                    .try_call_dunder_with_policy(
                        db,
                        env,
                        receiver,
                        name,
                        &mut arguments,
                        TypeContext::default(),
                        policy,
                    )
                    .map(|outcome| outcome.return_type(db, env))
                    .ok();
            }

            receiver
                .try_call_dunder_with_policy(
                    db,
                    env,
                    name,
                    &mut arguments,
                    TypeContext::default(),
                    policy,
                )
                .map(|outcome| outcome.return_type(db, env))
                .ok()
        };

        match reflected_method_priority(db, env, left, right) {
            ReflectedMethodPriority::Never => call_dunder(dunder, left, right)
                .or_else(|| call_dunder(reflected_dunder, right, left)),
            ReflectedMethodPriority::Possibly => {
                match (
                    call_dunder(dunder, left, right),
                    call_dunder(reflected_dunder, right, left),
                ) {
                    (Some(normal), Some(reflected)) => {
                        Some(UnionType::from_two_elements(db, env, normal, reflected))
                    }
                    (Some(result), None) | (None, Some(result)) => Some(result),
                    (None, None) => None,
                }
            }
            ReflectedMethodPriority::Definitely => call_dunder(reflected_dunder, right, left)
                .or_else(|| call_dunder(dunder, left, right)),
        }
    }

    /// Memoize the return type and deprecations from binary dunder resolution, without retaining
    /// the full call bindings or repeating overload selection at each expression.
    /// Failed resolution retains called-method metadata;
    /// callers remain responsible for call-site diagnostics.
    pub(crate) fn try_call_bin_op_result(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Result<&'db OperatorResult<'db>, &'db CallBinOpError<'db>> {
        #[salsa::tracked(returns(ref), cycle_initial=|_, _, _, _, _, _, _| Err(CallBinOpError::default()), heap_size=ruff_memory_usage::heap_size)]
        fn try_call_bin_op_result_impl<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            left_ty: Type<'db>,
            op: ast::Operator,
            right_ty: Type<'db>,
        ) -> Result<OperatorResult<'db>, CallBinOpError<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            Type::try_call_bin_op(db, env, left_ty, op, right_ty).map(|call| call.into_result(db))
        }

        try_call_bin_op_result_impl(db, env.program(db), left_ty, op, right_ty).as_ref()
    }

    pub(crate) fn try_call_bin_op(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Result<BinaryOperatorCall<'db>, CallBinOpError<'db>> {
        Self::try_call_bin_op_with_policy(
            db,
            env,
            left_ty,
            op,
            right_ty,
            MemberLookupPolicy::default(),
        )
    }

    pub(crate) fn try_call_bin_op_with_policy(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<BinaryOperatorCall<'db>, CallBinOpError<'db>> {
        // A different reflected implementation on a proper subclass is tried first.
        // A method that returns NotImplemented lets the next method run; parameter
        // binding failures retain their existing fallback behavior.
        let call_dunder = |receiver: Type<'db>, argument: Type<'db>, name| {
            operator_call_outcome(
                db,
                env,
                receiver.try_call_dunder_with_policy(
                    db,
                    env,
                    name,
                    &mut CallArguments::positional([argument]),
                    TypeContext::default(),
                    policy,
                ),
            )
        };
        let call_left = || call_dunder(left_ty, right_ty, op.dunder());
        let call_right = || {
            if left_ty == right_ty {
                return OperatorCallOutcome::Unavailable;
            }
            // Different specializations can still have the same exact runtime class.
            let same_runtime_class = has_exact_runtime_class(db, env, left_ty)
                && has_exact_runtime_class(db, env, right_ty)
                && operator_dispatch_class(db, env, left_ty)
                    .zip(operator_dispatch_class(db, env, right_ty))
                    .is_some_and(|(left, right)| left.class_literal(db) == right.class_literal(db));
            if same_runtime_class {
                OperatorCallOutcome::Unavailable
            } else {
                call_dunder(right_ty, left_ty, op.reflected_dunder())
            }
        };
        let reflected_priority = reflected_method_priority(db, env, left_ty, right_ty);
        if reflected_priority != ReflectedMethodPriority::Never {
            let reflected_dunder = op.reflected_dunder();
            let left_class = left_ty.to_meta_type(db, env);
            let right_class = right_ty.to_meta_type(db, env);
            let rhs_reflected = right_class.member(db, env, reflected_dunder).place;
            if !rhs_reflected.is_undefined()
                && !rhs_reflected.is_equal_ignoring_provenance(
                    left_class.member(db, env, reflected_dunder).place,
                )
            {
                if reflected_priority == ReflectedMethodPriority::Definitely {
                    return resolve_ordered_operator_calls(db, env, call_right(), call_left);
                }
                // Either method may have runtime priority. Both are evaluated once; if
                // either always completes, neither ordering can exhaust both methods.
                return resolve_possible_operator_calls(db, env, call_right(), call_left());
            }
        }
        resolve_ordered_operator_calls(db, env, call_left(), call_right)
    }
}

/// Wraps a [`Bindings`] for an unsuccessful call with information about why the call was
/// unsuccessful.
///
/// The bindings are boxed so that we do not pass around large `Err` variants on the stack.
#[derive(Debug)]
pub(crate) struct CallError<'db>(pub(crate) CallErrorKind, pub(crate) Box<Bindings<'db>>);

impl<'db> CallError<'db> {
    pub(crate) fn return_type(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.1.return_type(db, env)
    }

    /// Returns `Some(property)` if the call error was caused by an attempt to read a property
    /// that has no getter, and `None` otherwise.
    pub(crate) fn as_attempt_to_get_property_with_no_getter(
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
                BindingError::PropertyHasNoGetter(property) => Some(*property),
                _ => None,
            })
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

    pub(crate) fn report_diagnostics_with_override(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        overrides: &CallDiagnosticOverride<'_>,
    ) {
        self.1
            .report_diagnostics_with_override(context, node, overrides);
    }
}

/// The reason why calling a type failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
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
    CallError(CallErrorKind, Box<Bindings<'db>>, Provenance<'db>),

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
    /// A method can be deprecated even when it is missing on some union members or its
    /// signature rejects the implicit call.
    pub(super) fn deprecated_functions(
        &self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = OverloadLiteral<'db>> {
        let bindings = match self {
            Self::CallError(_, bindings, _) | Self::PossiblyUnbound { bindings, .. } => {
                Some(bindings)
            }
            Self::MethodNotAvailable => None,
        };
        bindings.into_iter().flat_map(move |bindings| {
            bindings
                .deprecated_functions(db)
                .map(|(_, function)| function)
        })
    }

    pub(super) fn provenance(&self) -> Provenance<'db> {
        match self {
            Self::CallError(_, _, provenance) => *provenance,
            Self::PossiblyUnbound { .. } | Self::MethodNotAvailable => Provenance::Unknown,
        }
    }

    pub(super) fn with_provenance(self, provenance: Provenance<'db>) -> Self {
        match self {
            Self::CallError(kind, bindings, _) => Self::CallError(kind, bindings, provenance),
            Self::PossiblyUnbound { .. } | Self::MethodNotAvailable => self,
        }
    }

    pub(super) fn return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        match self {
            Self::MethodNotAvailable | Self::CallError(CallErrorKind::NotCallable, _, _) => None,
            Self::CallError(_, bindings, _) => Some(bindings.return_type(db, env)),
            Self::PossiblyUnbound { bindings, .. } => Some(bindings.return_type(db, env)),
        }
    }

    pub(super) fn fallback_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.return_type(db, env).unwrap_or(Type::unknown())
    }
}

/// Called-method metadata retained when a binary operation is unsupported.
#[derive(Clone, Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CallBinOpError<'db> {
    /// Includes methods that were called but returned only `NotImplemented`.
    pub(super) deprecated_functions: Box<[OverloadLiteral<'db>]>,
}

impl<'db> CallBinOpError<'db> {
    fn from_call(db: &'db dyn Db, call: BinaryOperatorCall<'db>) -> Self {
        Self {
            deprecated_functions: call.into_result(db).deprecated_functions,
        }
    }
}
