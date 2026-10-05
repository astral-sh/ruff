//! Admitted continuations and cache ownership for the shared narrowing graph evaluator.

use rustc_hash::FxHashMap;
use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;
use ty_python_core::predicate::ScopedPredicateId;

use super::super::storage::{StorageQuote, slots};
use super::construction::{
    admit, checked, clone_predicate_constraints, insertion_quote, lookup_quote, sequence_quote,
};
use super::{SourceAccess, SourceEffects};
use crate::reachability::narrowing_evaluation::{
    Evaluation, EvaluationMode, Frame, NarrowingEvaluationEffects, NarrowingEvaluationFacts,
    evaluate_with, narrow_projected_with,
};
use crate::reachability::{
    NarrowingProjector, ProjectedNarrowingContext, ProjectedNarrowingEntry,
    ProjectedNarrowingNodeId,
};
use crate::types::narrow::admission::{clone_constraint, merge::merge_and};
use crate::types::{NarrowingConstraint, Type};

type NarrowedKey<'db> = (ProjectedNarrowingNodeId, Type<'db>);

fn take_frame<'db>(frame: &mut Frame<'db>) -> Frame<'db> {
    // The caller retains owned constraints until the storage quotation has been accepted.
    std::mem::replace(
        frame,
        Frame::Evaluate(
            ProjectedNarrowingNodeId::ALWAYS_FALSE,
            None,
            EvaluationMode::Path,
        ),
    )
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn evaluate_narrowing_graph(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            narrow_projected_with(projector, root, base_ty, NarrowingEvaluationFacts, self)
        })
        .await?
        .await
    }

    async fn cached_narrowed(
        &self,
        cache: &FxHashMap<NarrowedKey<'db>, Type<'db>>,
        retained_backing: usize,
        retained_key_bytes: usize,
        id: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<NarrowedKey<'db>>(
                    cache.capacity(),
                    retained_backing,
                    retained_key_bytes.max(base_ty.inline_payload_bytes()),
                ))?;
                admit(endpoint, quote)?;
                Ok(cache.get(&(id, base_ty)).copied())
            })
            .await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NarrowingEvaluationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn record_root(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
    ) -> RunResult<()> {
        self.local(4, 0, || projector.graph.record_reference(root))
            .await
    }

    async fn root_cached(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        root: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.cached_narrowed(
            &projector.narrowed_cache,
            projector.source_narrowed_backing,
            projector.source_narrowed_key_bytes,
            root,
            base_ty,
        )
        .await
    }

    async fn context<'a>(
        &self,
        projector: &'a mut NarrowingProjector<'_, 'db>,
        base_ty: Type<'db>,
    ) -> RunResult<ProjectedNarrowingContext<'a, 'db>> {
        self.local(
            size_of::<ProjectedNarrowingContext<'a, 'db>>() * 2 + 2,
            0,
            || ProjectedNarrowingContext::new(projector, base_ty),
        )
        .await
    }

    async fn evaluate(
        &self,
        context: &mut ProjectedNarrowingContext<'_, 'db>,
        root: ProjectedNarrowingNodeId,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| evaluate_with(context, root, None, NarrowingEvaluationFacts, self))
            .await?
            .await
    }

    async fn start(
        &self,
        id: ProjectedNarrowingNodeId,
        mut accumulated: Option<NarrowingConstraint<'db>>,
    ) -> RunResult<Evaluation<'db>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = checked(size_of::<Evaluation<'db>>().checked_mul(2))?;
                admit(endpoint, StorageQuote { work, bytes: 0 })?;
                let mut frames = SmallVec::new();
                frames.push(Frame::Evaluate(id, accumulated.take(), EvaluationMode::Path));
                Ok(Evaluation {
                    frames,
                    #[cfg(test)]
                    _lifetime: Some(
                        crate::reachability::narrowing_evaluation::evaluation_observations::owner_ready(
                            self.db(),
                        ),
                    ),
                })
            })
            .await)
    }

    async fn next(&self, evaluation: &mut Evaluation<'db>) -> RunResult<Option<Frame<'db>>> {
        self.local(size_of::<Frame<'db>>() * 2 + 2, 0, || {
            evaluation.frames.pop()
        })
        .await
    }

    async fn push(&self, evaluation: &mut Evaluation<'db>, mut frame: Frame<'db>) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, additional) = checked(sequence_quote::<Frame<'db>>(
                    evaluation.frames.len(),
                    evaluation.frames.capacity(),
                ))?;
                admit(endpoint, quote)?;
                evaluation.frames.reserve_exact(additional);
                evaluation.frames.push(take_frame(&mut frame));
                Ok(())
            })
            .await)
    }

    async fn finish(&self, evaluation: Evaluation<'db>) -> RunResult<()> {
        self.work(1).await?;
        drop(evaluation);
        Ok(())
    }

    async fn discard(&self, constraint: Option<NarrowingConstraint<'db>>) -> RunResult<()> {
        // Constraint producers prepay disposal, including cancellation between effects.
        self.work(size_of::<Option<NarrowingConstraint<'db>>>() * 2 + 1)
            .await?;
        drop(constraint);
        Ok(())
    }

    async fn is_join(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> RunResult<bool> {
        self.local(3, 0, || !id.is_terminal() && context.graph.joins[id.0])
            .await
    }

    async fn cached(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> RunResult<Option<Type<'db>>> {
        self.cached_narrowed(
            context.join_cache,
            *context.source_narrowed_backing,
            *context.source_narrowed_key_bytes,
            id,
            context.base_ty,
        )
        .await
    }

    async fn cache(
        &self,
        context: &mut ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let previous_backing = checked(slots(context.join_cache.capacity()))?
                    .max(*context.source_narrowed_backing);
                let key_bytes = (*context.source_narrowed_key_bytes)
                    .max(context.base_ty.inline_payload_bytes());
                let (quote, backing) = checked(insertion_quote::<NarrowedKey<'db>, Type<'db>>(
                    context.join_cache.len(),
                    context.join_cache.capacity(),
                    previous_backing,
                    key_bytes,
                ))?;
                admit(endpoint, quote)?;
                context.join_cache.reserve(1);
                context.join_cache.insert((id, context.base_ty), ty);
                *context.source_narrowed_backing = slots(context.join_cache.capacity())
                    .map_or(backing, |observed| previous_backing.max(observed));
                *context.source_narrowed_key_bytes = key_bytes;
                Ok(())
            })
            .await)
    }

    async fn node(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> RunResult<ProjectedNarrowingEntry<'db>> {
        self.local(size_of::<ProjectedNarrowingEntry<'db>>() + 1, 0, || {
            context.graph.node(id)
        })
        .await
    }

    async fn predicate_constraints(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<(
        Option<NarrowingConstraint<'db>>,
        Option<NarrowingConstraint<'db>>,
    )> {
        let endpoint = self.access.endpoint();
        let constraints = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<ScopedPredicateId>(
                    context.graph.predicate_constraints_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                context
                    .graph
                    .predicate_constraints_cache
                    .get(&id)
                    .ok_or(RunError::Contract("projected predicate has no constraints"))
            })
            .await;
        clone_predicate_constraints(endpoint, constraints).await
    }

    async fn clone_constraint(
        &self,
        constraint: &Option<NarrowingConstraint<'db>>,
    ) -> RunResult<Option<NarrowingConstraint<'db>>> {
        self.work(size_of::<Option<NarrowingConstraint<'db>>>() * 2 + 1)
            .await?;
        match constraint {
            Some(constraint) => Ok(Some(
                clone_constraint(self.access.endpoint(), constraint).await?,
            )),
            None => Ok(None),
        }
    }

    async fn accumulate(
        &self,
        accumulated: Option<NarrowingConstraint<'db>>,
        new: Option<NarrowingConstraint<'db>>,
    ) -> RunResult<Option<NarrowingConstraint<'db>>> {
        self.work(size_of::<Option<NarrowingConstraint<'db>>>() * 4 + 2)
            .await?;
        match (accumulated, new) {
            (Some(accumulated), Some(new)) => Ok(Some(
                merge_and(self.access.endpoint(), &new, accumulated).await?,
            )),
            (None, Some(new)) => Ok(Some(new)),
            (Some(accumulated), None) => Ok(Some(accumulated)),
            (None, None) => Ok(None),
        }
    }

    async fn apply(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        base_ty: Type<'db>,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> RunResult<Type<'db>> {
        self.work(size_of::<Option<NarrowingConstraint<'db>>>() * 2 + 1)
            .await?;
        let Some(accumulated) = accumulated else {
            return Ok(base_ty);
        };
        let base = self
            .local(size_of::<NarrowingConstraint<'db>>() * 2 + 1, 0, || {
                NarrowingConstraint::intersection(base_ty)
            })
            .await?;
        let constraint = merge_and(self.access.endpoint(), &base, accumulated).await?;
        self.apply_narrowing_constraint(context.env, constraint)
            .await
    }

    async fn union(
        &self,
        context: &ProjectedNarrowingContext<'_, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(context.env).await?;
        self.access.union_from_two_elements(left, right).await
    }
}
