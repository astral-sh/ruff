//! Validate an explicit `typing.Self` annotation while preserving its lexical binding.

use std::convert::Infallible;

use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::semantic_index;

use super::super::InlineConversion;
use crate::ProgramEnvironment;
use crate::types::generics::typing_self;
use crate::types::infer::{
    InferenceFlags, function_known_decorator_flags, nearest_enclosing_class,
};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, FunctionDecorators, InvalidTypeExpression,
    KnownClass, SpecialFormType, StaticClassLiteral, Type,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct SelfAnnotationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSelfAnnotationEffects)]
    pub(in crate::types) trait SelfAnnotationEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn begin(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn enclosing_class(&self, scope: ScopeId<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn bound_self(&self, scope: ScopeId<'db>, binding: Option<Definition<'db>>, class: StaticClassLiteral<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn binding_definition(&self, variable: BoundTypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(source)]
        async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn is_dunder_new(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_staticmethod(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn type_class(&self, env: &ProgramEnvironment<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn class_default(&self, class: StaticClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn is_subclass(&self, env: &ProgramEnvironment<'db>, class: ClassType<'db>, target: ClassType<'db>) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl SelfAnnotationFacts {
        fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_scope(scope)
        }

        fn incompatible_receiver(&self, flags: InferenceFlags) -> bool {
            flags.contains(InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER)
                && flags.intersects(InferenceFlags::IN_RETURN_TYPE | InferenceFlags::IN_PARAMETER_ANNOTATION)
        }
    }

    /// Resolves `Self` in its class and validates staticmethod, metaclass and receiver restrictions.
    /// Type-alias rejection remains with the surrounding special-form dispatcher. Valid unbound
    /// cases retain the Self special form; invalid cases return the original typed conversion error.
    #[synchronous(self_annotation_sync)]
    #[capabilities(effects = SelfAnnotationEffects, facts = SelfAnnotationFacts)]
    #[passive_values(Err, Type::TypeVar, Type::SpecialForm, SpecialFormType::TypingSelf, InvalidTypeExpression::InvalidType, InvalidTypeExpression::TypingSelfInStaticMethod, InvalidTypeExpression::TypingSelfInMetaclass, InvalidTypeExpression::TypingSelfWithIncompatibleReceiver)]
    pub(in crate::types) async fn self_annotation_with<'db, E: SelfAnnotationEffects<'db>>(
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
        facts: SelfAnnotationFacts,
        effects: &E,
    ) -> Result<Result<Type<'db>, InvalidTypeExpression<'db>>, E::Error> {
        effects.begin().await?;
        let env = facts.environment(scope);
        let Some(class) = effects.enclosing_class(scope).await? else {
            return Ok(Err(InvalidTypeExpression::InvalidType(Type::SpecialForm(SpecialFormType::TypingSelf), scope)));
        };
        let variable = effects.bound_self(scope, binding, class).await?;
        if let Some(variable) = variable
            && let Some(definition) = effects.binding_definition(variable).await?
            && effects.is_function(definition).await?
            && !effects.is_dunder_new(definition).await?
            && effects.is_staticmethod(definition).await?
        {
            return Ok(Err(InvalidTypeExpression::TypingSelfInStaticMethod));
        }
        if let Some(target) = effects.type_class(&env).await? {
            let class = effects.class_default(class).await?;
            if effects.is_subclass(&env, class, target).await? {
                return Ok(Err(InvalidTypeExpression::TypingSelfInMetaclass));
            }
        }
        if facts.incompatible_receiver(flags)
            && let Some(variable) = variable
        {
            return Ok(Err(InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(variable)));
        }
        Ok(Ok(match variable {
            Some(variable) => Type::TypeVar(variable),
            None => Type::SpecialForm(SpecialFormType::TypingSelf),
        }))
    }
}

impl<'db> SynchronousSelfAnnotationEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;

    fn begin(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn enclosing_class(
        &self,
        scope: ScopeId<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Self::Error> {
        let index = semantic_index(self.db, scope.program_file(self.db));
        Ok(nearest_enclosing_class(self.db, index, scope))
    }

    fn bound_self(
        &self,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(typing_self(
            self.db,
            scope,
            binding,
            ClassLiteral::Static(class),
        ))
    }

    fn binding_definition(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(variable.binding_context(self.db).definition())
    }

    fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
        Ok(matches!(
            definition.kind(self.db),
            DefinitionKind::Function(_)
        ))
    }

    fn is_dunder_new(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
        Ok(definition.name(self.db).as_deref() == Some("__new__"))
    }

    fn is_staticmethod(&self, definition: Definition<'db>) -> Result<bool, Self::Error> {
        Ok(function_known_decorator_flags(self.db, definition)
            .contains(FunctionDecorators::STATICMETHOD))
    }

    fn type_class(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(KnownClass::Type
            .to_class_literal(self.db, env)
            .to_class_type(self.db))
    }

    fn class_default(&self, class: StaticClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.default_specialization(self.db))
    }

    fn is_subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.is_subclass_of(self.db, env, target))
    }
}
