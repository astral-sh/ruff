//! Function-body ordering and the eligibility of its parameter and return checks.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use super::{TypeInferenceBuilder, validate_paramspec_components};

pub(super) struct FunctionBodyFacts;
pub(super) struct OrdinaryFunctionBodyEffects;

shared_semantic_family! {
    #[synchronous(SynchronousFunctionBodyEffects)]
    pub(super) trait FunctionBodyEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        #[progress]
        async fn next_parameter<'node>(&self, parameters: &mut ast::ParametersIterator<'node>) -> Result<Option<ast::AnyParameterRef<'node>>, Self::Error>;
        #[operation(child)]
        async fn infer_parameter(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameter: ast::AnyParameterRef<'_>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn paramspec_components(&self, builder: &TypeInferenceBuilder<'db, 'ast>, parameters: &ast::Parameters) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn unpacked_kwargs(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameters: &ast::Parameters) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_body(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, body: &[ast::Stmt]) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn return_types(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, function: &ast::StmtFunctionDef) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl FunctionBodyFacts {
        fn parameters<'node>(&self, function: &'node ast::StmtFunctionDef) -> &'node ast::Parameters {
            &function.parameters
        }
        fn parameter_cursor<'node>(&self, parameters: &'node ast::Parameters) -> ast::ParametersIterator<'node> {
            parameters.iter()
        }
        fn has_variadic_annotations(&self, parameters: &ast::Parameters) -> bool {
            parameters.vararg.as_deref().and_then(ast::Parameter::annotation).is_some()
                || parameters.kwarg.as_deref().and_then(ast::Parameter::annotation).is_some()
        }
        fn has_kwargs_annotation(&self, parameters: &ast::Parameters) -> bool {
            parameters.kwarg.as_deref().and_then(ast::Parameter::annotation).is_some()
        }
        fn body<'node>(&self, function: &'node ast::StmtFunctionDef) -> &'node [ast::Stmt] {
            &function.body
        }
        fn has_return_annotation(&self, function: &ast::StmtFunctionDef) -> bool {
            function.returns.is_some()
        }
    }

    #[synchronous(infer_function_body_sync)]
    #[capabilities(effects = FunctionBodyEffects, facts = FunctionBodyFacts)]
    #[passive_values()]
    pub(super) async fn infer_function_body_with<'db, 'ast, E: FunctionBodyEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &ast::StmtFunctionDef,
        facts: FunctionBodyFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        // Parameter definitions belong to the body scope even though their AST nodes are
        // outside the body, so visit them explicitly before the suite.
        let parameters = facts.parameters(function);
        let mut cursor = facts.parameter_cursor(parameters);
        #[cursor_loop]
        while let Some(parameter) = effects.next_parameter(&mut cursor).await? {
            effects.infer_parameter(builder, parameter).await?;
        }
        if facts.has_variadic_annotations(parameters) {
            effects.paramspec_components(builder, parameters).await?;
        }
        if facts.has_kwargs_annotation(parameters) {
            effects.unpacked_kwargs(builder, parameters).await?;
        }
        effects.infer_body(builder, facts.body(function)).await?;
        if facts.has_return_annotation(function) {
            effects.return_types(builder, function).await?;
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousFunctionBodyEffects<'db, 'ast> for OrdinaryFunctionBodyEffects {
    type Error = Infallible;

    fn next_parameter<'node>(
        &self,
        parameters: &mut ast::ParametersIterator<'node>,
    ) -> Result<Option<ast::AnyParameterRef<'node>>, Self::Error> {
        Ok(parameters.next())
    }

    fn infer_parameter(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: ast::AnyParameterRef<'_>,
    ) -> Result<(), Self::Error> {
        builder.infer_definition(parameter);
        Ok(())
    }

    fn paramspec_components(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        parameters: &ast::Parameters,
    ) -> Result<(), Self::Error> {
        validate_paramspec_components(&builder.context, builder.index, parameters, |expr| {
            builder.file_expression_type(expr)
        });
        Ok(())
    }

    fn unpacked_kwargs(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameters: &ast::Parameters,
    ) -> Result<(), Self::Error> {
        builder.validate_unpacked_typed_dict_kwargs(parameters);
        Ok(())
    }

    fn infer_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        body: &[ast::Stmt],
    ) -> Result<(), Self::Error> {
        builder.infer_body(body);
        Ok(())
    }

    fn return_types(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Self::Error> {
        builder.infer_function_return_types(function);
        Ok(())
    }
}
