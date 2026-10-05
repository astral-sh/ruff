//! Call argument inference as owned transitions that yield one expression request at a time.

mod preparation;
#[cfg(feature = "experimental-analysis")]
mod source;

use std::borrow::Cow;
use std::cell::OnceCell;
use std::convert::Infallible;
use std::iter::Enumerate;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

use super::super::{
    ArgExpr, ArgumentsIter, CallArgumentInferenceMode, MatchingArgumentTypeContext,
    TypeInferenceBuilder,
};
use super::{BuilderId, BuilderStore};
use crate::types::call::bind::{
    ArgumentTypeContext, CheckTypesMode, OverloadSet, requires_overload_evaluation,
};
use crate::types::call::{Binding, Bindings, CallArguments, CallErrorKind, CallableBinding};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::generics::Specialization;
use crate::types::typevar::TypeVarSet;
use crate::types::{Type, TypeContext};
use ruff_python_ast as ast;

pub(super) trait ArgumentStorage<'call, 'db> {
    fn parts(&self) -> (&CallArguments<'call, 'db>, &Bindings<'db>);
    fn parts_mut(&mut self) -> (&mut CallArguments<'call, 'db>, &mut Bindings<'db>);
}

pub(super) struct OwnedArguments<'call, 'db> {
    pub(super) arguments: CallArguments<'call, 'db>,
    pub(super) bindings: Bindings<'db>,
}

impl<'call, 'db> ArgumentStorage<'call, 'db> for OwnedArguments<'call, 'db> {
    fn parts(&self) -> (&CallArguments<'call, 'db>, &Bindings<'db>) {
        (&self.arguments, &self.bindings)
    }

    fn parts_mut(&mut self) -> (&mut CallArguments<'call, 'db>, &mut Bindings<'db>) {
        (&mut self.arguments, &mut self.bindings)
    }
}

pub(super) struct BorrowedArguments<'storage, 'call, 'db> {
    pub(super) arguments: &'storage mut CallArguments<'call, 'db>,
    pub(super) bindings: &'storage mut Bindings<'db>,
}

impl<'call, 'db> ArgumentStorage<'call, 'db> for BorrowedArguments<'_, 'call, 'db> {
    fn parts(&self) -> (&CallArguments<'call, 'db>, &Bindings<'db>) {
        (self.arguments, self.bindings)
    }

    fn parts_mut(&mut self) -> (&mut CallArguments<'call, 'db>, &mut Bindings<'db>) {
        (self.arguments, self.bindings)
    }
}

#[derive(Clone, Copy)]
pub(super) enum ArgumentPolicy {
    Ordinary,
    PermitParamSpec,
    External,
}

pub(super) struct Input<'db, 'arg, S> {
    builder: BuilderId,
    arguments: ArgumentsIter<'arg>,
    storage: S,
    policy: ArgumentPolicy,
    tcx: TypeContext<'db>,
}

pub(super) struct Context<'db, 'arg, 'call, S, B = ConstraintSetBuilder<'db>> {
    input: Input<'db, 'arg, S>,
    baseline: CallArguments<'call, 'db>,
    constraints: B,
    candidates: OverloadSet,
    generic_arguments: SmallVec<[usize; 4]>,
    typevar_occurrences: usize,
    has_generic_context: bool,
    teardown_cache: bool,
    active: Active<'call, 'db>,
}

enum Active<'call, 'db> {
    Root,
    Narrow(Trial<'call, 'db>),
}

pub(super) struct Trial<'call, 'db> {
    builder: BuilderId,
    tcx: TypeContext<'db>,
    storage: OwnedArguments<'call, 'db>,
    remaining: NarrowCursor<'db>,
}

pub(super) enum FinishedAttempt<'db, 'arg, 'call, S, B = ConstraintSetBuilder<'db>> {
    Root(Context<'db, 'arg, 'call, S, B>),
    Narrow(Context<'db, 'arg, 'call, S, B>, Trial<'call, 'db>),
}

impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> Context<'db, 'arg, 'call, S, B> {
    fn builder(&self) -> BuilderId {
        match &self.active {
            Active::Root => self.input.builder,
            Active::Narrow(trial) => trial.builder,
        }
    }

    fn tcx(&self) -> TypeContext<'db> {
        match &self.active {
            Active::Root => self.input.tcx,
            Active::Narrow(trial) => trial.tcx,
        }
    }

    fn parts(&self) -> (&CallArguments<'call, 'db>, &Bindings<'db>) {
        match &self.active {
            Active::Root => self.input.storage.parts(),
            Active::Narrow(trial) => trial.storage.parts(),
        }
    }

    fn parts_mut(&mut self) -> (&mut CallArguments<'call, 'db>, &mut Bindings<'db>) {
        match &mut self.active {
            Active::Root => self.input.storage.parts_mut(),
            Active::Narrow(trial) => trial.storage.parts_mut(),
        }
    }
}

pub(super) struct NarrowCursor<'db> {
    targets: Cow<'db, [Type<'db>]>,
    index: usize,
    preferred: bool,
}

pub(super) struct Unified<'call, 'db> {
    iteration: usize,
    next_bindings: Bindings<'db>,
    previous_arguments: CallArguments<'call, 'db>,
    next_arguments: CallArguments<'call, 'db>,
}

pub(super) enum AfterPass<'call, 'db> {
    Direct,
    SimpleSpeculative,
    SimpleCommitted {
        speculative: BuilderId,
        result: Result<(), CallErrorKind>,
    },
    Unified(Unified<'call, 'db>),
    UnifiedCommitted {
        speculative: BuilderId,
        bindings: Bindings<'db>,
        arguments: CallArguments<'call, 'db>,
        result: Result<(), CallErrorKind>,
    },
}

pub(super) struct Pass<'db, 'arg, 'call> {
    builder: BuilderId,
    arguments: Enumerate<ArgumentsIter<'arg>>,
    contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
    mode: CallArgumentInferenceMode,
    after: AfterPass<'call, 'db>,
}

impl<'db, 'arg, 'call> Pass<'db, 'arg, 'call> {
    fn arguments_mut<'s, S: ArgumentStorage<'call, 'db>, B>(
        &'s mut self,
        context: &'s mut Context<'db, 'arg, 'call, S, B>,
    ) -> &'s mut CallArguments<'call, 'db> {
        match &mut self.after {
            AfterPass::Unified(unified) => &mut unified.next_arguments,
            _ => context.parts_mut().0,
        }
    }
}

pub(super) struct Many<'db, 'arg> {
    index: usize,
    expression: &'arg ast::Expr,
    cursor: usize,
    inferred: FxHashMap<Option<Type<'db>>, Type<'db>>,
    teardown_cache: bool,
}

pub(super) enum State<'db, 'arg, 'call, S, B = ConstraintSetBuilder<'db>> {
    Start(Input<'db, 'arg, S>),
    Narrow(Context<'db, 'arg, 'call, S, B>, NarrowCursor<'db>),
    Select(Context<'db, 'arg, 'call, S, B>),
    Pass(Context<'db, 'arg, 'call, S, B>, Pass<'db, 'arg, 'call>),
    Many(
        Context<'db, 'arg, 'call, S, B>,
        Pass<'db, 'arg, 'call>,
        Many<'db, 'arg>,
    ),
    Finished(Context<'db, 'arg, 'call, S, B>, Result<(), CallErrorKind>),
}

impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> State<'db, 'arg, 'call, S, B> {
    pub(super) fn new(
        builder: BuilderId,
        arguments: ArgumentsIter<'arg>,
        storage: S,
        policy: ArgumentPolicy,
        tcx: TypeContext<'db>,
    ) -> Self {
        Self::Start(Input {
            builder,
            arguments,
            storage,
            policy,
            tcx,
        })
    }
}

pub(super) enum Action<'db, 'arg, 'call, S, B = ConstraintSetBuilder<'db>> {
    Continue(State<'db, 'arg, 'call, S, B>),
    Infer {
        pending: Pending<'db, 'arg, 'call, S, B>,
        builder: BuilderId,
        argument: ArgExpr<'db, 'arg>,
        policy: ArgumentPolicy,
    },
    Complete {
        storage: S,
        result: Result<(), CallErrorKind>,
    },
}

pub(super) struct Pending<'db, 'arg, 'call, S, B = ConstraintSetBuilder<'db>> {
    context: Context<'db, 'arg, 'call, S, B>,
    pass: Pass<'db, 'arg, 'call>,
    return_to: ReturnTo<'db, 'arg>,
}

pub(super) enum ReturnTo<'db, 'arg> {
    Unique {
        index: usize,
        context: Option<ArgumentTypeContext<'db>>,
    },
    Default(Many<'db, 'arg>),
    Context {
        many: Many<'db, 'arg>,
        context: Option<ArgumentTypeContext<'db>>,
        speculative: BuilderId,
    },
}

pub(super) enum ArgumentItem<'db, 'arg> {
    Skip,
    Unique {
        index: usize,
        expression: &'arg ast::Expr,
        context: Option<ArgumentTypeContext<'db>>,
    },
    Many {
        index: usize,
        expression: &'arg ast::Expr,
    },
}

pub(super) struct FinishedPass<'db, 'call> {
    builder: BuilderId,
    contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
    after: AfterPass<'call, 'db>,
}

pub(super) struct ArgumentFacts;
pub(super) struct InlineArgumentEffects;

shared_semantic_family! {
    #[synchronous(SynchronousArgumentEffects)]
    pub(super) trait ArgumentEffects<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>> {
        type Error;
        type Builder: std::borrow::Borrow<ConstraintSetBuilder<'db>>;
        #[operation(child)]
        async fn prepare(&self, input: Input<'db, 'arg, S>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Context<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(child)]
        async fn requires_overload_evaluation(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn install_cache(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn narrow_targets(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<NarrowCursor<'db>, Self::Error>;
        #[operation(local)]
        async fn next_narrow(&self, cursor: &mut NarrowCursor<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn other_targets(&self, cursor: NarrowCursor<'db>) -> Result<NarrowCursor<'db>, Self::Error>;
        #[operation(child)]
        async fn prefers_declared(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, ty: Type<'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn can_narrow(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, ty: Type<'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn begin_trial(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, cursor: NarrowCursor<'db>, ty: Type<'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn begin_fallback(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn contexts(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, candidates: bool, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error>;
        #[operation(child)]
        async fn baseline_contexts(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, candidates: bool, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error>;
        #[operation(local)]
        async fn start_simple(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>, speculative: bool, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Pass<'db, 'arg, 'call>, Self::Error>;
        #[operation(local)]
        async fn start_unified(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>, mode: CallArgumentInferenceMode, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Pass<'db, 'arg, 'call>, Self::Error>;
        #[operation(local)]
        async fn next_argument(&self, pass: &mut Pass<'db, 'arg, 'call>) -> Result<Option<ArgumentItem<'db, 'arg>>, Self::Error>;
        #[operation(local)]
        async fn unique_request(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, index: usize, expression: &'arg ast::Expr, argument_context: Option<ArgumentTypeContext<'db>>) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn many(&self, index: usize, expression: &'arg ast::Expr) -> Result<Many<'db, 'arg>, Self::Error>;
        #[operation(local)]
        async fn default_request(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, many: Many<'db, 'arg>) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn install_many_cache(&self, pass: &Pass<'db, 'arg, 'call>, many: &mut Many<'db, 'arg>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn next_context(&self, pass: &Pass<'db, 'arg, 'call>, many: &mut Many<'db, 'arg>) -> Result<Option<Option<ArgumentTypeContext<'db>>>, Self::Error>;
        #[operation(local)]
        async fn insert_cached(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, pass: &mut Pass<'db, 'arg, 'call>, many: &Many<'db, 'arg>, argument_context: Option<ArgumentTypeContext<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contextual_request(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, many: Many<'db, 'arg>, argument_context: Option<ArgumentTypeContext<'db>>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn teardown_many_cache(&self, pass: &Pass<'db, 'arg, 'call>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_pass(&self, pass: Pass<'db, 'arg, 'call>) -> Result<FinishedPass<'db, 'call>, Self::Error>;
        #[operation(child)]
        async fn check_active(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, builders: &mut BuilderStore<'root, 'db, 'ast>, recursion_guard: Option<&CallableRecursionGuard<'db>>) -> Result<Result<(), CallErrorKind>, Self::Error>;
        #[operation(child)]
        async fn simple_committed(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, speculative: BuilderId, result: Result<(), CallErrorKind>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Pass<'db, 'arg, 'call>, Self::Error>;
        #[operation(local)]
        async fn merge_expected(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, speculative: BuilderId, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_unified(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: &mut Unified<'call, 'db>, builders: &mut BuilderStore<'root, 'db, 'ast>, recursion_guard: Option<&CallableRecursionGuard<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn inferred_converged(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: &Unified<'call, 'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn contexts_converged(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, previous: &[Option<MatchingArgumentTypeContext<'db>>], next: &[Option<MatchingArgumentTypeContext<'db>>]) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn unified_contexts(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: &Unified<'call, 'db>, candidates: bool, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Self::Error>;
        #[operation(local)]
        async fn next_iteration(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: Unified<'call, 'db>, contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>, mode: CallArgumentInferenceMode, previous_builder: BuilderId, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Pass<'db, 'arg, 'call>, Self::Error>;
        #[operation(child)]
        async fn finalize_unified(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: &mut Unified<'call, 'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Result<(), CallErrorKind>, Self::Error>;
        #[operation(local)]
        async fn unified_committed(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, unified: Unified<'call, 'db>, contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>, speculative: BuilderId, result: Result<(), CallErrorKind>) -> Result<Pass<'db, 'arg, 'call>, Self::Error>;
        #[operation(local)]
        async fn commit_unified(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, unified: Unified<'call, 'db>, speculative: BuilderId, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn commit_bindings(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, bindings: Bindings<'db>, arguments: CallArguments<'call, 'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn accepts_trial(&self, context: &Context<'db, 'arg, 'call, S, Self::Builder>, trial: &Trial<'call, 'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn finish_attempt(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>) -> Result<FinishedAttempt<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn discard_trial(&self, trial: Trial<'call, 'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<NarrowCursor<'db>, Self::Error>;
        #[operation(local)]
        async fn commit_trial(&self, context: &mut Context<'db, 'arg, 'call, S, Self::Builder>, trial: Trial<'call, 'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn complete(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, result: Result<(), CallErrorKind>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<Action<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn resume_unique(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, index: usize, argument_context: Option<ArgumentTypeContext<'db>>, ty: Type<'db>) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn resume_default(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, many: Many<'db, 'arg>, ty: Type<'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
        #[operation(local)]
        async fn resume_context(&self, context: Context<'db, 'arg, 'call, S, Self::Builder>, pass: Pass<'db, 'arg, 'call>, many: Many<'db, 'arg>, argument_context: Option<ArgumentTypeContext<'db>>, speculative: BuilderId, ty: Type<'db>, builders: &mut BuilderStore<'root, 'db, 'ast>) -> Result<State<'db, 'arg, 'call, S, Self::Builder>, Self::Error>;
    }

    #[finite_capability]
    impl ArgumentFacts {
        fn generic<'db, 'arg, 'call, S, B>(&self, context: &Context<'db, 'arg, 'call, S, B>) -> bool {
            !context.generic_arguments.is_empty()
        }
        fn preferred(&self, cursor: &NarrowCursor<'_>) -> bool { cursor.preferred }
        fn matches_preference(&self, cursor: &NarrowCursor<'_>, preferred: bool) -> bool { cursor.preferred == preferred }
        fn default_inference(&self, pass: &Pass<'_, '_, '_>) -> bool { pass.mode.requires_default_inference() }
        fn teardown_many(&self, many: &Many<'_, '_>) -> bool { many.teardown_cache }
        fn cached<'db>(&self, many: &Many<'db, '_>, context: Option<ArgumentTypeContext<'db>>) -> Option<Type<'db>> {
            many.inferred.get(&context.map(ArgumentTypeContext::inference_cache_key)).copied()
        }
        fn checked_iteration(&self, unified: &Unified<'_, '_>) -> bool { unified.iteration > 0 }
        fn iteration_bound<'db, 'arg, 'call, S, B>(&self, context: &Context<'db, 'arg, 'call, S, B>, unified: &Unified<'call, 'db>) -> bool {
            unified.iteration == context.typevar_occurrences
        }
        fn failed(&self, result: Result<(), CallErrorKind>) -> bool { result.is_err() }
    }

    #[synchronous(advance_sync)]
    #[capabilities(effects = ArgumentEffects, facts = ArgumentFacts)]
    #[passive_values(Action::Continue, State::Narrow, State::Select, State::Pass, State::Many, State::Finished, CallArgumentInferenceMode::Speculate, CallArgumentInferenceMode::Commit)]
    pub(super) async fn advance_with<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>, E: ArgumentEffects<'root, 'db, 'ast, 'arg, 'call, S>>(
        state: State<'db, 'arg, 'call, S, E::Builder>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
        facts: ArgumentFacts,
        effects: &E,
    ) -> Result<Action<'db, 'arg, 'call, S, E::Builder>, E::Error> {
        match state {
            State::Start(input) => {
                let mut context = effects.prepare(input, builders).await?;
                if facts.generic(&context) || effects.requires_overload_evaluation(&context).await? {
                    effects.install_cache(&mut context, builders).await?;
                }
                let cursor = effects.narrow_targets(&context, builders).await?;
                Ok(Action::Continue(State::Narrow(context, cursor)))
            }
            State::Narrow(mut context, mut cursor) => {
                match effects.next_narrow(&mut cursor).await? {
                    Some(ty) => {
                        let preferred = effects.prefers_declared(&context, ty, builders).await?;
                        if facts.matches_preference(&cursor, preferred) && effects.can_narrow(&context, ty, builders).await? {
                            effects.begin_trial(&mut context, cursor, ty, builders).await?;
                            Ok(Action::Continue(State::Select(context)))
                        } else {
                            Ok(Action::Continue(State::Narrow(context, cursor)))
                        }
                    }
                    None => {
                        if facts.preferred(&cursor) {
                            let cursor = effects.other_targets(cursor).await?;
                            Ok(Action::Continue(State::Narrow(context, cursor)))
                        } else {
                            effects.begin_fallback(&mut context).await?;
                            Ok(Action::Continue(State::Select(context)))
                        }
                    }
                }
            }
            State::Select(context) => {
                let pass = if facts.generic(&context) {
                    let contexts = effects.contexts(&context, true, builders).await?;
                    let mode = if effects.requires_overload_evaluation(&context).await? { CallArgumentInferenceMode::Speculate } else { CallArgumentInferenceMode::Commit };
                    effects.start_unified(&context, contexts, mode, builders).await?
                } else {
                    let overloaded = effects.requires_overload_evaluation(&context).await?;
                    let contexts = effects.baseline_contexts(&context, overloaded, builders).await?;
                    effects.start_simple(&context, contexts, overloaded, builders).await?
                };
                Ok(Action::Continue(State::Pass(context, pass)))
            }
            State::Pass(mut context, mut pass) => {
                match effects.next_argument(&mut pass).await? {
                    Some(ArgumentItem::Skip) => Ok(Action::Continue(State::Pass(context, pass))),
                    Some(ArgumentItem::Unique { index, expression, context: argument_context }) => effects.unique_request(context, pass, index, expression, argument_context).await,
                    Some(ArgumentItem::Many { index, expression }) => {
                        let mut many = effects.many(index, expression).await?;
                        if facts.default_inference(&pass) {
                            effects.default_request(context, pass, many).await
                        } else {
                            effects.install_many_cache(&pass, &mut many, builders).await?;
                            Ok(Action::Continue(State::Many(context, pass, many)))
                        }
                    }
                    None => {
                        let FinishedPass { builder, contexts, after } = effects.finish_pass(pass).await?;
                        match after {
                            AfterPass::Direct => {
                                let result = effects.check_active(&mut context, builders, recursion_guard).await?;
                                Ok(Action::Continue(State::Finished(context, result)))
                            }
                            AfterPass::SimpleSpeculative => {
                                let result = effects.check_active(&mut context, builders, recursion_guard).await?;
                                let pass = effects.simple_committed(&mut context, builder, result, builders).await?;
                                Ok(Action::Continue(State::Pass(context, pass)))
                            }
                            AfterPass::SimpleCommitted { speculative, result } => {
                                effects.merge_expected(&context, speculative, builders).await?;
                                Ok(Action::Continue(State::Finished(context, result)))
                            }
                            AfterPass::Unified(mut unified) => {
                                let inferred_converged = effects.inferred_converged(&context, &unified).await?;
                                if !(facts.checked_iteration(&unified) && inferred_converged) {
                                    effects.check_unified(&context, &mut unified, builders, recursion_guard).await?;
                                    if !inferred_converged && !facts.iteration_bound(&context, &unified) {
                                        let next_contexts = effects.unified_contexts(&context, &unified, true, builders).await?;
                                        if !effects.contexts_converged(&context, &contexts, &next_contexts).await? {
                                            let mode = if effects.requires_overload_evaluation(&context).await? { CallArgumentInferenceMode::Speculate } else { CallArgumentInferenceMode::Commit };
                                            let pass = effects.next_iteration(&context, unified, next_contexts, mode, builder, builders).await?;
                                            return Ok(Action::Continue(State::Pass(context, pass)));
                                        }
                                    }
                                }
                                let result = effects.finalize_unified(&context, &mut unified, builders).await?;
                                if effects.requires_overload_evaluation(&context).await? {
                                    let contexts = effects.unified_contexts(&context, &unified, false, builders).await?;
                                    let pass = effects.unified_committed(&context, unified, contexts, builder, result).await?;
                                    Ok(Action::Continue(State::Pass(context, pass)))
                                } else {
                                    effects.commit_unified(&mut context, unified, builder, builders).await?;
                                    Ok(Action::Continue(State::Finished(context, result)))
                                }
                            }
                            AfterPass::UnifiedCommitted { speculative, bindings, arguments, result } => {
                                effects.merge_expected(&context, speculative, builders).await?;
                                effects.commit_bindings(&mut context, bindings, arguments).await?;
                                Ok(Action::Continue(State::Finished(context, result)))
                            }
                        }
                    }
                }
            }
            State::Many(mut context, mut pass, mut many) => {
                match effects.next_context(&pass, &mut many).await? {
                    Some(argument_context) => {
                        if let Some(ty) = facts.cached(&many, argument_context) {
                            // Equal inference keys can still have different original ParamSpec
                            // annotations. Insert through the current context on a cache hit too.
                            effects.insert_cached(&mut context, &mut pass, &many, argument_context, ty).await?;
                            Ok(Action::Continue(State::Many(context, pass, many)))
                        } else {
                            effects.contextual_request(context, pass, many, argument_context, builders).await
                        }
                    }
                    None => {
                        if facts.teardown_many(&many) { effects.teardown_many_cache(&pass, builders).await?; }
                        Ok(Action::Continue(State::Pass(context, pass)))
                    }
                }
            }
            State::Finished(context, result) => {
                match effects.finish_attempt(context).await? {
                    FinishedAttempt::Root(context) => effects.complete(context, result, builders).await,
                    FinishedAttempt::Narrow(mut context, trial) => {
                        if facts.failed(result) || !effects.accepts_trial(&context, &trial, builders).await? {
                            let cursor = effects.discard_trial(trial, builders).await?;
                            return Ok(Action::Continue(State::Narrow(context, cursor)));
                        }
                        effects.commit_trial(&mut context, trial, builders).await?;
                        effects.complete(context, result, builders).await
                    }
                }
            }
        }
    }

    #[synchronous(resume_sync)]
    #[capabilities(effects = ArgumentEffects)]
    #[passive_values()]
    pub(super) async fn resume_with<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>, E: ArgumentEffects<'root, 'db, 'ast, 'arg, 'call, S>>(
        pending: Pending<'db, 'arg, 'call, S, E::Builder>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
        effects: &E,
    ) -> Result<State<'db, 'arg, 'call, S, E::Builder>, E::Error> {
        let Pending { context, pass, return_to } = pending;
        match return_to {
            ReturnTo::Unique { index, context: argument_context } => effects.resume_unique(context, pass, index, argument_context, ty).await,
            ReturnTo::Default(many) => effects.resume_default(context, pass, many, ty, builders).await,
            ReturnTo::Context { many, context: argument_context, speculative } => effects.resume_context(context, pass, many, argument_context, speculative, ty, builders).await,
        }
    }
}

impl<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>>
    SynchronousArgumentEffects<'root, 'db, 'ast, 'arg, 'call, S> for InlineArgumentEffects
{
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;

    fn prepare(
        &self,
        input: Input<'db, 'arg, S>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Context<'db, 'arg, 'call, S>, Infallible> {
        Ok(crate::types::signatures::effects::legacy_inline(
            preparation::prepare_with(input, builders, self),
        ))
    }

    fn requires_overload_evaluation(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
    ) -> Result<bool, Infallible> {
        Ok(requires_overload_evaluation(&context.candidates))
    }

    fn install_cache(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        context.teardown_cache = builders.setup_expression_cache(context.input.builder);
        Ok(())
    }

    fn narrow_targets(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<NarrowCursor<'db>, Infallible> {
        let builder = builders.builder(context.input.builder);
        let targets = context
            .input
            .tcx
            .narrow_targets(builder.db(), builder.program_environment())
            // Type context affects narrowing only for generic calls.
            .filter(|_| context.has_generic_context)
            .unwrap_or_default();
        Ok(NarrowCursor {
            targets,
            index: 0,
            preferred: true,
        })
    }

    fn next_narrow(&self, cursor: &mut NarrowCursor<'db>) -> Result<Option<Type<'db>>, Infallible> {
        let ty = cursor.targets.get(cursor.index).copied();
        cursor.index += usize::from(ty.is_some());
        Ok(ty)
    }

    fn other_targets(&self, cursor: NarrowCursor<'db>) -> Result<NarrowCursor<'db>, Infallible> {
        Ok(NarrowCursor {
            index: 0,
            preferred: false,
            ..cursor
        })
    }

    fn prefers_declared(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Infallible> {
        let builder = builders.builder(context.input.builder);
        Ok(ty.may_prefer_declared_type(builder.db(), builder.program_environment()))
    }

    fn can_narrow(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Infallible> {
        let builder = builders.builder(context.input.builder);
        let db = builder.db();
        let env = builder.program_environment();
        Ok(context.parts().1.satisfies(|overload| {
            let inferable = overload
                .signature
                .generic_context
                .map(|generic_context| generic_context.inferable_typevars(db))
                .unwrap_or(TypeVarSet::None);
            !overload
                .return_ty
                .when_assignable_to(db, env, ty, &context.constraints, inferable)
                .is_never_satisfied(db, env)
        }))
    }

    fn begin_trial(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        cursor: NarrowCursor<'db>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        let bindings = context.parts().1.clone();
        let builder = builders.speculate(context.input.builder, false);
        let arguments = context.baseline.clone();
        context.active = Active::Narrow(Trial {
            builder,
            tcx: TypeContext::new(Some(ty)),
            storage: OwnedArguments {
                arguments,
                bindings,
            },
            remaining: cursor,
        });
        Ok(())
    }

    fn begin_fallback(&self, context: &mut Context<'db, 'arg, 'call, S>) -> Result<(), Infallible> {
        *context.input.storage.parts_mut().0 = context.baseline.clone();
        Ok(())
    }

    fn contexts(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        candidates: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Infallible> {
        let (arguments, bindings) = context.parts();
        Ok(collect_contexts(
            builders.builder(context.builder()),
            arguments,
            bindings,
            candidates.then_some(&context.candidates),
            &context.constraints,
            context.tcx(),
        ))
    }

    fn baseline_contexts(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        candidates: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Infallible> {
        Ok(collect_contexts(
            builders.builder(context.builder()),
            &context.baseline,
            context.parts().1,
            candidates.then_some(&context.candidates),
            &context.constraints,
            context.tcx(),
        ))
    }

    fn start_simple(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        speculative: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Infallible> {
        Ok(start_simple(context, contexts, speculative, builders))
    }

    fn start_unified(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        mode: CallArgumentInferenceMode,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Infallible> {
        let (arguments, bindings) = context.parts();
        let next_bindings = bindings.clone();
        let previous_arguments = arguments.clone();
        let next_arguments = arguments.clone();
        let builder = builders.speculate(context.builder(), false);
        Ok(Pass {
            builder,
            arguments: context.input.arguments.clone().enumerate(),
            contexts,
            mode,
            after: AfterPass::Unified(Unified {
                iteration: 0,
                next_bindings,
                previous_arguments,
                next_arguments,
            }),
        })
    }

    fn next_argument(
        &self,
        pass: &mut Pass<'db, 'arg, 'call>,
    ) -> Result<Option<ArgumentItem<'db, 'arg>>, Infallible> {
        let Some((index, argument)) = pass.arguments.next() else {
            return Ok(None);
        };
        // Splats were inferred before parameter matching to determine their length.
        // TODO: Re-infer splatted arguments with their type context.
        if argument.is_variadic() {
            return Ok(Some(ArgumentItem::Skip));
        }
        let expression = argument.value();
        Ok(Some(match &pass.contexts[index] {
            None => ArgumentItem::Skip,
            Some(MatchingArgumentTypeContext::Unique(context)) => ArgumentItem::Unique {
                index,
                expression,
                context: *context,
            },
            Some(MatchingArgumentTypeContext::Many(_)) => ArgumentItem::Many { index, expression },
        }))
    }

    fn unique_request(
        &self,
        context: Context<'db, 'arg, 'call, S>,
        pass: Pass<'db, 'arg, 'call>,
        index: usize,
        expression: &'arg ast::Expr,
        argument_context: Option<ArgumentTypeContext<'db>>,
    ) -> Result<Action<'db, 'arg, 'call, S>, Infallible> {
        Ok(unique_request(
            context,
            pass,
            index,
            expression,
            argument_context,
        ))
    }

    fn many(
        &self,
        index: usize,
        expression: &'arg ast::Expr,
    ) -> Result<Many<'db, 'arg>, Infallible> {
        Ok(Many {
            index,
            expression,
            cursor: 0,
            inferred: FxHashMap::default(),
            teardown_cache: false,
        })
    }

    fn default_request(
        &self,
        context: Context<'db, 'arg, 'call, S>,
        pass: Pass<'db, 'arg, 'call>,
        many: Many<'db, 'arg>,
    ) -> Result<Action<'db, 'arg, 'call, S>, Infallible> {
        Ok(default_request(context, pass, many))
    }

    fn install_many_cache(
        &self,
        pass: &Pass<'db, 'arg, 'call>,
        many: &mut Many<'db, 'arg>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        // Repeated speculative contexts share cached expressions to avoid exponential work
        // when generic calls are nested.
        many.teardown_cache = builders.setup_expression_cache(pass.builder);
        Ok(())
    }

    fn next_context(
        &self,
        pass: &Pass<'db, 'arg, 'call>,
        many: &mut Many<'db, 'arg>,
    ) -> Result<Option<Option<ArgumentTypeContext<'db>>>, Infallible> {
        let context = match &pass.contexts[many.index] {
            Some(MatchingArgumentTypeContext::Many(contexts)) => contexts.get(many.cursor).copied(),
            _ => None,
        };
        many.cursor += usize::from(context.is_some());
        Ok(context)
    }

    fn insert_cached(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        pass: &mut Pass<'db, 'arg, 'call>,
        many: &Many<'db, 'arg>,
        argument_context: Option<ArgumentTypeContext<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        insert_argument(
            pass.arguments_mut(context),
            many.index,
            argument_context,
            ty,
        );
        Ok(())
    }

    fn contextual_request(
        &self,
        context: Context<'db, 'arg, 'call, S>,
        pass: Pass<'db, 'arg, 'call>,
        many: Many<'db, 'arg>,
        argument_context: Option<ArgumentTypeContext<'db>>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Action<'db, 'arg, 'call, S>, Infallible> {
        let tcx = argument_context
            .map(ArgumentTypeContext::type_context)
            .unwrap_or_default();
        let builder = builders.speculate(pass.builder, false);
        let policy = context.input.policy;
        let argument = (many.index, many.expression, tcx);
        Ok(Action::Infer {
            pending: Pending {
                context,
                pass,
                return_to: ReturnTo::Context {
                    many,
                    context: argument_context,
                    speculative: builder,
                },
            },
            builder,
            argument,
            policy,
        })
    }

    fn teardown_many_cache(
        &self,
        pass: &Pass<'db, 'arg, 'call>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        builders.get_mut(pass.builder).teardown_expression_cache();
        Ok(())
    }

    fn finish_pass(
        &self,
        pass: Pass<'db, 'arg, 'call>,
    ) -> Result<FinishedPass<'db, 'call>, Infallible> {
        let Pass {
            builder,
            contexts,
            after,
            ..
        } = pass;
        Ok(FinishedPass {
            builder,
            contexts,
            after,
        })
    }

    fn check_active(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
        _recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<Result<(), CallErrorKind>, Infallible> {
        let builder = builders.builder(context.builder());
        let tcx = context.tcx();
        let Context {
            input,
            active,
            constraints,
            ..
        } = context;
        let (arguments, bindings) = match active {
            Active::Root => input.storage.parts_mut(),
            Active::Narrow(trial) => trial.storage.parts_mut(),
        };
        Ok(bindings.check_types_impl(
            builder.db(),
            builder.program_environment(),
            constraints,
            arguments,
            tcx,
            &builder.dataclass_field_specifiers,
            CheckTypesMode::Finalize,
        ))
    }

    fn simple_committed(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        speculative: BuilderId,
        result: Result<(), CallErrorKind>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Infallible> {
        let checked_arguments = context.parts().0.clone();
        let baseline = context.baseline.clone();
        *context.parts_mut().0 = baseline;
        // Re-infer after overload evaluation so only matching overloads contribute expressions
        // and diagnostics. Preserve the result of the speculative binding check.
        let contexts = collect_contexts(
            builders.builder(context.builder()),
            &checked_arguments,
            context.parts().1,
            None,
            &context.constraints,
            context.tcx(),
        );
        Ok(Pass {
            builder: context.builder(),
            arguments: context.input.arguments.clone().enumerate(),
            contexts,
            mode: CallArgumentInferenceMode::Commit,
            after: AfterPass::SimpleCommitted {
                speculative,
                result,
            },
        })
    }

    fn merge_expected(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        speculative: BuilderId,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        let speculative = builders.take_speculative(speculative);
        builders
            .get_mut(context.builder())
            .union_expected_types(&speculative.expected_types);
        Ok(())
    }

    fn check_unified(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: &mut Unified<'call, 'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
        _recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<(), Infallible> {
        let builder = builders.builder(context.builder());
        unified.next_bindings = context.parts().1.clone();
        let _ = unified.next_bindings.check_types_impl(
            builder.db(),
            builder.program_environment(),
            &context.constraints,
            &unified.next_arguments,
            context.tcx(),
            &builder.dataclass_field_specifiers,
            CheckTypesMode::Provisional,
        );
        Ok(())
    }

    fn inferred_converged(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: &Unified<'call, 'db>,
    ) -> Result<bool, Infallible> {
        Ok(unified
            .next_arguments
            .inferred_types_equal_at(&unified.previous_arguments, &context.generic_arguments))
    }

    fn contexts_converged(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        previous: &[Option<MatchingArgumentTypeContext<'db>>],
        next: &[Option<MatchingArgumentTypeContext<'db>>],
    ) -> Result<bool, Infallible> {
        Ok(context
            .generic_arguments
            .iter()
            .all(|&index| previous.get(index) == next.get(index)))
    }

    fn unified_contexts(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: &Unified<'call, 'db>,
        candidates: bool,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, Infallible> {
        Ok(collect_contexts(
            builders.builder(context.builder()),
            &unified.next_arguments,
            &unified.next_bindings,
            candidates.then_some(&context.candidates),
            &context.constraints,
            context.tcx(),
        ))
    }

    fn next_iteration(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: Unified<'call, 'db>,
        contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        mode: CallArgumentInferenceMode,
        previous_builder: BuilderId,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Pass<'db, 'arg, 'call>, Infallible> {
        let Unified {
            iteration,
            next_bindings,
            next_arguments: previous_arguments,
            ..
        } = unified;
        drop(builders.take_speculative(previous_builder));
        let next_arguments = context.parts().0.clone();
        let builder = builders.speculate(context.builder(), false);
        Ok(Pass {
            builder,
            arguments: context.input.arguments.clone().enumerate(),
            contexts,
            mode,
            after: AfterPass::Unified(Unified {
                iteration: iteration + 1,
                next_bindings,
                previous_arguments,
                next_arguments,
            }),
        })
    }

    fn finalize_unified(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: &mut Unified<'call, 'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Result<(), CallErrorKind>, Infallible> {
        let builder = builders.builder(context.builder());
        Ok(unified.next_bindings.finalize_argument_inference(
            builder.db(),
            builder.program_environment(),
            &unified.next_arguments,
            &builder.dataclass_field_specifiers,
        ))
    }

    fn unified_committed(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        unified: Unified<'call, 'db>,
        contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
        speculative: BuilderId,
        result: Result<(), CallErrorKind>,
    ) -> Result<Pass<'db, 'arg, 'call>, Infallible> {
        Ok(Pass {
            builder: context.builder(),
            arguments: context.input.arguments.clone().enumerate(),
            contexts,
            mode: CallArgumentInferenceMode::Commit,
            after: AfterPass::UnifiedCommitted {
                speculative,
                bindings: unified.next_bindings,
                arguments: unified.next_arguments,
                result,
            },
        })
    }

    fn commit_unified(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        unified: Unified<'call, 'db>,
        speculative: BuilderId,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        *context.parts_mut().0 = unified.next_arguments;
        let speculative = builders.take_speculative(speculative);
        builders.get_mut(context.builder()).extend(speculative);
        *context.parts_mut().1 = unified.next_bindings;
        Ok(())
    }

    fn commit_bindings(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        bindings: Bindings<'db>,
        _arguments: CallArguments<'call, 'db>,
    ) -> Result<(), Infallible> {
        *context.parts_mut().1 = bindings;
        Ok(())
    }

    fn accepts_trial(
        &self,
        context: &Context<'db, 'arg, 'call, S>,
        trial: &Trial<'call, 'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<bool, Infallible> {
        let builder = builders.builder(context.input.builder);
        let db = builder.db();
        let env = builder.program_environment();
        let return_ty = trial.storage.bindings.return_type(db, env);
        // A literal result can span several members of the declared union. Subtyping against
        // the complete union avoids another trial without accepting a gradual alternative.
        Ok(trial.tcx.annotation.is_some_and(|narrowed_ty| {
            return_ty.is_assignable_to(db, env, narrowed_ty)
                || (return_ty
                    .resolve_type_alias(db)
                    .is_literal_or_union_of_literals(db, env)
                    && context
                        .input
                        .tcx
                        .annotation
                        .is_some_and(|declared_ty| return_ty.is_subtype_of(db, env, declared_ty)))
        }))
    }

    fn finish_attempt(
        &self,
        context: Context<'db, 'arg, 'call, S>,
    ) -> Result<FinishedAttempt<'db, 'arg, 'call, S>, Infallible> {
        Ok(finish_attempt(context))
    }

    fn discard_trial(
        &self,
        trial: Trial<'call, 'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<NarrowCursor<'db>, Infallible> {
        drop(builders.take_speculative(trial.builder));
        Ok(trial.remaining)
    }

    fn commit_trial(
        &self,
        context: &mut Context<'db, 'arg, 'call, S>,
        trial: Trial<'call, 'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<(), Infallible> {
        let (arguments, bindings) = context.input.storage.parts_mut();
        *bindings = trial.storage.bindings;
        *arguments = trial.storage.arguments;
        let speculative = builders.take_speculative(trial.builder);
        builders.get_mut(context.input.builder).extend(speculative);
        Ok(())
    }

    fn complete(
        &self,
        context: Context<'db, 'arg, 'call, S>,
        result: Result<(), CallErrorKind>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<Action<'db, 'arg, 'call, S>, Infallible> {
        Ok(complete(context, result, builders))
    }

    fn resume_unique(
        &self,
        mut context: Context<'db, 'arg, 'call, S>,
        mut pass: Pass<'db, 'arg, 'call>,
        index: usize,
        argument_context: Option<ArgumentTypeContext<'db>>,
        ty: Type<'db>,
    ) -> Result<State<'db, 'arg, 'call, S>, Infallible> {
        insert_argument(
            pass.arguments_mut(&mut context),
            index,
            argument_context,
            ty,
        );
        Ok(State::Pass(context, pass))
    }

    fn resume_default(
        &self,
        mut context: Context<'db, 'arg, 'call, S>,
        mut pass: Pass<'db, 'arg, 'call>,
        mut many: Many<'db, 'arg>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<State<'db, 'arg, 'call, S>, Infallible> {
        pass.arguments_mut(&mut context)
            .insert_type(many.index, TypeContext::default(), ty);
        many.inferred.insert(None, ty);
        many.teardown_cache = builders.setup_expression_cache(pass.builder);
        Ok(State::Many(context, pass, many))
    }

    fn resume_context(
        &self,
        mut context: Context<'db, 'arg, 'call, S>,
        mut pass: Pass<'db, 'arg, 'call>,
        mut many: Many<'db, 'arg>,
        argument_context: Option<ArgumentTypeContext<'db>>,
        speculative: BuilderId,
        ty: Type<'db>,
        builders: &mut BuilderStore<'root, 'db, 'ast>,
    ) -> Result<State<'db, 'arg, 'call, S>, Infallible> {
        insert_argument(
            pass.arguments_mut(&mut context),
            many.index,
            argument_context,
            ty,
        );
        many.inferred.insert(
            argument_context.map(ArgumentTypeContext::inference_cache_key),
            ty,
        );
        let speculative = builders.take_speculative(speculative);
        builders
            .get_mut(pass.builder)
            .union_expected_types(&speculative.expected_types);
        Ok(State::Many(context, pass, many))
    }
}

fn start_simple<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B>(
    context: &Context<'db, 'arg, 'call, S, B>,
    contexts: Vec<Option<MatchingArgumentTypeContext<'db>>>,
    speculative: bool,
    builders: &mut BuilderStore<'root, 'db, 'ast>,
) -> Pass<'db, 'arg, 'call> {
    let (builder, mode, after) = if speculative {
        (
            builders.speculate(context.builder(), false),
            CallArgumentInferenceMode::Speculate,
            AfterPass::SimpleSpeculative,
        )
    } else {
        (
            context.builder(),
            CallArgumentInferenceMode::Commit,
            AfterPass::Direct,
        )
    };
    Pass {
        builder,
        arguments: context.input.arguments.clone().enumerate(),
        contexts,
        mode,
        after,
    }
}

fn unique_request<'db, 'arg, 'call, S, B>(
    context: Context<'db, 'arg, 'call, S, B>,
    pass: Pass<'db, 'arg, 'call>,
    index: usize,
    expression: &'arg ast::Expr,
    argument_context: Option<ArgumentTypeContext<'db>>,
) -> Action<'db, 'arg, 'call, S, B> {
    let builder = pass.builder;
    let policy = context.input.policy;
    let tcx = argument_context
        .map(ArgumentTypeContext::type_context)
        .unwrap_or_default();
    Action::Infer {
        pending: Pending {
            context,
            pass,
            return_to: ReturnTo::Unique {
                index,
                context: argument_context,
            },
        },
        builder,
        argument: (index, expression, tcx),
        policy,
    }
}

fn default_request<'db, 'arg, 'call, S, B>(
    context: Context<'db, 'arg, 'call, S, B>,
    pass: Pass<'db, 'arg, 'call>,
    many: Many<'db, 'arg>,
) -> Action<'db, 'arg, 'call, S, B> {
    let builder = pass.builder;
    let policy = context.input.policy;
    let argument = (many.index, many.expression, TypeContext::default());
    Action::Infer {
        pending: Pending {
            context,
            pass,
            return_to: ReturnTo::Default(many),
        },
        builder,
        argument,
        policy,
    }
}

fn finish_attempt<'db, 'arg, 'call, S, B>(
    context: Context<'db, 'arg, 'call, S, B>,
) -> FinishedAttempt<'db, 'arg, 'call, S, B> {
    let Context {
        active,
        input,
        baseline,
        constraints,
        candidates,
        generic_arguments,
        typevar_occurrences,
        has_generic_context,
        teardown_cache,
    } = context;
    let context = Context {
        active: Active::Root,
        input,
        baseline,
        constraints,
        candidates,
        generic_arguments,
        typevar_occurrences,
        has_generic_context,
        teardown_cache,
    };
    match active {
        Active::Root => FinishedAttempt::Root(context),
        Active::Narrow(trial) => FinishedAttempt::Narrow(context, trial),
    }
}

fn complete<'root, 'db, 'ast, 'arg, 'call, S, B>(
    context: Context<'db, 'arg, 'call, S, B>,
    result: Result<(), CallErrorKind>,
    builders: &mut BuilderStore<'root, 'db, 'ast>,
) -> Action<'db, 'arg, 'call, S, B> {
    if context.teardown_cache {
        builders
            .get_mut(context.input.builder)
            .teardown_expression_cache();
    }
    Action::Complete {
        storage: context.input.storage,
        result,
    }
}

fn insert_argument<'db>(
    arguments: &mut CallArguments<'_, 'db>,
    index: usize,
    context: Option<ArgumentTypeContext<'db>>,
    ty: Type<'db>,
) {
    if let Some(context) = context {
        context.insert_inferred_type_into(arguments, index, ty);
    } else {
        arguments.insert_type(index, TypeContext::default(), ty);
    }
}

fn collect_contexts<'db>(
    builder: &TypeInferenceBuilder<'db, '_>,
    arguments: &CallArguments<'_, 'db>,
    bindings: &Bindings<'db>,
    candidates: Option<&OverloadSet>,
    constraints: &ConstraintSetBuilder<'db>,
    tcx: TypeContext<'db>,
) -> Vec<Option<MatchingArgumentTypeContext<'db>>> {
    crate::types::signatures::effects::legacy_inline(preparation::collect_contexts_with(
        builder,
        arguments,
        bindings,
        candidates,
        constraints,
        tcx,
        &InlineArgumentEffects,
    ))
}

pub(super) fn advance<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>>(
    state: State<'db, 'arg, 'call, S>,
    builders: &mut BuilderStore<'root, 'db, 'ast>,
) -> Action<'db, 'arg, 'call, S> {
    let Ok(action) = advance_sync(state, builders, None, ArgumentFacts, &InlineArgumentEffects);
    action
}

pub(super) fn resume<'root, 'db, 'ast, 'arg, 'call, S: ArgumentStorage<'call, 'db>>(
    pending: Pending<'db, 'arg, 'call, S>,
    ty: Type<'db>,
    builders: &mut BuilderStore<'root, 'db, 'ast>,
) -> State<'db, 'arg, 'call, S> {
    let Ok(state) = resume_sync(pending, ty, builders, &InlineArgumentEffects);
    state
}

/// Describes retained ownership without constructing or changing an inference phase.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ObservedPass {
    Direct,
    SimpleSpeculative,
    SimpleCommitted,
    Unified,
    UnifiedCommitted,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Observation {
    pub(super) input: BuilderId,
    pub(super) active: BuilderId,
    pub(super) narrow: Option<BuilderId>,
    pub(super) pass: BuilderId,
    pub(super) kind: ObservedPass,
    pub(super) predecessor: Option<BuilderId>,
    pub(super) semantic_result: Option<Result<(), CallErrorKind>>,
    pub(super) contextual_child: Option<BuilderId>,
}

#[cfg(test)]
impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> Context<'db, 'arg, 'call, S, B> {
    fn observe_pass(&self, pass: &Pass<'db, 'arg, 'call>) -> Observation {
        let (kind, predecessor, semantic_result) = match &pass.after {
            AfterPass::Direct => (ObservedPass::Direct, None, None),
            AfterPass::SimpleSpeculative => (ObservedPass::SimpleSpeculative, None, None),
            AfterPass::SimpleCommitted {
                speculative,
                result,
            } => (
                ObservedPass::SimpleCommitted,
                Some(*speculative),
                Some(*result),
            ),
            AfterPass::Unified(_) => (ObservedPass::Unified, None, None),
            AfterPass::UnifiedCommitted {
                speculative,
                result,
                ..
            } => (
                ObservedPass::UnifiedCommitted,
                Some(*speculative),
                Some(*result),
            ),
        };
        Observation {
            input: self.input.builder,
            active: self.builder(),
            narrow: match &self.active {
                Active::Root => None,
                Active::Narrow(trial) => Some(trial.builder),
            },
            pass: pass.builder,
            kind,
            predecessor,
            semantic_result,
            contextual_child: None,
        }
    }
}

#[cfg(test)]
impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> State<'db, 'arg, 'call, S, B> {
    pub(super) fn finished_root_result(&self) -> Option<Result<(), CallErrorKind>> {
        match self {
            Self::Finished(context, result) if matches!(context.active, Active::Root) => {
                Some(*result)
            }
            _ => None,
        }
    }

    pub(super) fn observe(&self) -> Option<Observation> {
        match self {
            Self::Pass(context, pass) | Self::Many(context, pass, _) => {
                Some(context.observe_pass(pass))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> Pending<'db, 'arg, 'call, S, B> {
    pub(super) fn observe(&self) -> Observation {
        let mut observation = self.context.observe_pass(&self.pass);
        if let ReturnTo::Context { speculative, .. } = &self.return_to {
            observation.contextual_child = Some(*speculative);
        }
        observation
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreparedRequest {
    Unique,
    Contextual,
}

#[cfg(test)]
impl<'db, 'arg, 'call, S: ArgumentStorage<'call, 'db>, B> State<'db, 'arg, 'call, S, B> {
    /// Recognizes transitions that only move the next prepared argument into a pending owner.
    /// Checking bindings, collecting contexts, and all expression inference remain unavailable.
    pub(super) fn prepared_request(&self) -> Option<PreparedRequest> {
        match self {
            Self::Pass(_, pass) => {
                let (index, argument) = pass.arguments.clone().next()?;
                (!argument.is_variadic()
                    && matches!(
                        pass.contexts.get(index),
                        Some(Some(MatchingArgumentTypeContext::Unique(_)))
                    ))
                .then_some(PreparedRequest::Unique)
            }
            Self::Many(_, pass, many) => {
                let Some(MatchingArgumentTypeContext::Many(contexts)) =
                    pass.contexts.get(many.index)?
                else {
                    return None;
                };
                let context = contexts.get(many.cursor)?;
                (!many
                    .inferred
                    .contains_key(&context.map(ArgumentTypeContext::inference_cache_key)))
                .then_some(PreparedRequest::Contextual)
            }
            _ => None,
        }
    }
}
