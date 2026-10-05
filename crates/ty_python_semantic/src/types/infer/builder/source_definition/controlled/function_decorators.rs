//! Function decorator expressions retain their own inference region and canonical result.

use ruff_python_ast as ast;
#[cfg(test)]
use ruff_text_size::Ranged;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult};
use ty_python_core::definition::{Definition, DefinitionKind};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::call::function_bindings::FunctionBindingEffects;
use crate::types::class::KnownClass;
use crate::types::class::static_literal::decorators::DecoratorExpressionCursor;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::infer::builder::function::decorators::{
    FunctionDecoratorEffects, FunctionDecoratorFacts, function_decorators_with,
};
use crate::types::infer::builder::{TypeInferenceBuilder, local};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::function_decorators as observations;
use crate::types::infer::{FunctionDecoratorInference, InferenceFlags, InferenceRegion};
use crate::types::{ClassLiteral, Type, TypeContext};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_function_decorators(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecoratorInference<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        #[cfg(test)]
        let _lifetime = observations::OwnerLifetime::new(definition);
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract(
                "prepared decorator definition file is foreign",
            ));
        }
        let env = ProgramEnvironment::from_file(source.file);
        let mut owner = self
            .empty_builder(
                &source,
                &env,
                InferenceRegion::FunctionDecorators(definition),
            )
            .await?;
        let kind = super::DefinitionEffects::definition_kind(self, self.db(), definition).await?;
        let function = self
            .local(1, 0, || match kind {
                DefinitionKind::Function(function) => Some(function.node(&source.module)),
                _ => None,
            })
            .await?;
        let classification =
            function_decorators_with(&mut owner.builder, function, FunctionDecoratorFacts, self)
                .await?;

        // The common quotation covers freezing decorator expressions and compact arrays,
        // diagnostics, and retiring the builder's unused collections and caches.
        let quote = self.scope_finalization_quote(&owner.builder).await?;
        let work = Self::checked(quote.work.checked_add(8))?;
        let bytes = Self::checked(
            quote
                .bytes
                .checked_add(size_of::<FunctionDecoratorInference<'db>>()),
        )?;
        #[cfg(test)]
        observations::finalizing(self.db(), definition);
        let mut owner = Some(owner);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(work)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: bytes,
                })?;
                endpoint.check_completion()?;
                let owner = owner
                    .take()
                    .ok_or(RunError::Contract("decorator owner already consumed"))?;
                Ok(owner
                    .builder
                    .finish_inferred_function_decorators(classification))
            })
            .await)
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> FunctionDecoratorEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn expression_cursor<'source>(
        &self,
        function: &'source ast::StmtFunctionDef,
    ) -> RunResult<DecoratorExpressionCursor<'source>> {
        self.local(1, 0, || {
            DecoratorExpressionCursor::new(&function.decorator_list)
        })
        .await
    }

    async fn next_expression<'source>(
        &self,
        cursor: &mut DecoratorExpressionCursor<'source>,
    ) -> RunResult<Option<&'source ast::Expr>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let ty =
            local::source::expression(builder, expression, TypeContext::default(), self).await?;
        #[cfg(test)]
        if let InferenceRegion::FunctionDecorators(definition) = builder.region {
            observations::decorator_completed(
                self.db(),
                definition,
                expression.range(),
                builder.context.inference_flags,
            );
        }
        Ok(ty)
    }

    async fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.work(1).await?;
        let work = Self::checked(
            super::storage::slots(builder.expressions.capacity())
                .and_then(|slots| slots.checked_add(8)),
        )?;
        self.local(work, 0, || builder.expression_type(expression))
            .await
    }

    async fn known_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<Option<KnownFunction>> {
        FunctionBindingEffects::known(self, builder.db(), function).await
    }

    async fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.known_call_class(builder.db(), Type::ClassLiteral(class))
            .await
    }

    async fn suppress_diagnostics(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.local(1, 0, || {
            builder.context.inference_flags |= InferenceFlags::IN_NO_TYPE_CHECK;
        })
        .await
    }
}
