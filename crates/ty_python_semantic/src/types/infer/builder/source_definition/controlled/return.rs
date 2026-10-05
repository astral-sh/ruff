//! Controlled return inference retains the original lexical signature and local bindings.

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::AncestorsIter;
use ty_python_core::definition::Definition;
use ty_python_core::scope::Scope;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::function::FunctionType;
use crate::types::infer::builder::source_return::{self, ReturnEffects, ReturnFacts};
use crate::types::infer::builder::{TypeInferenceBuilder, local};
use crate::types::infer::{DefinitionTypes, TypeAndRange};
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::{Type, TypeContext};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn infer_return_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        statement: &ast::StmtReturn,
    ) -> RunResult<()> {
        source_return::infer_return_with(builder, statement, ReturnFacts, self).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ReturnEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn ancestors(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<AncestorsIter<'db>> {
        let file_scope = self
            .field(builder.scope().read_fields(builder.db()).file_scope_id())
            .await?;
        self.local(2, 0, || builder.index.ancestor_scopes(file_scope))
            .await
    }

    async fn next_ancestor(
        &self,
        ancestors: &mut AncestorsIter<'db>,
    ) -> RunResult<Option<&'db Scope>> {
        self.local(2, 0, || ancestors.next().map(|(_, scope)| scope))
            .await
    }

    async fn function_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: &Scope,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(usize::BITS as usize * 4 + 8, 0, || {
            scope
                .node()
                .as_function()
                .map(|function| builder.index.expect_single_definition(function))
        })
        .await
    }

    async fn function_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<FunctionType<'db>>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.declarations.len(),
            _ => 1,
        };
        self.local(Self::checked(entries.checked_add(2))?, 0, || {
            inference.function_type(definition)
        })
        .await
    }

    async fn raw_lexical_return_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<Type<'db>> {
        let last_definition = self
            .field(function.field_requests(self.db()).literal())
            .await?
            .last_definition;
        let signature = last_definition
            .raw_signature_with(self.db(), ReturnCallableTypeVarScope::Lexical, self)
            .await?;
        let work = Self::checked(
            signature
                .retirement_work()
                .and_then(|work| work.checked_add(1)),
        )?;
        self.local(work, 0, || {
            let return_ty = signature.return_ty;
            drop(signature);
            return_ty
        })
        .await
    }

    async fn is_generator(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        let file_scope = self
            .field(builder.scope().read_fields(builder.db()).file_scope_id())
            .await?;
        self.local(2, 0, || file_scope.is_generator_function(builder.index))
            .await
    }

    async fn generator_return_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ReturnGeneratorType).await
    }

    async fn infer_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        local::source::expression(builder, expression, context, self).await
    }

    async fn none_type(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ReturnNoneType).await
    }

    async fn record_return(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        range: TextRange,
    ) -> RunResult<()> {
        let returns = &builder.return_types_and_ranges;
        let grows = returns.len() == returns.capacity();
        let work = if grows {
            Self::checked(returns.len().checked_add(4))?
        } else {
            4
        };
        let bytes = if grows {
            Self::checked(
                returns
                    .len()
                    .checked_add(1)
                    .and_then(|count| count.checked_mul(size_of::<TypeAndRange<'db>>())),
            )?
        } else {
            0
        };
        self.local(work, bytes, || {
            if grows {
                builder.return_types_and_ranges.reserve_exact(1);
            }
            builder.record_return_type(ty, range);
        })
        .await
    }
}
