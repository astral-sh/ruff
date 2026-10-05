//! Dependencies of ordinary argument checking that can suspend or remain unsupported.

use std::borrow::Cow;
use std::convert::Infallible;
use std::future::{Future, ready};

use salsa::execution_probe::FieldRequest;

use super::{
    ArgumentTypeChecker, BinderCondition, Binding, Bindings, CallArguments, CallError,
    CallableBinding, MatchingOverloadIndex,
};
use crate::Db;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionMetadataEffects, FunctionType,
    KnownFunction, LegacyFunctionIdentityEffects,
    OverloadLiteral,
};
use crate::types::method::BoundMethodReceiver;
use crate::types::{
    BoundMethodType, ClassLiteral, DescriptorOrigin, KnownClass, ProgramEnvironment,
    PropertyInstanceType, Type, legacy_inline,
};

pub(super) mod sealed {
    pub(in crate::types::call::bind) trait Sealed {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BinderLegacyEffect {
    ArgumentExpansion,
    GenericInference,
    KnownFunction,
    OverloadFiltering,
    ParameterUnion,
    ParamSpec,
    Specialization,
    Splat,
}

/// Legacy operations require a decision from every provider before their closures execute.
/// The queued provider cannot silently inherit an operation that starts another semantic walk.
pub(super) trait BinderEffects<'db>: sealed::Sealed {
    type Error;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>>;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn function_overloads(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || {
            legacy_inline(
                LegacyFunctionIdentityEffects.overloads_and_implementation(db, last_definition),
            )
        })
        .await
    }

    async fn function_known(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<Option<KnownFunction>, Self::Error> {
        let literal = self.field(function.field_requests(db).literal()).await?;
        self.field(literal.last_definition.field_requests(db).known())
            .await
    }

    async fn class_known(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        match self.local(Some(1), Some(0), || class.as_static()).await? {
            Some(class) => self.field(class.field_requests(db).known()).await,
            None => Ok(None),
        }
    }

    async fn type_is_none(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<bool, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || ty.is_none(db))
            .await
    }

    async fn property_instance(
        &self,
        db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Result<PropertyInstanceType<'db>, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || {
            PropertyInstanceType::new(db, getter, setter, deleter)
        })
        .await
    }

    /// Interns the flags and ordered field specifiers returned by a transform factory.
    async fn dataclass_transformer_params(
        &self,
        db: &'db dyn Db,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> Result<DataclassTransformerParams<'db>, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || {
            DataclassTransformerParams::new(db, flags, field_specifiers)
        })
        .await
    }

    async fn property_function(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<Option<(PropertyInstanceType<'db>, FunctionType<'db>)>, Self::Error> {
        let receiver = self.field(method.receiver_request(db.into())).await?;
        let property = self
            .local(Some(4), Some(0), || {
                let (BoundMethodReceiver::Instance(receiver)
                | BoundMethodReceiver::Constrained { receiver, .. }) = receiver;
                match receiver {
                    Type::PropertyInstance(property) => Some(property),
                    _ => None,
                }
            })
            .await?;
        let Some(property) = property else {
            return Ok(None);
        };
        let function = self.field(method.field_requests(db).func()).await?;
        Ok(function
            .as_function_literal()
            .map(|function| (property, function)))
    }

    async fn property_accessor_call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        accessor: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || {
            accessor.try_call_with_recursion_guard(
                db,
                env,
                &CallArguments::positional(arguments.iter().copied()),
                recursion_guard,
            )
        })
        .await
    }

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> Result<DescriptorOrigin<'db>, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || {
            bindings.descriptor_origin(db, env, arguments)
        })
        .await
    }

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.operation(BinderLegacyEffect::KnownFunction, || bindings.return_type(db, env))
            .await
    }

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(operation())
    }

    async fn operation<T>(
        &self,
        effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        self.legacy(effect, operation)
    }

    async fn bound_arguments<'a, 'call>(
        &self,
        arguments: &'a CallArguments<'call, 'db>,
        bound: Option<Type<'db>>,
        operation: impl FnOnce(Cow<'a, CallArguments<'call, 'db>>),
    ) -> Result<(), Self::Error> {
        operation(arguments.with_self(bound));
        Ok(())
    }

    async fn prepare_callable(&self, _binding: &CallableBinding<'db>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn trace_matching(&self, binding: &CallableBinding<'db>, stage: &'static str) {
        tracing::trace!(target: "ty_python_semantic::types::call::bind", matching_overload_index = ?binding.matching_overload_index(), "{stage}");
    }

    async fn prepare_binding(
        &self,
        _binding: &Binding<'db>,
        _arguments: &CallArguments<'_, 'db>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn overload_index(
        &self,
        binding: &CallableBinding<'db>,
    ) -> Result<MatchingOverloadIndex, Self::Error> {
        Ok(binding.matching_overload_index())
    }

    async fn expandable_variadic(
        &self,
        expansions: &super::CallArgumentExpansions<'_, '_, 'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        for (index, (argument, _)) in arguments.iter().enumerate() {
            let variadic = self
                .local(Some(1), Some(0), || {
                    matches!(argument, super::Argument::Variadic)
                })
                .await?;
            if variadic
                && self
                    .operation(BinderLegacyEffect::ArgumentExpansion, || {
                        expansions.argument_types(index).is_some()
                    })
                    .await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn inspect_expansions(
        &self,
        arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, Self::Error> {
        self.inspect_argument_expansions(arguments, inspect)
    }

    async fn argument_type(
        &self,
        types: &super::CallArgumentTypes<'db>,
        declared: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(types.get_for_declared_type(declared))
    }

    async fn constructor_receiver(
        &self,
        db: &'db dyn Db,
        declared: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(
            matches!(declared.resolve_type_alias(db), Type::SubclassOf(subclass_of) if subclass_of.into_type_var().is_some()),
        )
    }

    async fn defer_typevartuple(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.defer_typevartuple_check(checker, declared, expected, argument)
    }

    fn legacy<T>(
        &self,
        effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    fn inspect_argument_expansions(
        &self,
        arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, Self::Error>;

    async fn condition(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> Result<bool, Self::Error>;

    fn defer_typevartuple_check(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Self::Error>;

    fn trace_span(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        signature: Type<'db>,
    ) -> tracing::Span;
}

#[derive(Default)]
pub(super) struct InlineBinderEffects<'guard, 'db> {
    pub(super) recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl sealed::Sealed for InlineBinderEffects<'_, '_> {}

impl<'db> BinderEffects<'db> for InlineBinderEffects<'_, 'db> {
    type Error = Infallible;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>> {
        self.recursion_guard
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Infallible> {
        Ok(request.read_ordinary())
    }

    fn legacy<T>(
        &self,
        _effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(operation())
    }

    fn condition(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> impl Future<Output = Result<bool, Infallible>> {
        ready(Ok(condition.evaluate(db, env, constraints)))
    }

    fn inspect_argument_expansions(
        &self,
        _arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, Infallible> {
        Ok(inspect())
    }

    fn defer_typevartuple_check(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(checker.should_defer_typevartuple_callable_check(
            declared,
            expected,
            argument,
            self.recursion_guard,
        ))
    }

    fn trace_span(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        signature: Type<'db>,
    ) -> tracing::Span {
        tracing::trace_span!(
            target: "ty_python_semantic::types::call::bind",
            "CallableBinding::check_types",
            arguments = %arguments.display(db, env),
            signature = %signature.display(db, env),
        )
    }
}
