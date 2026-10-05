//! Speculative intersection expansion preserves the original signed element order.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::IntersectionBuilder;
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    BoundTypeVarInstance, NegativeIntersectionElements, NewType, Type, TypeVarBoundOrConstraints,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

shared_semantic_family! {
    #[synchronous(SynchronousSpeculationEffects)]
    pub(in crate::types) trait SpeculationEffects<'db> {
        type Error;

        #[operation(local)]
        async fn new_intersection(&self) -> Result<IntersectionBuilder<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_positive(&self, positive: &FxOrderSet<Type<'db>>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_negative(&self, negative: &NegativeIntersectionElements<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn require_bounds(&self, typevar: BoundTypeVarInstance<'db>) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error>;
        #[operation(source)]
        async fn constraints_as_type(&self, constraints: TypeVarConstraints<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn add_positive(&self, builder: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn add_negative(&self, builder: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn build(&self, builder: &mut IntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(expand_sync)]
    #[capabilities(effects = SpeculationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn expand_with<'db, E: SpeculationEffects<'db>>(
        positive: &FxOrderSet<Type<'db>>,
        negative: &NegativeIntersectionElements<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut builder = effects.new_intersection().await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(element) = effects.next_positive(positive, &mut cursor).await? {
            let expanded = match element {
                Type::TypeVar(typevar) => match effects.require_bounds(typevar).await? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => bound,
                    TypeVarBoundOrConstraints::Constraints(constraints) => effects.constraints_as_type(constraints).await?,
                },
                Type::NewTypeInstance(newtype) => effects.newtype_base(newtype).await?,
                _ => element,
            };
            effects.add_positive(&mut builder, expanded).await?;
        }

        let mut cursor = 0;
        #[cursor_loop]
        while let Some(element) = effects.next_negative(negative, &mut cursor).await? {
            effects.add_negative(&mut builder, element).await?;
        }
        effects.build(&mut builder).await
    }
}

struct OrdinarySpeculationEffects<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
}

pub(in crate::types::set_theoretic) fn expand<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    positive: &FxOrderSet<Type<'db>>,
    negative: &NegativeIntersectionElements<'db>,
) -> Type<'db> {
    match expand_sync(positive, negative, &OrdinarySpeculationEffects { db, env }) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

impl<'db> SynchronousSpeculationEffects<'db> for OrdinarySpeculationEffects<'_, 'db> {
    type Error = Infallible;

    fn new_intersection(&self) -> Result<IntersectionBuilder<'db>, Self::Error> {
        Ok(IntersectionBuilder::new(self.db, self.env))
    }

    fn next_positive(
        &self,
        positive: &FxOrderSet<Type<'db>>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = positive.get_index(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn next_negative(
        &self,
        negative: &NegativeIntersectionElements<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = match negative {
            NegativeIntersectionElements::Empty => None,
            NegativeIntersectionElements::Single(ty) => {
                if *cursor == 0 {
                    Some(*ty)
                } else {
                    None
                }
            }
            NegativeIntersectionElements::Multiple(elements) => {
                elements.get_index(*cursor).copied()
            }
        };
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn require_bounds(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error> {
        Ok(typevar.require_bound_or_constraints(self.db, self.env))
    }

    fn constraints_as_type(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(constraints.as_type(self.db, self.env))
    }

    fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(newtype.concrete_base_type(self.db))
    }

    fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.add_positive_in_place(ty);
        Ok(())
    }

    fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.add_negative_in_place(ty);
        Ok(())
    }

    fn build(&self, builder: &mut IntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(super::intersection_assembly::build(builder))
    }
}
