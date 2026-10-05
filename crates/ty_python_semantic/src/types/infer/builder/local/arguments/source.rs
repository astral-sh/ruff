//! Admitted argument transitions borrow the existing source root.

use std::future::Future;
use std::pin::Pin;

use super::*;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use salsa::execution_probe::{RunError, RunResult};

use super::preparation::ArgumentPreparationEffects;
use crate::types::call::bind::argument_context::{ArgumentContextEffects, ArgumentContextRequest};
use crate::types::call::bind::source_check::{CheckerOperation, SourceCheckerAccess};
use crate::types::call::CallError;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionMetadataEffects, OverloadLiteral,
};
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::relation::source::{RelationSourceEffects, resources::RelationResourceAccess};
use crate::types::{
    BoundTypeVarInstance, CallableType, DescriptorOrigin, KnownClass, NewType, PropertyDeprecations,
    PropertyInstanceType, TypeDispatchEffects, UnionType, union_like_with,
};
use crate::{Db, ProgramEnvironment};

/// Quotes the transform interner factory's transfers around the existing future allocation.
/// `allocate_future` pays for the future's storage; these are the factory and return carriers.
fn transform_future_carrier_bytes<F: Future, M: FnOnce() -> F>(_: &M) -> Option<usize> {
    size_of::<M>()
        .checked_mul(2)?
        .checked_add(size_of::<Option<M>>())?
        .checked_add(size_of::<Pin<Box<F>>>().checked_mul(2)?)?
        .checked_add(size_of::<RunResult<Pin<Box<F>>>>().checked_mul(2)?)?
        .checked_add(size_of::<RunResult<()>>().checked_mul(2)?)?
        .checked_add(size_of::<Option<()>>())
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceCheckerAccess<'run, 'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn function_overloads(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        FunctionMetadataEffects::overloads_and_implementation(self, db, last_definition).await
    }

    async fn type_is_none(&self, ty: Type<'db>) -> RunResult<bool> {
        let instance = self.local(1, 0, || ty.as_nominal_instance()).await?;
        let Some(instance) = instance else {
            return Ok(false);
        };
        let known = nominal_known_class_with(instance, NominalClassFacts, self).await?;
        self.local(1, 0, || known == Some(KnownClass::NoneType))
            .await
    }

    async fn property_instance(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> RunResult<PropertyInstanceType<'db>> {
        self.property_instance_value(getter, setter, deleter).await
    }

    async fn dataclass_transformer_params(
        &self,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> RunResult<DataclassTransformerParams<'db>> {
        let make = move || self.access.intern_dataclass_transformer_params(flags, field_specifiers);
        let quote = transform_future_carrier_bytes(&make)
            .map(|bytes| (6, bytes))
            .ok_or(RunError::Contract("transform interner future carriers overflow"));
        // `make` retains the box outside either admission callback until pending children drain.
        self.local_quoted(quote, || ()).await?;
        self.allocate_future(make).await?.await
    }

    async fn property_accessor_call(
        &self,
        env: &ProgramEnvironment<'db>,
        accessor: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        self.positional_synthetic_call(env, accessor, arguments, recursion_guard)
            .await
    }

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.environment_program(env).await?;
        self.allocate_future(|| bindings.descriptor_origin_with(db, env, arguments, self))
            .await?
            .await
    }

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.allocate_future(|| bindings.return_type_with(db, env, self))
            .await?
            .await
    }

    async fn checker_local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = work
            .ok_or(RunError::Contract("checker work quotation overflow"))
            .and_then(|work| {
                bytes
                    .filter(|bytes| *bytes <= isize::MAX as usize)
                    .map(|bytes| (work, bytes))
                    .ok_or(RunError::Contract("checker storage quotation overflow"))
            });
        self.local_quoted(quote, operation).await
    }

    async fn checker_unavailable<T>(&self, operation: CheckerOperation) -> RunResult<T> {
        self.unavailable(match operation {
            CheckerOperation::ArgumentExpansion => SourceOperation::CheckerArgumentExpansion,
            CheckerOperation::GenericInference => SourceOperation::CheckerGenericInference,
            CheckerOperation::KnownFunction => SourceOperation::CheckerKnownFunction,
            CheckerOperation::OverloadFiltering => SourceOperation::CheckerOverloadFiltering,
            CheckerOperation::ParameterUnion => SourceOperation::CheckerParameterUnion,
            CheckerOperation::ParamSpec => SourceOperation::CheckerParamSpec,
            CheckerOperation::Specialization => SourceOperation::CheckerSpecialization,
            CheckerOperation::Splat => SourceOperation::CheckerSplat,
            CheckerOperation::Constructor => SourceOperation::CheckerConstructor,
            CheckerOperation::DownstreamConstructor => {
                SourceOperation::CheckerDownstreamConstructor
            }
            CheckerOperation::Equivalent => SourceOperation::CheckerEquivalent,
            CheckerOperation::ClassInfo => SourceOperation::CheckerClassInfo,
            CheckerOperation::ConstructorReceiver => SourceOperation::CheckerConstructorReceiver,
        })
        .await
    }

    async fn defer_typevartuple_callable(
        &self,
        env: &ProgramEnvironment<'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<bool> {
        SourceEffects::defer_typevartuple_callable(
            self,
            env,
            declared,
            expected,
            argument,
            recursion_guard,
        )
        .await
    }
}

impl<
    'root,
    'run,
    'db: 'run,
    'ast,
    'arg,
    'call,
    S: ArgumentStorage<'call, 'db>,
    A: SourceAccess<'run, 'db>,
> ArgumentEffects<'root, 'db, 'ast, 'arg, 'call, S> for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;
    async fn prepare(
        &self,
        input: Input<'db, 'arg, S>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Context<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        preparation::prepare_with(input, builders, self).await
    }
    async fn requires_overload_evaluation(
        &self,
        context: &Context<'db, 'arg, 'call, S, Self::Builder>,
    ) -> Result<bool, Self::Error> {
        self.local(
            Self::checked(context.candidates.len().checked_add(1))?,
            0,
            || requires_overload_evaluation(&context.candidates),
        )
        .await
    }
    async fn install_cache(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn narrow_targets(
        &self,
        context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<NarrowCursor<'db>, Self::Error> {
        let db = builders.builder(context.input.builder).db();
        let targets = if let Some(annotation) = context.input.tcx.annotation {
            if let Some(union) = union_like_with(annotation, &NarrowEffects(self)).await? {
                if self.local(1, 0, || union.has_aliases(db)).await? {
                    return self.unavailable(SourceOperation::ArgumentNarrowing).await;
                }
                Some(
                    self.local(1, 0, || Cow::Borrowed(union.elements(db)))
                        .await?,
                )
            } else {
                None
            }
        } else {
            None
        };
        Ok(NarrowCursor {
            targets: targets
                .filter(|_| context.has_generic_context)
                .unwrap_or_default(),
            index: 0,
            preferred: true,
        })
    }
    async fn next_narrow(
        &self,
        cursor: &mut NarrowCursor<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.local(2, 0, || {
            infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                'root,
                'db,
                'ast,
                'arg,
                'call,
                S,
            >>::next_narrow(&InlineArgumentEffects, cursor))
        })
        .await
    }
    async fn other_targets(
        &self,
        cursor: NarrowCursor<'db>,
    ) -> Result<NarrowCursor<'db>, Self::Error> {
        let mut cursor = Some(cursor);
        self.local(2, 0, || {
            cursor.take().map(|cursor| {
                infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                    'root,
                    'db,
                    'ast,
                    'arg,
                    'call,
                    S,
                >>::other_targets(
                    &InlineArgumentEffects, cursor
                ))
            })
        })
        .await?
        .ok_or(RunError::Contract(
            "argument narrowing owner was already consumed",
        ))
    }
    async fn prefers_declared(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn can_narrow(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn begin_trial(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _cursor: NarrowCursor<'db>,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn begin_fallback(
        &self,
        context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
    ) -> Result<(), Self::Error> {
        let baseline = ArgumentPreparationEffects::clone_arguments(self, &context.baseline).await?;
        self.work(Self::checked(
            context
                .input
                .storage
                .parts()
                .0
                .len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(1)),
        )?)
        .await?;
        let (work, _) = context
            .input
            .storage
            .parts()
            .0
            .clone_storage_quote()
            .ok_or(RunError::Contract("argument retirement quotation overflow"))?;
        let mut baseline = Some(baseline);
        self.local(work, 0, || {
            if let Some(baseline) = baseline.take() {
                *context.input.storage.parts_mut().0 = baseline;
            }
        })
        .await
    }
    async fn contexts(
        &self,
        context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        candidates: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error> {
        let (arguments, bindings) = context.parts();
        preparation::collect_contexts_with(
            builders.builder(context.builder()),
            arguments,
            bindings,
            candidates.then_some(&context.candidates),
            std::borrow::Borrow::borrow(&context.constraints),
            context.tcx(),
            self,
        )
        .await
    }
    async fn baseline_contexts(
        &self,
        context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        candidates: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error> {
        preparation::collect_contexts_with(
            builders.builder(context.builder()),
            &context.baseline,
            context.parts().1,
            candidates.then_some(&context.candidates),
            std::borrow::Borrow::borrow(&context.constraints),
            context.tcx(),
            self,
        )
        .await
    }
    async fn start_simple(
        &self,
        context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        speculative: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Self::Error> {
        if speculative {
            return self.unavailable(SourceOperation::ArgumentSpeculation).await;
        }
        let mut contexts = Some(contexts);
        self.local(8, 0, || {
            contexts
                .take()
                .map(|contexts| start_simple(context, contexts, speculative, builders))
        })
        .await?
        .ok_or(RunError::Contract(
            "argument context owner was already consumed",
        ))
    }
    async fn start_unified(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        _mode: CallArgumentInferenceMode,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn next_argument(
        &self,
        pass: &mut Pass<'db, 'arg, 'call>,
    ) -> Result<Option<ArgumentItem<'db, 'arg>>, Self::Error> {
        self.local(4, 0, || {
            infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                'root,
                'db,
                'ast,
                'arg,
                'call,
                S,
            >>::next_argument(&InlineArgumentEffects, pass))
        })
        .await
    }
    async fn unique_request(
        &self,
        context: Context<'db, 'arg, 'call, S, Self::Builder>,
        pass: Pass<'db, 'arg, 'call>,
        index: usize,
        expression: &'arg ast::Expr,
        argument_context: Option<ArgumentTypeContext<'db>>,
    ) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        let mut owners = Some((context, pass));
        self.local(8, 0, || {
            owners.take().map(|(context, pass)| {
                unique_request(context, pass, index, expression, argument_context)
            })
        })
        .await?
        .ok_or(RunError::Contract(
            "argument request owner was already consumed",
        ))
    }
    async fn many(
        &self,
        index: usize,
        expression: &'arg ast::Expr,
    ) -> Result<Many<'db, 'arg>, Self::Error> {
        self.local(4, 0, || {
            infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                'root,
                'db,
                'ast,
                'arg,
                'call,
                S,
            >>::many(
                &InlineArgumentEffects, index, expression
            ))
        })
        .await
    }
    async fn default_request(
        &self,
        context: Context<'db, 'arg, 'call, S, Self::Builder>,
        pass: Pass<'db, 'arg, 'call>,
        many: Many<'db, 'arg>,
    ) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        let mut owners = Some((context, pass, many));
        self.local(8, 0, || {
            owners
                .take()
                .map(|(context, pass, many)| default_request(context, pass, many))
        })
        .await?
        .ok_or(RunError::Contract(
            "argument request owner was already consumed",
        ))
    }
    async fn install_many_cache(
        &self,
        _pass: &Pass<'db, 'arg, 'call>,
        _many: &mut Many<'db, 'arg>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn next_context(
        &self,
        pass: &Pass<'db, 'arg, 'call>,
        many: &mut Many<'db, 'arg>,
    ) -> Result<Option<Option<ArgumentTypeContext<'db>>>, Self::Error> {
        self.local(4, 0, || {
            infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                'root,
                'db,
                'ast,
                'arg,
                'call,
                S,
            >>::next_context(
                &InlineArgumentEffects, pass, many
            ))
        })
        .await
    }
    async fn insert_cached(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _pass: &mut Pass<'db, 'arg, 'call>,
        _many: &Many<'db, 'arg>,
        _argument_context: Option<ArgumentTypeContext<'db>>,
        _ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn contextual_request(
        &self,
        _context: Context<'db, 'arg, 'call, S, Self::Builder>,
        _pass: Pass<'db, 'arg, 'call>,
        _many: Many<'db, 'arg>,
        _argument_context: Option<ArgumentTypeContext<'db>>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn teardown_many_cache(
        &self,
        _pass: &Pass<'db, 'arg, 'call>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn finish_pass(
        &self,
        pass: Pass<'db, 'arg, 'call>,
    ) -> Result<FinishedPass<'db, 'call>, Self::Error> {
        self.work(Self::checked(pass.contexts.len().checked_add(1))?)
            .await?;
        let work = Self::checked(pass.contexts.iter().try_fold(4usize, |work, context| {
            work.checked_add(match context {
                Some(MatchingArgumentTypeContext::Many(contexts)) => {
                    contexts.len().checked_add(1)?
                }
                _ => 1,
            })
        }))?;
        let mut pass = Some(pass);
        self.local(work, 0, || {
            pass.take().map(|pass| {
                infallible(<InlineArgumentEffects as SynchronousArgumentEffects<
                    'root,
                    'db,
                    'ast,
                    'arg,
                    'call,
                    S,
                >>::finish_pass(&InlineArgumentEffects, pass))
            })
        })
        .await?
        .ok_or(RunError::Contract(
            "argument pass owner was already consumed",
        ))
    }
    async fn check_active(
        &self,
        context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<Result<(), CallErrorKind>, Self::Error> {
        let recursion_guard = recursion_guard.ok_or(RunError::Contract(
            "controlled AST call lost its invocation guard",
        ))?;
        let builder = builders.builder(context.builder());
        #[cfg(test)]
        {
            let (arguments, bindings) = context.parts();
            self.local(1, 0, || {
                crate::types::relation::source::resources::observations::observe_invocation(
                    builder.db(),
                    std::borrow::Borrow::borrow(&context.constraints),
                    crate::types::relation::source::resources::observations::InvocationStage::ArgumentCheck,
                );
                crate::types::infer::builder::source_definition::controlled::observations::arguments_ready(
                    builder.db(),
                    arguments,
                    bindings,
                )
            })
            .await?;
        }
        let tcx = context.tcx();
        let constraints = context.constraints;
        let Context { input, active, .. } = context;
        let (arguments, bindings) = match active {
            Active::Root => input.storage.parts_mut(),
            Active::Narrow(trial) => trial.storage.parts_mut(),
        };
        let result = crate::types::call::bind::source_check::check(
            builder.db(),
            builder.program_environment(),
            constraints,
            arguments,
            bindings,
            tcx,
            &builder.dataclass_field_specifiers,
            Some(recursion_guard),
            self,
        )
        .await?;
        #[cfg(test)]
        self.local(1, 0, || {
            crate::types::infer::builder::source_definition::controlled::observations::arguments_checked(
                builder.db(), arguments, bindings,
            );
        }).await?;
        Ok(result)
    }
    async fn simple_committed(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _speculative: BuilderId,
        _result: Result<(), CallErrorKind>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn merge_expected(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _speculative: BuilderId,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn check_unified(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: &mut Unified<'call, 'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
        _recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn inferred_converged(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: &Unified<'call, 'db>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn contexts_converged(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _previous: &[Option<MatchingArgumentTypeContext<'db>>],
        _next: &[Option<MatchingArgumentTypeContext<'db>>],
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn unified_contexts(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: &Unified<'call, 'db>,
        _candidates: bool,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn next_iteration(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: Unified<'call, 'db>,
        _contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        _mode: CallArgumentInferenceMode,
        _previous_builder: BuilderId,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn finalize_unified(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: &mut Unified<'call, 'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Result<(), CallErrorKind>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn unified_committed(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: Unified<'call, 'db>,
        _contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        _speculative: BuilderId,
        _result: Result<(), CallErrorKind>,
    ) -> Result<Pass<'db, 'arg, 'call>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn commit_unified(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _unified: Unified<'call, 'db>,
        _speculative: BuilderId,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn commit_bindings(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _bindings: Bindings<'db>,
        _arguments: CallArguments<'call, 'db>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn accepts_trial(
        &self,
        _context: &Context<'db, 'arg, 'call, S, Self::Builder>,
        _trial: &Trial<'call, 'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn finish_attempt(
        &self,
        context: Context<'db, 'arg, 'call, S, Self::Builder>,
    ) -> Result<FinishedAttempt<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        let mut context = Some(context);
        self.local(8, 0, || context.take().map(finish_attempt))
        .await?
        .ok_or(RunError::Contract(
            "completed argument context was already consumed",
        ))
    }
    async fn discard_trial(
        &self,
        _trial: Trial<'call, 'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<NarrowCursor<'db>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn commit_trial(
        &self,
        _context: &mut Context<'db, 'arg, 'call, S, Self::Builder>,
        _trial: Trial<'call, 'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn complete(
        &self,
        context: Context<'db, 'arg, 'call, S, Self::Builder>,
        result: Result<(), CallErrorKind>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        if context.teardown_cache {
            return self.unavailable(SourceOperation::ArgumentSpeculation).await;
        }
        self.work(Self::checked(
            context
                .baseline
                .len()
                .checked_mul(4)
                .and_then(|work| work.checked_add(context.candidates.len()))
                .and_then(|work| work.checked_add(4)),
        )?)
        .await?;
        let (baseline_work, _) = context
            .baseline
            .clone_storage_quote()
            .ok_or(RunError::Contract("argument retirement quotation overflow"))?;
        let work = Self::checked(
            context
                .candidates
                .iter()
                .try_fold(baseline_work, |work, candidates| {
                    work.checked_add(candidates.len().checked_add(1)?)
                })
                .and_then(|work| work.checked_add(context.generic_arguments.len()))
                .and_then(|work| work.checked_add(8)),
        )?;
        // finish_attempt has detached any narrow trial. The original completion moves the
        // input storage out and retires only this context's baseline and inference metadata.
        let mut context = Some(context);
        self.local(work, 0, || {
            context
                .take()
                .map(|context| complete(context, result, builders))
        })
        .await?
        .ok_or(RunError::Contract(
            "completed argument context was already consumed",
        ))
    }
    async fn resume_unique(
        &self,
        context: Context<'db, 'arg, 'call, S, Self::Builder>,
        pass: Pass<'db, 'arg, 'call>,
        index: usize,
        argument_context: Option<ArgumentTypeContext<'db>>,
        ty: Type<'db>,
    ) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        let mut context = context;
        let mut pass = pass;
        let arguments = pass.arguments_mut(&mut context);
        self.work(Self::checked(
            arguments
                .len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(1)),
        )?)
        .await?;
        let (scan, _) = arguments
            .clone_storage_quote()
            .ok_or(RunError::Contract("argument storage quotation overflow"))?;
        self.work(scan).await?;
        let (keys, count) = match argument_context {
            None => ([None, None], 1),
            Some(ArgumentTypeContext::Standard {
                raw_parameter_type, ..
            }) => ([Some(raw_parameter_type), None], 1),
            Some(ArgumentTypeContext::ParamSpec {
                paramspec_parameter_type,
                declared_type,
            }) => ([Some(declared_type), Some(paramspec_parameter_type)], 2),
        };
        let (work, bytes) = arguments
            .insert_types_storage_quote(index, &keys[..count])
            .ok_or(RunError::Contract("argument insertion quotation overflow"))?;
        self.local(work, bytes, || {
            insert_argument(arguments, index, argument_context, ty)
        })
        .await?;
        Ok(State::Pass(context, pass))
    }
    async fn resume_default(
        &self,
        _context: Context<'db, 'arg, 'call, S, Self::Builder>,
        _pass: Pass<'db, 'arg, 'call>,
        _many: Many<'db, 'arg>,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
    async fn resume_context(
        &self,
        _context: Context<'db, 'arg, 'call, S, Self::Builder>,
        _pass: Pass<'db, 'arg, 'call>,
        _many: Many<'db, 'arg>,
        _argument_context: Option<ArgumentTypeContext<'db>>,
        _speculative: BuilderId,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error> {
        self.unavailable(SourceOperation::ArgumentSpeculation).await
    }
}

fn infallible<T>(value: Result<T, Infallible>) -> T {
    match value {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ArgumentPreparationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;

    async fn new_builder(&self) -> RunResult<Self::Builder> {
        self.resources().invocation_builder(self.endpoint()).await
    }

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        SourceEffects::local(self, Self::checked(work)?, Self::checked(bytes)?, action).await
    }
    async fn clone_arguments<'call>(
        &self,
        arguments: &CallArguments<'call, 'db>,
    ) -> RunResult<CallArguments<'call, 'db>> {
        self.work(Self::checked(
            arguments
                .len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(1)),
        )?)
        .await?;
        let (work, bytes) = arguments
            .clone_storage_quote()
            .ok_or(RunError::Contract("argument clone quotation overflow"))?;
        self.local(work, bytes, || arguments.clone()).await
    }
    async fn callables<'a>(
        &self,
        bindings: &'a Bindings<'db>,
    ) -> RunResult<Vec<&'a CallableBinding<'db>>> {
        self.work(Self::checked(
            bindings.argument_context_root_len().checked_add(1),
        )?)
        .await?;
        let work = Self::checked(bindings.direct_type_context_work())?;
        self.work(work).await?;
        let count = bindings.direct_type_context_callables().count();
        let mut callables = Vec::new();
        self.local(
            count + 1,
            Self::checked(count.checked_mul(size_of::<&CallableBinding<'db>>()))?,
            || callables.reserve_exact(count),
        )
        .await?;
        for (binding, downstream) in bindings.direct_type_context_callables() {
            self.local(1, 0, || callables.push(binding)).await?;
            if downstream.is_some() {
                return self.unavailable(SourceOperation::ArgumentPreparation).await;
            }
        }
        Ok(callables)
    }
    async fn binding_work(&self, binding: &CallableBinding<'db>) -> RunResult<()> {
        self.work(Self::checked(binding.overloads().len().checked_add(1))?)
            .await?;
        self.work(Self::checked(binding.argument_context_metadata_work())?)
            .await
    }
    async fn candidate_indices(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<SmallVec<[usize; 1]>> {
        binding
            .candidate_overload_indices_with(db, env, arguments, self)
            .await
    }
    async fn occurrence_count(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _overload: &Binding<'db>,
        _binding: &CallableBinding<'db>,
        _index: usize,
    ) -> RunResult<usize> {
        self.unavailable(SourceOperation::ArgumentGenericContext)
            .await
    }
    async fn parameter_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        overload: &Binding<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        index: usize,
        tcx: TypeContext<'db>,
        specialization: &OnceCell<Option<Specialization<'db>>>,
    ) -> RunResult<Option<ArgumentTypeContext<'db>>> {
        overload
            .argument_type_context_with(
                db,
                env,
                constraints,
                binding,
                arguments,
                index,
                tcx,
                || async {
                    if let Some(specialization) = specialization.get() {
                        Ok(*specialization)
                    } else {
                        self.unavailable(SourceOperation::ArgumentGenericContext)
                            .await
                    }
                },
                self,
            )
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ArgumentContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        SourceEffects::local(self, Self::checked(work)?, Self::checked(bytes)?, action).await
    }
    async fn upper_bound(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn without_paramspec_attr(
        &self,
        _db: &'db dyn Db,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn merged_specialization(
        &self,
        _db: &'db dyn Db,
        _overload: &Binding<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn specialization_binding(
        &self,
        _db: &'db dyn Db,
        _specialization: Specialization<'db>,
        _paramspec: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn apply_specialization(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _specialization: Specialization<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn paramspec_context(
        &self,
        _request: &ArgumentContextRequest<'_, '_, 'db>,
        _callable: CallableType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn typevartuple_context(
        &self,
        _request: &ArgumentContextRequest<'_, '_, 'db>,
        _expected_return_ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ArgumentTypeContext).await
    }
    async fn has_expandable_variadic(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<bool> {
        for (argument, _) in arguments.iter() {
            self.work(1).await?;
            if matches!(argument, crate::types::call::Argument::Variadic) {
                return self.unavailable(SourceOperation::ArgumentCandidates).await;
            }
        }
        Ok(false)
    }
}

struct NarrowEffects<'a, 'access, 'run, 'db: 'run, A>(&'a SourceEffects<'access, 'run, 'db, A>);

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeDispatchEffects<'db>
    for NarrowEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;
    async fn checkpoint(&self) -> RunResult<()> {
        self.0.work(1).await
    }
    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.0.resolve_context_alias(ty).await
    }
    async fn newtype_union(&self, _newtype: NewType<'db>) -> RunResult<Option<UnionType<'db>>> {
        self.0.unavailable(SourceOperation::ArgumentNarrowing).await
    }
    async fn collect_properties(
        &self,
        _ty: Type<'db>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.0.unavailable(SourceOperation::ArgumentNarrowing).await
    }
}
