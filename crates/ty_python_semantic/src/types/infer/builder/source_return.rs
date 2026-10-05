//! Return expressions use the lexical signature of the nearest enclosing function.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::AncestorsIter;
use ty_python_core::definition::Definition;
use ty_python_core::scope::Scope;

use super::TypeInferenceBuilder;
use crate::types::function::{FunctionType, same_module_uncached_raw_signature};
use crate::types::infer::infer_definition_types;
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::{Type, TypeContext};

pub(super) struct ReturnFacts;
pub(super) struct OrdinaryReturnEffects;

shared_semantic_family! {
    #[synchronous(SynchronousReturnEffects)]
    pub(super) trait ReturnEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        async fn ancestors(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<AncestorsIter<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor(&self, ancestors: &mut AncestorsIter<'db>) -> Result<Option<&'db Scope>, Self::Error>;
        #[operation(local)]
        async fn function_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: &Scope) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(child)]
        async fn function_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<FunctionType<'db>>, Self::Error>;
        #[operation(source)]
        async fn raw_lexical_return_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn is_generator(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn generator_return_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn infer_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, context: TypeContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn none_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn record_return(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, range: TextRange) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ReturnFacts {
        fn has_value(&self, statement: &ast::StmtReturn) -> bool {
            statement.value.is_some()
        }
        fn value<'a>(&self, statement: &'a ast::StmtReturn) -> Option<&'a ast::Expr> {
            statement.value.as_deref()
        }
        fn range(&self, statement: &ast::StmtReturn) -> TextRange {
            statement.value.as_ref().map_or(statement.range(), |value| value.range())
        }
        fn default_context<'db>(&self) -> TypeContext<'db> {
            TypeContext::default()
        }
        fn context<'db>(&self, ty: Type<'db>) -> TypeContext<'db> {
            TypeContext::new(Some(ty))
        }
    }

    #[synchronous(infer_return_sync)]
    #[capabilities(effects = ReturnEffects, facts = ReturnFacts)]
    #[passive_values()]
    pub(super) async fn infer_return_with<'db, 'ast, E: ReturnEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtReturn,
        facts: ReturnFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        #[passive_state]
        let mut context = facts.default_context();
        if facts.has_value(statement) {
            let mut ancestors = effects.ancestors(builder).await?;
            #[cursor_loop]
            while let Some(scope) = effects.next_ancestor(&mut ancestors).await? {
                if let Some(definition) = effects.function_definition(builder, scope).await?
                    && let Some(function) = effects.function_type(builder, definition).await?
                {
                    // Body expressions retain lexical type variables, and async return types
                    // remain unwrapped. A generator contributes its return type parameter.
                    let return_ty = effects.raw_lexical_return_type(builder, function).await?;
                    let context_ty = if effects.is_generator(builder).await? {
                        match effects.generator_return_type(builder, return_ty).await? {
                            Some(ty) => ty,
                            None => return_ty,
                        }
                    } else {
                        return_ty
                    };
                    context = facts.context(context_ty);
                    break;
                }
            }
        }
        let ty = if let Some(value) = facts.value(statement) {
            effects.infer_expression(builder, value, context).await?
        } else {
            effects.none_type(builder).await?
        };
        effects.record_return(builder, ty, facts.range(statement)).await
    }
}

impl<'db, 'ast> SynchronousReturnEffects<'db, 'ast> for OrdinaryReturnEffects {
    type Error = Infallible;

    fn ancestors(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<AncestorsIter<'db>, Self::Error> {
        Ok(builder
            .index
            .ancestor_scopes(builder.scope().file_scope_id(builder.db())))
    }

    fn next_ancestor(
        &self,
        ancestors: &mut AncestorsIter<'db>,
    ) -> Result<Option<&'db Scope>, Self::Error> {
        Ok(ancestors.next().map(|(_, scope)| scope))
    }

    fn function_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: &Scope,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(scope
            .node()
            .as_function()
            .map(|function| builder.index.expect_single_definition(function)))
    }

    fn function_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<FunctionType<'db>>, Self::Error> {
        Ok(infer_definition_types(builder.db(), definition).function_type(definition))
    }

    fn raw_lexical_return_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(same_module_uncached_raw_signature(
            builder.db(),
            function,
            ReturnCallableTypeVarScope::Lexical,
        )
        .return_ty)
    }

    fn is_generator(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error> {
        Ok(builder
            .scope()
            .file_scope_id(builder.db())
            .is_generator_function(builder.index))
    }

    fn generator_return_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty.generator_return_type(builder.db(), builder.program_environment()))
    }

    fn infer_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.infer_expression(expression, context))
    }

    fn none_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::none(builder.db(), builder.program_environment()))
    }

    fn record_return(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        range: TextRange,
    ) -> Result<(), Self::Error> {
        builder.record_return_type(ty, range);
        Ok(())
    }
}
