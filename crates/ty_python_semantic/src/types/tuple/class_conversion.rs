//! Conversion of an exact tuple to its builtin class and tuple-preserving specialization.

use std::convert::Infallible;

use super::{Tuple, TupleSpec, TupleType};
use crate::types::{
    ClassType, GenericContext, KnownClass, Specialization, StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

/// The ordered elements used for the builtin tuple class's single type parameter.
/// A `TypeVarTuple` remains a type variable here, rather than becoming its runtime element type.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TupleClassElements<'a, 'db> {
    pub prefix: &'a [Type<'db>],
    pub variable: Option<Type<'db>>,
    pub suffix: &'a [Type<'db>],
}

impl<'db> TupleSpec<'db> {
    /// Borrows the class-parameter elements in prefix, variable-segment, then suffix order.
    pub(in crate::types) fn class_elements(&self) -> TupleClassElements<'_, 'db> {
        match self {
            Tuple::Fixed(tuple) => TupleClassElements {
                prefix: tuple.all_elements(),
                variable: None,
                suffix: &[],
            },
            Tuple::Variable(tuple) => TupleClassElements {
                prefix: tuple.prefix_elements(),
                variable: Some(tuple.variable().tuple_class_type()),
                suffix: tuple.suffix_elements(),
            },
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleClassEffects)]
    pub(in crate::types) trait TupleClassEffects<'db> {
        type Error;

        #[operation(source)]
        async fn environment(&self, tuple: TupleType<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(child)]
        async fn tuple_class(&self, env: &ProgramEnvironment<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
        #[operation(child)]
        async fn apply_class(&self, class: StaticClassLiteral<'db>, env: &ProgramEnvironment<'db>, tuple: TupleType<'db>, cycle: Option<salsa::Id>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn is_single_parameter(&self, context: GenericContext<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn element_union(&self, env: &ProgramEnvironment<'db>, tuple: TupleType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn divergent_element(&self, id: salsa::Id) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn specialize_tuple(&self, context: GenericContext<'db>, element: Type<'db>, tuple: TupleType<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(child)]
        async fn default_specialization(&self, context: GenericContext<'db>) -> Result<Specialization<'db>, Self::Error>;
    }

    /// Resolves an exact tuple's builtin class using the tuple's program and the class's generic context.
    /// During cycle initialization, `cycle` supplies the divergent element while retaining the tuple.
    #[synchronous(tuple_class_sync)]
    #[capabilities(effects = TupleClassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn tuple_class_with<'db, E: TupleClassEffects<'db>>(
        tuple: TupleType<'db>, cycle: Option<salsa::Id>, effects: &E,
    ) -> Result<ClassType<'db>, E::Error> {
        let env = effects.environment(tuple).await?;
        let class = effects.tuple_class(&env).await?;
        effects.apply_class(class, &env, tuple, cycle).await
    }

    /// Specializes a context with one type parameter using the tuple's element union and original handle.
    /// Cycle initialization substitutes a divergent element. Contexts with other numbers of type
    /// parameters retain the builtin tuple class's ordinary default specialization.
    #[synchronous(tuple_class_specialization_sync)]
    #[capabilities(effects = TupleClassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn tuple_class_specialization_with<'db, E: TupleClassEffects<'db>>(
        context: GenericContext<'db>, env: &ProgramEnvironment<'db>, tuple: TupleType<'db>,
        cycle: Option<salsa::Id>, effects: &E,
    ) -> Result<Specialization<'db>, E::Error> {
        if effects.is_single_parameter(context).await? {
            let element = match cycle {
                Some(id) => effects.divergent_element(id).await?,
                None => effects.element_union(env, tuple).await?,
            };
            effects.specialize_tuple(context, element, tuple).await
        } else {
            effects.default_specialization(context).await
        }
    }
}

/// Supplies ordinary field access and canonical construction for tuple class conversion.
pub(super) struct OrdinaryTupleClassEffects<'db> {
    pub db: &'db dyn Db,
}

impl<'db> SynchronousTupleClassEffects<'db> for OrdinaryTupleClassEffects<'db> {
    type Error = Infallible;

    fn environment(&self, tuple: TupleType<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_program(tuple.program(self.db)))
    }

    fn tuple_class(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<StaticClassLiteral<'db>, Infallible> {
        Ok(KnownClass::Tuple
            .try_to_class_literal(self.db, env)
            .expect("Typeshed should always have a `tuple` class in `builtins.pyi`"))
    }

    fn apply_class(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        tuple: TupleType<'db>,
        cycle: Option<salsa::Id>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(class.apply_specialization(self.db, |context| {
            let Ok(specialization) =
                tuple_class_specialization_sync(context, env, tuple, cycle, self);
            specialization
        }))
    }

    fn is_single_parameter(&self, context: GenericContext<'db>) -> Result<bool, Infallible> {
        Ok(context.variables(self.db).len() == 1)
    }

    fn element_union(
        &self,
        env: &ProgramEnvironment<'db>,
        tuple: TupleType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(tuple.tuple(self.db).tuple_class_type(self.db, env))
    }

    fn divergent_element(&self, id: salsa::Id) -> Result<Type<'db>, Infallible> {
        Ok(Type::divergent(id))
    }

    fn specialize_tuple(
        &self,
        context: GenericContext<'db>,
        element: Type<'db>,
        tuple: TupleType<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(context.specialize_tuple(self.db, element, tuple))
    }

    fn default_specialization(
        &self,
        context: GenericContext<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(context.default_specialization(self.db, Some(KnownClass::Tuple)))
    }
}
