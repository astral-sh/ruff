//! Continuation checks for facts owned by class declarations.

#[cfg(test)]
mod apply_tests;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::convert::Infallible;

#[cfg(test)]
use salsa::plumbing::AsId;

#[cfg(test)]
use crate::types::constructor::expansion_probe::{self, Observation};
use crate::types::generics::defaults::{
    DefaultSpecializationEffects, DefaultSpecializationWork, Unrestricted,
    default_specialization_with,
};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::tuple::TupleType;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, GenericAlias, GenericContext, Specialization,
    StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
pub(super) type SourceClassError = crate::types::constructor::expansion_probe::Incomplete;
#[cfg(not(test))]
pub(super) type SourceClassError = Infallible;

/// A declaration owns its base expressions, inherited context, defaults and instance flags. Constructors reached while
/// inferring those facts still use the installed attempt and its remaining allowance.
pub(super) struct SourceClassEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SourceClassEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl SourceReadControl for SourceClassEffects<'_> {
    type Error = SourceClassError;

    fn check(&self) -> Result<(), SourceClassError> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::active() {
            crate::types::constructor::expansion_probe::continue_work(self.db)?;
        } else if salsa::attempt_probe::is_incomplete(self.db) {
            return Err(SourceClassError::Interrupted);
        }
        Ok(())
    }
}

/// Computes defaults from the class declaration, without accepting supplied type arguments.
pub(in crate::types) fn default_class_specialization_with<'db, C: SourceReadControl>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    control: &C,
) -> Result<ClassType<'db>, C::Error> {
    class_default_specialization_sync(class, &InlineClassDefaultEffects { db, control })
}

pub(super) fn apply_class_specialization<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    specialize: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
) -> ClassType<'db> {
    let Ok(class) = apply_class_specialization_sync(
        class,
        specialize,
        &InlineApplyClassSpecializationEffects { db },
    );
    class
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousApplyClassSpecializationEffects)]
    pub(in crate::types) trait ApplyClassSpecializationEffects<'db, I> {
        type Error;

        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn specialize(&self, context: GenericContext<'db>, input: I) -> Result<Specialization<'db>, Self::Error>;
        #[operation(local)]
        async fn generic_alias(&self, class: StaticClassLiteral<'db>, specialization: Specialization<'db>) -> Result<ClassType<'db>, Self::Error>;
    }

    #[synchronous(apply_class_specialization_sync)]
    #[capabilities(effects = ApplyClassSpecializationEffects)]
    #[passive_values(ClassType::NonGeneric, ClassLiteral::Static)]
    pub(in crate::types) async fn apply_class_specialization_with<'db, I, E: ApplyClassSpecializationEffects<'db, I>>(
        class: StaticClassLiteral<'db>,
        input: I,
        effects: &E,
    ) -> Result<ClassType<'db>, E::Error> {
        let Some(context) = effects.generic_context(class).await? else {
            return Ok(ClassType::NonGeneric(ClassLiteral::Static(class)));
        };
        let specialization = effects.specialize(context, input).await?;
        effects.generic_alias(class, specialization).await
    }

    #[synchronous(SynchronousClassDefaultSpecializationEffects)]
    pub(in crate::types) trait ClassDefaultSpecializationEffects<'db> {
        type Error;

        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn defaults(&self, class: StaticClassLiteral<'db>, context: GenericContext<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(local)]
        async fn generic_alias(&self, class: StaticClassLiteral<'db>, specialization: Specialization<'db>) -> Result<ClassType<'db>, Self::Error>;
    }

    #[synchronous(class_default_specialization_sync)]
    #[capabilities(effects = ClassDefaultSpecializationEffects)]
    #[passive_values(ClassType::NonGeneric, ClassLiteral::Static)]
    pub(in crate::types) async fn class_default_specialization_with<'db, E: ClassDefaultSpecializationEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<ClassType<'db>, E::Error> {
        let Some(context) = effects.generic_context(class).await? else {
            return Ok(ClassType::NonGeneric(ClassLiteral::Static(class)));
        };
        let specialization = effects.defaults(class, context).await?;
        effects.generic_alias(class, specialization).await
    }
}

struct InlineApplyClassSpecializationEffects<'db> {
    db: &'db dyn Db,
}

impl<'db, I: FnOnce(GenericContext<'db>) -> Specialization<'db>>
    SynchronousApplyClassSpecializationEffects<'db, I>
    for InlineApplyClassSpecializationEffects<'db>
{
    type Error = Infallible;

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(class.generic_context(self.db))
    }

    fn specialize(
        &self,
        context: GenericContext<'db>,
        input: I,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(input(context))
    }

    fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(ClassType::Generic(GenericAlias::new(
            self.db,
            class,
            specialization,
        )))
    }
}

struct InlineClassDefaultEffects<'db, 'control, C> {
    db: &'db dyn Db,
    control: &'control C,
}

impl<'db, C: SourceReadControl> SynchronousClassDefaultSpecializationEffects<'db>
    for InlineClassDefaultEffects<'db, '_, C>
{
    type Error = C::Error;

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        read_source(self.control, || class.generic_context(self.db))
    }

    fn defaults(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        default_specialization_with(
            self.db,
            context,
            class.known(self.db),
            &DeclarationDefaults {
                control: self.control,
            },
        )
    }

    fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        read_source(self.control, || {
            ClassType::Generic(GenericAlias::new(self.db, class, specialization))
        })
    }
}

// Only the class operation can create this adapter: arbitrary contexts can carry transformed
// defaults, even when their variables retain the original source definitions.
struct DeclarationDefaults<'a, C> {
    control: &'a C,
}

#[cfg(test)]
struct DefaultReadObservation(salsa::Id);

#[cfg(test)]
impl DefaultReadObservation {
    fn new(variable: BoundTypeVarInstance<'_>) -> Self {
        let id = variable.as_id();
        expansion_probe::observe(Observation::ClassDefaultReadEntered(id));
        Self(id)
    }
}

#[cfg(test)]
impl Drop for DefaultReadObservation {
    fn drop(&mut self) {
        expansion_probe::observe(Observation::ClassDefaultReadExited(self.0));
    }
}

impl<'db, C: SourceReadControl> DefaultSpecializationEffects<'db> for DeclarationDefaults<'_, C> {
    type Error = C::Error;

    fn checkpoint(&self, _: DefaultSpecializationWork) -> Result<(), Self::Error> {
        self.control.check()
    }

    fn default_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        read_source(self.control, || {
            #[cfg(test)]
            let _observation = DefaultReadObservation::new(variable);
            let Ok(value) = Unrestricted.default_type(db, env, variable);
            value
        })
    }

    fn map_default(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &[Type<'db>],
    ) -> Result<Type<'db>, Self::Error> {
        read_source(self.control, || {
            let Ok(value) = Unrestricted.map_default(db, env, default, context, prefix);
            value
        })
    }

    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        read_source(self.control, || {
            let Ok(value) = Unrestricted.unknown_tuple(db, env);
            value
        })
    }

    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Self::Error> {
        read_source(self.control, || {
            let Ok(value) = Unrestricted.unknown_paramspec(db);
            value
        })
    }

    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        read_source(self.control, || {
            let Ok(value) = Unrestricted.intern_specialization(db, context, types, tuple);
            value
        })
    }
}
