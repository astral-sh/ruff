//! Controlled narrowing uses the shared entry, graph-construction and evaluation decisions.

mod application;
mod construction;
mod evaluation;
mod expression;

use ruff_index::IndexSlice;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::expression::Expression;
use ty_python_core::narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::{NarrowingEvaluator, PredicateNarrowingTargets};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::reachability::NarrowingProjector;
use crate::reachability::narrowing_construction::Frame;
use crate::reachability::narrowing_entry::{
    NarrowingEntryEffects, NarrowingEntryFacts, narrow_projector_with,
};
use crate::types::Type;
use crate::types::narrow::ExpressionNarrowingConstraints;
use crate::types::narrow::expression::produce_with;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn create_narrowing_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        constraints: &'map NarrowingConstraints,
        predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        predicate_narrowing_targets: &'map PredicateNarrowingTargets,
        place: ScopedPlaceId,
        base_ty: Type<'db>,
    ) -> RunResult<NarrowingProjector<'map, 'db>>
    where
        'db: 'map,
    {
        self.environment_program(env).await?;
        // Empty projector collections allocate no backing storage. Creation prepays disposal of
        // their fixed-size headers if a later target lookup or graph evaluation refuses.
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = Self::checked(
                    size_of::<NarrowingProjector<'map, 'db>>()
                        .checked_mul(2)
                        .and_then(|work| work.checked_add(3)),
                )?;
                endpoint.admit_work(work)?;
                endpoint.check_completion()?;
                Ok(NarrowingProjector::new(
                    self.db(),
                    env,
                    constraints,
                    predicates,
                    predicate_narrowing_targets,
                    place,
                    base_ty,
                ))
            })
            .await)
    }

    pub(in crate::types::infer) async fn expression_narrowing(
        &self,
        expression: Expression<'db>,
    ) -> RunResult<ExpressionNarrowingConstraints<'db>> {
        let file = self.expression_file(expression).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract(
                "prepared narrowing expression file is foreign",
            ));
        }
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() + 1, 0, || {
                ProgramEnvironment::from_file(file)
            })
            .await?;
        self.allocate_future(|| produce_with(self.db(), &env, &source.module, expression, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NarrowingEntryEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn create_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        evaluator: &NarrowingEvaluator<'map, 'db>,
        place: ScopedPlaceId,
        base_ty: Type<'db>,
    ) -> RunResult<NarrowingProjector<'map, 'db>>
    where
        'db: 'map,
    {
        self.create_narrowing_projector(
            env,
            evaluator.narrowing_constraints(),
            evaluator.predicates(),
            evaluator.predicate_narrowing_targets(),
            place,
            base_ty,
        )
        .await
    }

    async fn set_base_type(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        base_ty: Type<'db>,
    ) -> RunResult<()> {
        self.local(1, 0, || projector.set_base_type(base_ty)).await
    }

    async fn contains_place(
        &self,
        targets: &PredicateNarrowingTargets,
        place: ScopedPlaceId,
    ) -> RunResult<bool> {
        self.local(targets.contains_place_work(), 0, || {
            targets.contains_place(place)
        })
        .await
    }

    async fn narrow_projector(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        narrow_projector_with(projector, constraint, base_ty, NarrowingEntryFacts, self).await
    }

    async fn narrow_graph(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let root = self
            .build_narrowing_graph(
                projector,
                Frame::Project {
                    root: constraint,
                    use_root_checkpoint: true,
                },
            )
            .await?;
        self.evaluate_narrowing_graph(projector, root, base_ty)
            .await
    }

    async fn retire_projector(&self, projector: NarrowingProjector<'_, 'db>) -> RunResult<()> {
        // Construction already paid for disposal of the projector and its retained storage.
        self.work(1).await?;
        drop(projector);
        Ok(())
    }
}
