//! Literal promotion with explicit scalar, enum, and callable dependencies.

use crate::types::literal::{EnumLiteralType, LiteralFallback};
use crate::types::mapping::effects::{
    MappingEffects, MappingOperation, MappingWork, SynchronousMappingEffects,
};
use crate::types::{FunctionType, KnownClass, LiteralValueType, Type};
use crate::{Db, ProgramEnvironment};

/// Fixed decisions and transfers surrounding a promotion leaf's semantic children.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum PromotionLeafWork {
    Dispatch,
    LiteralClassification,
    ScalarRequest,
    EnumRequest,
    FunctionRequest,
    Result,
}

/// Reads the promotability flag and fallback category stored by a literal.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct PromotionLeafFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousPromotionLeafEffects)]
    pub(in crate::types) trait PromotionLeafEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: PromotionLeafWork) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn scalar_fallback(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn enum_fallback(&self, env: &ProgramEnvironment<'db>, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn function_callable(&self, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl PromotionLeafFacts {
        fn is_promotable<'db>(&self, literal: LiteralValueType<'db>) -> bool {
            literal.is_promotable()
        }

        fn fallback<'db>(&self, literal: LiteralValueType<'db>) -> LiteralFallback<'db> {
            literal.fallback_target()
        }
    }

    /// Promotes one literal or function without traversing types nested inside it.
    /// Literals marked unpromotable retain their original type.
    #[synchronous(promote_leaf_sync)]
    #[capabilities(effects = PromotionLeafEffects, facts = PromotionLeafFacts)]
    #[passive_values(PromotionLeafWork::Dispatch, PromotionLeafWork::LiteralClassification, PromotionLeafWork::ScalarRequest, PromotionLeafWork::EnumRequest, PromotionLeafWork::FunctionRequest, PromotionLeafWork::Result)]
    pub(in crate::types) async fn promote_leaf_with<'db, E: PromotionLeafEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        facts: PromotionLeafFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint(PromotionLeafWork::Dispatch).await?;
        let promoted = match ty {
            Type::LiteralValue(literal) => {
                effects.checkpoint(PromotionLeafWork::LiteralClassification).await?;
                if facts.is_promotable(literal) {
                    match facts.fallback(literal) {
                        LiteralFallback::Scalar(class) => {
                            effects.checkpoint(PromotionLeafWork::ScalarRequest).await?;
                            effects.scalar_fallback(env, class).await?
                        }
                        LiteralFallback::Enum(literal) => {
                            effects.checkpoint(PromotionLeafWork::EnumRequest).await?;
                            effects.enum_fallback(env, literal).await?
                        }
                    }
                } else {
                    ty
                }
            }
            Type::FunctionLiteral(function) => {
                effects.checkpoint(PromotionLeafWork::FunctionRequest).await?;
                effects.function_callable(function).await?
            }
            _ => ty,
        };
        effects.checkpoint(PromotionLeafWork::Result).await?;
        Ok(promoted)
    }
}

/// Adapts mapping providers to promotion's scalar and legacy conversion dependencies.
pub(in crate::types) struct MappingPromotionEffects<'a, 'db, E> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) effects: &'a E,
}

impl<'db, E: MappingEffects<'db>> PromotionLeafEffects<'db>
    for MappingPromotionEffects<'_, 'db, E>
{
    type Error = E::Error;

    async fn checkpoint(&self, work: PromotionLeafWork) -> Result<(), Self::Error> {
        match work {
            PromotionLeafWork::ScalarRequest => {
                self.effects.checkpoint(MappingWork::ScalarFallbackLookup).await
            }
            PromotionLeafWork::Dispatch
            | PromotionLeafWork::LiteralClassification
            | PromotionLeafWork::EnumRequest
            | PromotionLeafWork::FunctionRequest
            | PromotionLeafWork::Result => Ok(()),
        }
    }

    async fn scalar_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.scalar_fallback(self.db, env, class)
    }

    async fn enum_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        literal: EnumLiteralType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.legacy(MappingOperation::Promotion, || {
            literal.enum_class_instance(self.db, env)
        })
    }

    async fn function_callable(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.legacy(MappingOperation::Promotion, || {
            Type::Callable(function.into_callable_type(self.db))
        })
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousPromotionLeafEffects<'db>
    for MappingPromotionEffects<'_, 'db, E>
{
    type Error = E::Error;

    fn checkpoint(&self, work: PromotionLeafWork) -> Result<(), Self::Error> {
        match work {
            PromotionLeafWork::ScalarRequest => {
                self.effects.checkpoint(MappingWork::ScalarFallbackLookup)
            }
            PromotionLeafWork::Dispatch
            | PromotionLeafWork::LiteralClassification
            | PromotionLeafWork::EnumRequest
            | PromotionLeafWork::FunctionRequest
            | PromotionLeafWork::Result => Ok(()),
        }
    }

    fn scalar_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.scalar_fallback(self.db, env, class)
    }

    fn enum_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        literal: EnumLiteralType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.legacy(MappingOperation::Promotion, || {
            literal.enum_class_instance(self.db, env)
        })
    }

    fn function_callable(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.legacy(MappingOperation::Promotion, || {
            Type::Callable(function.into_callable_type(self.db))
        })
    }
}
