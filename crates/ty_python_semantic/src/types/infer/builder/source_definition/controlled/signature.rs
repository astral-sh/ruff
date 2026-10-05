//! Canonical source signatures borrow the source root's admission and preparation capabilities.

mod context;
mod implicit_receiver;

use ruff_python_ast as ast;
use salsa::execution_probe::{FieldRequest, RunError, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{Scope, ScopeId};
use ty_python_core::{ProgramFile, SemanticIndex};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class::class_default_specialization_with;
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::function::source::{FunctionSignatureEffects, FunctionSignatureSource};
use crate::types::function::{FunctionMetadataEffects, FunctionType, OverloadLiteral};
use crate::types::generics::GenericContext;
use crate::types::infer::{DefinitionInferenceExtra, TypeExpressionFlags};
use crate::types::generics::signature_context::{merge_signature_contexts_with, signature_context_with};
use crate::types::signatures::annotations::{
    SignatureAnnotationEffects, signature_annotation_flags_with, signature_annotation_type_with,
};
use crate::types::signatures::implicit_receiver::{context_has_explicit_self_with, install_implicit_receiver_with};
use crate::types::signatures::source::SignatureSourceEffects;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::subclass_of::{SubclassConstructionFacts, subclass_from_with};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, KnownClass, SubclassOfInner, Type,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_function_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<CallableSignature<'db>> {
        let file = self.function_file(function).await?;
        self.check_file_program(file).await?;
        let signature = function.literal_signature_with(self.db(), self).await?;
        #[cfg(test)]
        self.local(1, 0, || {
            super::observations::observe(
                self.db(),
                super::observations::Event::FunctionSignatureReady,
            );
        })
        .await?;
        Ok(signature)
    }

}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SignatureAnnotationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn annotation_scope(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<ScopeId<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        // The scope lookup binary-searches a sorted map of fixed-size node keys.
        self.local(usize::BITS as usize + 8, 0, || {
            index
                .try_expression_scope_id(expression)
                .map(|scope| index.scope_id(scope))
                .ok_or(RunError::Contract(
                    "signature annotation is absent from its semantic index",
                ))
        })
        .await?
    }

    async fn is_definition_scope(
        &self,
        definition: Definition<'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<bool> {
        let definition_scope = self.definition_scope(definition).await?;
        self.local(1, 0, || scope == definition_scope).await
    }

    async fn deferred_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let inference = self.access.deferred_definition(definition).await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(3))?;
        self.local(work, 0, || inference.expression_type(expression))
            .await
    }

    async fn deferred_flags(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<TypeExpressionFlags> {
        let inference = self.access.deferred_definition(definition).await?;
        let length = match inference.extra.as_deref() {
            Some(DefinitionInferenceExtra::Other(extra)) => {
                extra.type_expression_flags.iter().len()
            }
            Some(_) | None => 0,
        };
        let work = Self::checked(length.checked_add(3))?;
        self.local(work, 0, || inference.type_expression_flags(expression))
            .await
    }

    async fn complete_scope_type(
        &self,
        scope: ScopeId<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let inference = self.complete_scope_types(scope).await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(3))?;
        self.local(work, 0, || inference.expression_type(expression))
            .await
    }

    async fn complete_scope_flags(
        &self,
        scope: ScopeId<'db>,
        expression: &ast::Expr,
    ) -> RunResult<TypeExpressionFlags> {
        let inference = self.complete_scope_types(scope).await?;
        let length = inference
            .extra
            .as_deref()
            .map_or(0, |extra| extra.type_expression_flags.iter().len());
        let work = Self::checked(length.checked_add(3))?;
        self.local(work, 0, || inference.type_expression_flags(expression))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SignatureSourceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        SourceEffects::local(
            self,
            Self::checked(work)?,
            Self::checked(requested_bytes)?,
            action,
        )
        .await
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        SourceEffects::field(self, request).await
    }

    async fn semantic_index(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<&'db SemanticIndex<'db>> {
        self.access.semantic_index(file).await
    }

    async fn parameter_annotation(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<(Type<'db>, TypeExpressionFlags)> {
        let ty = signature_annotation_type_with(definition, expression, self).await?;
        let flags = signature_annotation_flags_with(definition, expression, self).await?;
        Ok((ty, flags))
    }

    async fn return_annotation(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        signature_annotation_type_with(definition, expression, self).await
    }

    async fn unpacked_kwargs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _flags: TypeExpressionFlags,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SignatureParameterNormalization)
            .await
    }

    async fn extend_unpacked_kwargs(
        &self,
        _db: &'db dyn Db,
        _parameters: &mut Vec<Parameter<'db>>,
        _parameter: &Parameter<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SignatureParameterNormalization)
            .await
    }

    async fn normalize_paramspec(
        &self,
        _db: &'db dyn Db,
        _args: BoundTypeVarInstance<'db>,
        _kwargs: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.unavailable(SourceOperation::SignatureParameterNormalization)
            .await
    }

    /// Collects this signature's owned legacy variables through an admitted shared child.
    async fn legacy_generic_context(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.type_parameter_future(|| signature_context_with(definition, parameters, return_ty, self))
            .await?.await
    }

    /// Applies the shared PEP 695 and legacy context policy through an admitted child.
    async fn merge_generic_contexts(
        &self,
        _db: &'db dyn Db,
        pep695: GenericContext<'db>,
        legacy: GenericContext<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.type_parameter_future(|| merge_signature_contexts_with(Some(pep695), Some(legacy), self))
            .await?.await
    }

    async fn rescope_return_callables(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
        definition: Definition<'db>,
    ) -> RunResult<(Option<GenericContext<'db>>, Type<'db>)> {
        self.type_parameter_future(|| self.scope_return_callables(context, parameters, return_ty, definition))
            .await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionSignatureEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn prepare(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> RunResult<FunctionSignatureSource<'db>> {
        let scope = self.field(function.field_requests(db).body_scope()).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        let metadata = self.scope_metadata(scope).await?;
        let work = self
            .local(1, 0, || source.index.definition_lookup_work())
            .await?;
        let definition = self
            .local(Self::checked(work.checked_add(2))?, 0, || {
                let Some(function) = metadata.node().as_function() else {
                    return Err(RunError::Contract("function scope has no function node"));
                };
                Ok(source.index.expect_single_definition(function))
            })
            .await??;
        Ok(FunctionSignatureSource {
            module: source.module,
            index: source.index,
            scope,
            definition,
        })
    }

    async fn scope_metadata(&self, _db: &'db dyn Db, scope: ScopeId<'db>) -> RunResult<&'db Scope> {
        SourceEffects::scope_metadata(self, scope).await
    }

    async fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        FunctionMetadataEffects::overloads_and_implementation(self, db, last_definition).await
    }

    async fn pep695_context(
        &self,
        _db: &'db dyn Db,
        index: &SemanticIndex<'db>,
        definition: Definition<'db>,
        type_params: &ast::TypeParams,
    ) -> RunResult<GenericContext<'db>> {
        self.pep695_context_source(index, definition, type_params).await
    }

    async fn class_is_protocol(
        &self,
        db: &'db dyn Db,
        class_definition: Definition<'db>,
    ) -> RunResult<bool> {
        let Some(class) = self.original_receiver_class(db, class_definition).await? else {
            return Ok(false);
        };
        let ClassLiteral::Static(class) = class else {
            return self
                .unavailable(SourceOperation::SignatureClassReceiver)
                .await;
        };
        let specialized = class_default_specialization_with(class, self).await?;
        let Some((class, _)) = self.static_class_identity(specialized).await? else {
            return Ok(false);
        };
        static_is_protocol_with(class, self).await
    }

    async fn receiver_method_has_explicit_self(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<bool> {
        self.type_parameter_future(|| context_has_explicit_self_with(context, self))
            .await?
            .await
    }

    async fn original_receiver_class(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<Option<ClassLiteral<'db>>> {
        let ty = self.scope_original_class_type(definition).await?;
        self.local(1, 0, || ty.and_then(Type::as_class_literal))
            .await
    }

    async fn receiver_class_is_generic(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        let ClassLiteral::Static(class) = class else {
            return self
                .unavailable(SourceOperation::SignatureClassReceiver)
                .await;
        };
        Ok(self.access.class_generic_context(class).await?.is_some())
    }

    async fn receiver_class_is_fallback(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        let ClassLiteral::Static(class) = class else {
            return self
                .unavailable(SourceOperation::SignatureClassReceiver)
                .await;
        };
        let known = self.field(class.field_requests(db).known()).await?;
        self.local(1, 0, || known.is_some_and(KnownClass::is_fallback_class))
            .await
    }

    async fn synthetic_receiver_self(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        // Prepay the fixed class/binding/result decisions across the canonical children.
        // Missing class or binding is an ordinary signature invariant, not unsupported syntax.
        let binding = self.local_with_fixed_transfers(
            10,
            size_of::<Option<ClassLiteral<'db>>>() * 2
                + size_of::<ClassLiteral<'db>>() * 2
                + size_of::<Option<BoundTypeVarInstance<'db>>>() * 2
                + size_of::<RunResult<BoundTypeVarInstance<'db>>>() * 2,
            || Some(definition),
        ).await?;
        let scope = self.definition_scope(definition).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let class = self.nearest_enclosing_class(index, scope).await?
            .ok_or(RunError::Contract("implicit receiver has no enclosing class"))?;
        let bound = self.type_parameter_future(|| {
            self.typing_self_source(scope, binding, ClassLiteral::Static(class))
        }).await?.await?;
        bound.ok_or(RunError::Contract("implicit receiver Self has no binding"))
    }

    async fn receiver_is_classmethod(
        &self,
        db: &'db dyn Db,
        literal: OverloadLiteral<'db>,
    ) -> RunResult<bool> {
        literal.is_classmethod_with(db, self).await
    }

    async fn receiver_subclass(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        receiver: SubclassOfInner<'db>,
    ) -> RunResult<Type<'db>> {
        subclass_from_with(receiver, SubclassConstructionFacts, self).await
    }

    async fn receiver_instance(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        if !matches!(class, ClassLiteral::Static(_)) {
            return self
                .unavailable(SourceOperation::SignatureClassReceiver)
                .await;
        }
        Type::instance_with(db, env, self, ClassType::NonGeneric(class)).await
    }

    async fn apply_implicit_receiver(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &mut Signature<'db>,
        receiver: Type<'db>,
    ) -> RunResult<()> {
        self.type_parameter_future(|| install_implicit_receiver_with(env, signature, receiver, self))
            .await?
            .await
    }

    async fn wrap_async_return(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _return_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::SignatureAsyncReturn)
            .await
    }

    async fn binding_type(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<Type<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::overload_collection::signature_binding_requested(
            self.db(),
            definition,
        );
        let inference = self.access.definition(definition).await?;
        self.inferred_binding_type(inference, definition).await
    }

    async fn clone_signature(
        &self,
        _db: &'db dyn Db,
        signature: &Signature<'db>,
    ) -> RunResult<Signature<'db>> {
        self.work(Self::checked(signature.parameters().len().checked_add(2))?)
            .await?;
        let work = Self::checked(signature.retirement_work())?;
        let bytes = Self::checked(signature.clone_requested_bytes())?;
        self.local(work, bytes, || signature.clone()).await
    }
}
