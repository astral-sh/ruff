//! Normalize tuple specifications before assigning their canonical tuple identity.

use std::convert::Infallible;

use super::{FixedLengthTuple, TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleConstructionEffects)]
    pub(in crate::types) trait TupleConstructionEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn fixed_without_variable(&self, tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn intern_borrowed(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, spec: &TupleSpec<'db>) -> Result<TupleType<'db>, Self::Error>;
        #[operation(child)]
        async fn intern_owned(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, spec: TupleSpec<'db>) -> Result<TupleType<'db>, Self::Error>;
    }

    #[synchronous(tuple_type_sync)]
    #[capabilities(effects = TupleConstructionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn tuple_type<'db, E: TupleConstructionEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
        effects: &E,
    ) -> Result<TupleType<'db>, E::Error> {
        effects.checkpoint().await?;
        // If the variable-length portion is Never, it can only be instantiated with zero elements.
        // That means this isn't a variable-length tuple after all!
        if let TupleSpec::Variable(tuple) = spec
            && matches!(tuple.variable_segment, VariableSegment::Homogeneous(Type::Never))
        {
            let tuple = effects.fixed_without_variable(tuple).await?;
            return effects.intern_owned(db, env, tuple).await;
        }

        effects.intern_borrowed(db, env, spec).await
    }
}

pub(in crate::types) fn fixed_without_variable<'db>(
    tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
) -> TupleSpec<'db> {
    TupleSpec::Fixed(FixedLengthTuple::from_elements(
        tuple
            .iter_prefix_elements()
            .chain(tuple.iter_suffix_elements()),
    ))
}

pub(super) struct OrdinaryTupleConstruction;

impl<'db> SynchronousTupleConstructionEffects<'db> for OrdinaryTupleConstruction {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn fixed_without_variable(
        &self,
        tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    ) -> Result<TupleSpec<'db>, Self::Error> {
        Ok(fixed_without_variable(tuple))
    }

    fn intern_borrowed(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        Ok(TupleType::new_internal(db, env.program(db), spec))
    }

    fn intern_owned(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: TupleSpec<'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        Ok(TupleType::new_internal(db, env.program(db), spec))
    }
}
