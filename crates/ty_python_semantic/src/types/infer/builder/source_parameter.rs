//! Parameter definitions share their annotation, default, receiver, and binding decisions.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeKind};

use super::TypeInferenceBuilder;
use crate::types::{Type, UnionType};

pub(super) struct ParameterFacts;
pub(super) struct OrdinaryParameterEffects;

shared_semantic_family! {
    #[synchronous(SynchronousParameterEffects)]
    pub(super) trait ParameterEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn annotated(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameter: &'ast ast::ParameterWithDefault, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn default_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, default: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn receiver_type(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameter: &ast::Parameter) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn receiver_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<FileScopeId, Self::Error>;
        #[operation(source)]
        async fn bind(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameter: &'ast ast::Parameter, definition: Definition<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn method_receiver(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameter: &ast::Parameter, function: &AstNodeRef<ast::StmtFunctionDef>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<Option<Type<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl ParameterFacts {
        fn annotation<'a>(&self, parameter: &'a ast::ParameterWithDefault) -> Option<&'a ast::Expr> {
            parameter.parameter.annotation.as_deref()
        }
        fn default<'a>(&self, parameter: &'a ast::ParameterWithDefault) -> Option<&'a ast::Expr> {
            parameter.default.as_deref()
        }
        fn parameter<'a>(&self, parameter: &'a ast::ParameterWithDefault) -> &'a ast::Parameter {
            &parameter.parameter
        }
        fn unknown<'db>(&self) -> Type<'db> {
            Type::unknown()
        }
        fn receiver_context<'db>(&self, builder: &TypeInferenceBuilder<'db, '_>, file_scope: FileScopeId) -> Option<(&'db AstNodeRef<ast::StmtFunctionDef>, &'db AstNodeRef<ast::StmtClassDef>)> {
            let function_scope = builder.index.scope(file_scope);
            let function = function_scope.node().as_function()?;
            let mut parent = builder.index.scope(function_scope.parent()?);
            if matches!(parent.kind(), ScopeKind::TypeParams | ScopeKind::TypeAlias) {
                parent = builder.index.scope(parent.parent()?);
            }
            Some((function, parent.node().as_class()?))
        }
    }

    #[synchronous(infer_parameter_definition_sync)]
    #[capabilities(effects = ParameterEffects, facts = ParameterFacts)]
    #[passive_values()]
    pub(super) async fn infer_parameter_definition_with<'db, 'ast, E: ParameterEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
        facts: ParameterFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        if let Some(_annotation) = facts.annotation(parameter) {
            return effects.annotated(builder, parameter, definition).await;
        }
        let ty = if let Some(default) = facts.default(parameter) {
            effects.default_type(builder, default).await?
        } else if let Some(ty) = effects.receiver_type(builder, facts.parameter(parameter)).await? {
            ty
        } else {
            facts.unknown()
        };
        effects.bind(builder, facts.parameter(parameter), definition, ty).await
    }

    #[synchronous(special_first_parameter_sync)]
    #[capabilities(effects = ParameterEffects, facts = ParameterFacts)]
    #[passive_values()]
    pub(super) async fn special_first_parameter_with<'db, 'ast, E: ParameterEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &ast::Parameter,
        facts: ParameterFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let file_scope = effects.receiver_scope(builder).await?;
        let Some((function, class)) = facts.receiver_context(builder, file_scope) else {
            return Ok(None);
        };
        effects.method_receiver(builder, parameter, function, class).await
    }
}

impl<'db, 'ast> SynchronousParameterEffects<'db, 'ast> for OrdinaryParameterEffects {
    type Error = Infallible;

    fn annotated(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_annotated_parameter_definition(parameter, definition);
        Ok(())
    }

    fn default_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        default: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        let default_ty = builder.file_expression_type(default);
        Ok(UnionType::from_two_elements(
            builder.db(),
            builder.program_environment(),
            Type::unknown(),
            default_ty,
        ))
    }

    fn receiver_type(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &ast::Parameter,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        special_first_parameter_sync(builder, parameter, ParameterFacts, self)
    }

    fn bind(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::Parameter,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .add_binding(parameter.into(), definition)
            .insert(builder, ty);
        Ok(())
    }

    fn receiver_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<FileScopeId, Self::Error> {
        Ok(builder.scope().file_scope_id(builder.db()))
    }

    fn method_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &ast::Parameter,
        function: &AstNodeRef<ast::StmtFunctionDef>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(builder.infer_method_receiver_parameter_type(parameter, function, class))
    }
}
