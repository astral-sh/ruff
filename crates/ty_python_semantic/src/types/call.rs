use super::context::InferContext;
use super::{ClassType, Signature, Type, TypeContext, UnionType};
use crate::place::Provenance;
use crate::types::call::bind::BindingError;
use crate::types::{MemberLookupPolicy, PropertyInstanceType};
use crate::{Db, Program, SemanticEnvironment};
use ruff_python_ast::{self as ast};

mod arguments;
pub(crate) mod bind;
pub(super) use arguments::{Argument, CallArguments};
pub(super) use bind::{
    Binding, Bindings, CallDiagnosticOverride, CallableBinding, MatchedArgument,
};

/// Whether the right operand's reflected method has priority based on the possible runtime
/// classes of both operands.
///
/// `Possibly` requires preserving both dispatch results because the static types admit runtime
/// pairs for which either method has priority.
#[derive(PartialEq, Eq)]
enum ReflectedMethodPriority {
    Never,
    Possibly,
    Definitely,
}

/// Returns whether every inhabitant of `ty` has the same nominal runtime class.
///
/// This is intentionally conservative: a false negative only widens a binary operation's result,
/// while a false positive could discard a valid normal-method result.
fn has_exact_runtime_class<'db>(env: &SemanticEnvironment<'db>, ty: Type<'db>) -> bool {
    match ty {
        Type::ClassLiteral(_) | Type::LiteralValue(_) => true,
        Type::NominalInstance(instance) => instance.class(env).is_final(env),
        Type::TypeAlias(alias) => has_exact_runtime_class(env, alias.value_type(env)),
        _ => false,
    }
}

/// Returns the nominal runtime class used to dispatch binary operators.
///
/// Instances dispatch through their nominal class, while class objects dispatch through their
/// metaclass.
fn operator_dispatch_class<'db>(
    env: &SemanticEnvironment<'db>,
    ty: Type<'db>,
) -> Option<ClassType<'db>> {
    match ty {
        Type::ClassLiteral(class) => class.metaclass(env).to_class_type(env),
        _ => ty.nominal_class(env),
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
    env: &SemanticEnvironment<'db>,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) -> ReflectedMethodPriority {
    if left_ty == right_ty {
        return ReflectedMethodPriority::Never;
    }

    let db = env.db();
    if let (Some(left_class), Some(right_class)) = (
        operator_dispatch_class(env, left_ty),
        operator_dispatch_class(env, right_ty),
    ) && left_class.class_literal(db) != right_class.class_literal(db)
        && right_class.is_subtype_of_class_literal(env, left_class.class_literal(db))
    {
        if has_exact_runtime_class(env, left_ty) {
            ReflectedMethodPriority::Definitely
        } else {
            ReflectedMethodPriority::Possibly
        }
    } else if right_ty.is_subtype_of(env, left_ty) {
        ReflectedMethodPriority::Possibly
    } else {
        ReflectedMethodPriority::Never
    }
}

impl<'db> Type<'db> {
    /// Memoize the pure return-type part of binary dunder resolution so repeated identical
    /// expressions don't re-run overload selection at every call site.
    pub(crate) fn try_call_bin_op_return_type(
        env: &SemanticEnvironment<'db>,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
        fn try_call_bin_op_return_type_impl<'db>(
            db: &'db dyn Db,
            program: Program,
            left_ty: Type<'db>,
            op: ast::Operator,
            right_ty: Type<'db>,
        ) -> Option<Type<'db>> {
            let env = &SemanticEnvironment::from_program(db, program);
            Type::try_call_bin_op(env, left_ty, op, right_ty)
                .ok()
                .map(|bindings| bindings.return_type(env))
        }

        try_call_bin_op_return_type_impl(env.db(), env.program(), left_ty, op, right_ty)
    }

    pub(crate) fn try_call_bin_op(
        env: &SemanticEnvironment<'db>,
        left_ty: Type<'db>,
        op: ast::Operator,
        right_ty: Type<'db>,
    ) -> Result<Bindings<'db>, CallBinOpError> {
        Self::try_call_bin_op_with_policy(env, left_ty, op, right_ty, MemberLookupPolicy::default())
    }

    pub(crate) fn try_call_bin_op_with_policy(
        env: &SemanticEnvironment<'db>,
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

        // Runtime classes determine reflected priority, but static operand types may only
        // establish that priority conditionally.
        let reflected_priority = reflected_method_priority(env, left_ty, right_ty);

        let left_class = left_ty.to_meta_type(env);
        let right_class = right_ty.to_meta_type(env);
        if reflected_priority != ReflectedMethodPriority::Never {
            let reflected_dunder = op.reflected_dunder();
            let rhs_reflected = right_class.member(env, reflected_dunder).place;
            // TODO: if `rhs_reflected` is possibly unbound, we should union the two possible
            // Bindings together
            if !rhs_reflected.is_undefined()
                && !rhs_reflected
                    .is_equal_ignoring_provenance(left_class.member(env, reflected_dunder).place)
            {
                let call_on_right_instance = right_ty.try_call_dunder_with_policy(
                    env,
                    reflected_dunder,
                    &mut CallArguments::positional([left_ty]),
                    TypeContext::default(),
                    policy,
                );

                if reflected_priority == ReflectedMethodPriority::Definitely {
                    return Ok(call_on_right_instance.or_else(|_| {
                        left_ty.try_call_dunder_with_policy(
                            env,
                            op.dunder(),
                            &mut CallArguments::positional([right_ty]),
                            TypeContext::default(),
                            policy,
                        )
                    })?);
                }

                let call_on_left_instance = left_ty.try_call_dunder_with_policy(
                    env,
                    op.dunder(),
                    &mut CallArguments::positional([right_ty]),
                    TypeContext::default(),
                    policy,
                );

                return match (call_on_right_instance, call_on_left_instance) {
                    (Ok(right_bindings), Ok(left_bindings)) => {
                        let callable_type = UnionType::from_two_elements(
                            env,
                            right_bindings.callable_type(),
                            left_bindings.callable_type(),
                        );
                        Ok(Bindings::from_union(
                            callable_type,
                            [right_bindings, left_bindings],
                        ))
                    }
                    (Ok(bindings), Err(_)) | (Err(_), Ok(bindings)) => Ok(bindings),
                    (Err(_), Err(error)) => Err(error.into()),
                };
            }
        }

        let call_on_left_instance = left_ty.try_call_dunder_with_policy(
            env,
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
                    env,
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
    pub(crate) fn return_type(&self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        self.1.return_type(env)
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

    pub(super) fn return_type(&self, env: &SemanticEnvironment<'db>) -> Option<Type<'db>> {
        match self {
            Self::MethodNotAvailable | Self::CallError(CallErrorKind::NotCallable, _, _) => None,
            Self::CallError(_, bindings, _) => Some(bindings.return_type(env)),
            Self::PossiblyUnbound { bindings, .. } => Some(bindings.return_type(env)),
        }
    }

    pub(super) fn fallback_return_type(&self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        self.return_type(env).unwrap_or(Type::unknown())
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
            CallDunderError::CallError(_, _, _) => Self::CallError,
            CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound { .. } => {
                CallBinOpError::NotSupported
            }
        }
    }
}
