//! Class definitions use canonical interning and admitted unpublished builder storage.

use std::alloc::Layout;

use ruff_python_ast::helpers::ExpressionSearchFrame;
use ruff_python_ast::{self as ast, PythonVersion};
#[cfg(test)]
use ruff_text_size::Ranged;
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_module_resolver::KnownModule;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::storage::{StorageQuote, sequence_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::call::CallError;
use crate::types::class::StaticClassLiteral;
use crate::types::context::InferContext;
use crate::types::function::{DataclassTransformerParams, FunctionType};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::class::source_effects::{
    ClassDefinitionEffects, ClassDefinitionWork, ClassIdentity, sealed,
};
#[cfg(test)]
use crate::types::infer::builder::expression_search::observations;
use crate::types::infer::builder::expression_search::{
    ExpressionSearchCursor, ExpressionSearchEffects, ExpressionSearchPlan, ExpressionSearchVisit,
    contains_string_literal_with,
};
use crate::types::infer::builder::local;
use crate::types::infer::builder::source_definition::SourceDefinitionEffect;
use crate::types::{DataclassParams, KnownClass, Type, TypeContext};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    async fn class_python_version(
        &self,
        context: &InferContext<'db, '_>,
    ) -> RunResult<PythonVersion> {
        let program = self
            .environment_program(context.program_environment())
            .await?;
        let fields = self.access.endpoint().field_request_context();
        let environment = self
            .field(program.field_requests(fields).resolver_environment())
            .await?;
        self.field(environment.read_fields(fields).python_version())
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassDefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> RunResult<bool> {
        self.file_is_stub(context.file()).await
    }

    async fn allocate_vec<T>(&self, capacity: usize) -> RunResult<Vec<T>> {
        let bytes = Self::checked(capacity.checked_mul(size_of::<T>()))?;
        self.local(Self::checked(capacity.checked_add(1))?, bytes, || {
            Vec::with_capacity(capacity)
        })
        .await
    }

    async fn class_literal(
        &self,
        _db: &'db dyn Db,
        identity: ClassIdentity<'_, 'db>,
    ) -> RunResult<StaticClassLiteral<'db>> {
        self.access.class_literal(identity).await
    }

    async fn bind_class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
        ty: Type<'db>,
        undecorated_ty: Option<Type<'db>>,
    ) -> RunResult<()> {
        self.local(1, 0, || builder.undecorated_type = undecorated_ty)
            .await?;
        self.bind_source_declaration(builder, class.into(), definition, ty)
            .await
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.record_source_deferred(builder, definition).await
    }

    async fn checkpoint(&self, work: ClassDefinitionWork) -> Result<(), Self::Error> {
        let units = match work {
            ClassDefinitionWork::InspectDefinition {
                decorators,
                keywords,
                name_bytes,
            } => decorators
                .checked_mul(2)
                .and_then(|count| {
                    keywords
                        .checked_mul(2)
                        .and_then(|keywords| count.checked_add(keywords))
                })
                .and_then(|count| count.checked_add(name_bytes))
                .and_then(|count| count.checked_add(1))
                .ok_or(RunError::Contract("class source quotation overflow"))?,
            ClassDefinitionWork::DecoratorExpression
            | ClassDefinitionWork::MetadataDecorator
            | ClassDefinitionWork::RuntimeDecorator
            | ClassDefinitionWork::OriginalClass
            | ClassDefinitionWork::RecordBinding
            | ClassDefinitionWork::KeywordExpression
            | ClassDefinitionWork::BaseExpression
            | ClassDefinitionWork::RecordDeferred => 1,
        };
        self.work(units).await
    }

    async fn infer_class_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error> {
        local::source::expression(builder, &decorator.expression, TypeContext::default(), self)
            .await
    }

    async fn infer_class_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        local::source::expression(builder, expression, TypeContext::default(), self).await
    }

    async fn class_body_scope(
        &self,
        _db: &'db dyn Db,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
    ) -> Result<ScopeId<'db>, Self::Error> {
        self.check_file_program(builder.program_file()).await?;
        self.local(1, 0, || builder.index.scope_id(scope)).await
    }

    async fn known_class(
        &self,
        _db: &'db dyn Db,
        context: &InferContext<'db, '_>,
        name: &str,
    ) -> Result<Option<KnownClass>, Self::Error> {
        let candidates = self
            .local(
                Self::checked(KnownClass::classification_work(name))?,
                0,
                || KnownClass::candidates_from_name(name),
            )
            .await?;
        let Some(candidates) = candidates else {
            return Ok(None);
        };
        if let Some(minimum) = candidates.minimum_python_version
            && self.class_python_version(context).await? < minimum
        {
            return Ok(None);
        }
        let file = context.program_file();
        self.check_file_program(file).await?;
        let Some(module) = self.access.known_module(file).await? else {
            return Ok(None);
        };
        let python_version = self.class_python_version(context).await?;
        self.local(8, 0, || candidates.matching_module(python_version, module))
            .await
    }

    async fn known_module(
        &self,
        _db: &'db dyn Db,
        context: &InferContext<'db, '_>,
    ) -> Result<Option<KnownModule>, Self::Error> {
        let file = context.program_file();
        self.check_file_program(file).await?;
        self.access.known_module(file).await
    }

    async fn default_dataclass_params(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> Result<DataclassParams<'db>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassMetadata,
        ))
        .await
    }

    async fn dataclass_transformer_params(
        &self,
        _db: &'db dyn Db,
        _function: FunctionType<'db>,
    ) -> Result<Option<DataclassTransformerParams<'db>>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassMetadata,
        ))
        .await
    }

    async fn dataclass_params_from_transformer(
        &self,
        _db: &'db dyn Db,
        _params: DataclassTransformerParams<'db>,
    ) -> Result<DataclassParams<'db>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassMetadata,
        ))
        .await
    }

    async fn apply_class_decorator(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _decorator_ty: Type<'db>,
        _decorated_ty: Type<'db>,
    ) -> Result<Result<Type<'db>, CallError<'db>>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassDecorator,
        ))
        .await
    }

    async fn decorator_error_return_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _error: &CallError<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassDecorator,
        ))
        .await
    }

    async fn is_unknown_decorator_result(
        &self,
        _db: &'db dyn Db,
        _result_ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassRelation,
        ))
        .await
    }

    async fn type_retains_original_class(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _decorated_class: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassRelation,
        ))
        .await
    }

    async fn class_decorator_preserves_class_binding(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _decorated_class: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassRelation,
        ))
        .await
    }

    async fn merge_class_preserving_decorator_result(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _current_binding: Type<'db>,
        _decorated_binding: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.unavailable(SourceOperation::Definition(
            SourceDefinitionEffect::ClassRelation,
        ))
        .await
    }

    async fn class_bases_contain_string_literal(
        &self,
        class: &ast::StmtClassDef,
    ) -> Result<bool, Self::Error> {
        contains_string_literal_with(class.bases(), self).await
    }
}

fn search_append_quote(len: usize, capacity: usize) -> Option<(StorageQuote, usize)> {
    let mut quote = sequence_merge::<ExpressionSearchFrame<'_>>(len, capacity, 1)?;
    let frame_bytes = size_of::<ExpressionSearchFrame<'_>>();
    quote.work = quote.work.checked_add(frame_bytes.checked_mul(2)?)?;
    let additional = if quote.bytes == 0 {
        0
    } else {
        let requested = quote.bytes.checked_div(frame_bytes)?;
        Layout::array::<ExpressionSearchFrame<'_>>(requested).ok()?;
        // Growth pays for moving the live prefix, retiring the old backing, and disposing of
        // the new backing even if a later admission refuses or native cancellation unwinds.
        quote.work = quote
            .work
            .checked_add(len.checked_mul(frame_bytes)?)?
            .checked_add(capacity.checked_mul(frame_bytes)?)?
            .checked_add(quote.bytes.checked_mul(2)?)?;
        requested.checked_sub(len)?
    };
    Some((quote, additional))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExpressionSearchEffects
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start<'ast>(
        &self,
        expressions: &'ast [ast::Expr],
    ) -> RunResult<ExpressionSearchCursor<'ast>> {
        self.local(size_of::<ExpressionSearchCursor<'ast>>() * 2 + 1, 0, || {
            let cursor =
                ExpressionSearchCursor::new(ExpressionSearchFrame::expressions(expressions));
            #[cfg(test)]
            let cursor = {
                let mut cursor = cursor;
                cursor.observe_lifetime();
                cursor
            };
            cursor
        })
        .await
    }

    async fn next<'ast>(
        &self,
        cursor: &mut ExpressionSearchCursor<'ast>,
    ) -> RunResult<Option<ExpressionSearchVisit<'ast>>> {
        let (plan, quote, additional) = self
            .local(size_of::<ExpressionSearchPlan<'ast>>() * 2 + 32, 0, || {
                let plan = cursor.plan();
                let (quote, additional) = if plan.enters_child() {
                    let (len, capacity) = cursor.storage();
                    search_append_quote(len, capacity).ok_or(RunError::Contract(
                        "expression search storage quotation overflow",
                    ))?
                } else {
                    (StorageQuote::default(), 0)
                };
                Ok::<_, RunError>((plan, quote, additional))
            })
            .await??;
        #[cfg(test)]
        if additional != 0 {
            observations::before_growth(self.db(), cursor.storage());
        }
        self.local(
            Self::checked(
                quote
                    .work
                    .checked_add(size_of::<ExpressionSearchPlan<'ast>>() * 2 + 1),
            )?,
            quote.bytes,
            || {
                let step = cursor.commit(plan, additional);
                #[cfg(test)]
                if additional != 0 {
                    observations::after_growth(self.db(), cursor.storage());
                }
                step
            },
        )
        .await
    }

    async fn is_string_literal(&self, expression: &ast::Expr) -> RunResult<bool> {
        self.local(1, 0, || {
            #[cfg(test)]
            observations::visit(expression.range());
            expression.is_string_literal_expr()
        })
        .await
    }

    async fn finish(
        &self,
        _cursor: &mut ExpressionSearchCursor<'_>,
        found: bool,
    ) -> RunResult<bool> {
        self.local(1, 0, || found).await
    }
}
