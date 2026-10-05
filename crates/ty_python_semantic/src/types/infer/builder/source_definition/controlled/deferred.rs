//! Deferred source inference retains the complete definition transaction through child calls.

use std::future::Future;
use std::pin::Pin;

use ruff_python_ast as ast;
#[cfg(test)]
use ruff_text_size::Ranged;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionNodeKey};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::DeferredInferenceOperation;
use crate::types::context::InferContext;
use crate::types::definition_expression::{
    DefinitionExpressionEffects, definition_expression_type_with,
};
use crate::types::function::FunctionDecorators;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::builder::annotated_assignment::AnnotatedAssignmentEffects;
use crate::types::infer::builder::deferred::assignment::{
    DeferredAssignmentFacts, infer_assignment_deferred_with,
};
use crate::types::infer::builder::deferred::{
    DeferredClassWork, DeferredEffects,
};
use crate::types::infer::builder::function::annotations::{
    AnnotationEffects, AnnotationFacts, ParameterCursor, TypeParameterCursor,
    function_annotations_with, function_type_parameters_with, receiver_annotation_with,
    signature_annotations_with, type_parameters_with,
};
use crate::types::infer::builder::function::{
    MethodReceiverKind, function_has_deferred_annotations,
};
use crate::types::infer::builder::type_expression::TypeExpressionMode;
use crate::types::infer::builder::typevar::pep695::TypeParameterDefinitionNode;
use crate::types::infer::builder::{TypeInferenceBuilder, local};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::signature_annotations as observations;
use crate::types::infer::{DefinitionInference, InferenceFlags, InferenceRegion};
use crate::types::{SubclassOfInner, Type, TypeContext, TypeVarKind};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits an annotation continuation, its factory and fixed result transfers before boxing it.
    /// Captures remain owned by the calling frame if admission fails while children are draining.
    pub(super) async fn function_annotation_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        let quote = size_of::<F::Output>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
            .map(|bytes| (1, bytes))
            .ok_or(RunError::Contract(
                "function annotation future quotation overflow",
            ));
        self.local_quoted_with_fixed_transfers(quote, || Box::pin(make()))
            .await
    }

    /// Checkpoints the builder state used by function annotations, funding its restoration on abort.
    /// Inference maps remain in the enclosing unpublished scope or definition transaction.
    async fn function_annotation_transaction<'builder, 'ast>(
        &self,
        builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<local::BuilderStore<'builder, 'db, 'ast>> {
        // The fixed work covers checkpoint fields, empty owners, completion state and restoration.
        // The transfer helper accounts for the actual store, factory and result representations.
        self.local_with_fixed_transfers(24, 0, || {
            #[cfg(test)]
            observations::transaction_started(
                self.db(),
                local::function_annotation_state(builder),
            );
            let store = local::BuilderStore::new(builder);
            #[cfg(test)]
            let store = {
                let mut store = store;
                store.observe_function_annotations();
                store
            };
            store
        })
        .await
    }

    /// Disarms annotation-state restoration after the shared traversal restores its binding context.
    async fn complete_function_annotation_transaction(
        &self,
        transaction: &mut local::BuilderStore<'_, 'db, '_>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(4, 0, || {
            #[cfg(test)]
            observations::transaction_completed(
                self.db(),
                local::function_annotation_state(transaction.builder(local::BuilderId::ROOT)),
            );
            transaction.complete();
        })
        .await
    }

    /// Infers PEP 695 signature annotations while retaining an abort checkpoint for their scope.
    pub(super) async fn infer_function_type_parameter_annotations<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        let mut transaction = self.function_annotation_transaction(builder).await?;
        let builder = transaction.get_mut(local::BuilderId::ROOT);
        let annotations = self
            .function_annotation_future(|| {
                function_type_parameters_with(builder, function, AnnotationFacts, self)
            })
            .await?;
        #[cfg(test)]
        let annotations = observations::observe_type_parameter_polling(annotations);
        annotations.await?;
        self.complete_function_annotation_transaction(&mut transaction)
            .await
    }

    pub(in crate::types::infer) async fn infer_deferred_definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract(
                "prepared deferred definition file is foreign",
            ));
        }
        let env = ProgramEnvironment::from_file(source.file);
        let mut owner = self
            .empty_builder(&source, &env, InferenceRegion::Deferred(definition))
            .await?;
        owner
            .builder
            .infer_region_deferred_with(self, definition)
            .await?;
        self.finish_definition_owner(owner, definition).await
    }

    pub(in crate::types::infer) async fn definition_expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        definition_expression_type_with(definition, expression, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DeferredEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(8).await
    }

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        super::DefinitionEffects::definition_kind(self, db, definition).await
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> RunResult<bool> {
        self.file_is_stub(context.file()).await
    }

    async fn class_checkpoint(&self, work: DeferredClassWork) -> RunResult<()> {
        let units = match work {
            DeferredClassWork::Begin { keywords } => Self::checked(keywords.checked_add(8))?,
            DeferredClassWork::Base | DeferredClassWork::ExtraItems => 4,
        };
        self.work(units).await
    }

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        local::source::expression(builder, expression, TypeContext::default(), self).await
    }

    async fn is_typed_dict(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _definition: Definition<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::ExtraItemsClassification,
        ))
        .await
    }

    async fn extra_items(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::ExtraItemsAnnotation,
        ))
        .await
    }

    async fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: &'ast ast::Expr,
    ) -> RunResult<()> {
        self.work(8).await?;
        infer_assignment_deferred_with(builder, target, value, DeferredAssignmentFacts, self).await
    }

    async fn function_annotations<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        let mut transaction = self.function_annotation_transaction(builder).await?;
        let builder = transaction.get_mut(local::BuilderId::ROOT);
        self.function_annotation_future(|| {
            function_annotations_with(builder, definition, function, AnnotationFacts, self)
        })
        .await?
        .await?;
        self.complete_function_annotation_transaction(&mut transaction)
            .await
    }

    async fn type_parameter<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: TypeParameterDefinitionNode<'_>,
    ) -> RunResult<()> {
        self.infer_deferred_type_parameter_source(builder, node).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AnnotationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        key: DefinitionNodeKey,
    ) -> RunResult<Definition<'db>> {
        self.function_annotation_future(|| {
            TypeVarBindingEffects::definition(self, builder.index, key)
        })
        .await?
        .await
    }

    async fn type_parameter_cursor<'param>(
        &self,
        parameters: &'param ast::TypeParams,
    ) -> RunResult<TypeParameterCursor<'param>> {
        self.local_with_fixed_transfers(2, 0, || TypeParameterCursor::new(parameters))
            .await
    }

    async fn next_type_parameter<'param>(
        &self,
        cursor: &mut TypeParameterCursor<'param>,
    ) -> RunResult<Option<&'param ast::TypeParam>> {
        self.local_with_fixed_transfers(2, 0, || cursor.next())
            .await
    }

    async fn definition_inference(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        self.function_annotation_future(|| self.access.definition(definition))
            .await?
            .await
    }

    async fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        self.function_annotation_future(|| {
            builder.extend_definition_with(definition, inference, self)
        })
        .await?
        .await
    }

    async fn has_deferred_annotations(&self, function: &ast::StmtFunctionDef) -> RunResult<bool> {
        let parameters = &function.parameters;
        let work = Self::checked(
            parameters
                .posonlyargs
                .len()
                .checked_add(parameters.args.len())
                .and_then(|len| len.checked_add(parameters.kwonlyargs.len()))
                .and_then(|len| len.checked_add(4)),
        )?;
        self.local(work, 0, || function_has_deferred_annotations(function))
            .await
    }

    async fn known_decorators(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<FunctionDecorators> {
        let inference = self.access.function_known_decorators(definition).await?;
        self.local(1, 0, || inference.known_decorators()).await
    }

    async fn replace_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        binding: Option<Definition<'db>>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(1, 0, || {
            std::mem::replace(&mut builder.typevar_binding_context, binding)
        })
        .await
    }

    async fn replace_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> RunResult<bool> {
        self.local(1, 0, || {
            builder.context.inference_flags.replace(flag, value)
        })
        .await
    }

    async fn signature_annotations(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        signature_annotations_with(builder, definition, function, AnnotationFacts, self).await
    }

    async fn receiver_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<Option<bool>> {
        receiver_annotation_with(builder, definition, function, AnnotationFacts, self).await
    }

    async fn in_class_scope(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let scope = self.definition_scope(definition).await?;
        let metadata = self.scope_metadata(scope).await?;
        self.local(1, 0, || metadata.kind().is_class()).await
    }

    async fn accepts_receiver(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        kind: MethodReceiverKind,
        annotation: Type<'db>,
    ) -> RunResult<bool> {
        let variable = self
            .local(2, 0, || match (kind, annotation) {
                (MethodReceiverKind::Instance, Type::TypeVar(variable)) => Some(variable),
                (MethodReceiverKind::Class, Type::SubclassOf(subclass)) => {
                    match subclass.subclass_of() {
                        SubclassOfInner::TypeVar(variable) => Some(variable),
                        _ => None,
                    }
                }
                _ => None,
            })
            .await?;
        let Some(variable) = variable else {
            return Ok(false);
        };
        let variable = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        let kind = TypeVarBindingEffects::kind(self, variable).await?;
        self.local(1, 0, || matches!(kind, TypeVarKind::TypingSelf))
            .await
    }

    async fn annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.local(1, 0, || {
            #[cfg(test)]
            observations::annotation_entered(
                self.db(),
                local::function_annotation_state(builder),
                expression.range(),
            );
        })
        .await?;
        let state = AnnotatedAssignmentEffects::defer_annotations(self, builder).await?;
        let ty = local::source::type_expression(
            builder,
            expression,
            TypeExpressionMode::ScopedWithState(state),
            self,
        )
        .await?;
        self.local(1, 0, || {
            #[cfg(test)]
            observations::annotation_completed(
                self.db(),
                local::function_annotation_state(builder),
                expression.range(),
            );
        })
        .await?;
        Ok(ty)
    }

    async fn type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameters: &ast::TypeParams,
    ) -> RunResult<()> {
        self.function_annotation_future(|| {
            type_parameters_with(builder, parameters, AnnotationFacts, self)
        })
        .await?
        .await
    }

    async fn parameter_cursor<'param>(
        &self,
        parameters: &'param ast::Parameters,
        skip_first: bool,
    ) -> RunResult<ParameterCursor<'param>> {
        self.local(4, 0, || ParameterCursor::new(parameters, skip_first))
            .await
    }

    async fn next_parameter<'param>(
        &self,
        cursor: &mut ParameterCursor<'param>,
    ) -> RunResult<Option<&'param ast::ParameterWithDefault>> {
        self.local(3, 0, || cursor.next()).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DefinitionExpressionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn in_definition_scope(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<bool> {
        let scope = self.definition_scope(definition).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        self.local(4, 0, || {
            source.index.expression_scope_id(expression) == file_scope
        })
        .await
    }

    async fn definition_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Option<Type<'db>>> {
        let inference = self.access.definition(definition).await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(1))?;
        self.local(work, 0, || inference.try_expression_type(expression))
            .await
    }

    async fn deferred_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Option<Type<'db>>> {
        let inference = self.access.deferred_definition(definition).await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(1))?;
        self.local(work, 0, || inference.try_expression_type(expression))
            .await
    }

    async fn is_function(&self, definition: Definition<'db>) -> RunResult<bool> {
        Ok(matches!(
            self.field(definition.read_fields(self.db()).kind()).await?,
            DefinitionKind::Function(_)
        ))
    }

    async fn function_default_type(
        &self,
        _definition: Definition<'db>,
        _expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::FunctionDefaults,
        ))
        .await
    }

    async fn complete_scope_type(
        &self,
        _definition: Definition<'db>,
        _expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::ExpressionScope,
        ))
        .await
    }
}
