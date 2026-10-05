use std::convert::Infallible;

use smallvec::smallvec_inline;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use super::InlineConversion;
use crate::ProgramEnvironment;
use crate::types::generics::bind_typevar;
use crate::types::infer::InferenceFlags;
use crate::types::known_instance::{InternedType, UnionTypeInstance};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, InvalidTypeExpression, InvalidTypeExpressionError, KnownInstanceType,
    Type, TypeVarKind,
};

pub(in crate::types) struct KnownInstanceConversionFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousKnownInstanceConversionEffects)]
    pub(in crate::types) trait KnownInstanceConversionEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn dispatch(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn kind(&self, variable: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(source)]
        async fn scope_program_file(&self, scope: ScopeId<'db>) -> Result<ProgramFile<'db>, Self::Error>;
        #[operation(child)]
        async fn semantic_index(&self, file: ProgramFile<'db>) -> Result<&'db SemanticIndex<'db>, Self::Error>;
        #[operation(source)]
        async fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> Result<FileScopeId, Self::Error>;
        #[operation(child)]
        async fn bind_typevar(&self, index: &SemanticIndex<'db>, scope: FileScopeId, binding: Option<Definition<'db>>, variable: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn interned_inner(&self, inner: InternedType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_result(&self, union: UnionTypeInstance<'db>) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error>;
        #[operation(child)]
        async fn to_meta_type(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl KnownInstanceConversionFacts {
        fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_scope(scope)
        }

        fn allow_paramspec(&self, flags: InferenceFlags) -> bool {
            flags.contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR)
        }

        fn in_unpack(&self, flags: InferenceFlags) -> bool {
            flags.contains(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT)
        }

        fn is_paramspec(&self, kind: TypeVarKind) -> bool {
            kind.is_paramspec()
        }

        fn is_typevartuple(&self, kind: TypeVarKind) -> bool {
            kind.is_typevartuple()
        }

        fn invalid<'db>(&self, invalid: InvalidTypeExpression<'db>) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
            Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec_inline![invalid],
                fallback_type: Type::unknown(),
            })
        }
    }

    #[synchronous(in_type_expression_known_instance_sync)]
    #[capabilities(effects = KnownInstanceConversionEffects, facts = KnownInstanceConversionFacts)]
    #[passive_values(Type::TypeAlias, Type::NewTypeInstance, Type::TypeVar, Type::KnownInstance, Type::Callable, InvalidTypeExpression::InvalidBareParamSpec, InvalidTypeExpression::InvalidBareTypeVarTuple, InvalidTypeExpression::Deprecated, InvalidTypeExpression::Field, InvalidTypeExpression::ConstraintSet, InvalidTypeExpression::ConstraintSetSolution, InvalidTypeExpression::GenericContext, InvalidTypeExpression::Specialization, InvalidTypeExpression::Protocol, InvalidTypeExpression::Generic, InvalidTypeExpression::NamedTupleSpec, InvalidTypeExpression::InvalidType)]
    pub(in crate::types) async fn in_type_expression_known_instance_with<'db, E: KnownInstanceConversionEffects<'db>>(
        known: KnownInstanceType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
        facts: KnownInstanceConversionFacts,
        effects: &E,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, E::Error> {
        effects.dispatch().await?;
        let env = facts.environment(scope);
        match known {
            KnownInstanceType::TypeAliasType(alias) => Ok(Ok(Type::TypeAlias(alias))),
            KnownInstanceType::NewType(newtype) => Ok(Ok(Type::NewTypeInstance(newtype))),
            KnownInstanceType::TypeVar(typevar) => {
                if !facts.allow_paramspec(flags)
                    && facts.is_paramspec(effects.kind(typevar).await?)
                {
                    return Ok(facts.invalid(InvalidTypeExpression::InvalidBareParamSpec(typevar)));
                }
                if !facts.in_unpack(flags)
                    && facts.is_typevartuple(effects.kind(typevar).await?)
                {
                    return Ok(facts.invalid(InvalidTypeExpression::InvalidBareTypeVarTuple(typevar)));
                }
                let file = effects.scope_program_file(scope).await?;
                let index = effects.semantic_index(file).await?;
                let file_scope = effects.scope_file_scope_id(scope).await?;
                Ok(Ok(match effects.bind_typevar(index, file_scope, binding, typevar).await? {
                    Some(bound) => Type::TypeVar(bound),
                    None => Type::KnownInstance(known),
                }))
            }
            KnownInstanceType::Deprecated(_) => Ok(facts.invalid(InvalidTypeExpression::Deprecated)),
            KnownInstanceType::Field(_) => Ok(facts.invalid(InvalidTypeExpression::Field)),
            KnownInstanceType::ConstraintSet(_) => Ok(facts.invalid(InvalidTypeExpression::ConstraintSet)),
            KnownInstanceType::ConstraintSetSolution(_) => Ok(facts.invalid(InvalidTypeExpression::ConstraintSetSolution)),
            KnownInstanceType::GenericContext(_) => Ok(facts.invalid(InvalidTypeExpression::GenericContext)),
            KnownInstanceType::Specialization(_) => Ok(facts.invalid(InvalidTypeExpression::Specialization)),
            KnownInstanceType::SubscriptedProtocol(_) => Ok(facts.invalid(InvalidTypeExpression::Protocol)),
            KnownInstanceType::SubscriptedGeneric(_) => Ok(facts.invalid(InvalidTypeExpression::Generic)),
            KnownInstanceType::NamedTupleSpec(_) => Ok(facts.invalid(InvalidTypeExpression::NamedTupleSpec)),
            KnownInstanceType::UnionType(instance) => {
                // Cloning here is cheap if the result is a `Type` (which is `Copy`). It's more
                // expensive if there are errors.
                effects.union_result(instance).await
            }
            KnownInstanceType::Literal(ty) => Ok(Ok(effects.interned_inner(ty).await?)),
            KnownInstanceType::Annotated(ty) => Ok(Ok(effects.interned_inner(ty).await?)),
            KnownInstanceType::TypeGenericAlias(instance) => {
                // When `type[…]` appears in a value position (e.g. in an implicit type alias),
                // we infer its argument as a type expression. This ensures that we can emit
                // diagnostics for invalid type expressions, and more importantly, that we can
                // make use of stringified annotations. The drawback is that we need to turn
                // instances back into the corresponding subclass-of types here. This process
                // (`int` -> instance of `int` -> subclass of `int`) can be lossy, but it is
                // okay for all valid arguments to `type[…]`.

                let inner = effects.interned_inner(instance).await?;
                Ok(Ok(effects.to_meta_type(inner, &env).await?))
            }
            KnownInstanceType::Callable(callable) => Ok(Ok(Type::Callable(callable))),
            KnownInstanceType::LiteralStringAlias(ty) => Ok(Ok(effects.interned_inner(ty).await?)),
            KnownInstanceType::Sentinel(_) => Ok(Ok(Type::KnownInstance(known))),
            KnownInstanceType::FunctoolsPartial(_)
            | KnownInstanceType::FunctoolsPartialCall(_)
            | KnownInstanceType::MethodWrapper(_)
            | KnownInstanceType::Range { .. } => Ok(facts.invalid(InvalidTypeExpression::InvalidType(
                Type::KnownInstance(known), scope,
            ))),
        }
    }
}

impl<'db> SynchronousKnownInstanceConversionEffects<'db> for InlineConversion<'db> {
    type Error = Infallible;

    fn dispatch(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn kind(&self, variable: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error> {
        Ok(variable.kind(self.db))
    }

    fn scope_program_file(&self, scope: ScopeId<'db>) -> Result<ProgramFile<'db>, Self::Error> {
        Ok(scope.program_file(self.db))
    }

    fn semantic_index(
        &self,
        file: ProgramFile<'db>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error> {
        Ok(semantic_index(self.db, file))
    }

    fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> Result<FileScopeId, Self::Error> {
        Ok(scope.file_scope_id(self.db))
    }

    fn bind_typevar(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        binding: Option<Definition<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        Ok(bind_typevar(self.db, index, scope, binding, variable))
    }

    fn interned_inner(&self, inner: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(inner.inner(self.db))
    }

    fn union_result(
        &self,
        union: UnionTypeInstance<'db>,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
        Ok(union.union_type(self.db).clone())
    }

    fn to_meta_type(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.to_meta_type(self.db, env))
    }
}

#[cfg(test)]
mod tests;
