use crate::{
    semantic_index::{definition::Definition, scope::NodeWithScopeRef},
    types::{
        CallArguments, ClassLiteral, DataclassParams, KnownClass, KnownInstanceType,
        MemberLookupPolicy, SpecialFormType, StaticClassLiteral, Type, TypeContext,
        call::CallError,
        function::KnownFunction,
        infer::{
            TypeInferenceBuilder,
            builder::{DeclaredAndInferredType, DeferredExpressionState},
        },
        signatures::ParameterForm,
        special_form::TypeQualifier,
    },
};
use ruff_python_ast::{self as ast, helpers::any_over_expr};
use rustc_hash::FxHashMap;
use ty_module_resolver::{KnownModule, file_to_module};

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

        let mut decorator_types_and_nodes: Vec<(Type<'db>, &ast::Decorator)> =
            Vec::with_capacity(decorator_list.len());
        let mut deprecated = None;
        let mut type_check_only = false;
        let mut dataclass_params = None;
        let mut dataclass_transformer_params = None;
        let mut total_ordering = false;
        for decorator in decorator_list {
            let decorator_ty = self.infer_decorator(decorator);
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

            decorator_types_and_nodes.push((decorator_ty, decorator));
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

        let inferred_ty = match (maybe_known_class, &*name.id) {
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

        // Validate decorator calls (but don't use return types yet).
        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            if let Err(CallError(_, bindings)) =
                decorator_ty.try_call(db, &CallArguments::positional([inferred_ty]))
            {
                bindings.report_diagnostics(&self.context, (*decorator_node).into());
            }
        }

        self.add_declaration_with_binding(
            class_node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(inferred_ty),
        );

        // if there are type parameters, then the keywords and bases are within that scope
        // and we don't need to run inference here
        if type_params.is_none() {
            // Inference of bases deferred in stubs, or if any are string literals.
            if self.in_stub()
                || class_node
                    .bases()
                    .iter()
                    .any(|expr| any_over_expr(expr, &ast::Expr::is_string_literal_expr))
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

            self.infer_class_keyword_arguments(inferred_ty, class_node);
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
    }

    /// Infer class keyword argument types with type context from `__init_subclass__` parameters.
    fn infer_class_keyword_arguments(
        &mut self,
        class_ty: Type<'db>,
        class_node: &ast::StmtClassDef,
    ) {
        if class_node.keywords().is_empty() {
            return;
        }

        let db = self.db();

        // Build a map from keyword name to the parameter's annotated type.
        let mut param_types: FxHashMap<String, Type<'db>> = FxHashMap::default();
        if let Type::ClassLiteral(ClassLiteral::Static(class)) = class_ty {
            let init_subclass_type = class
                .class_member_from_mro(
                    db,
                    "__init_subclass__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    // skip(1) to skip the current class and only consider base classes.
                    class.iter_mro(db, None).skip(1),
                )
                .ignore_possibly_undefined();

            if let Some(init_subclass) = init_subclass_type {
                let bindings = init_subclass.bindings(db);
                for binding in bindings.iter_flat() {
                    for overload in binding.overloads() {
                        let parameters = overload.signature.parameters();
                        for param in parameters {
                            if let Some(name) = param.name() {
                                param_types
                                    .entry(name.to_string())
                                    .or_insert(param.annotated_type());
                            }
                        }
                    }
                }
            }
        }

        // In stub files, keyword values may reference names that are defined later in the file.
        let in_stub = self.in_stub();
        let previous_deferred_state = std::mem::replace(&mut self.deferred_state, in_stub.into());

        for keyword in class_node.keywords() {
            let tcx = keyword
                .arg
                .as_ref()
                .and_then(|name| param_types.get(name.id.as_str()).copied())
                .map(|param_ty| TypeContext::new(Some(param_ty)))
                .unwrap_or_default();
            self.infer_expression(&keyword.value, tcx);
        }

        self.deferred_state = previous_deferred_state;
    }
}
