//! Function bodies consume canonical parameter definitions before traversing their suite.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::source_function_body::{
    FunctionBodyEffects, FunctionBodyFacts, infer_function_body_with,
};
use crate::types::infer::builder::source_statement::infer_body_with;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn infer_function_body_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        self.work(6).await?;
        infer_function_body_with(builder, function, FunctionBodyFacts, self).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> FunctionBodyEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next_parameter<'node>(
        &self,
        parameters: &mut ast::ParametersIterator<'node>,
    ) -> RunResult<Option<ast::AnyParameterRef<'node>>> {
        self.local(6, 0, || parameters.next()).await
    }

    async fn infer_parameter(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: ast::AnyParameterRef<'_>,
    ) -> RunResult<()> {
        let definition = self
            .local(2, 0, || builder.index.expect_single_definition(parameter))
            .await?;
        let inference = self.access.definition(definition).await?;
        self.merge_definition(builder, definition, inference).await
    }

    async fn paramspec_components(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _parameters: &ast::Parameters,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FunctionParamSpec).await
    }

    async fn unpacked_kwargs(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _parameters: &ast::Parameters,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FunctionUnpackedKwargs)
            .await
    }

    async fn infer_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        body: &[ast::Stmt],
    ) -> RunResult<()> {
        infer_body_with(builder, body, self).await
    }

    async fn return_types(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FunctionReturnCheck).await
    }
}
