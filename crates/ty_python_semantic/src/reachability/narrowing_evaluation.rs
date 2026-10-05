//! Ordered evaluation of projected narrowing graphs with an explicit continuation stack.
//!
//! The stack removes recursion between graph nodes. Constraint operations and type unions remain
//! separate semantic effects: making the traversal iterative does not bound those operations.

use std::convert::Infallible;

use smallvec::{SmallVec, smallvec};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::predicate::ScopedPredicateId;

use super::{
    NarrowingProjector, ProjectedNarrowingContext, ProjectedNarrowingEntry, ProjectedNarrowingNode,
    ProjectedNarrowingNodeId, accumulate_constraint, apply_accumulated_narrowing,
};
use crate::types::{NarrowingConstraint, Type, UnionType};

pub(crate) struct NarrowingEvaluationFacts;
pub(super) struct OrdinaryNarrowingEvaluationEffects;

#[derive(Clone, Copy)]
pub(crate) enum EvaluationMode {
    Path,
    JoinSuffix,
}

enum Branches {
    True,
    False,
    All,
}

/// Continuations contain flat constraint payloads, never another continuation or future.
pub(crate) enum Frame<'db> {
    Evaluate(
        ProjectedNarrowingNodeId,
        Option<NarrowingConstraint<'db>>,
        EvaluationMode,
    ),
    FinishJoin {
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
    },
    AfterTrue {
        if_uncertain: ProjectedNarrowingNodeId,
        if_false: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
        negative: Option<NarrowingConstraint<'db>>,
    },
    AfterUncertain {
        if_false: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
        negative: Option<NarrowingConstraint<'db>>,
        true_ty: Type<'db>,
    },
    AfterFalse {
        true_ty: Type<'db>,
        uncertain_ty: Type<'db>,
    },
}

/// Owns suspended graph evaluations so interruption retires a flat sequence of frames.
pub(crate) struct Evaluation<'db> {
    pub(crate) frames: SmallVec<[Frame<'db>; 4]>,
    #[cfg(test)]
    pub(crate) _lifetime: Option<evaluation_observations::OwnerLifetime>,
}

#[cfg(test)]
pub(crate) mod evaluation_observations {
    use std::cell::Cell;

    use crate::Db;

    thread_local! {
        static LIVE: Cell<usize> = const { Cell::new(0) };
        static ENTERED: Cell<usize> = const { Cell::new(0) };
        static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static CANCEL: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn reset(cancel: bool) {
        assert_eq!(LIVE.get(), 0);
        ENTERED.set(0);
        REMAINING.set(None);
        CANCEL.set(cancel);
    }

    pub(crate) fn progress() -> (usize, usize, Option<usize>) {
        (LIVE.get(), ENTERED.get(), REMAINING.get())
    }

    pub(crate) struct OwnerLifetime;

    impl Drop for OwnerLifetime {
        fn drop(&mut self) {
            LIVE.set(LIVE.get() - 1);
        }
    }

    pub(crate) fn owner_ready(db: &dyn Db) -> OwnerLifetime {
        LIVE.set(LIVE.get() + 1);
        ENTERED.set(ENTERED.get() + 1);
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        if CANCEL.replace(false) {
            db.cancellation_token().cancel();
        }
        OwnerLifetime
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousNarrowingEvaluationEffects)]
    pub(crate) trait NarrowingEvaluationEffects<'db> {
        type Error;

        #[operation(local)]
        async fn record_root(&self, projector: &mut NarrowingProjector<'_, 'db>, root: ProjectedNarrowingNodeId) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn root_cached(&self, projector: &NarrowingProjector<'_, 'db>, root: ProjectedNarrowingNodeId, base_ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn context<'a>(&self, projector: &'a mut NarrowingProjector<'_, 'db>, base_ty: Type<'db>) -> Result<ProjectedNarrowingContext<'a, 'db>, Self::Error>;
        #[operation(source)]
        async fn evaluate(&self, context: &mut ProjectedNarrowingContext<'_, 'db>, root: ProjectedNarrowingNodeId) -> Result<Type<'db>, Self::Error>;

        // Storage operations include allocation and retirement. A controlled provider must fund
        // retained payloads and their eventual disposal before creating or retaining them.
        #[operation(local)]
        async fn start(&self, id: ProjectedNarrowingNodeId, accumulated: Option<NarrowingConstraint<'db>>) -> Result<Evaluation<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, evaluation: &mut Evaluation<'db>) -> Result<Option<Frame<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, evaluation: &mut Evaluation<'db>, frame: Frame<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, evaluation: Evaluation<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn discard(&self, constraint: Option<NarrowingConstraint<'db>>) -> Result<(), Self::Error>;

        #[operation(local)]
        async fn is_join(&self, context: &ProjectedNarrowingContext<'_, 'db>, id: ProjectedNarrowingNodeId) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn cached(&self, context: &ProjectedNarrowingContext<'_, 'db>, id: ProjectedNarrowingNodeId) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn cache(&self, context: &mut ProjectedNarrowingContext<'_, 'db>, id: ProjectedNarrowingNodeId, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn node(&self, context: &ProjectedNarrowingContext<'_, 'db>, id: ProjectedNarrowingNodeId) -> Result<ProjectedNarrowingEntry<'db>, Self::Error>;

        // These boundaries require complete semantic operations or explicit refusal. Their
        // ordinary implementations can clone and traverse variable-sized DNF payloads or invoke
        // canonical type-algebra queries; a controlled provider cannot treat them as local work.
        #[operation(source)]
        async fn predicate_constraints(&self, context: &ProjectedNarrowingContext<'_, 'db>, id: ScopedPredicateId) -> Result<(Option<NarrowingConstraint<'db>>, Option<NarrowingConstraint<'db>>), Self::Error>;
        #[operation(source)]
        async fn clone_constraint(&self, constraint: &Option<NarrowingConstraint<'db>>) -> Result<Option<NarrowingConstraint<'db>>, Self::Error>;
        #[operation(source)]
        async fn accumulate(&self, accumulated: Option<NarrowingConstraint<'db>>, new: Option<NarrowingConstraint<'db>>) -> Result<Option<NarrowingConstraint<'db>>, Self::Error>;
        #[operation(source)]
        async fn apply(&self, context: &ProjectedNarrowingContext<'_, 'db>, base_ty: Type<'db>, accumulated: Option<NarrowingConstraint<'db>>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union(&self, context: &ProjectedNarrowingContext<'_, 'db>, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl NarrowingEvaluationFacts {
        fn base_type<'db>(&self, context: &ProjectedNarrowingContext<'_, 'db>) -> Type<'db> {
            context.base_ty
        }

        fn unreachable(&self, id: ProjectedNarrowingNodeId) -> bool {
            id == ProjectedNarrowingNodeId::ALWAYS_FALSE
        }

        fn unconstrained(&self, id: ProjectedNarrowingNodeId) -> bool {
            id == ProjectedNarrowingNodeId::ALWAYS_TRUE
        }

        fn branches(&self, node: ProjectedNarrowingNode) -> Branches {
            if node.if_true == ProjectedNarrowingNodeId::ALWAYS_FALSE
                && node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_FALSE
            {
                Branches::False
            } else if node.if_false == ProjectedNarrowingNodeId::ALWAYS_FALSE
                && node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_FALSE
            {
                Branches::True
            } else {
                Branches::All
            }
        }
    }

    #[synchronous(narrow_projected_sync)]
    #[capabilities(effects = NarrowingEvaluationEffects, facts = NarrowingEvaluationFacts)]
    #[passive_values(Type::Never)]
    pub(crate) async fn narrow_projected_with<'db, E: NarrowingEvaluationEffects<'db>>(
        projector: &mut NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
        facts: NarrowingEvaluationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if facts.unconstrained(root) {
            return Ok(base_ty);
        }
        if facts.unreachable(root) {
            return Ok(Type::Never);
        }

        // A repeated binding root can become a join even when its result is already cached.
        effects.record_root(projector, root).await?;
        if let Some(cached) = effects.root_cached(projector, root, base_ty).await? {
            return Ok(cached);
        }

        let mut context = effects.context(projector, base_ty).await?;
        let narrowed = effects.evaluate(&mut context, root).await?;
        effects.cache(&mut context, root, narrowed).await?;
        Ok(narrowed)
    }

    #[synchronous(evaluate_sync)]
    #[capabilities(effects = NarrowingEvaluationEffects, facts = NarrowingEvaluationFacts)]
    #[passive_values(Frame::Evaluate, Frame::FinishJoin, Frame::AfterTrue, Frame::AfterUncertain, Frame::AfterFalse, EvaluationMode::Path, EvaluationMode::JoinSuffix, Type::Never)]
    pub(crate) async fn evaluate_with<'db, E: NarrowingEvaluationEffects<'db>>(
        context: &mut ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
        facts: NarrowingEvaluationFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut evaluation = effects.start(id, accumulated).await?;
        // Each continuation follows a child evaluation, which supplies its current result.
        #[passive_state]
        let mut narrowed = facts.base_type(context);

        #[cursor_loop]
        while let Some(frame) = effects.next(&mut evaluation).await? {
            match frame {
                Frame::Evaluate(id, accumulated, mode) => {
                    let is_join = match mode {
                        EvaluationMode::Path => effects.is_join(context, id).await?,
                        EvaluationMode::JoinSuffix => false,
                    };
                    if is_join {
                        // Preserve replacement narrowing order at a join: evaluate the shared
                        // suffix once, then apply the incoming prefix to its narrowed type.
                        match effects.cached(context, id).await? {
                            Some(suffix_ty) => {
                                narrowed = effects.apply(context, suffix_ty, accumulated).await?;
                            }
                            None => {
                                effects.push(&mut evaluation, Frame::FinishJoin { id, accumulated }).await?;
                                effects.push(&mut evaluation, Frame::Evaluate(id, None, EvaluationMode::JoinSuffix)).await?;
                            }
                        }
                        continue;
                    }

                    if facts.unreachable(id) {
                        effects.discard(accumulated).await?;
                        narrowed = Type::Never;
                        continue;
                    }
                    if facts.unconstrained(id) {
                        narrowed = effects.apply(context, facts.base_type(context), accumulated).await?;
                        continue;
                    }

                    let node = match effects.node(context, id).await? {
                        ProjectedNarrowingEntry::Predicate(node) => node,
                        ProjectedNarrowingEntry::Checkpoint { ty, .. } => {
                            narrowed = effects.apply(context, ty, accumulated).await?;
                            continue;
                        }
                    };
                    let (positive, negative) = effects.predicate_constraints(context, node.atom).await?;
                    match facts.branches(node) {
                        Branches::False => {
                            effects.discard(positive).await?;
                            let accumulated = effects.accumulate(accumulated, negative).await?;
                            effects.push(&mut evaluation, Frame::Evaluate(node.if_false, accumulated, EvaluationMode::Path)).await?;
                        }
                        Branches::True => {
                            effects.discard(negative).await?;
                            let accumulated = effects.accumulate(accumulated, positive).await?;
                            effects.push(&mut evaluation, Frame::Evaluate(node.if_true, accumulated, EvaluationMode::Path)).await?;
                        }
                        Branches::All => {
                            let true_accumulated = effects.clone_constraint(&accumulated).await?;
                            let true_accumulated = effects.accumulate(true_accumulated, positive).await?;
                            effects.push(&mut evaluation, Frame::AfterTrue {
                                if_uncertain: node.if_uncertain,
                                if_false: node.if_false,
                                accumulated,
                                negative,
                            }).await?;
                            effects.push(&mut evaluation, Frame::Evaluate(node.if_true, true_accumulated, EvaluationMode::Path)).await?;
                        }
                    }
                }
                Frame::FinishJoin { id, accumulated } => {
                    effects.cache(context, id, narrowed).await?;
                    narrowed = effects.apply(context, narrowed, accumulated).await?;
                }
                Frame::AfterTrue { if_uncertain, if_false, accumulated, negative } => {
                    let uncertain_accumulated = effects.clone_constraint(&accumulated).await?;
                    effects.push(&mut evaluation, Frame::AfterUncertain {
                        if_false,
                        accumulated,
                        negative,
                        true_ty: narrowed,
                    }).await?;
                    effects.push(&mut evaluation, Frame::Evaluate(if_uncertain, uncertain_accumulated, EvaluationMode::Path)).await?;
                }
                Frame::AfterUncertain { if_false, accumulated, negative, true_ty } => {
                    let false_accumulated = effects.accumulate(accumulated, negative).await?;
                    effects.push(&mut evaluation, Frame::AfterFalse { true_ty, uncertain_ty: narrowed }).await?;
                    effects.push(&mut evaluation, Frame::Evaluate(if_false, false_accumulated, EvaluationMode::Path)).await?;
                }
                Frame::AfterFalse { true_ty, uncertain_ty } => {
                    // Both canonical unions run after all three alternatives. Keep them
                    // left-associated to preserve their query keys and normalization order.
                    let true_or_uncertain = effects.union(context, true_ty, uncertain_ty).await?;
                    narrowed = effects.union(context, true_or_uncertain, narrowed).await?;
                }
            }
        }

        effects.finish(evaluation).await?;
        Ok(narrowed)
    }
}

impl<'db> SynchronousNarrowingEvaluationEffects<'db> for OrdinaryNarrowingEvaluationEffects {
    type Error = Infallible;

    fn record_root(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
    ) -> Result<(), Self::Error> {
        projector.graph.record_reference(root);
        Ok(())
    }

    fn root_cached(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(projector.narrowed_cache.get(&(root, base_ty)).copied())
    }

    fn context<'a>(
        &self,
        projector: &'a mut NarrowingProjector<'_, 'db>,
        base_ty: Type<'db>,
    ) -> Result<ProjectedNarrowingContext<'a, 'db>, Self::Error> {
        Ok(ProjectedNarrowingContext::new(projector, base_ty))
    }

    fn evaluate(
        &self,
        context: &mut ProjectedNarrowingContext<'_, 'db>,
        root: ProjectedNarrowingNodeId,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(context.narrow(root, None))
    }

    fn start(
        &self,
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> Result<Evaluation<'db>, Self::Error> {
        Ok(Evaluation {
            frames: smallvec![Frame::Evaluate(id, accumulated, EvaluationMode::Path)],
            #[cfg(test)]
            _lifetime: None,
        })
    }

    fn next(&self, evaluation: &mut Evaluation<'db>) -> Result<Option<Frame<'db>>, Self::Error> {
        Ok(evaluation.frames.pop())
    }

    fn push(&self, evaluation: &mut Evaluation<'db>, frame: Frame<'db>) -> Result<(), Self::Error> {
        evaluation.frames.push(frame);
        Ok(())
    }

    fn finish(&self, evaluation: Evaluation<'db>) -> Result<(), Self::Error> {
        drop(evaluation);
        Ok(())
    }

    fn discard(&self, constraint: Option<NarrowingConstraint<'db>>) -> Result<(), Self::Error> {
        drop(constraint);
        Ok(())
    }

    fn is_join(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> Result<bool, Self::Error> {
        Ok(!id.is_terminal() && context.graph.joins[id.0])
    }

    fn cached(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(context.join_cache.get(&(id, context.base_ty)).copied())
    }

    fn cache(
        &self,
        context: &mut ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        context.join_cache.insert((id, context.base_ty), ty);
        Ok(())
    }

    fn node(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> Result<ProjectedNarrowingEntry<'db>, Self::Error> {
        Ok(context.graph.node(id))
    }

    fn predicate_constraints(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<
        (
            Option<NarrowingConstraint<'db>>,
            Option<NarrowingConstraint<'db>>,
        ),
        Self::Error,
    > {
        Ok(context.graph.predicate_constraints_cache[&id].clone())
    }

    fn clone_constraint(
        &self,
        constraint: &Option<NarrowingConstraint<'db>>,
    ) -> Result<Option<NarrowingConstraint<'db>>, Self::Error> {
        Ok(constraint.clone())
    }

    fn accumulate(
        &self,
        accumulated: Option<NarrowingConstraint<'db>>,
        new: Option<NarrowingConstraint<'db>>,
    ) -> Result<Option<NarrowingConstraint<'db>>, Self::Error> {
        Ok(accumulate_constraint(accumulated, new))
    }

    fn apply(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        base_ty: Type<'db>,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(apply_accumulated_narrowing(
            context.db,
            context.env,
            base_ty,
            accumulated,
        ))
    }

    fn union(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(
            context.db,
            context.env,
            left,
            right,
        ))
    }
}
