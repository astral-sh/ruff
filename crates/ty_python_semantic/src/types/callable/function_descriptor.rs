use std::convert::Infallible;

use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::method::BoundMethodReceiver;
use crate::types::{BoundMethodType, FunctionType, KnownInstanceType, Type};
use crate::{Db, Program, ProgramEnvironment};

pub(in crate::types) struct FunctionBindingFacts;

pub(in crate::types) struct OrdinaryFunctionBinding<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousFunctionBindingEffects)]
    pub(in crate::types) trait FunctionBindingEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn function_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(source)]
        async fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Self::Error>;
        #[operation(source)]
        async fn wrapper_kind(&self, wrapper: MethodWrapper<'db>) -> Result<MethodWrapperKind, Self::Error>;
        #[operation(child)]
        async fn function_underlying(&self, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn callable_underlying(&self, callable: CallableType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn wrapper_wrapped(&self, wrapper: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn kind(&self, ty: Type<'db>) -> Result<Option<CallableTypeKind>, Self::Error>;
        #[operation(child)]
        async fn underlying(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn owner_is_none(&self, owner: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn meta_type(&self, instance: Type<'db>, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(child)]
        async fn bound_method(&self, func: Type<'db>, program: Program<'db>, receiver: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_bound_method(&self, func: Type<'db>, program: Program<'db>, class_method: bool, receiver: Type<'db>) -> Result<BoundMethodType<'db>, Self::Error>;
    }

    #[finite_capability]
    impl FunctionBindingFacts {
        pub(in crate::types) fn is_method_like(&self, kind: CallableTypeKind) -> bool {
            matches!(kind, CallableTypeKind::FunctionLike | CallableTypeKind::StaticMethodLike | CallableTypeKind::ClassMethodLike)
        }

        fn is_classmethod(&self, kind: Option<CallableTypeKind>) -> bool {
            kind == Some(CallableTypeKind::ClassMethodLike)
        }
    }

    #[synchronous(function_like_kind_sync)]
    #[capabilities(effects = FunctionBindingEffects, facts = FunctionBindingFacts)]
    #[passive_values(CallableTypeKind::StaticMethodLike, CallableTypeKind::ClassMethodLike)]
    pub(in crate::types) async fn function_like_kind_with<'db, E: FunctionBindingEffects<'db>>(
        ty: Type<'db>, facts: FunctionBindingFacts, effects: &E,
    ) -> Result<Option<CallableTypeKind>, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::FunctionLiteral(function) => Ok(Some(effects.function_kind(function).await?)),
            Type::Callable(callable) => {
                if facts.is_method_like(effects.callable_kind(callable).await?) {
                    Ok(Some(effects.callable_kind(callable).await?))
                } else {
                    Ok(None)
                }
            }
            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
                Ok(Some(match effects.wrapper_kind(wrapper).await? {
                    MethodWrapperKind::Staticmethod => CallableTypeKind::StaticMethodLike,
                    MethodWrapperKind::Classmethod => CallableTypeKind::ClassMethodLike,
                }))
            }
            _ => Ok(None),
        }
    }

    #[synchronous(underlying_function_sync)]
    #[capabilities(effects = FunctionBindingEffects, facts = FunctionBindingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn underlying_function_with<'db, E: FunctionBindingEffects<'db>>(
        ty: Type<'db>, facts: FunctionBindingFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::FunctionLiteral(function) => effects.function_underlying(function).await,
            Type::Callable(callable) => {
                if facts.is_method_like(effects.callable_kind(callable).await?) {
                    effects.callable_underlying(callable).await
                } else {
                    Ok(ty)
                }
            }
            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => effects.wrapper_wrapped(wrapper).await,
            _ => Ok(ty),
        }
    }

    #[synchronous(bind_function_descriptor_sync)]
    #[capabilities(effects = FunctionBindingEffects)]
    #[passive_values()]
    pub(in crate::types) async fn bind_function_descriptor_with<'db, E: FunctionBindingEffects<'db>>(
        ty: Type<'db>, env: &ProgramEnvironment<'db>, instance: Option<Type<'db>>, owner: Option<Type<'db>>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        let Some(kind) = effects.kind(ty).await? else {
            return Ok(None);
        };
        let receiver = match kind {
            CallableTypeKind::StaticMethodLike => return Ok(Some(effects.underlying(ty).await?)),
            CallableTypeKind::ClassMethodLike => match owner {
                Some(owner) if !effects.owner_is_none(owner).await? => Some(owner),
                _ => match instance {
                    Some(instance) => Some(effects.meta_type(instance, env).await?),
                    None => None,
                },
            },
            _ => instance,
        };
        Ok(Some(match receiver {
            Some(receiver) => {
                let program = effects.program(env).await?;
                effects.bound_method(ty, program, receiver).await?
            }
            None => effects.underlying(ty).await?,
        }))
    }

    #[synchronous(bound_method_from_callable_sync)]
    #[capabilities(effects = FunctionBindingEffects, facts = FunctionBindingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn bound_method_from_callable_with<'db, E: FunctionBindingEffects<'db>>(
        func: Type<'db>, program: Program<'db>, receiver: Type<'db>, facts: FunctionBindingFacts, effects: &E,
    ) -> Result<BoundMethodType<'db>, E::Error> {
        effects.checkpoint().await?;
        let underlying = effects.underlying(func).await?;
        let class_method = facts.is_classmethod(effects.kind(func).await?);
        effects.intern_bound_method(underlying, program, class_method, receiver).await
    }
}

impl<'db> SynchronousFunctionBindingEffects<'db> for OrdinaryFunctionBinding<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn function_kind(&self, function: FunctionType<'db>) -> Result<CallableTypeKind, Self::Error> {
        Ok(function.callable_type_kind(self.db))
    }

    fn callable_kind(&self, callable: CallableType<'db>) -> Result<CallableTypeKind, Self::Error> {
        Ok(callable.kind(self.db))
    }

    fn wrapper_kind(&self, wrapper: MethodWrapper<'db>) -> Result<MethodWrapperKind, Self::Error> {
        Ok(wrapper.kind(self.db))
    }

    fn function_underlying(&self, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::FunctionLiteral(function.underlying_function(self.db)))
    }

    fn callable_underlying(&self, callable: CallableType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Callable(callable.into_function_like(self.db)))
    }

    fn wrapper_wrapped(&self, wrapper: MethodWrapper<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(wrapper.wrapped(self.db))
    }

    fn kind(&self, ty: Type<'db>) -> Result<Option<CallableTypeKind>, Self::Error> {
        Ok(ty.function_like_kind(self.db))
    }

    fn underlying(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.underlying_function(self.db))
    }

    fn owner_is_none(&self, owner: Type<'db>) -> Result<bool, Self::Error> {
        Ok(owner.is_none(self.db))
    }

    fn meta_type(
        &self,
        instance: Type<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(instance.to_meta_type(self.db, env))
    }

    fn program(&self, env: &ProgramEnvironment<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(env.program(self.db))
    }

    fn bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        receiver: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::BoundMethod(BoundMethodType::from_callable(
            self.db, func, program, receiver,
        )))
    }

    fn intern_bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        class_method: bool,
        receiver: Type<'db>,
    ) -> Result<BoundMethodType<'db>, Self::Error> {
        Ok(BoundMethodType::new_internal(
            self.db,
            func,
            program,
            class_method,
            BoundMethodReceiver::Instance(receiver),
        ))
    }
}
