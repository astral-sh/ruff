use crate::{
    place::Place,
    types::{
        CallArguments, DataclassParams, KnownClass, KnownInstanceType, MemberLookupPolicy,
        SpecialFormType, StaticClassLiteral, SubclassOfType, Type, TypeContext,
        function::KnownFunction,
        infer::{
            TypeInferenceBuilder,
            builder::{DeclaredAndInferredType, DeferredExpressionState},
        },
        infer_definition_types,
        signatures::ParameterForm,
        special_form::TypeQualifier,
    },
};
use ruff_python_ast::{self as ast, helpers::any_over_expr, name::Name};
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{definition::Definition, scope::NodeWithScopeRef};

/// Return true if a decorator result still binds the name to the original class.
fn class_decorator_preserves_class_binding<'db>(
    db: &'db dyn crate::Db,
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
            .into_class(db)
            .is_some_and(|class| class == original_literal.default_specialization(db)),
        Type::Divergent(_) => true,
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| class_decorator_preserves_class_binding(db, original_class, *element)),
        Type::TypeAlias(alias) => {
            class_decorator_preserves_class_binding(db, original_class, alias.value_type(db))
        }
        _ => SubclassOfType::try_from_type(db, original_class).is_some_and(|original_meta_type| {
            decorated_class.is_equivalent_to(db, original_meta_type)
        }),
    }
}

/// Return true if an unknown class-decorator result should preserve the decorated class binding.
fn can_preserve_unknown_class_decorator_result<'db>(
    db: &'db dyn crate::Db,
    decorator_ty: Type<'db>,
) -> bool {
    if decorator_ty.is_unknown() {
        return false;
    }

    class_decorator_known_preservation(db, decorator_ty)
        .unwrap_or_else(|| decorator_ty.try_upcast_to_callable(db).is_some())
}

fn is_unknown_decorator_result<'db>(db: &'db dyn crate::Db, ty: Type<'db>) -> bool {
    if ty.is_unknown() {
        return true;
    }

    let Type::SubclassOf(subclass_of) = ty.resolve_type_alias(db) else {
        return false;
    };

    subclass_of
        .subclass_of()
        .into_dynamic()
        .is_some_and(|dynamic| Type::Dynamic(dynamic).is_unknown())
}

fn class_decorator_known_preservation<'db>(
    db: &'db dyn crate::Db,
    decorator_ty: Type<'db>,
) -> Option<bool> {
    match decorator_ty {
        Type::FunctionLiteral(function) => Some(!function.has_explicit_return_annotation(db)),
        Type::BoundMethod(method) => Some(!method.function(db).has_explicit_return_annotation(db)),
        Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
            let call_symbol = decorator_ty
                .member_lookup_with_policy(
                    db,
                    Name::new_static("__call__"),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place;

            if let Place::Defined(place) = call_symbol
                && place.is_definitely_defined()
            {
                Some(class_decorator_known_preservation(db, place.ty).unwrap_or(false))
            } else {
                Some(false)
            }
        }
        Type::Union(union) => Some(
            union
                .elements(db)
                .iter()
                .all(|element| class_decorator_known_preservation(db, *element).unwrap_or(false)),
        ),
        Type::TypeAlias(alias) => {
            Some(class_decorator_known_preservation(db, alias.value_type(db)).unwrap_or(false))
        }
        Type::Callable(_) => Some(true),
        _ => None,
    }
}

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

        if let Some(arguments) = class.arguments.as_deref() {
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());
            let mut call_arguments =
                CallArguments::from_arguments(arguments, |arg_or_keyword, splatted_value| {
                    let ty = self.infer_expression(splatted_value, TypeContext::default());
                    if let ast::ArgOrKeyword::Arg(argument) = arg_or_keyword
                        && argument.is_starred_expr()
                    {
                        self.store_expression_type(argument, ty);
                    }
                    ty
                });
            let argument_forms = vec![Some(ParameterForm::Value); call_arguments.len()];
            self.infer_argument_types(arguments, &mut call_arguments, &argument_forms);
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

        let decorator_types_and_nodes: Vec<(Type<'db>, &ast::Decorator)> = decorator_list
            .iter()
            .map(|decorator| (self.infer_decorator(decorator), decorator))
            .collect();
        let mut decorators_to_apply = Vec::with_capacity(decorator_types_and_nodes.len());
        let mut metadata_applies_to_original_class = true;
        let mut deprecated = None;
        let mut type_check_only = false;
        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        let mut total_ordering = false;
        for &(decorator_ty, decorator) in decorator_types_and_nodes.iter().rev() {
            if metadata_applies_to_original_class {
                if decorator_ty
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
                {
                    dataclass_params = Some(DataclassParams::default_params(db));
                    continue;
                }

                if decorator_ty
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(db, KnownFunction::TotalOrdering))
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
                    && self
                        .expression_type(&call.func)
                        .as_function_literal()
                        .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
                {
                    continue;
                }

                if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) =
                    decorator_ty
                {
                    deprecated = Some(deprecated_inst);
                    continue;
                }

                if decorator_ty
                    .as_function_literal()
                    .is_some_and(|function| function.is_known(db, KnownFunction::TypeCheckOnly))
                {
                    type_check_only = true;
                    continue;
                }

                // Skip identity decorators to avoid salsa cycles on typeshed.
                if decorator_ty.as_function_literal().is_some_and(|function| {
                    matches!(
                        function.known(db),
                        Some(
                            KnownFunction::Final
                                | KnownFunction::DisjointBase
                                | KnownFunction::RuntimeCheckable
                        )
                    )
                }) {
                    continue;
                }

                if let Type::FunctionLiteral(f) = decorator_ty {
                    // We do not yet detect or flag `@dataclass_transform` applied to more than one
                    // overload, or an overload and the implementation both. Nevertheless, this is not
                    // allowed. We do not try to treat the offenders intelligently -- just use the
                    // params of the last seen usage of `@dataclass_transform`
                    let transformer_params = f
                        .iter_overloads_and_implementation(db)
                        .rev()
                        .find_map(|overload| overload.dataclass_transformer_params(db));
                    if let Some(transformer_params) = transformer_params {
                        dataclass_params = Some(DataclassParams::from_transformer_params(
                            db,
                            transformer_params,
                        ));
                        continue;
                    }
                }

                if let Type::DataclassTransformer(params) = decorator_ty {
                    dataclass_transformer_params = Some(params);
                    continue;
                }

                let decorator_preserves_unknown_result =
                    class_decorator_known_preservation(db, decorator_ty).unwrap_or(false)
                        || matches!(&decorator.expression, ast::Expr::Call(call) if {
                            class_decorator_known_preservation(db, self.expression_type(&call.func))
                                .unwrap_or(false)
                        });
                if !decorator_preserves_unknown_result {
                    metadata_applies_to_original_class = false;
                }
            }

            decorators_to_apply.push((decorator_ty, decorator));
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class_node))
            .to_scope_id(db, self.file());

        let maybe_known_class = KnownClass::try_from_file_and_name(db, self.file(), name);

        let known_module = || file_to_module(db, self.file()).and_then(|module| module.known(db));
        let in_typing_module = || {
            matches!(
                known_module(),
                Some(KnownModule::Typing | KnownModule::TypingExtensions)
            )
        };

        let mut inferred_ty = match (maybe_known_class, &*name.id) {
            (None, "NamedTuple") if in_typing_module() => {
                Type::SpecialForm(SpecialFormType::NamedTuple)
            }
            (None, "Any") if in_typing_module() => Type::SpecialForm(SpecialFormType::Any),
            (None, "InitVar") if known_module() == Some(KnownModule::Dataclasses) => {
                Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar))
            }
            _ => Type::from(StaticClassLiteral::new(
                db,
                name.id.clone(),
                body_scope,
                maybe_known_class,
                deprecated,
                type_check_only,
                dataclass_params,
                dataclass_transformer_params,
                total_ordering,
            )),
        };

        let original_class_ty = inferred_ty;
        let mut class_ty_before_replacement = inferred_ty;
        let mut replaces_class_binding = false;
        for (decorator_ty, decorator_node) in decorators_to_apply {
            let decorated_ty = match self.apply_decorator(decorator_ty, inferred_ty, decorator_node)
            {
                Type::DataclassDecorator(_) | Type::DataclassTransformer(_) => Type::unknown(),
                decorated_ty => decorated_ty,
            };
            // If a class decorator application loses all precision, preserve the original class
            // binding when the decorator is known to preserve unknown results.
            let preserves_unknown_decorator_binding = is_unknown_decorator_result(db, decorated_ty)
                && (can_preserve_unknown_class_decorator_result(db, decorator_ty)
                    || matches!(&decorator_node.expression, ast::Expr::Call(call) if {
                        can_preserve_unknown_class_decorator_result(
                            db,
                            self.expression_type(&call.func),
                        )
                    }));
            inferred_ty = if preserves_unknown_decorator_binding {
                inferred_ty
            } else if class_decorator_preserves_class_binding(db, original_class_ty, decorated_ty) {
                decorated_ty
                    .as_class_literal()
                    .map(Type::ClassLiteral)
                    .unwrap_or(original_class_ty)
            } else {
                decorated_ty
            };

            if class_decorator_preserves_class_binding(db, original_class_ty, inferred_ty) {
                class_ty_before_replacement = inferred_ty;
            } else {
                replaces_class_binding = true;
            }
        }

        if replaces_class_binding {
            self.undecorated_type = Some(class_ty_before_replacement);
        }

        self.add_declaration_with_binding(
            class_node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(inferred_ty),
        );

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            // In stub files, keyword values may reference names that are defined later in the file.
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());
            for keyword in class_node.keywords() {
                if keyword.arg.as_deref() != Some("extra_items") {
                    self.infer_expression(&keyword.value, TypeContext::default());
                }
            }
            self.deferred_state = previous_deferred_state;

            // Inference of bases deferred in stubs, or if any are string literals.
            if self.in_stub()
                || class_node
                    .bases()
                    .iter()
                    .any(|expr| any_over_expr(expr, &ast::Expr::is_string_literal_expr))
                || class_node
                    .arguments
                    .as_deref()
                    .and_then(|args| args.find_keyword("extra_items"))
                    .is_some()
            {
                self.deferred.insert(definition);
            } else {
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                for base in class_node.bases() {
                    self.infer_expression(base, TypeContext::default());
                }
                self.typevar_binding_context = previous_typevar_binding_context;
            }
        }
    }

    pub(super) fn infer_class_deferred(
        &mut self,
        definition: Definition<'db>,
        class: &ast::StmtClassDef,
    ) {
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        for base in class.bases() {
            if self.in_stub() {
                self.infer_expression_with_state(
                    base,
                    TypeContext::default(),
                    DeferredExpressionState::Deferred,
                );
            } else {
                self.infer_expression(base, TypeContext::default());
            }
        }
        self.typevar_binding_context = previous_typevar_binding_context;

        if let Some(arguments) = class.arguments.as_deref()
            && let Some(extra_items_keyword) = arguments.find_keyword("extra_items")
        {
            let class_type = infer_definition_types(self.db(), definition).binding_type(definition);
            if let Type::ClassLiteral(class_literal) = class_type
                && class_literal.is_typed_dict(self.db())
            {
                self.infer_extra_items_kwarg(&extra_items_keyword.value);
            } else if self.in_stub() {
                self.infer_expression_with_state(
                    &extra_items_keyword.value,
                    TypeContext::default(),
                    DeferredExpressionState::Deferred,
                );
            } else {
                self.infer_expression(&extra_items_keyword.value, TypeContext::default());
            }
        }
    }
}
