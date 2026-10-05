//! Lazy final assembly of the inner branches of an intersection.

use std::convert::Infallible;

use super::{InnerIntersectionBuilder, IntersectionBuilder};
use crate::types::Type;
use crate::types::set_theoretic::assembly::{
    self, InlineTypeAssembly, TypeAssemblyEffects, TypeElements,
};
use crate::types::signatures::effects::legacy_inline;
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct IntersectionBranches<'db> {
    inner: std::vec::IntoIter<InnerIntersectionBuilder<'db>>,
    #[cfg(any(test, feature = "experimental-analysis"))]
    capacity: usize,
}

impl<'db> IntersectionBranches<'db> {
    pub(in crate::types) fn take(builder: &mut IntersectionBuilder<'db>) -> Self {
        Self::from_intersections(std::mem::take(&mut builder.intersections))
    }

    pub(in crate::types) fn from_intersections(
        intersections: Vec<InnerIntersectionBuilder<'db>>,
    ) -> Self {
        Self {
            #[cfg(any(test, feature = "experimental-analysis"))]
            capacity: intersections.capacity(),
            inner: intersections.into_iter(),
        }
    }

    // Exhausting the iterator leaves its original Vec allocation in place. A controlled
    // adapter must account for that backing as well as every remaining inner builder.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn storage(&self) -> (&[InnerIntersectionBuilder<'db>], usize) {
        (self.inner.as_slice(), self.capacity)
    }

    pub(in crate::types) fn next(&mut self) -> Option<InnerIntersectionBuilder<'db>> {
        self.inner.next()
    }
}

// A controlled adapter prepays cleanup when taking or removing branches: both the iterator
// and a removed inner builder can be dropped after refusal or cancellation. Borrowed inputs
// let it admit each transfer before taking ownership inside a rejectable callback.
pub(in crate::types) trait IntersectionAssemblyEffects<'db> {
    type Error;

    async fn take_elements(
        &self,
        builder: &mut IntersectionBuilder<'db>,
    ) -> Result<IntersectionBranches<'db>, Self::Error>;

    async fn next_element(
        &self,
        branches: &mut IntersectionBranches<'db>,
    ) -> Result<Option<InnerIntersectionBuilder<'db>>, Self::Error>;

    async fn build_inner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn finish_elements(
        &self,
        branches: &mut Option<IntersectionBranches<'db>>,
    ) -> Result<(), Self::Error>;
}

struct IntersectionElements<'builder, 'effects, 'db, E> {
    db: &'db dyn Db,
    env: &'builder ProgramEnvironment<'db>,
    branches: &'builder mut Option<IntersectionBranches<'db>>,
    effects: &'effects E,
}

impl<'db, E: IntersectionAssemblyEffects<'db>> TypeElements<'db>
    for IntersectionElements<'_, '_, 'db, E>
{
    type Error = E::Error;
    type Item = Type<'db>;

    async fn next(&mut self) -> Result<Option<Type<'db>>, Self::Error> {
        let Some(branches) = self.branches.as_mut() else {
            return Ok(None);
        };
        let Some(mut inner) = self.effects.next_element(branches).await? else {
            return Ok(None);
        };
        self.effects
            .build_inner(self.db, self.env, &mut inner)
            .await
            .map(Some)
    }
}

pub(in crate::types) async fn build_with<'db, E, A>(
    builder: &mut IntersectionBuilder<'db>,
    effects: &E,
    assembly: &A,
) -> Result<Type<'db>, E::Error>
where
    E: IntersectionAssemblyEffects<'db>,
    A: TypeAssemblyEffects<'db, Error = E::Error>,
{
    let mut branches = Some(effects.take_elements(builder).await?);
    let result = assembly::union_from_elements(
        builder.db,
        &builder.env,
        &mut IntersectionElements {
            db: builder.db,
            env: &builder.env,
            branches: &mut branches,
            effects,
        },
        assembly,
    )
    .await?;
    effects.finish_elements(&mut branches).await?;
    Ok(result)
}

struct OrdinaryIntersectionAssemblyEffects;

impl<'db> IntersectionAssemblyEffects<'db> for OrdinaryIntersectionAssemblyEffects {
    type Error = Infallible;

    async fn take_elements(
        &self,
        builder: &mut IntersectionBuilder<'db>,
    ) -> Result<IntersectionBranches<'db>, Self::Error> {
        Ok(IntersectionBranches::take(builder))
    }

    async fn next_element(
        &self,
        branches: &mut IntersectionBranches<'db>,
    ) -> Result<Option<InnerIntersectionBuilder<'db>>, Self::Error> {
        Ok(branches.next())
    }

    async fn build_inner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(std::mem::take(builder).build(db, env))
    }

    async fn finish_elements(
        &self,
        branches: &mut Option<IntersectionBranches<'db>>,
    ) -> Result<(), Self::Error> {
        drop(branches.take());
        Ok(())
    }
}

pub(in crate::types) fn build<'db>(builder: &mut IntersectionBuilder<'db>) -> Type<'db> {
    legacy_inline(build_with(
        builder,
        &OrdinaryIntersectionAssemblyEffects,
        &InlineTypeAssembly,
    ))
}
