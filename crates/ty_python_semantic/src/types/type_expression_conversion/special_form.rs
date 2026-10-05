pub(in crate::types) mod typing_self;

use std::convert::Infallible;

use smallvec::smallvec_inline;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::InlineConversion;
use crate::ProgramEnvironment;
use crate::types::infer::InferenceFlags;
use crate::types::special_form::LegacyStdlibAlias;
use crate::types::{
    CallableType, DynamicType, IntersectionType, InvalidTypeExpression,
    InvalidTypeExpressionError, KnownClass, SpecialFormType, Type, TypeFormType,
};

#[derive(Clone, Copy)]
pub(in crate::types) struct SpecialFormConversionFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSpecialFormConversionEffects)]
    pub(in crate::types) trait SpecialFormConversionEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn dispatch(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn known_class_instance(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn homogeneous_tuple(&self, env: &ProgramEnvironment<'db>, element: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn type_form(&self, argument: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn intersection(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn unknown_callable(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn typing_self(&self, scope: ScopeId<'db>, binding: Option<Definition<'db>>, flags: InferenceFlags) -> Result<Result<Type<'db>, InvalidTypeExpression<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl SpecialFormConversionFacts {
        fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_scope(scope)
        }

        fn in_type_alias(&self, flags: InferenceFlags) -> bool {
            flags.contains(InferenceFlags::IN_TYPE_ALIAS)
        }

        fn in_valid_concatenate_context(&self, flags: InferenceFlags) -> bool {
            flags.contains(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT)
        }

        fn any<'db>(&self) -> Type<'db> {
            Type::any()
        }

        fn unknown<'db>(&self) -> Type<'db> {
            Type::unknown()
        }

        fn object<'db>(&self) -> Type<'db> {
            Type::object()
        }

        fn literal_string<'db>(&self) -> Type<'db> {
            Type::literal_string()
        }

        fn aliased_class(&self, alias: LegacyStdlibAlias) -> KnownClass {
            alias.aliased_class()
        }

        fn invalid<'db>(&self, err: InvalidTypeExpression<'db>) -> InvalidTypeExpressionError<'db> {
            let fallback_type = match err {
                InvalidTypeExpression::Concatenate
                | InvalidTypeExpression::RequiresTwoArguments(
                    SpecialFormType::Concatenate,
                ) => Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
                InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(typing_self) => {
                    Type::TypeVar(typing_self)
                }
                _ => Type::unknown(),
            };

            InvalidTypeExpressionError {
                fallback_type,
                invalid_expressions: smallvec_inline![err],
            }
        }
    }

    /// Interpret this special form as an unparameterized type in a type-expression context.
    #[synchronous(special_form_type_expression_sync)]
    #[capabilities(effects = SpecialFormConversionEffects, facts = SpecialFormConversionFacts)]
    #[passive_values(Err, Type::Never, Type::AlwaysTruthy, Type::AlwaysFalsy, Type::SpecialForm, KnownClass::NamedTupleLike, KnownClass::Type, InvalidTypeExpression::InvalidType, InvalidTypeExpression::TypingSelfInTypeAlias, InvalidTypeExpression::TypeAlias, InvalidTypeExpression::TypedDict, InvalidTypeExpression::RequiresArguments, InvalidTypeExpression::Protocol, InvalidTypeExpression::Generic, InvalidTypeExpression::Concatenate, InvalidTypeExpression::RequiresTwoArguments, InvalidTypeExpression::RequiresOneArgument, InvalidTypeExpression::TypeQualifier)]
    pub(in crate::types) async fn special_form_type_expression_with<'db, E: SpecialFormConversionEffects<'db>>(
        special_form: SpecialFormType,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
        facts: SpecialFormConversionFacts,
        effects: &E,
    ) -> Result<Result<Type<'db>, InvalidTypeExpression<'db>>, E::Error> {
        effects.dispatch().await?;
        let env = facts.environment(scope);
        Ok(match special_form {
            SpecialFormType::Never | SpecialFormType::NoReturn => Ok(Type::Never),
            SpecialFormType::LiteralString => Ok(facts.literal_string()),
            SpecialFormType::Any => Ok(facts.any()),
            SpecialFormType::Unknown => Ok(facts.unknown()),
            SpecialFormType::Divergent | SpecialFormType::Todo => Err(InvalidTypeExpression::InvalidType(
                Type::SpecialForm(special_form),
                scope,
            )),
            SpecialFormType::AlwaysTruthy => Ok(Type::AlwaysTruthy),
            SpecialFormType::AlwaysFalsy => Ok(Type::AlwaysFalsy),

            // Special case: `NamedTuple` in a type expression is understood to describe the type
            // `tuple[object, ...] & <a protocol that any `NamedTuple` class would satisfy>`.
            // This isn't very principled (since at runtime, `NamedTuple` is just a function),
            // but it appears to be what users often expect, and it improves compatibility with
            // other type checkers such as mypy.
            // See conversation in https://github.com/astral-sh/ruff/pull/19915.
            SpecialFormType::NamedTuple => {
                let tuple = effects.homogeneous_tuple(&env, facts.object()).await?;
                let protocol = effects.known_class_instance(&env, KnownClass::NamedTupleLike).await?;
                Ok(effects.intersection(&env, tuple, protocol).await?)
            }

            SpecialFormType::TypingSelf => {
                if facts.in_type_alias(flags) {
                    Err(InvalidTypeExpression::TypingSelfInTypeAlias)
                } else {
                    effects.typing_self(scope, binding, flags).await?
                }
            }
            // We ensure that `typing.TypeAlias` used in the expected position (annotating an
            // annotated assignment statement) doesn't reach here. Using it in any other type
            // expression is an error.
            SpecialFormType::TypeAlias => Err(InvalidTypeExpression::TypeAlias),
            SpecialFormType::TypedDict(_) => Err(InvalidTypeExpression::TypedDict),

            SpecialFormType::Literal | SpecialFormType::Union | SpecialFormType::Intersection => {
                Err(InvalidTypeExpression::RequiresArguments(special_form))
            }

            SpecialFormType::Protocol => Err(InvalidTypeExpression::Protocol),
            SpecialFormType::Generic => Err(InvalidTypeExpression::Generic),

            // `Concatenate` is just always invalid in this context in a type expression
            SpecialFormType::Concatenate
                if !facts.in_valid_concatenate_context(flags) =>
            {
                Err(InvalidTypeExpression::Concatenate)
            }

            SpecialFormType::Concatenate | SpecialFormType::Annotated => {
                Err(InvalidTypeExpression::RequiresTwoArguments(special_form))
            }

            SpecialFormType::Optional
            | SpecialFormType::Not
            | SpecialFormType::Top
            | SpecialFormType::Bottom
            | SpecialFormType::TypeOf
            | SpecialFormType::TypeIs
            | SpecialFormType::TypeGuard
            | SpecialFormType::Unpack
            | SpecialFormType::CallableTypeOf
            | SpecialFormType::RegularCallableTypeOf => Err(InvalidTypeExpression::RequiresOneArgument(special_form)),

            // We treat `typing.Type` exactly the same as `builtins.type`:
            SpecialFormType::Type => Ok(effects.known_class_instance(&env, KnownClass::Type).await?),
            SpecialFormType::TypeForm => Ok(effects.type_form(facts.any()).await?),
            SpecialFormType::Tuple => Ok(effects.homogeneous_tuple(&env, facts.unknown()).await?),
            SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                Ok(effects.unknown_callable().await?)
            }
            SpecialFormType::LegacyStdlibAlias(alias) => {
                Ok(effects.known_class_instance(&env, facts.aliased_class(alias)).await?)
            }
            SpecialFormType::TypeQualifier(qualifier) => {
                Err(InvalidTypeExpression::TypeQualifier(qualifier))
            }
        })
    }

}

pub(in crate::types) async fn in_type_expression_special_form_with<
    'db,
    E: SpecialFormConversionEffects<'db>,
>(
    special_form: SpecialFormType,
    scope: ScopeId<'db>,
    binding: Option<Definition<'db>>,
    flags: InferenceFlags,
    facts: SpecialFormConversionFacts,
    effects: &E,
) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, E::Error> {
    Ok(
        special_form_type_expression_with(special_form, scope, binding, flags, facts, effects)
            .await?
            .map_err(|err| facts.invalid(err)),
    )
}

pub(in crate::types) fn in_type_expression_special_form_sync<
    'db,
    E: SynchronousSpecialFormConversionEffects<'db>,
>(
    special_form: SpecialFormType,
    scope: ScopeId<'db>,
    binding: Option<Definition<'db>>,
    flags: InferenceFlags,
    facts: SpecialFormConversionFacts,
    effects: &E,
) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, E::Error> {
    Ok(
        special_form_type_expression_sync(special_form, scope, binding, flags, facts, effects)?
            .map_err(|err| facts.invalid(err)),
    )
}

impl<'db> SynchronousSpecialFormConversionEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;

    fn dispatch(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn known_class_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, env))
    }

    fn homogeneous_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        element: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::homogeneous_tuple(self.db, env, element))
    }

    fn type_form(&self, argument: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(TypeFormType::from_type_expression(self.db, argument))
    }

    fn intersection(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(IntersectionType::from_two_elements(
            self.db, env, left, right,
        ))
    }

    fn unknown_callable(&self) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Callable(CallableType::unknown(self.db)))
    }

    fn typing_self(
        &self,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Result<Type<'db>, InvalidTypeExpression<'db>>, Self::Error> {
        typing_self::self_annotation_sync(
            scope_id,
            typevar_binding_context,
            inference_flags,
            typing_self::SelfAnnotationFacts,
            self,
        )
    }
}

#[cfg(test)]
mod tests;
