use crate::Db;
use crate::ProgramEnvironment;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{
    CallArguments, KnownInstanceType, SpecialFormType, SubclassOfType, Type, TypeContext,
    TypingModule,
    call::CallError,
    function::{FunctionType, KnownFunction},
    infer::{TypeInferenceBuilder, builder::DeferredExpressionState},
    special_form::TypeQualifier,
};
use ruff_python_ast as ast;
use ty_module_resolver::KnownModule;
use ty_python_core::{definition::Definition, scope::NodeWithScopeRef};

use self::source_effects::{
    ClassDefinitionEffects, ClassDefinitionWork, ClassIdentity, LegacyInlineEffects,
};
use super::deferred::{DeferredClassWork, DeferredEffects};

pub(in crate::types::infer) mod source_effects;

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    pub(super) fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        let type_params = class
            .type_params
            .as_deref()
            .expect("class type params scope without type params");

        let binding_context = self.index.expect_single_definition(class);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);

        self.infer_type_parameters(type_params);

        if class.arguments.is_some() {
            let previous_deferred_state = self.replace_deferred_state(self.in_stub().into());

            // PEP 695 class headers are inferred in the type-parameter scope, before the completed
            // class type is available. Infer the bases first because `extra_items=T` is an
            // annotation in `class C[T](TypedDict, extra_items=T)`, but an ordinary value argument
            // in `class C[T](Base, extra_items=T)`.
            let mut is_typed_dict = false;

            for base in class.bases() {
                let ty = if let ast::Expr::Starred(starred) = base {
                    let ty = self.infer_expression(&starred.value, TypeContext::default());
                    self.store_expression_type(base, ty);
                    ty
                } else {
                    self.infer_expression(base, TypeContext::default())
                };
                is_typed_dict |= match ty {
                    ty if TypingModule::from_typed_dict_type(self.db(), ty).is_some() => true,
                    Type::ClassLiteral(class) => class.is_typed_dict(self.db()),
                    Type::GenericAlias(alias) => alias.is_typed_dict(self.db()),
                    _ => false,
                };
            }

            for keyword in class.keywords() {
                if is_typed_dict && keyword.arg.as_deref() == Some("extra_items") {
                    self.infer_extra_items_kwarg(&keyword.value);
                } else {
                    self.infer_expression(&keyword.value, TypeContext::default());
                }
            }

            self.deferred_state = previous_deferred_state;
        }

        self.typevar_binding_context = previous_typevar_binding_context;
    }

    pub(super) fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        self.infer_definition(class);
    }

    pub(super) fn infer_class_definition(
        &mut self,
        class_node: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) {
        legacy_inline(self.infer_class_definition_with(
            &LegacyInlineEffects,
            class_node,
            definition,
        ));
    }

    pub(in crate::types::infer) async fn infer_class_definition_with<E>(
        &mut self,
        effects: &E,
        class_node: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> Result<(), E::Error>
    where
        E: ClassDefinitionEffects<'db>,
    {
        effects
            .checkpoint(ClassDefinitionWork::InspectDefinition {
                decorators: class_node.decorator_list.len(),
                keywords: class_node.keywords().len(),
                name_bytes: class_node.name.id.len(),
            })
            .await?;
        let env = self.program_environment();
        let ast::StmtClassDef {
            range: _,
            node_index: _,
            name,
            type_params,
            decorator_list,
            arguments: _,
            body: _,
        } = class_node;
        let db = self.db();
        let known_function = async |function: FunctionType<'db>| {
            let literal = effects.field(function.field_requests(db).literal()).await?;
            effects
                .field(literal.last_definition.field_requests(db).known())
                .await
        };

        let mut decorator_types_and_nodes: Vec<(Type<'db>, &ast::Decorator)> =
            effects.allocate_vec(decorator_list.len()).await?;
        for decorator in decorator_list {
            effects
                .checkpoint(ClassDefinitionWork::DecoratorExpression)
                .await?;
            let decorator_ty = effects.infer_class_decorator(self, decorator).await?;
            decorator_types_and_nodes.push((decorator_ty, decorator));
        }

        let body_scope = effects
            .class_body_scope(
                db,
                self,
                self.index.node_scope(NodeWithScopeRef::Class(class_node)),
            )
            .await?;

        let context = &self.context;
        let maybe_known_class = effects.known_class(db, context, name).await?;

        let mut decorators_to_apply = effects
            .allocate_vec(decorator_types_and_nodes.len())
            .await?;
        let mut metadata_applies_to_original_class = true;
        let mut deprecated = None;
        let mut type_check_only = false;
        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        let mut total_ordering = false;
        let has_explicit_bases = class_node
            .arguments
            .as_deref()
            .is_some_and(|arguments| !arguments.args.is_empty());
        let has_explicit_metaclass = class_node
            .arguments
            .as_deref()
            .is_some_and(|arguments| arguments.find_keyword("metaclass").is_some());
        let infer_original_class_ty = async |deprecated,
                                             type_check_only,
                                             dataclass_params,
                                             dataclass_transformer_params,
                                             total_ordering| {
            effects
                .checkpoint(ClassDefinitionWork::OriginalClass)
                .await?;
            let original_class_ty = match (maybe_known_class, &*name.id) {
                (None, "NamedTuple")
                    if matches!(
                        effects.known_module(db, context).await?,
                        Some(KnownModule::Typing | KnownModule::TypingExtensions)
                    ) =>
                {
                    Type::SpecialForm(SpecialFormType::NamedTuple)
                }
                (None, "Any")
                    if matches!(
                        effects.known_module(db, context).await?,
                        Some(KnownModule::Typing | KnownModule::TypingExtensions)
                    ) =>
                {
                    Type::SpecialForm(SpecialFormType::Any)
                }
                (None, "InitVar")
                    if effects.known_module(db, context).await?
                        == Some(KnownModule::Dataclasses) =>
                {
                    Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar))
                }
                _ => Type::from(
                    effects
                        .class_literal(
                            db,
                            ClassIdentity {
                                name: &name.id,
                                body_scope,
                                known: maybe_known_class,
                                deprecated,
                                type_check_only,
                                dataclass_params,
                                dataclass_transformer_params,
                                total_ordering,
                                has_decorators: !class_node.decorator_list.is_empty(),
                                has_type_params: class_node.type_params.is_some(),
                                has_explicit_bases,
                                has_explicit_metaclass,
                            },
                        )
                        .await?,
                ),
            };
            Ok::<_, E::Error>(original_class_ty)
        };
        // In the first pass, collect metadata decorators that shape the original class object.
        // Once an inner decorator replaces the public binding, outer decorators are ordinary
        // runtime applications only: they cannot retroactively add metadata to the original class.
        // For ordinary decorators that still apply to the original class, precompute the call so
        // the second pass can reuse it if no inner decorator has changed the binding.
        for &(decorator_ty, decorator) in decorator_types_and_nodes.iter().rev() {
            effects
                .checkpoint(ClassDefinitionWork::MetadataDecorator)
                .await?;
            if !metadata_applies_to_original_class {
                decorators_to_apply.push((decorator_ty, decorator, None));
                continue;
            }

            if let Some(function) = decorator_ty.as_function_literal()
                && known_function(function).await? == Some(KnownFunction::Dataclass)
            {
                dataclass_params = Some(effects.default_dataclass_params(db, env).await?);
                continue;
            }

            if let Some(function) = decorator_ty.as_function_literal()
                && known_function(function).await? == Some(KnownFunction::TotalOrdering)
            {
                total_ordering = true;
                continue;
            }

            if let Type::DataclassDecorator(params) = decorator_ty {
                dataclass_params = Some(params);
                continue;
            }

            if decorator_ty.is_unknown()
                && let ast::Expr::Call(call) = &decorator.expression
                && let Some(function) = self.expression_type(&call.func).as_function_literal()
                && known_function(function).await? == Some(KnownFunction::Dataclass)
            {
                continue;
            }

            if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) =
                decorator_ty
            {
                deprecated = Some(deprecated_inst);
                continue;
            }

            if let Some(function) = decorator_ty.as_function_literal()
                && known_function(function).await? == Some(KnownFunction::TypeCheckOnly)
            {
                type_check_only = true;
                continue;
            }

            // Skip identity decorators to avoid salsa cycles on typeshed.
            if let Some(function) = decorator_ty.as_function_literal()
                && matches!(
                    known_function(function).await?,
                    Some(
                        KnownFunction::Final
                            | KnownFunction::DisjointBase
                            | KnownFunction::RuntimeCheckable
                    )
                )
            {
                continue;
            }

            if let Type::FunctionLiteral(f) = decorator_ty {
                // We do not yet detect or flag `@dataclass_transform` applied to more than one
                // overload, or an overload and the implementation both. Nevertheless, this is not
                // allowed. We do not try to treat the offenders intelligently -- just use the
                // params of the last seen usage of `@dataclass_transform`.
                //
                // In class-decorator position, dataclass-transform metadata shapes the
                // original class object. We keep it metadata-only here because the call path
                // uses synthetic dataclass-transform return types to model decorator factories;
                // treating this as an ordinary replacement-returning class decorator would
                // conflate those two cases.
                let transformer_params = effects.dataclass_transformer_params(db, f).await?;
                if let Some(transformer_params) = transformer_params {
                    dataclass_params = Some(
                        effects
                            .dataclass_params_from_transformer(db, transformer_params)
                            .await?,
                    );
                    continue;
                }
            }

            if let Type::DataclassTransformer(params) = decorator_ty {
                dataclass_transformer_params = Some(params);
                continue;
            }

            let original_class_ty = infer_original_class_ty(
                deprecated,
                type_check_only,
                dataclass_params,
                dataclass_transformer_params,
                total_ordering,
            )
            .await?;
            let decorator_result = effects
                .apply_class_decorator(db, env, decorator_ty, original_class_ty)
                .await?;
            let decorated_ty = match &decorator_result {
                Ok(return_ty) => *return_ty,
                Err(error) => effects.decorator_error_return_type(db, env, error).await?,
            };
            if !effects
                .is_unknown_decorator_result(db, decorated_ty)
                .await?
                && !effects
                    .type_retains_original_class(db, env, original_class_ty, decorated_ty)
                    .await?
            {
                metadata_applies_to_original_class = false;
            }

            decorators_to_apply.push((
                decorator_ty,
                decorator,
                Some((original_class_ty, decorator_result)),
            ));
        }

        let mut inferred_ty = infer_original_class_ty(
            deprecated,
            type_check_only,
            dataclass_params,
            dataclass_transformer_params,
            total_ordering,
        )
        .await?;

        let original_class_ty = inferred_ty;
        let mut undecorated_ty = None;

        // In the second pass, apply class decorators from inner to outer and use their return types
        // to update the public binding. `original_class_ty` remains the class object whose body and
        // metadata were inferred above.
        for (decorator_ty, decorator_node, precomputed_result) in decorators_to_apply {
            effects
                .checkpoint(ClassDefinitionWork::RuntimeDecorator)
                .await?;
            let decorator_result = match precomputed_result {
                // The metadata pass already called this decorator with the same input. If an inner
                // decorator changed the binding, apply this decorator to the new public binding.
                Some((precomputed_input_ty, decorator_result))
                    if precomputed_input_ty == inferred_ty =>
                {
                    decorator_result
                }
                _ => {
                    effects
                        .apply_class_decorator(db, env, decorator_ty, inferred_ty)
                        .await?
                }
            };
            let decorated_ty = match decorator_result {
                Ok(return_ty) => return_ty,
                Err(error) => {
                    self.defer_decorator_call(decorator_node, inferred_ty);
                    effects.decorator_error_return_type(db, env, &error).await?
                }
            };
            let decorated_ty = match decorated_ty {
                Type::DataclassDecorator(_) | Type::DataclassTransformer(_) => Type::unknown(),
                decorated_ty => decorated_ty,
            };
            inferred_ty = if effects
                .is_unknown_decorator_result(db, decorated_ty)
                .await?
            {
                inferred_ty
            } else if effects
                .class_decorator_preserves_class_binding(db, env, original_class_ty, decorated_ty)
                .await?
            {
                effects
                    .merge_class_preserving_decorator_result(
                        db,
                        env,
                        original_class_ty,
                        inferred_ty,
                        decorated_ty,
                    )
                    .await?
            } else {
                // Only record an undecorated type once a decorator actually replaces the public
                // binding. If all decorators preserve the class, there is no alternate class type
                // to expose.
                undecorated_ty.get_or_insert(inferred_ty);
                decorated_ty
            };
        }

        effects
            .checkpoint(ClassDefinitionWork::RecordBinding)
            .await?;
        effects
            .bind_class(self, class_node, definition, inferred_ty, undecorated_ty)
            .await?;

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            // In stub files, keyword values may reference names that are defined later in the file.
            let in_stub = effects.in_stub(&self.context).await?;
            let previous_deferred_state = self.replace_deferred_state(in_stub.into());
            let keyword_result = async {
                for keyword in class_node.keywords() {
                    effects
                        .checkpoint(ClassDefinitionWork::KeywordExpression)
                        .await?;
                    if keyword.arg.as_deref() != Some("extra_items") {
                        effects.infer_class_expression(self, &keyword.value).await?;
                    }
                }
                Ok::<_, E::Error>(())
            }
            .await;
            self.deferred_state = previous_deferred_state;
            keyword_result?;

            // Inference of bases deferred in stubs, or if any are string literals.
            if effects.in_stub(&self.context).await?
                || effects
                    .class_bases_contain_string_literal(class_node)
                    .await?
                || class_node
                    .arguments
                    .as_deref()
                    .and_then(|args| args.find_keyword("extra_items"))
                    .is_some()
            {
                effects
                    .checkpoint(ClassDefinitionWork::RecordDeferred)
                    .await?;
                effects.record_deferred(self, definition).await?;
            } else {
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                let base_result = async {
                    for base in class_node.bases() {
                        effects
                            .checkpoint(ClassDefinitionWork::BaseExpression)
                            .await?;
                        effects.infer_class_expression(self, base).await?;
                    }
                    Ok::<_, E::Error>(())
                }
                .await;
                self.typevar_binding_context = previous_typevar_binding_context;
                base_result?;
            }
        }
        Ok(())
    }

    pub(super) async fn infer_class_deferred_with<E: DeferredEffects<'db>>(
        &mut self,
        effects: &E,
        definition: Definition<'db>,
        class: &ast::StmtClassDef,
    ) -> Result<(), E::Error> {
        effects
            .class_checkpoint(DeferredClassWork::Begin {
                keywords: class.keywords().len(),
            })
            .await?;
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        let result = async {
            for base in class.bases() {
                effects.class_checkpoint(DeferredClassWork::Base).await?;
                self.infer_deferred_class_expression_with(effects, base)
                    .await?;
            }

            if let Some(arguments) = class.arguments.as_deref()
                && let Some(extra_items_keyword) = arguments.find_keyword("extra_items")
            {
                effects
                    .class_checkpoint(DeferredClassWork::ExtraItems)
                    .await?;
                if effects.is_typed_dict(self, definition).await? {
                    effects
                        .extra_items(self, &extra_items_keyword.value)
                        .await?;
                } else {
                    self.infer_deferred_class_expression_with(effects, &extra_items_keyword.value)
                        .await?;
                }
            }
            Ok(())
        }
        .await;
        self.typevar_binding_context = previous_typevar_binding_context;
        result
    }

    async fn infer_deferred_class_expression_with<E: DeferredEffects<'db>>(
        &mut self,
        effects: &E,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, E::Error> {
        if !effects.in_stub(&self.context).await? {
            return effects.expression(self, expression).await;
        }
        let previous = self.replace_deferred_state(DeferredExpressionState::Deferred);
        let result = effects.expression(self, expression).await;
        self.deferred_state = previous;
        result
    }
}

fn apply_class_decorator<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    decorator_ty: Type<'db>,
    decorated_ty: Type<'db>,
) -> Result<Type<'db>, CallError<'db>> {
    let call_arguments = CallArguments::positional([decorated_ty]);
    decorator_ty
        .try_call(db, env, &call_arguments)
        .map(|bindings| bindings.return_type(db, env))
}

/// Return true if a decorator result still binds the name to the original class.
///
/// For example, an identity decorator keeps the public name bound to the same class:
/// ```python
/// def identity[T](cls: type[T]) -> type[T]:
///     return cls
///
/// @identity
/// class C: ...
/// ```
///
/// This also accepts metaclass-shaped results such as `type[C]`, because those still describe the
/// original class object even if the decorator call produced a `SubclassOf` type internally.
fn class_decorator_preserves_class_binding<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    original_class: Type<'db>,
    decorated_class: Type<'db>,
) -> bool {
    let Type::ClassLiteral(original_literal) = original_class else {
        return false;
    };

    match decorated_class {
        Type::ClassLiteral(decorated_literal) => {
            let decorated_definition = decorated_literal.definition(db);
            decorated_literal == original_literal
                || decorated_definition.is_some()
                    && decorated_definition == original_literal.definition(db)
        }
        Type::SubclassOf(subclass_of) => subclass_of
            .subclass_of()
            .into_class(db, env)
            .is_some_and(|class| class == original_literal.default_specialization(db)),
        Type::Divergent(_) => true,
        Type::Union(union) => union.elements(db).iter().all(|element| {
            class_decorator_preserves_class_binding(db, env, original_class, *element)
        }),
        Type::TypeAlias(alias) => {
            class_decorator_preserves_class_binding(db, env, original_class, alias.value_type(db))
        }
        _ => SubclassOfType::try_from_type(db, env, original_class).is_some_and(
            |original_meta_type| decorated_class.is_equivalent_to(db, env, original_meta_type),
        ),
    }
}

/// Return true if a type still contains the original class object, even if it also carries extra
/// intersection members.
fn type_retains_original_class<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    original_class: Type<'db>,
    decorated_class: Type<'db>,
) -> bool {
    match decorated_class {
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| type_retains_original_class(db, env, original_class, *element)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| type_retains_original_class(db, env, original_class, *element)),
        Type::TypeAlias(alias) => {
            type_retains_original_class(db, env, original_class, alias.value_type(db))
        }
        _ => class_decorator_preserves_class_binding(db, env, original_class, decorated_class),
    }
}

/// Return true if a class-decorator result should leave the current binding unchanged.
///
/// This also handles `type[Unknown]` results from generic decorator factories whose type
/// variables are specialized before the returned decorator receives the class. Explicit `Any`
/// results do not trigger this fallback.
fn is_unknown_decorator_result<'db>(db: &'db dyn Db, result_ty: Type<'db>) -> bool {
    match result_ty.resolve_type_alias(db) {
        Type::SubclassOf(subclass_of) => subclass_of
            .subclass_of()
            .into_dynamic()
            .is_some_and(|dynamic| Type::Dynamic(dynamic).is_unknown()),
        result_ty => result_ty.is_unknown(),
    }
}

/// Merge a class-preserving decorator result into the public binding.
///
/// If earlier decorators already exposed extra members through an intersection, keep those
/// members instead of collapsing back to the undecorated class when a later decorator simply
/// returns the original class object again.
fn merge_class_preserving_decorator_result<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    original_class: Type<'db>,
    current_binding: Type<'db>,
    decorated_binding: Type<'db>,
) -> Type<'db> {
    if current_binding == original_class
        || type_retains_original_class(db, env, original_class, current_binding)
    {
        current_binding
    } else {
        decorated_binding
            .as_class_literal()
            .map(Type::ClassLiteral)
            .unwrap_or(original_class)
    }
}
