use std::convert::Infallible;
use std::slice;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::{Definition, DefinitionNodeKey};

use super::{MethodReceiverKind, function_has_deferred_annotations};
use crate::types::Type;
use crate::types::function::FunctionDecorators;
use crate::types::infer::builder::{DeferredExpressionState, TypeInferenceBuilder};
use crate::types::infer::{
    DefinitionInference, InferenceFlags, function_known_decorator_flags, infer_definition_types,
};

pub(in crate::types::infer) struct AnnotationFacts;
pub(in crate::types::infer::builder) struct OrdinaryAnnotationEffects;

/// Visits the explicit type-parameter declarations in their source order.
#[derive(Debug)]
pub(in crate::types::infer) struct TypeParameterCursor<'param> {
    parameters: slice::Iter<'param, ast::TypeParam>,
}

impl<'param> TypeParameterCursor<'param> {
    pub(in crate::types::infer) fn new(parameters: &'param ast::TypeParams) -> Self {
        Self {
            parameters: parameters.type_params.iter(),
        }
    }

    pub(in crate::types::infer) fn next(&mut self) -> Option<&'param ast::TypeParam> {
        self.parameters.next()
    }
}

pub(in crate::types::infer) struct ParameterCursor<'param> {
    positional_only: slice::Iter<'param, ast::ParameterWithDefault>,
    positional: slice::Iter<'param, ast::ParameterWithDefault>,
    keyword_only: slice::Iter<'param, ast::ParameterWithDefault>,
}

impl<'param> ParameterCursor<'param> {
    pub(in crate::types::infer) fn new(
        parameters: &'param ast::Parameters,
        skip_first: bool,
    ) -> Self {
        let mut cursor = Self {
            positional_only: parameters.posonlyargs.iter(),
            positional: parameters.args.iter(),
            keyword_only: parameters.kwonlyargs.iter(),
        };
        if skip_first {
            let _ = cursor.next();
        }
        cursor
    }

    pub(in crate::types::infer) fn next(&mut self) -> Option<&'param ast::ParameterWithDefault> {
        self.positional_only
            .next()
            .or_else(|| self.positional.next())
            .or_else(|| self.keyword_only.next())
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousAnnotationEffects)]
    pub(in crate::types::infer) trait AnnotationEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, key: DefinitionNodeKey) -> Result<Definition<'db>, Self::Error>;
        #[operation(local)]
        async fn type_parameter_cursor<'param>(&self, parameters: &'param ast::TypeParams) -> Result<TypeParameterCursor<'param>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type_parameter<'param>(&self, cursor: &mut TypeParameterCursor<'param>) -> Result<Option<&'param ast::TypeParam>, Self::Error>;
        #[operation(child)]
        async fn definition_inference(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error>;
        #[operation(child)]
        async fn extend_definition(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, inference: &DefinitionInference<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_deferred_annotations(&self, function: &ast::StmtFunctionDef) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn known_decorators(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<FunctionDecorators, Self::Error>;
        #[operation(local)]
        async fn replace_binding(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, binding: Option<Definition<'db>>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(local)]
        async fn replace_flag(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flag: InferenceFlags, value: bool) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn signature_annotations(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &ast::StmtFunctionDef) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn receiver_annotation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &ast::StmtFunctionDef) -> Result<Option<bool>, Self::Error>;
        #[operation(source)]
        async fn in_class_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn accepts_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>, kind: MethodReceiverKind, annotation: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn annotation(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, parameters: &ast::TypeParams) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn parameter_cursor<'param>(&self, parameters: &'param ast::Parameters, skip_first: bool) -> Result<ParameterCursor<'param>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_parameter<'param>(&self, cursor: &mut ParameterCursor<'param>) -> Result<Option<&'param ast::ParameterWithDefault>, Self::Error>;
    }

    #[finite_capability]
    impl AnnotationFacts {
        fn function_key(&self, function: &ast::StmtFunctionDef) -> DefinitionNodeKey { <DefinitionNodeKey as From<&ast::StmtFunctionDef>>::from(function) }
        fn type_parameter_key(&self, parameter: &ast::TypeParam) -> DefinitionNodeKey {
            match parameter {
                ast::TypeParam::TypeVar(node) => <DefinitionNodeKey as From<&ast::TypeParamTypeVar>>::from(node),
                ast::TypeParam::ParamSpec(node) => <DefinitionNodeKey as From<&ast::TypeParamParamSpec>>::from(node),
                ast::TypeParam::TypeVarTuple(node) => <DefinitionNodeKey as From<&ast::TypeParamTypeVarTuple>>::from(node),
            }
        }
        fn has_decorators(&self, function: &ast::StmtFunctionDef) -> bool { !function.decorator_list.is_empty() }
        fn no_type_check(&self, flags: FunctionDecorators) -> bool { flags.contains(FunctionDecorators::NO_TYPE_CHECK) }
        fn empty_decorators(&self) -> FunctionDecorators { FunctionDecorators::empty() }
        fn parameters<'a>(&self, function: &'a ast::StmtFunctionDef) -> &'a ast::Parameters { &function.parameters }
        fn returns<'a>(&self, function: &'a ast::StmtFunctionDef) -> Option<&'a ast::Expr> { function.returns.as_deref() }
        fn type_parameters<'a>(&self, function: &'a ast::StmtFunctionDef) -> Option<&'a ast::TypeParams> { function.type_params.as_deref() }
        fn receiver<'a>(&self, function: &'a ast::StmtFunctionDef) -> Option<&'a ast::Expr> {
            function.parameters.posonlyargs.first().or_else(|| function.parameters.args.first())?.parameter.annotation.as_deref()
        }
        fn receiver_kind(&self, function: &ast::StmtFunctionDef, decorators: FunctionDecorators) -> Option<MethodReceiverKind> {
            MethodReceiverKind::from_decorators(function, decorators)
        }
        fn init_receiver(&self, function: &ast::StmtFunctionDef, kind: MethodReceiverKind) -> bool {
            function.name.id == "__init__" && kind == MethodReceiverKind::Instance
        }
        fn annotation<'a>(&self, parameter: &'a ast::ParameterWithDefault) -> Option<&'a ast::Expr> { parameter.parameter.annotation.as_deref() }
        fn vararg<'a>(&self, parameters: &'a ast::Parameters) -> Option<&'a ast::Parameter> { parameters.vararg.as_deref() }
        fn kwarg<'a>(&self, parameters: &'a ast::Parameters) -> Option<&'a ast::Parameter> { parameters.kwarg.as_deref() }
        fn variadic_annotation<'a>(&self, parameter: &'a ast::Parameter) -> Option<&'a ast::Expr> { parameter.annotation.as_deref() }
        fn inferred_receiver(&self, incompatible: Option<bool>) -> bool { incompatible.is_some() }
        fn incompatible_receiver(&self, incompatible: Option<bool>) -> bool { incompatible == Some(true) }
    }

    /// Infers each explicit type-parameter declaration and merges its complete canonical result.
    /// Deferred bounds and defaults remain recorded in the containing scope for its deferred drain.
    #[synchronous(type_parameters_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn type_parameters_with<'db, 'ast, E: AnnotationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, parameters: &ast::TypeParams, facts: AnnotationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let mut cursor = effects.type_parameter_cursor(parameters).await?;
        #[cursor_loop]
        while let Some(parameter) = effects.next_type_parameter(&mut cursor).await? {
            let definition = effects.definition(builder, facts.type_parameter_key(parameter)).await?;
            let inference = effects.definition_inference(builder, definition).await?;
            effects.extend_definition(builder, definition, inference).await?;
        }
        Ok(())
    }

    /// Infers a PEP 695 function's signature annotations in its type-parameter scope.
    /// The function owns bindings introduced by those annotations. Fallible callers retain a
    /// builder checkpoint until the previous binding context has been restored successfully.
    #[synchronous(function_type_parameters_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn function_type_parameters_with<'db, 'ast, E: AnnotationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, function: &ast::StmtFunctionDef, facts: AnnotationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let definition = effects.definition(builder, facts.function_key(function)).await?;
        let previous = effects.replace_binding(builder, Some(definition)).await?;
        effects.signature_annotations(builder, definition, function).await?;
        effects.replace_binding(builder, previous).await?;
        Ok(())
    }

    #[synchronous(function_annotations_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values(InferenceFlags::IN_NO_TYPE_CHECK)]
    pub(in crate::types::infer) async fn function_annotations_with<'db, 'ast, E: AnnotationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &ast::StmtFunctionDef, facts: AnnotationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        // PEP 695 annotations are inferred in the function's type-parameter scope.
        if !effects.has_deferred_annotations(function).await? { return Ok(()); }
        if facts.has_decorators(function) {
            let decorators = effects.known_decorators(builder, definition).await?;
            if facts.no_type_check(decorators) {
                effects.replace_flag(builder, InferenceFlags::IN_NO_TYPE_CHECK, true).await?;
            }
        }
        let previous = effects.replace_binding(builder, Some(definition)).await?;
        effects.signature_annotations(builder, definition, function).await?;
        effects.replace_binding(builder, previous).await?;
        Ok(())
    }

    #[synchronous(signature_annotations_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values(InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER, InferenceFlags::IN_RETURN_TYPE, InferenceFlags::IN_PARAMETER_ANNOTATION, InferenceFlags::IN_VARARG_ANNOTATION, InferenceFlags::IN_KWARG_ANNOTATION)]
    pub(in crate::types::infer) async fn signature_annotations_with<'db, 'ast, E: AnnotationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &ast::StmtFunctionDef, facts: AnnotationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let receiver = effects.receiver_annotation(builder, definition, function).await?;
        let previous = effects.replace_flag(builder, InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER, facts.incompatible_receiver(receiver)).await?;
        if let Some(returns) = facts.returns(function) {
            effects.replace_flag(builder, InferenceFlags::IN_RETURN_TYPE, true).await?;
            effects.annotation(builder, returns).await?;
            effects.replace_flag(builder, InferenceFlags::IN_RETURN_TYPE, false).await?;
        }
        if let Some(parameters) = facts.type_parameters(function) {
            effects.type_parameters(builder, parameters).await?;
        }
        effects.replace_flag(builder, InferenceFlags::IN_PARAMETER_ANNOTATION, true).await?;
        let parameters = facts.parameters(function);
        let mut cursor = effects.parameter_cursor(parameters, facts.inferred_receiver(receiver)).await?;
        #[cursor_loop]
        while let Some(parameter) = effects.next_parameter(&mut cursor).await? {
            if let Some(annotation) = facts.annotation(parameter) {
                effects.annotation(builder, annotation).await?;
            }
        }
        if let Some(vararg) = facts.vararg(parameters) {
            effects.replace_flag(builder, InferenceFlags::IN_VARARG_ANNOTATION, true).await?;
            if let Some(annotation) = facts.variadic_annotation(vararg) {
                effects.annotation(builder, annotation).await?;
            }
            effects.replace_flag(builder, InferenceFlags::IN_VARARG_ANNOTATION, false).await?;
        }
        if let Some(kwarg) = facts.kwarg(parameters) {
            effects.replace_flag(builder, InferenceFlags::IN_KWARG_ANNOTATION, true).await?;
            if let Some(annotation) = facts.variadic_annotation(kwarg) {
                effects.annotation(builder, annotation).await?;
            }
            effects.replace_flag(builder, InferenceFlags::IN_KWARG_ANNOTATION, false).await?;
        }
        effects.replace_flag(builder, InferenceFlags::IN_PARAMETER_ANNOTATION, false).await?;
        effects.replace_flag(builder, InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER, previous).await?;
        Ok(())
    }

    #[synchronous(receiver_annotation_sync)]
    #[capabilities(effects = AnnotationEffects, facts = AnnotationFacts)]
    #[passive_values(InferenceFlags::IN_PARAMETER_ANNOTATION, InferenceFlags::IN_INIT_RECEIVER_ANNOTATION)]
    pub(in crate::types::infer) async fn receiver_annotation_with<'db, 'ast, E: AnnotationEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &ast::StmtFunctionDef, facts: AnnotationFacts, effects: &E,
    ) -> Result<Option<bool>, E::Error> {
        let Some(annotation) = facts.receiver(function) else { return Ok(None); };
        if !effects.in_class_scope(builder, definition).await? { return Ok(None); }
        let decorators = if facts.has_decorators(function) {
            effects.known_decorators(builder, definition).await?
        } else { facts.empty_decorators() };
        let Some(kind) = facts.receiver_kind(function, decorators) else { return Ok(None); };
        let previous_parameter = effects.replace_flag(builder, InferenceFlags::IN_PARAMETER_ANNOTATION, true).await?;
        let previous_init = effects.replace_flag(builder, InferenceFlags::IN_INIT_RECEIVER_ANNOTATION, facts.init_receiver(function, kind)).await?;
        let ty = effects.annotation(builder, annotation).await?;
        effects.replace_flag(builder, InferenceFlags::IN_PARAMETER_ANNOTATION, previous_parameter).await?;
        effects.replace_flag(builder, InferenceFlags::IN_INIT_RECEIVER_ANNOTATION, previous_init).await?;
        let accepts = effects.accepts_receiver(builder, kind, ty).await?;
        Ok(Some(!accepts))
    }
}

impl<'db, 'ast> SynchronousAnnotationEffects<'db, 'ast> for OrdinaryAnnotationEffects {
    type Error = Infallible;

    fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        key: DefinitionNodeKey,
    ) -> Result<Definition<'db>, Infallible> {
        Ok(builder.index.expect_single_definition(key))
    }

    fn type_parameter_cursor<'param>(
        &self,
        parameters: &'param ast::TypeParams,
    ) -> Result<TypeParameterCursor<'param>, Infallible> {
        Ok(TypeParameterCursor::new(parameters))
    }

    fn next_type_parameter<'param>(
        &self,
        cursor: &mut TypeParameterCursor<'param>,
    ) -> Result<Option<&'param ast::TypeParam>, Infallible> {
        Ok(cursor.next())
    }

    fn definition_inference(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Infallible> {
        Ok(infer_definition_types(builder.db(), definition))
    }

    fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> Result<(), Infallible> {
        builder.extend_definition(definition, inference);
        Ok(())
    }

    fn has_deferred_annotations(
        &self,
        function: &ast::StmtFunctionDef,
    ) -> Result<bool, Infallible> {
        Ok(function_has_deferred_annotations(function))
    }
    fn known_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<FunctionDecorators, Infallible> {
        Ok(function_known_decorator_flags(builder.db(), definition))
    }
    fn replace_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        binding: Option<Definition<'db>>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(std::mem::replace(
            &mut builder.typevar_binding_context,
            binding,
        ))
    }
    fn replace_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> Result<bool, Infallible> {
        Ok(builder.context.inference_flags.replace(flag, value))
    }
    fn signature_annotations(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Infallible> {
        builder.infer_function_signature_annotations(function, definition);
        Ok(())
    }
    fn receiver_annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> Result<Option<bool>, Infallible> {
        Ok(builder.infer_method_receiver_annotation(function, definition))
    }
    fn in_class_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(definition
            .scope(builder.db())
            .scope(builder.db())
            .kind()
            .is_class())
    }
    fn accepts_receiver(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        kind: MethodReceiverKind,
        annotation: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(kind.accepts_annotation(builder.db(), annotation))
    }
    fn annotation(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_type_expression_with_state(
            expression,
            DeferredExpressionState::from(builder.defer_annotations()),
        ))
    }
    fn type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameters: &ast::TypeParams,
    ) -> Result<(), Infallible> {
        type_parameters_sync(builder, parameters, AnnotationFacts, self)
    }
    fn parameter_cursor<'param>(
        &self,
        parameters: &'param ast::Parameters,
        skip_first: bool,
    ) -> Result<ParameterCursor<'param>, Infallible> {
        Ok(ParameterCursor::new(parameters, skip_first))
    }
    fn next_parameter<'param>(
        &self,
        cursor: &mut ParameterCursor<'param>,
    ) -> Result<Option<&'param ast::ParameterWithDefault>, Infallible> {
        Ok(cursor.next())
    }
}
