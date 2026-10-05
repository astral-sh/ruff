//! Call checking borrows the invocation's owners and the source execution endpoint.

use std::borrow::Cow;
use std::marker::PhantomData;

use salsa::execution_probe::{BorrowOrCopy, FieldRequest, RunError, RunResult};

use super::checking_effects::{BindingsEffects, CheckContext};
use super::effects::{self, BinderEffects, BinderLegacyEffect};
use super::*;
use crate::types::function::{DataclassTransformerFlags, DataclassTransformerParams, OverloadLiteral};
use crate::types::relation::source::RelationSourceEffects;
use crate::types::relation::source::resources::RelationResourceAccess;

#[derive(Clone, Copy)]
pub(in crate::types) enum CheckerOperation {
    ArgumentExpansion,
    GenericInference,
    KnownFunction,
    OverloadFiltering,
    ParameterUnion,
    ParamSpec,
    Specialization,
    Splat,
    Constructor,
    DownstreamConstructor,
    Equivalent,
    ClassInfo,
    ConstructorReceiver,
}

pub(in crate::types) trait SourceCheckerAccess<'run, 'db: 'run>:
    RelationSourceEffects<'run, 'db>
{
    async fn function_overloads(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)>;

    async fn type_is_none(&self, ty: Type<'db>) -> RunResult<bool>;

    async fn property_instance(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> RunResult<PropertyInstanceType<'db>>;

    /// Constructs canonical metadata from a field-specifier buffer whose allocation and
    /// initialized-entry cleanup are already funded. This method funds the later transfers.
    async fn dataclass_transformer_params(
        &self,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> RunResult<DataclassTransformerParams<'db>>;

    async fn property_accessor_call(
        &self,
        env: &ProgramEnvironment<'db>,
        accessor: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>>;

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> RunResult<DescriptorOrigin<'db>>;

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>>;

    async fn defer_typevartuple_callable(
        &self,
        env: &ProgramEnvironment<'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<bool>;

    async fn checker_local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T>;

    async fn checker_unavailable<T>(&self, operation: CheckerOperation) -> RunResult<T>;
}

#[expect(clippy::too_many_arguments)]
pub(in crate::types) async fn check<'run, 'db: 'run, A: SourceCheckerAccess<'run, 'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constraints: <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    arguments: &CallArguments<'_, 'db>,
    bindings: &mut Bindings<'db>,
    tcx: TypeContext<'db>,
    dataclass_field_specifiers: &[Type<'db>],
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
    access: &A,
) -> RunResult<Result<(), CallErrorKind>> {
    let effects = SourceBinder {
        access,
        constraints,
        recursion_guard,
        lifetimes: PhantomData,
    };
    #[cfg(test)]
    effects
        .local(Some(1), Some(0), || {
            crate::types::relation::source::resources::observations::observe_invocation(
            db,
            std::borrow::Borrow::borrow(&constraints),
            crate::types::relation::source::resources::observations::InvocationStage::BinderCheck,
        );
        })
        .await?;
    bindings
        .check_types_impl_with_effects(
            CheckContext {
                db,
                env,
                constraints: std::borrow::Borrow::borrow(&constraints),
                arguments,
                tcx,
                dataclass_field_specifiers,
            },
            CheckTypesMode::Finalize,
            &effects,
        )
        .await
}

struct SourceBinder<'a, 'run, 'db: 'run, A: SourceCheckerAccess<'run, 'db>> {
    access: &'a A,
    constraints: <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    recursion_guard: Option<&'a CallableRecursionGuard<'db>>,
    lifetimes: PhantomData<&'run &'db ()>,
}

impl<'run, 'db: 'run, A: SourceCheckerAccess<'run, 'db>> effects::sealed::Sealed
    for SourceBinder<'_, 'run, 'db, A>
{
}

#[cfg(test)]
pub(in crate::types) async fn condition_for_test<
    'run,
    'db: 'run,
    A: SourceCheckerAccess<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    retained: <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    supplied: &ConstraintSetBuilder<'db>,
    comparison: (Type<'db>, Type<'db>, TypeVarSet<'db>),
    always: bool,
    access: &A,
) -> RunResult<bool> {
    let comparison = BinderComparison::Assignable {
        source: comparison.0,
        target: comparison.1,
        inferable_typevars: comparison.2,
    };
    let condition = if always {
        BinderCondition::Always(comparison)
    } else {
        BinderCondition::Never(comparison)
    };
    SourceBinder {
        access,
        constraints: retained,
        recursion_guard: None,
        lifetimes: PhantomData,
    }
    .condition(db, env, supplied, condition)
    .await
}

impl<'run, 'db: 'run, A: SourceCheckerAccess<'run, 'db>> BinderEffects<'db>
    for SourceBinder<'_, 'run, 'db, A>
{
    type Error = RunError;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>> {
        self.recursion_guard
    }

    async fn function_overloads(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        self.access.function_overloads(db, last_definition).await
    }

    async fn type_is_none(&self, _db: &'db dyn Db, ty: Type<'db>) -> RunResult<bool> {
        self.access.type_is_none(ty).await
    }

    async fn property_instance(
        &self,
        _db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> RunResult<PropertyInstanceType<'db>> {
        self.access.property_instance(getter, setter, deleter).await
    }

    async fn dataclass_transformer_params(
        &self,
        _db: &'db dyn Db,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> RunResult<DataclassTransformerParams<'db>> {
        self.access
            .dataclass_transformer_params(flags, field_specifiers)
            .await
    }

    async fn property_accessor_call(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        accessor: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        self.access
            .property_accessor_call(env, accessor, arguments, recursion_guard)
            .await
    }

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.access.bindings_origin(db, env, bindings, arguments).await
    }

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>> {
        self.access.bindings_return_type(db, env, bindings).await
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        Ok(self
            .access
            .endpoint()
            .read_field(request, &BorrowOrCopy)
            .await)
    }

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.access.checker_local(work, bytes, operation).await
    }

    async fn operation<T>(
        &self,
        effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let dependency = match effect {
            BinderLegacyEffect::ArgumentExpansion => CheckerOperation::ArgumentExpansion,
            BinderLegacyEffect::GenericInference => CheckerOperation::GenericInference,
            BinderLegacyEffect::KnownFunction => CheckerOperation::KnownFunction,
            BinderLegacyEffect::OverloadFiltering => CheckerOperation::OverloadFiltering,
            BinderLegacyEffect::ParameterUnion => CheckerOperation::ParameterUnion,
            BinderLegacyEffect::ParamSpec => CheckerOperation::ParamSpec,
            BinderLegacyEffect::Specialization => CheckerOperation::Specialization,
            BinderLegacyEffect::Splat => CheckerOperation::Splat,
        };
        let result = self.access.checker_unavailable(dependency).await;
        // A refused operation can capture a partial checker. Keep that capture through drain.
        drop(operation);
        result
    }

    async fn bound_arguments<'a, 'call>(
        &self,
        arguments: &'a CallArguments<'call, 'db>,
        bound: Option<Type<'db>>,
        operation: impl FnOnce(Cow<'a, CallArguments<'call, 'db>>),
    ) -> RunResult<()> {
        let quote = self
            .local(arguments.len().checked_add(1), Some(0), || {
                arguments.with_self_storage_quote(bound)
            })
            .await?
            .ok_or(RunError::Contract(
                "checker bound-argument quotation overflow",
            ))?;
        self.local(Some(quote.0), Some(quote.1), || {
            operation(arguments.with_self(bound))
        })
        .await
    }

    async fn prepare_callable(&self, binding: &CallableBinding<'db>) -> RunResult<()> {
        let work = self
            .local(binding.overloads.len().checked_add(1), Some(0), || {
                let scans = binding.overloads.iter().try_fold(1usize, |n, overload| {
                    n.checked_add(overload.errors.len().checked_add(2)?)
                })?;
                scans.checked_mul(binding.overloads.len().checked_add(2)?)
            })
            .await?;
        self.local(work, Some(0), || ()).await
    }

    fn trace_matching(&self, _binding: &CallableBinding<'db>, _stage: &'static str) {}

    async fn prepare_binding(
        &self,
        binding: &Binding<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<()> {
        let metadata = binding
            .signature
            .parameters()
            .len()
            .checked_add(binding.argument_matches.len())
            .and_then(|n| n.checked_add(4));
        let quote = self
            .local(metadata, Some(0), || {
                let matches = binding
                    .argument_matches
                    .iter()
                    .try_fold(0usize, |n, arg| n.checked_add(arg.parameters.len()))?;
                let p = binding.signature.parameters().len();
                let a = arguments.len();
                let work = p
                    .checked_add(a)?
                    .checked_add(matches)?
                    .checked_add(binding.errors.capacity())?
                    .checked_add(16)?
                    .checked_mul(16)?;
                let errors = binding
                    .errors
                    .capacity()
                    .checked_add(matches)?
                    .checked_add(4)?
                    .checked_mul(4)?
                    .checked_mul(size_of::<BindingError<'db>>())?;
                let bytes = a.checked_mul(size_of::<bool>())?.checked_add(errors)?;
                Some((work, bytes))
            })
            .await?
            .ok_or(RunError::Contract("checker binding quotation overflow"))?;
        self.local(Some(quote.0), Some(quote.1), || ()).await
    }

    async fn overload_index(
        &self,
        binding: &CallableBinding<'db>,
    ) -> RunResult<MatchingOverloadIndex> {
        let work = self
            .local(binding.overloads.len().checked_add(1), Some(0), || {
                binding.overloads.iter().try_fold(1usize, |n, overload| {
                    n.checked_add(overload.errors.len().checked_add(2)?)
                })
            })
            .await?;
        let bytes = binding
            .overloads
            .len()
            .checked_add(4)
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_mul(size_of::<usize>()));
        let mut result = None;
        self.local(work, bytes, || {
            result = Some(binding.matching_overload_index())
        })
        .await?;
        result.ok_or(RunError::Contract("checker overload selection missing"))
    }

    async fn argument_type(
        &self,
        types: &CallArgumentTypes<'db>,
        declared: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let work = self
            .local(types.lookup_metadata_work(), Some(0), || {
                types.lookup_work(declared)
            })
            .await?;
        self.local(work, Some(0), || types.get_for_declared_type(declared))
            .await
    }

    async fn condition(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> RunResult<bool> {
        let matches = self
            .local(Some(1), Some(0), || {
                std::ptr::eq(constraints, std::borrow::Borrow::borrow(&self.constraints))
            })
            .await?;
        if !matches {
            return Err(RunError::Contract(
                "argument condition uses a different constraint builder",
            ));
        }
        let (comparison, always) = match condition {
            BinderCondition::Always(comparison) => (comparison, true),
            BinderCondition::Never(comparison) => (comparison, false),
        };
        match comparison {
            BinderComparison::Assignable {
                source,
                target,
                inferable_typevars,
            } => {
                self.access
                    .resources()
                    .assignability(
                        db,
                        env,
                        self.constraints,
                        source,
                        target,
                        inferable_typevars,
                        always,
                        self.access,
                    )
                    .await
            }
            BinderComparison::Equivalent { .. } => {
                self.access
                    .checker_unavailable(CheckerOperation::Equivalent)
                    .await
            }
        }
    }

    async fn constructor_receiver(
        &self,
        _db: &'db dyn Db,
        _declared: Type<'db>,
    ) -> RunResult<bool> {
        self.access
            .checker_unavailable(CheckerOperation::ConstructorReceiver)
            .await
    }

    async fn defer_typevartuple(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> RunResult<bool> {
        self.access
            .defer_typevartuple_callable(
                checker.env,
                declared,
                expected,
                argument,
                self.recursion_guard,
            )
            .await
    }

    async fn inspect_expansions(
        &self,
        _arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> RunResult<bool> {
        let result = self
            .access
            .checker_unavailable(CheckerOperation::ArgumentExpansion)
            .await;
        drop(inspect);
        result
    }

    fn legacy<T>(
        &self,
        _effect: BinderLegacyEffect,
        _operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        Err(RunError::Contract(
            "source checker requires an asynchronous operation",
        ))
    }
    fn inspect_argument_expansions(
        &self,
        _arguments: &CallArguments<'_, 'db>,
        _inspect: impl FnOnce() -> bool,
    ) -> RunResult<bool> {
        Err(RunError::Contract(
            "source checker requires asynchronous expansion",
        ))
    }
    fn defer_typevartuple_check(
        &self,
        _checker: &ArgumentTypeChecker<'_, 'db>,
        _declared: Type<'db>,
        _expected: Type<'db>,
        _argument: Type<'db>,
    ) -> RunResult<bool> {
        Err(RunError::Contract(
            "source checker requires asynchronous callable inspection",
        ))
    }
    fn trace_span(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _arguments: &CallArguments<'_, 'db>,
        _signature: Type<'db>,
    ) -> tracing::Span {
        tracing::Span::none()
    }
}

impl<'run, 'db: 'run, A: SourceCheckerAccess<'run, 'db>> BindingsEffects<'db>
    for SourceBinder<'_, 'run, 'db, A>
{
    async fn constructor(
        &self,
        _binding: &mut ConstructorBinding<'db>,
        _context: CheckContext<'_, 'db>,
        _mode: CheckTypesMode,
    ) -> RunResult<()> {
        self.access
            .checker_unavailable(CheckerOperation::Constructor)
            .await
    }
    async fn known_cases(
        &self,
        bindings: &mut Bindings<'db>,
        context: CheckContext<'_, 'db>,
    ) -> RunResult<()> {
        bindings
            .evaluate_known_cases_with(
                context.db,
                context.env,
                context.arguments,
                context.dataclass_field_specifiers,
                self.recursion_guard,
                self,
            )
            .await
    }
    async fn downstream(
        &self,
        _binding: &mut ConstructorBinding<'db>,
        _context: CheckContext<'_, 'db>,
    ) -> RunResult<()> {
        self.access
            .checker_unavailable(CheckerOperation::DownstreamConstructor)
            .await
    }
    async fn step_work(&self, bindings: &Bindings<'db>) -> RunResult<Option<usize>> {
        let mut work = bindings.elements.len().checked_add(4);
        for element in &bindings.elements {
            self.local(Some(1), Some(0), || ()).await?;
            work = work.and_then(|n| n.checked_add(element.items.len()));
            for item in &element.items {
                let callable = item.callable();
                let item_work = self
                    .local(callable.overloads.len().checked_add(1), Some(0), || {
                        callable.overloads.iter().try_fold(1usize, |n, binding| {
                            n.checked_add(binding.errors.len().checked_add(2)?)
                        })
                    })
                    .await?;
                work = work.and_then(|n| n.checked_add(item_work?));
            }
        }
        Ok(work.and_then(|n| n.checked_mul(n)?.checked_mul(8)))
    }
    async fn admitted_step<T>(
        &self,
        work: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local(work, Some(0), operation).await
    }
    fn step<T>(&self, _operation: impl FnOnce() -> T) -> RunResult<T> {
        Err(RunError::Contract("source checker requires admitted steps"))
    }
}
