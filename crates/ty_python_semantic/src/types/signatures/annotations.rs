use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::semantic_index;

use crate::Db;
use crate::types::Type;
use crate::types::infer::{TypeExpressionFlags, infer_complete_scope_types, infer_deferred_types};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSignatureAnnotationEffects)]
    pub(in crate::types) trait SignatureAnnotationEffects<'db> {
        type Error;

        #[operation(child)]
        async fn annotation_scope(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<ScopeId<'db>, Self::Error>;
        #[operation(local)]
        async fn is_definition_scope(&self, definition: Definition<'db>, scope: ScopeId<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn deferred_type(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn deferred_flags(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<TypeExpressionFlags, Self::Error>;
        #[operation(child)]
        async fn complete_scope_type(&self, scope: ScopeId<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn complete_scope_flags(&self, scope: ScopeId<'db>, expression: &ast::Expr) -> Result<TypeExpressionFlags, Self::Error>;
    }

    #[synchronous(signature_annotation_type_sync)]
    #[capabilities(effects = SignatureAnnotationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn signature_annotation_type_with<'db, E: SignatureAnnotationEffects<'db>>(
        definition: Definition<'db>,
        expression: &ast::Expr,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let scope = effects.annotation_scope(definition, expression).await?;
        if effects.is_definition_scope(definition, scope).await? {
            // expression is in the function definition scope, but always deferred
            effects.deferred_type(definition, expression).await
        } else {
            // expression is in the PEP-695 type params sub-scope
            effects.complete_scope_type(scope, expression).await
        }
    }

    #[synchronous(signature_annotation_flags_sync)]
    #[capabilities(effects = SignatureAnnotationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn signature_annotation_flags_with<'db, E: SignatureAnnotationEffects<'db>>(
        definition: Definition<'db>,
        expression: &ast::Expr,
        effects: &E,
    ) -> Result<TypeExpressionFlags, E::Error> {
        let scope = effects.annotation_scope(definition, expression).await?;
        if effects.is_definition_scope(definition, scope).await? {
            // expression is in the function definition scope, but always deferred
            effects.deferred_flags(definition, expression).await
        } else {
            // expression is in the PEP-695 type params sub-scope
            effects.complete_scope_flags(scope, expression).await
        }
    }
}

pub(super) struct OrdinarySignatureAnnotationEffects<'db>(pub(super) &'db dyn Db);

impl<'db> SynchronousSignatureAnnotationEffects<'db> for OrdinarySignatureAnnotationEffects<'db> {
    type Error = Infallible;

    fn annotation_scope(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<ScopeId<'db>, Infallible> {
        let file = definition.program_file(self.0);
        let index = semantic_index(self.0, file);
        let file_scope = index.expression_scope_id(expression);
        Ok(file_scope.to_scope_id(self.0, file))
    }

    fn is_definition_scope(
        &self,
        definition: Definition<'db>,
        scope: ScopeId<'db>,
    ) -> Result<bool, Infallible> {
        Ok(scope == definition.scope(self.0))
    }

    fn deferred_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(infer_deferred_types(self.0, definition).expression_type(expression))
    }

    fn deferred_flags(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<TypeExpressionFlags, Infallible> {
        Ok(infer_deferred_types(self.0, definition).type_expression_flags(expression))
    }

    fn complete_scope_type(
        &self,
        scope: ScopeId<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(infer_complete_scope_types(self.0, scope).expression_type(expression))
    }

    fn complete_scope_flags(
        &self,
        scope: ScopeId<'db>,
        expression: &ast::Expr,
    ) -> Result<TypeExpressionFlags, Infallible> {
        Ok(infer_complete_scope_types(self.0, scope).type_expression_flags(expression))
    }
}
