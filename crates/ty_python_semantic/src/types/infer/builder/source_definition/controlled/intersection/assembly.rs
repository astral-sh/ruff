//! Finalized branches enter the shared union builder in their original lazy order.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::Type;
use crate::types::set_theoretic::assembly::{TypeAssemblyEffects, TypeElements};
use crate::types::set_theoretic::builder::intersection_assembly::{
    IntersectionAssemblyEffects, IntersectionBranches, build_with,
};
use crate::types::set_theoretic::builder::intersection_distribution::DistributionEffects;
use crate::types::set_theoretic::builder::{InnerIntersectionBuilder, IntersectionBuilder};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::{Db, ProgramEnvironment};

struct IntersectionAssembly<'source, 'env, 'access, 'run, 'db: 'run, A> {
    source: &'source SourceEffects<'access, 'run, 'db, A>,
    env: &'env ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn intersection_build(
        &self,
        builder: &mut IntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                builder.environment().clone()
            })
            .await?;
        self.environment_program(&env).await?;
        let effects = IntersectionAssembly {
            source: self,
            env: &env,
        };
        self.allocate_future(|| build_with(builder, &effects, &effects))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> IntersectionAssemblyEffects<'db>
    for IntersectionAssembly<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn take_elements(
        &self,
        builder: &mut IntersectionBuilder<'db>,
    ) -> RunResult<IntersectionBranches<'db>> {
        DistributionEffects::take_branches(self.source, builder).await
    }

    async fn next_element(
        &self,
        branches: &mut IntersectionBranches<'db>,
    ) -> RunResult<Option<InnerIntersectionBuilder<'db>>> {
        DistributionEffects::next_candidate(self.source, branches).await
    }

    async fn build_inner(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .inner_intersection_build(self.env, builder)
            .await
    }

    async fn finish_elements(
        &self,
        branches: &mut Option<IntersectionBranches<'db>>,
    ) -> RunResult<()> {
        let branches = self
            .source
            .local(size_of::<IntersectionBranches<'db>>() * 2 + 1, 0, || {
                branches.take()
            })
            .await?;
        if let Some(branches) = branches {
            // The initial branch transfer prepays cleanup, including the original Vec backing
            // retained by an exhausted iterator. The retirement effect owns them across refusal.
            DistributionEffects::finish_branches(self.source, branches).await?;
        }
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeAssemblyEffects<'db>
    for IntersectionAssembly<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn union<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        let mut builder = PairUnionEffects::new_union(self.source, self.env).await?;
        PairUnionEffects::union_add(self.source, &mut builder, first.into()).await?;
        PairUnionEffects::union_add(self.source, &mut builder, second.into()).await?;
        while let Some(element) = remaining.next().await? {
            PairUnionEffects::union_add(self.source, &mut builder, element.into()).await?;
        }
        PairUnionEffects::union_build(self.source, builder).await
    }

    async fn intersection<I: TypeElements<'db, Error = RunError>>(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: I::Item,
        _second: I::Item,
        _remaining: &mut I,
    ) -> RunResult<Type<'db>> {
        self.source.unavailable(SourceOperation::Narrowing).await
    }
}
