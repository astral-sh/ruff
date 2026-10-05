use crate::types::visitor::{SearchControl, SearchWork};
use crate::types::{CallableType, NominalInstanceType, Type, visitor};
use crate::{Db, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousMemberSelfBindingEffects)]
    pub(in crate::types) trait MemberSelfBindingEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&mut self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn callable_is_function_like(&mut self, callable: CallableType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn nominal_is_definition_generic(&mut self, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn contains_self(&mut self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn search_self(&mut self, ty: Type<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(contains_self_sync)]
    #[capabilities(effects = MemberSelfBindingEffects)]
    #[passive_values()]
    pub(in crate::types) async fn contains_self_with<'db, E: MemberSelfBindingEffects<'db>>(
        ty: Type<'db>,
        effects: &mut E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        if let Type::NominalInstance(instance) = ty
            && !effects.nominal_is_definition_generic(instance).await?
        {
            return Ok(false);
        }

        // Type alias bodies cannot declare `Self`, but their explicit type arguments can
        // contain the `Self` from an enclosing method or class.
        effects.search_self(ty).await
    }

    #[synchronous(supports_self_binding_sync)]
    #[capabilities(effects = MemberSelfBindingEffects)]
    #[passive_values()]
    pub(in crate::types) async fn supports_self_binding_with<'db, E: MemberSelfBindingEffects<'db>>(
        ty: Type<'db>,
        effects: &mut E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) => {
                Ok(false)
            }
            Type::Callable(callable) if effects.callable_is_function_like(callable).await? => Ok(false),
            _ => effects.contains_self(ty).await,
        }
    }
}

pub(in crate::types) struct InlineMemberSelfBindingEffects<'env, 'control, 'db, C> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    control: &'control mut C,
}

impl<'env, 'control, 'db, C> InlineMemberSelfBindingEffects<'env, 'control, 'db, C> {
    pub(in crate::types) fn new(
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
        control: &'control mut C,
    ) -> Self {
        Self { db, env, control }
    }
}

impl<'db, C: SearchControl> SynchronousMemberSelfBindingEffects<'db>
    for InlineMemberSelfBindingEffects<'_, '_, 'db, C>
{
    type Error = C::Error;

    fn checkpoint(&mut self) -> Result<(), Self::Error> {
        self.control.admit(SearchWork::Advance)
    }

    fn callable_is_function_like(
        &mut self,
        callable: CallableType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(callable.is_function_like(self.db))
    }

    fn nominal_is_definition_generic(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(instance.is_definition_generic(self.db))
    }

    fn search_self(&mut self, ty: Type<'db>) -> Result<bool, Self::Error> {
        visitor::try_any_over_type_including_alias_arguments(
            self.db,
            self.env,
            ty,
            |ty| {
                ty.as_typevar()
                    .is_some_and(|tv| tv.typevar(self.db).is_self(self.db))
            },
            self.control,
        )
    }

    fn contains_self(&mut self, ty: Type<'db>) -> Result<bool, Self::Error> {
        contains_self_sync(ty, self)
    }
}
