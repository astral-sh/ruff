//! Selection of the canonical inference result that owns a definition's expression.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::semantic_index;

use crate::Db;
use crate::types::Type;
use crate::types::infer::{
    infer_complete_scope_types, infer_deferred_types, infer_definition_types,
    infer_function_default_types,
};

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousDefinitionExpressionEffects)]
pub(in crate::types) trait DefinitionExpressionEffects<'db> {
    type Error;

    #[operation(child)]
    async fn in_definition_scope(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<bool, Self::Error>;

    #[operation(child)]
    async fn definition_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    #[operation(child)]
    async fn deferred_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    #[operation(local)]
    async fn is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;

    #[operation(child)]
    async fn function_default_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    #[operation(child)]
    async fn complete_scope_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;
}

#[synchronous(definition_expression_type_sync)]
#[capabilities(effects = DefinitionExpressionEffects)]
#[passive_values(Type::unknown)]
pub(in crate::types) async fn definition_expression_type_with<
    'db,
    E: DefinitionExpressionEffects<'db>,
>(
    definition: Definition<'db>,
    expression: &ast::Expr,
    effects: &E,
) -> Result<Type<'db>, E::Error> {
    if !effects.in_definition_scope(definition, expression).await? {
        return effects.complete_scope_type(definition, expression).await;
    }
    if let Some(ty) = effects.definition_type(definition, expression).await? {
        return Ok(ty);
    }
    if let Some(ty) = effects.deferred_type(definition, expression).await? {
        return Ok(ty);
    }
    if effects.is_function(definition).await? {
        return effects.function_default_type(definition, expression).await;
    }
    Ok(Type::unknown())
}
}

pub(in crate::types) struct InlineDefinitionExpressionEffects<'db>(pub &'db dyn Db);

impl<'db> SynchronousDefinitionExpressionEffects<'db> for InlineDefinitionExpressionEffects<'db> {
    type Error = Infallible;

    fn in_definition_scope(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<bool, Infallible> {
        let file = definition.program_file(self.0);
        let index = semantic_index(self.0, file);
        Ok(index.expression_scope_id(expression) == definition.scope(self.0).file_scope_id(self.0))
    }

    fn definition_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(infer_definition_types(self.0, definition).try_expression_type(expression))
    }

    fn deferred_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(infer_deferred_types(self.0, definition).try_expression_type(expression))
    }

    fn is_function(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(matches!(
            definition.kind(self.0),
            DefinitionKind::Function(_)
        ))
    }

    fn function_default_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(infer_function_default_types(self.0, definition).expression_type(expression))
    }

    fn complete_scope_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        let file = definition.program_file(self.0);
        let index = semantic_index(self.0, file);
        let scope = index
            .expression_scope_id(expression)
            .to_scope_id(self.0, file);
        Ok(infer_complete_scope_types(self.0, scope).expression_type(expression))
    }
}
