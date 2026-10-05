//! Admitted ownership and genuine semantic descendants for ordered constraint application.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::narrow::application::{
    ApplicationEffects, ApplicationFacts, ConjunctionApplication, Conjuncts, ConstraintApplication,
    Disjuncts, conjunction_with, evaluate_with,
};
use crate::types::narrow::{Conjunctions, NarrowingConstraint, NarrowingOperation};
use crate::types::set_theoretic::builder::controlled_union::{
    UnionFacts, add_in_place_with, try_build_with,
};
use crate::types::{Type, UnionBuilder};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Apply an owned constraint whose producer has already admitted its eventual disposal.
    pub(in crate::types::infer) async fn apply_narrowing_constraint(
        &self,
        env: &ProgramEnvironment<'db>,
        mut constraint: NarrowingConstraint<'db>,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                ProgramEnvironment::from_program(program)
            })
            .await?;
        self.allocate_future(|| evaluate_with(std::mem::take(&mut constraint), &env, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ApplicationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start(
        &self,
        mut constraint: NarrowingConstraint<'db>,
    ) -> RunResult<ConstraintApplication<'db>> {
        self.local(ConstraintApplication::start_work(), 0, || {
            ConstraintApplication::new(std::mem::take(&mut constraint))
        })
        .await
    }

    async fn next_disjunct(
        &self,
        disjuncts: &mut Disjuncts<'db>,
    ) -> RunResult<Option<Conjunctions<'db>>> {
        self.local(size_of::<Conjunctions<'db>>() * 2 + 2, 0, || {
            disjuncts.next()
        })
        .await
    }

    async fn finish_disjuncts(&self, disjuncts: Disjuncts<'db>) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.admit_work(Self::checked(disjuncts.retirement_work())?)?;
                endpoint.check_completion()
            })
            .await;
        drop(disjuncts);
        Ok(())
    }

    async fn conjunction(
        &self,
        env: &ProgramEnvironment<'db>,
        mut conjunction: Conjunctions<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| conjunction_with(conjunction.take(), env, ApplicationFacts, self))
            .await?
            .await
    }

    async fn start_conjunction(
        &self,
        mut conjunction: Conjunctions<'db>,
    ) -> RunResult<ConjunctionApplication<'db>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = Self::checked(conjunction.application_start_work())?;
                endpoint.admit_work(work)?;
                endpoint.check_completion()?;
                Ok(ConjunctionApplication::new(conjunction.take()))
            })
            .await)
    }

    async fn next_operation(
        &self,
        conjuncts: &mut Conjuncts<'db>,
    ) -> RunResult<Option<NarrowingOperation<'db>>> {
        self.local(size_of::<NarrowingOperation<'db>>() * 2 + 1, 0, || {
            conjuncts.next()
        })
        .await
    }

    async fn finish_conjuncts(&self, conjuncts: Conjuncts<'db>) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.admit_work(Self::checked(conjuncts.retirement_work())?)?;
                endpoint.check_completion()
            })
            .await;
        drop(conjuncts);
        Ok(())
    }

    async fn new_union(&self, env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>> {
        self.local(size_of::<UnionBuilder<'db>>() * 2 + 1, 0, || {
            UnionBuilder::new(self.db(), env)
        })
        .await
    }

    async fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        add_in_place_with(union, ty, UnionFacts, self).await
    }

    async fn union_build(&self, union: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        Ok(try_build_with(union, UnionFacts, self)
            .await?
            .unwrap_or(Type::Never))
    }

    async fn intersection(
        &self,
        _env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.access
            .intersection_from_two_elements(left, right)
            .await
    }

    async fn generic_filtering(
        &self,
        _env: &ProgramEnvironment<'db>,
        _subject: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }
}
