//! Shared scope phase ordering and result finalization.

#[cfg(test)]
mod tests;

use std::convert::Infallible;

use rustc_hash::FxHashSet;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{AnnotatedAssignmentDefinitionKind, FunctionDefinitionKind};
use ty_python_core::place::ScopedPlaceId;

use super::function::{function_has_deferred_annotations, parameters_have_defaults};
use crate::types::infer::infer_function_default_types;

use super::{
    Definition, DefinitionInference, DefinitionKind, ExpressionNodeKey, FrozenMap, FrozenSet,
    FrozenValueMap, FunctionType, FxHashMap, FxIndexSet, InferenceRegion, NodeWithScopeKind,
    ScopeId, ScopeInference, ScopeInferenceExtra, Type, TypeCheckDiagnostics, TypeContext,
    TypeExpressionFlags, TypeInferenceBuilder, TypeQualifiers, ast, infer_deferred_types,
    original_class_type, post_inference,
};

pub(super) struct OrdinaryScopeEffects;
pub(super) struct ScopeFacts;

pub(super) struct SeenFunctions<'db> {
    pub(super) overloaded_places: FxHashSet<ScopedPlaceId>,
    pub(super) public_functions: FxHashSet<FunctionType<'db>>,
}

shared_semantic_family! {
    #[synchronous(SynchronousScopeEffects)]
    pub(super) trait ScopeEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn scope_node(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: ScopeId<'db>) -> Result<&'db NodeWithScopeKind, Self::Error>;
        #[operation(source)]
        async fn infer_module(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_function(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, function: &AstNodeRef<ast::StmtFunctionDef>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_lambda(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, lambda: &AstNodeRef<ast::ExprLambda>, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_class(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_class_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_function_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, function: &AstNodeRef<ast::StmtFunctionDef>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_type_alias_type_parameters(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, type_alias: &AstNodeRef<ast::StmtTypeAlias>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_type_alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, type_alias: &AstNodeRef<ast::StmtTypeAlias>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_list_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, comprehension: &AstNodeRef<ast::ExprListComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_set_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, comprehension: &AstNodeRef<ast::ExprSetComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_dict_comprehension(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, comprehension: &AstNodeRef<ast::ExprDictComp>, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_generator(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, generator: &AstNodeRef<ast::ExprGenerator>, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn take_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<Vec<Definition<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_deferred(&self, definitions: &[Definition<'db>], cursor: &mut usize) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(child)]
        async fn definition_kind(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionKind<'db>, Self::Error>;
        #[operation(local)]
        async fn has_deferred_annotations(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: &FunctionDefinitionKind) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn has_parameter_defaults(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: &FunctionDefinitionKind) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn function_default_types(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error>;
        #[operation(child)]
        async fn deferred_types(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<&'db DefinitionInference<'db>, Self::Error>;
        #[operation(child)]
        async fn extend_definition(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, inferred: &DefinitionInference<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn check_deferred_empty(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn should_check_file(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn seen_functions(&self) -> Result<SeenFunctions<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_declaration(&self, builder: &TypeInferenceBuilder<'db, 'ast>, cursor: &mut usize) -> Result<Option<(Definition<'db>, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn function_decorators(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, function: &FunctionDefinitionKind) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn function_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn overloaded_function(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, definition: Definition<'db>, seen: &mut SeenFunctions<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn type_guard_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, function: &FunctionDefinitionKind) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn class_decorators(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn original_class_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn static_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, class: &AstNodeRef<ast::StmtClassDef>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn annotation_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn mark_implicit_alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn dynamic_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_called_function(&self, builder: &TypeInferenceBuilder<'db, 'ast>, cursor: &mut usize) -> Result<Option<FunctionType<'db>>, Self::Error>;
        #[operation(child)]
        async fn function_definition_id(&self, builder: &TypeInferenceBuilder<'db, 'ast>, function: FunctionType<'db>) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn final_without_value(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ScopeFacts {
        fn region<'db, 'ast>(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> InferenceRegion<'db> {
            builder.region
        }
        fn same_definition<'db>(&self, left: Definition<'db>, right: Definition<'db>) -> bool {
            left == right
        }
        fn undecorated_type<'db, 'ast>(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Option<Type<'db>> {
            builder.undecorated_type
        }
        fn original_or_declared<'db>(&self, original: Option<Type<'db>>, declared: Type<'db>) -> Type<'db> {
            original.unwrap_or(declared)
        }
        fn has_assignment_value<'db, 'ast>(&self, builder: &TypeInferenceBuilder<'db, 'ast>, assignment: &AnnotatedAssignmentDefinitionKind) -> bool {
            assignment.value(builder.module()).is_some()
        }
        fn is_type_alias<'db>(&self, ty: Type<'db>) -> bool {
            ty.is_typealias_special_form()
        }
    }

    #[synchronous(infer_scope_sync)]
    #[capabilities(effects = ScopeEffects, facts = ScopeFacts)]
    #[passive_values(Type::FunctionLiteral)]
    pub(super) async fn infer_scope_with<'db, 'ast, E: ScopeEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        scope: ScopeId<'db>,
        tcx: TypeContext<'db>,
        facts: ScopeFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let node = effects.scope_node(builder, scope).await?;
        match node {
            NodeWithScopeKind::Module => effects.infer_module(builder).await?,
            NodeWithScopeKind::Function(function) => effects.infer_function(builder, function).await?,
            NodeWithScopeKind::Lambda(lambda) => effects.infer_lambda(builder, lambda, tcx).await?,
            NodeWithScopeKind::Class(class) => effects.infer_class(builder, class).await?,
            NodeWithScopeKind::ClassTypeParameters(class) => effects.infer_class_type_parameters(builder, class).await?,
            NodeWithScopeKind::FunctionTypeParameters(function) => effects.infer_function_type_parameters(builder, function).await?,
            NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => effects.infer_type_alias_type_parameters(builder, type_alias).await?,
            NodeWithScopeKind::TypeAlias(type_alias) => effects.infer_type_alias(builder, type_alias).await?,
            NodeWithScopeKind::ListComprehension(comprehension) => effects.infer_list_comprehension(builder, comprehension, tcx).await?,
            NodeWithScopeKind::SetComprehension(comprehension) => effects.infer_set_comprehension(builder, comprehension, tcx).await?,
            NodeWithScopeKind::DictComprehension(comprehension) => effects.infer_dict_comprehension(builder, comprehension, tcx).await?,
            NodeWithScopeKind::GeneratorExpression(generator) => effects.infer_generator(builder, generator, tcx).await?,
        }

        // Infer deferred types for all definitions.
        let deferred_definitions = effects.take_deferred(builder).await?;
        let mut deferred_cursor = 0;
        #[cursor_loop]
        while let Some(definition) = effects.next_deferred(&deferred_definitions, &mut deferred_cursor).await? {
            if let DefinitionKind::Function(function) = effects.definition_kind(builder, definition).await? {
                if effects.has_deferred_annotations(builder, function).await? {
                    let inferred = effects.deferred_types(builder, definition).await?;
                    effects.extend_definition(builder, definition, inferred).await?;
                }
                if effects.has_parameter_defaults(builder, function).await? {
                    let inferred = effects.function_default_types(builder, definition).await?;
                    effects.extend_definition(builder, definition, inferred).await?;
                }
            } else {
                let inferred = effects.deferred_types(builder, definition).await?;
                effects.extend_definition(builder, definition, inferred).await?;
            }
        }
        effects.check_deferred_empty(builder).await?;

        if effects.should_check_file(builder).await? {
            let mut seen = effects.seen_functions().await?;
            let mut declaration_cursor = 0;
            #[cursor_loop]
            while let Some(declaration) = effects.next_declaration(builder, &mut declaration_cursor).await? {
                let (definition, ty) = declaration;
                match effects.definition_kind(builder, definition).await? {
                    DefinitionKind::Function(function) => {
                        effects.function_decorators(builder, definition, function).await?;
                        effects.function_definition(builder, definition).await?;
                        effects.overloaded_function(builder, ty, definition, &mut seen).await?;
                        effects.type_guard_definition(builder, ty, function).await?;
                    }
                    DefinitionKind::Class(class) => {
                        effects.class_decorators(builder, definition, class).await?;
                        let original_ty = match facts.region(builder) {
                            InferenceRegion::Definition(current) if facts.same_definition(current, definition) => facts.undecorated_type(builder),
                            _ => effects.original_class_type(builder, definition).await?,
                        };
                        let ty = facts.original_or_declared(original_ty, ty);
                        effects.static_class(builder, ty, class).await?;
                    }
                    DefinitionKind::AnnotatedAssignment(assignment) if facts.has_assignment_value(builder, assignment) => {
                        let annotation = effects.annotation_type(builder, assignment).await?;
                        if facts.is_type_alias(annotation) {
                            effects.mark_implicit_alias(builder, definition).await?;
                        }
                    }
                    _ => {}
                }
            }

            let mut deferred_cursor = 0;
            #[cursor_loop]
            while let Some(definition) = effects.next_deferred(&deferred_definitions, &mut deferred_cursor).await? {
                effects.dynamic_class(builder, definition).await?;
            }

            let mut called_cursor = 0;
            #[cursor_loop]
            while let Some(function) = effects.next_called_function(builder, &mut called_cursor).await? {
                let definition = effects.function_definition_id(builder, function).await?;
                effects.overloaded_function(builder, Type::FunctionLiteral(function), definition, &mut seen).await?;
            }
            effects.final_without_value(builder).await?;
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousScopeEffects<'db, 'ast> for OrdinaryScopeEffects {
    type Error = Infallible;
    fn scope_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: ScopeId<'db>,
    ) -> Result<&'db NodeWithScopeKind, Self::Error> {
        Ok(scope.node(builder.db()))
    }
    fn infer_module(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        builder.infer_module(builder.module().syntax());
        Ok(())
    }
    fn infer_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &AstNodeRef<ast::StmtFunctionDef>,
    ) -> Result<(), Self::Error> {
        builder.infer_function_body(function.node(builder.module()));
        Ok(())
    }
    fn infer_lambda(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        lambda: &AstNodeRef<ast::ExprLambda>,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_lambda_body(lambda.node(builder.module()), tcx);
        Ok(())
    }
    fn infer_class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Result<(), Self::Error> {
        builder.infer_class_body(class.node(builder.module()));
        Ok(())
    }
    fn infer_class_type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Result<(), Self::Error> {
        builder.infer_class_type_params(class.node(builder.module()));
        Ok(())
    }
    fn infer_function_type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &AstNodeRef<ast::StmtFunctionDef>,
    ) -> Result<(), Self::Error> {
        builder.infer_function_type_params(function.node(builder.module()));
        Ok(())
    }
    fn infer_type_alias_type_parameters(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        type_alias: &AstNodeRef<ast::StmtTypeAlias>,
    ) -> Result<(), Self::Error> {
        builder.infer_type_alias_type_params(type_alias.node(builder.module()));
        Ok(())
    }
    fn infer_type_alias(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        type_alias: &AstNodeRef<ast::StmtTypeAlias>,
    ) -> Result<(), Self::Error> {
        builder.infer_type_alias(type_alias.node(builder.module()));
        Ok(())
    }
    fn infer_list_comprehension(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        comprehension: &AstNodeRef<ast::ExprListComp>,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .infer_list_comprehension_expression_scope(comprehension.node(builder.module()), tcx);
        Ok(())
    }
    fn infer_set_comprehension(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        comprehension: &AstNodeRef<ast::ExprSetComp>,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_set_comprehension_expression_scope(comprehension.node(builder.module()), tcx);
        Ok(())
    }
    fn infer_dict_comprehension(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        comprehension: &AstNodeRef<ast::ExprDictComp>,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .infer_dict_comprehension_expression_scope(comprehension.node(builder.module()), tcx);
        Ok(())
    }
    fn infer_generator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        generator: &AstNodeRef<ast::ExprGenerator>,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_generator_expression_scope(generator.node(builder.module()), tcx);
        Ok(())
    }
    fn take_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Vec<Definition<'db>>, Self::Error> {
        Ok(std::mem::take(&mut builder.deferred).into_iter().collect())
    }
    fn next_deferred(
        &self,
        definitions: &[Definition<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        let next = definitions.get(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }
    fn definition_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error> {
        Ok(definition.kind(builder.db()))
    }
    fn has_deferred_annotations(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> Result<bool, Self::Error> {
        Ok(function_has_deferred_annotations(function.node(builder.module())))
    }
    fn has_parameter_defaults(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> Result<bool, Self::Error> {
        Ok(parameters_have_defaults(&function.node(builder.module()).parameters))
    }
    fn function_default_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Self::Error> {
        Ok(infer_function_default_types(builder.db(), definition))
    }
    fn deferred_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Self::Error> {
        Ok(infer_deferred_types(builder.db(), definition))
    }
    fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        inferred: &DefinitionInference<'db>,
    ) -> Result<(), Self::Error> {
        builder.extend_definition(definition, inferred);
        Ok(())
    }
    fn check_deferred_empty(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        assert!(
            builder.deferred.is_empty(),
            "Inferring deferred types should not add more deferred definitions"
        );
        Ok(())
    }
    fn should_check_file(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        Ok(builder.db().should_check_file(builder.file()))
    }
    fn seen_functions(&self) -> Result<SeenFunctions<'db>, Self::Error> {
        Ok(SeenFunctions {
            overloaded_places: FxHashSet::default(),
            public_functions: FxHashSet::default(),
        })
    }
    // Post-inference checks do not append declarations. Read each entry before a child
    // check so the cursor retains its order without borrowing the builder's collection.
    fn next_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        cursor: &mut usize,
    ) -> Result<Option<(Definition<'db>, Type<'db>)>, Self::Error> {
        let next = builder
            .declarations
            .0
            .get(*cursor)
            .map(|(definition, ty)| (*definition, ty.inner_type()));
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }
    fn function_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &FunctionDefinitionKind,
    ) -> Result<(), Self::Error> {
        post_inference::decorator::check_decorator_calls(
            &builder.context,
            definition,
            &function.node(builder.module()).decorator_list,
        );
        Ok(())
    }
    fn function_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        post_inference::function::check_function_definition(
            &builder.context,
            definition,
            &|expr| builder.file_expression_type(expr),
        );
        Ok(())
    }
    fn overloaded_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        definition: Definition<'db>,
        seen: &mut SeenFunctions<'db>,
    ) -> Result<(), Self::Error> {
        post_inference::overloaded_function::check_overloaded_function(
            &builder.context,
            ty,
            definition,
            builder.scope.scope(builder.db()).node(),
            builder.index,
            &mut seen.overloaded_places,
            &mut seen.public_functions,
        );
        Ok(())
    }
    fn type_guard_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        function: &FunctionDefinitionKind,
    ) -> Result<(), Self::Error> {
        post_inference::typeguard::check_type_guard_definition(
            &builder.context,
            ty,
            function.node(builder.module()),
        );
        Ok(())
    }
    fn class_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Result<(), Self::Error> {
        post_inference::decorator::check_decorator_calls(
            &builder.context,
            definition,
            &class.node(builder.module()).decorator_list,
        );
        Ok(())
    }
    fn original_class_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(original_class_type(builder.db(), definition).map(Type::ClassLiteral))
    }
    fn static_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Result<(), Self::Error> {
        post_inference::static_class::check_static_class_definitions(
            &builder.context,
            ty,
            class.node(builder.module()),
            builder.index,
            &|expr| builder.file_expression_type(expr),
        );
        Ok(())
    }
    fn annotation_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        assignment: &AnnotatedAssignmentDefinitionKind,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.file_expression_type(assignment.annotation(builder.module())))
    }
    fn mark_implicit_alias(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder.implicit_aliases.insert(definition);
        Ok(())
    }
    fn dynamic_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        post_inference::dynamic_class::check_dynamic_class_definition(&builder.context, definition);
        Ok(())
    }
    fn next_called_function(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        cursor: &mut usize,
    ) -> Result<Option<FunctionType<'db>>, Self::Error> {
        let next = builder.called_functions.get_index(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }
    fn function_definition_id(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> Result<Definition<'db>, Self::Error> {
        Ok(function.definition(builder.db()))
    }
    fn final_without_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        post_inference::final_variable::check_final_without_value(&builder.context, builder.index);
        Ok(())
    }
}

struct ScopeFinishData<'db> {
    implicit_aliases: FxIndexSet<Definition<'db>>,
    string_annotations: FxHashSet<ExpressionNodeKey>,
    expected_types: FxHashMap<ExpressionNodeKey, Type<'db>>,
    type_expression_flags: FxHashMap<ExpressionNodeKey, TypeExpressionFlags>,
    collection_use_constraints: FxHashMap<Definition<'db>, FxIndexSet<Type<'db>>>,
    expressions: FxHashMap<ExpressionNodeKey, Type<'db>>,
    cycle_recovery: Option<Type<'db>>,
    qualifiers: FxHashMap<ExpressionNodeKey, TypeQualifiers>,
    diagnostics: TypeCheckDiagnostics,
}

struct FinishScopeFacts;

enum ScopeInferenceState {
    Uninferred,
    #[cfg(any(test, feature = "experimental-analysis"))]
    Inferred,
}

shared_semantic_family! {
    #[synchronous(SynchronousFinishScopeEffects)]
    trait FinishScopeEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn infer_region(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_context(&self, builder: TypeInferenceBuilder<'db, 'ast>) -> Result<ScopeFinishData<'db>, Self::Error>;
        #[operation(local)]
        async fn freeze_with_extra(&self, data: ScopeFinishData<'db>) -> Result<ScopeInference<'db>, Self::Error>;
        #[operation(local)]
        async fn freeze_without_extra(&self, data: ScopeFinishData<'db>) -> Result<ScopeInference<'db>, Self::Error>;
    }

    #[finite_capability]
    impl FinishScopeFacts {
        fn needs_inference(&self, state: ScopeInferenceState) -> bool {
            matches!(state, ScopeInferenceState::Uninferred)
        }

        fn has_extra<'db>(&self, data: &ScopeFinishData<'db>) -> bool {
            !data.implicit_aliases.is_empty()
                || !data.string_annotations.is_empty()
                || !data.expected_types.is_empty()
                || !data.diagnostics.is_empty()
                || data.cycle_recovery.is_some()
                || !data.type_expression_flags.is_empty()
                || !data.collection_use_constraints.is_empty()
                || !data.qualifiers.is_empty()
        }
    }

    #[synchronous(finish_scope_sync)]
    #[capabilities(effects = FinishScopeEffects, facts = FinishScopeFacts)]
    #[passive_values()]
    async fn finish_scope_with<'db, 'ast, E: FinishScopeEffects<'db, 'ast>>(
        mut builder: TypeInferenceBuilder<'db, 'ast>,
        state: ScopeInferenceState,
        facts: FinishScopeFacts,
        effects: &E,
    ) -> Result<ScopeInference<'db>, E::Error> {
        if facts.needs_inference(state) {
            effects.infer_region(&mut builder).await?;
        }
        let data = effects.finish_context(builder).await?;
        if facts.has_extra(&data) {
            effects.freeze_with_extra(data).await
        } else {
            effects.freeze_without_extra(data).await
        }
    }
}

impl<'db, 'ast> SynchronousFinishScopeEffects<'db, 'ast> for OrdinaryScopeEffects {
    type Error = Infallible;

    fn infer_region(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        builder.infer_region();
        Ok(())
    }

    fn finish_context(
        &self,
        builder: TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<ScopeFinishData<'db>, Self::Error> {
        let TypeInferenceBuilder {
            implicit_aliases,
            context,
            string_annotations,
            expected_types,
            type_expression_flags,
            collection_use_constraints,
            expressions,
            comparison_truthiness: _,
            #[cfg(any(test, feature = "experimental-analysis"))]
                source_truthiness_backing: _,
            scope,
            cycle_recovery,
            qualifiers,

            // Ignored, never leaked into other scopes
            deferred: _,
            bindings: _,
            declarations: _,

            // Ignored; only relevant to definition regions
            undecorated_type: _,
            deferred_decorator_calls: _,
            discards_dict_key_assignments: _,

            // Builder only state
            expression_cache: _,
            reachability_cache: _,
            dataclass_field_specifiers: _,
            typevar_binding_context: _,
            deferred_state: _,
            called_functions: _,
            index: _,
            region: _,
            return_types_and_ranges: _,
        } = builder;

        let _ = scope;
        let diagnostics = context.finish();

        Ok(ScopeFinishData {
            implicit_aliases,
            string_annotations,
            expected_types,
            type_expression_flags,
            collection_use_constraints,
            expressions,
            cycle_recovery,
            qualifiers,
            diagnostics,
        })
    }

    fn freeze_with_extra(
        &self,
        data: ScopeFinishData<'db>,
    ) -> Result<ScopeInference<'db>, Self::Error> {
        let ScopeFinishData {
            implicit_aliases,
            string_annotations,
            expected_types,
            type_expression_flags,
            mut collection_use_constraints,
            expressions,
            cycle_recovery,
            qualifiers,
            diagnostics,
        } = data;
        collection_use_constraints.shrink_to_fit();
        let extra = Box::new(ScopeInferenceExtra {
            implicit_aliases: implicit_aliases.into_iter().collect(),
            string_annotations: FrozenSet::from(string_annotations),
            qualifiers: FrozenMap::from(qualifiers),
            expected_types: FrozenMap::from(expected_types),
            type_expression_flags: FrozenMap::from(type_expression_flags),
            collection_use_constraints,
            cycle_recovery,
            diagnostics,
        });
        Ok(ScopeInference {
            expressions: FrozenValueMap::from(expressions),
            extra: Some(extra),
        })
    }

    fn freeze_without_extra(
        &self,
        data: ScopeFinishData<'db>,
    ) -> Result<ScopeInference<'db>, Self::Error> {
        Ok(ScopeInference {
            expressions: FrozenValueMap::from(data.expressions),
            extra: None,
        })
    }
}

pub(super) fn finish_scope<'db>(builder: TypeInferenceBuilder<'db, '_>) -> ScopeInference<'db> {
    match finish_scope_sync(
        builder,
        ScopeInferenceState::Uninferred,
        FinishScopeFacts,
        &OrdinaryScopeEffects,
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) fn finish_inferred_scope<'db>(
    builder: TypeInferenceBuilder<'db, '_>,
) -> ScopeInference<'db> {
    match finish_scope_sync(
        builder,
        ScopeInferenceState::Inferred,
        FinishScopeFacts,
        &OrdinaryScopeEffects,
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
