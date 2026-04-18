use crate::{
    Db,
    reachability::ReachabilityConstraintsExtension,
    types::{
        KnownClass, KnownInstanceType, ParamSpecAttrKind, SubclassOfInner, SubclassOfType, Type,
        TypeContext, UnionType,
        context::InNoTypeCheck,
        diagnostic::{
            FINAL_ON_NON_METHOD, INVALID_PARAMETER_DEFAULT, INVALID_PARAMSPEC, INVALID_TYPE_FORM,
            USELESS_OVERLOAD_BODY, add_type_expression_reference_link,
            is_invalid_typed_dict_literal, report_implicit_return_type,
            report_invalid_generator_function_return_type, report_invalid_return_type,
            report_shadowed_type_variable,
        },
        function::{
            FunctionBodyKind, FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction,
            OverloadLiteral, function_body_kind, is_implicit_classmethod,
        },
        generics::{enclosing_generic_contexts, typing_self},
        infer::{
            InferenceFlags, TypeInferenceBuilder,
            builder::{
                DeclaredAndInferredType, DeferredExpressionState, TypeAndRange,
                validate_paramspec_components,
            },
            function_known_decorators, nearest_enclosing_function,
        },
        infer_definition_types, infer_scope_types, todo_type,
        typed_dict::extract_unpacked_typed_dict_keys_from_kwargs_annotation,
    },
};
use ty_python_core::{
    UseDefMap,
    definition::{Definition, DefinitionKind},
    scope::NodeWithScopeRef,
};

use ruff_python_ast as ast;
use ruff_text_size::Ranged;

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(super) fn infer_function_body(&mut self, function: &ast::StmtFunctionDef) {
        fn can_implicitly_return_none<'db>(db: &'db dyn Db, use_def: &UseDefMap<'db>) -> bool {
            !use_def
                .reachability_constraints()
                .evaluate(
                    db,
                    use_def.predicates(),
                    use_def.end_of_scope_reachability(),
                )
                .is_always_false()
        }

        let db = self.db();

        // Parameters are odd: they are Definitions in the function body scope, but have no
        // constituent nodes that are part of the function body. In order to get diagnostics
        // merged/emitted for them, we need to explicitly infer their definitions here.
        for parameter in &function.parameters {
            self.infer_definition(parameter);
        }

        validate_paramspec_components(&self.context, &function.parameters, |expr| {
            self.file_expression_type(expr)
        });
        self.validate_unpacked_typed_dict_kwargs(&function.parameters);

        self.infer_body(&function.body);

        if let Some(returns) = function.returns.as_deref() {
            let has_empty_body = self.return_types_and_ranges.is_empty()
                && function_body_kind(db, function, |expr| self.expression_type(expr))
                    == FunctionBodyKind::Stub;

            let mut enclosing_class_context = None;

            if has_empty_body {
                if self.in_stub() {
                    return;
                }
                if self.in_function_overload_or_abstractmethod() {
                    return;
                }
                if self.is_in_type_checking_block(self.scope(), function) {
                    return;
                }
                if let Some(class) = self.class_context_of_current_method() {
                    enclosing_class_context = Some(class);
                    if class.is_protocol(db) {
                        return;
                    }
                }
            }

            let enclosing_function = nearest_enclosing_function(db, self.index, self.scope())
                .expect("should be in a function body scope");
            let declared_ty = enclosing_function
                .last_definition_raw_signature(db)
                .return_ty;
            let expected_ty = match declared_ty {
                Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_instance(db),
                ty => ty,
            };

            let scope_id = self.index.node_scope(NodeWithScopeRef::Function(function));
            if scope_id.is_generator_function(self.index) {
                // TODO: `AsyncGeneratorType` and `GeneratorType` are both generic classes.
                //
                // If type arguments are supplied to `(Async)Iterable`, `(Async)Iterator`,
                // `(Async)Generator` or `(Async)GeneratorType` in the return annotation,
                // we should iterate over the `yield` expressions and `return` statements
                // in the function to check that they are consistent with the type arguments
                // provided. Once we do this, the `.to_instance_unknown` call below should
                // be replaced with `.to_specialized_instance`.
                let inferred_return = if function.is_async {
                    KnownClass::AsyncGeneratorType
                } else {
                    KnownClass::GeneratorType
                };

                if !inferred_return
                    .to_instance_unknown(db)
                    .is_assignable_to(db, expected_ty)
                {
                    report_invalid_generator_function_return_type(
                        &self.context,
                        returns.range(),
                        inferred_return,
                        declared_ty,
                    );
                }

                if let Some(expected_return_ty) = declared_ty.generator_return_type(db) {
                    for invalid in
                        self.return_types_and_ranges
                            .iter()
                            .copied()
                            .filter(|actual_return_ty| {
                                !actual_return_ty.ty.is_assignable_to(db, expected_return_ty)
                            })
                    {
                        report_invalid_return_type(
                            &self.context,
                            invalid.range,
                            returns.range(),
                            expected_return_ty,
                            invalid.ty,
                        );
                    }

                    let use_def = self.index.use_def_map(scope_id);

                    if can_implicitly_return_none(db, use_def)
                        && !Type::none(db).is_assignable_to(db, expected_return_ty)
                    {
                        let no_return = self.return_types_and_ranges.is_empty();
                        report_implicit_return_type(
                            &self.context,
                            returns.range(),
                            expected_return_ty,
                            false,
                            None,
                            no_return,
                        );
                    }
                }

                return;
            }

            for invalid in self
                .return_types_and_ranges
                .iter()
                .copied()
                .filter_map(|ty_range| match ty_range.ty {
                    // We skip `is_assignable_to` checks for `NotImplemented`,
                    // so we remove it beforehand.
                    Type::Union(union) => Some(TypeAndRange {
                        ty: union.filter(db, |ty| !ty.is_notimplemented(db)),
                        range: ty_range.range,
                    }),
                    ty if ty.is_notimplemented(db) => None,
                    _ => Some(ty_range),
                })
                .filter(|ty_range| !ty_range.ty.is_assignable_to(db, expected_ty))
            {
                report_invalid_return_type(
                    &self.context,
                    invalid.range,
                    returns.range(),
                    declared_ty,
                    invalid.ty,
                );
            }
            let use_def = self.index.use_def_map(scope_id);
            if can_implicitly_return_none(db, use_def)
                && !Type::none(db).is_assignable_to(db, expected_ty)
            {
                let no_return = self.return_types_and_ranges.is_empty();
                report_implicit_return_type(
                    &self.context,
                    returns.range(),
                    declared_ty,
                    has_empty_body,
                    enclosing_class_context,
                    no_return,
                );
            }
        }
    }

    pub(super) fn infer_function_definition_statement(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_definition(function);
    }

    pub(super) fn infer_function_definition(
        &mut self,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) {
        let ast::StmtFunctionDef {
            range: _,
            node_index: _,
            is_async: _,
            name,
            type_params,
            parameters,
            returns: _,
            body: _,
            decorator_list,
        } = function;

        let db = self.db();

        let decorator_inference = function_known_decorators(db, definition);
        self.context.extend(decorator_inference.diagnostics());
        self.expressions.extend(
            decorator_inference
                .expression_types()
                .iter()
                .map(|(expression, ty)| (*expression, *ty)),
        );
        self.bindings.extend(decorator_inference.bindings());
        self.called_functions
            .extend(decorator_inference.called_functions().iter().copied());

        let mut decorator_types_and_nodes = Vec::with_capacity(decorator_list.len());
        let mut function_decorators = FunctionDecorators::empty();
        let mut deprecated = None;
        let mut dataclass_transformer_params = None;
        let mut final_decorator = None;

        for decorator in decorator_list {
            let decorator_type = decorator_inference
                .expression_type(&decorator.expression)
                .unwrap_or_else(Type::unknown);
            let decorator_function_decorator =
                FunctionDecorators::from_decorator_type(db, decorator_type);
            function_decorators |= decorator_function_decorator;

            match decorator_type {
                Type::FunctionLiteral(function) => match function.known(db) {
                    Some(KnownFunction::NoTypeCheck) => {
                        // If the function is decorated with the `no_type_check` decorator,
                        // we need to suppress any errors that come after the decorators.
                        self.context.set_in_no_type_check(InNoTypeCheck::Yes);
                        continue;
                    }
                    Some(KnownFunction::Final) => {
                        final_decorator = Some(decorator);
                        continue;
                    }
                    _ => {}
                },
                Type::KnownInstance(KnownInstanceType::Deprecated(deprecated_inst)) => {
                    deprecated = Some(deprecated_inst);
                }
                Type::DataclassTransformer(params) => {
                    dataclass_transformer_params = Some(params);
                }
                _ => {}
            }
            if !decorator_function_decorator.is_empty() {
                continue;
            }

            decorator_types_and_nodes.push((decorator_type, decorator));
        }

        // Check for `@final` applied to non-method functions.
        // `@final` is only meaningful on methods and classes.
        if let Some(final_decorator) = final_decorator
            && !self
                .index
                .scope(self.scope().file_scope_id(db))
                .kind()
                .is_class()
            && let Some(builder) = self
                .context
                .report_lint(&FINAL_ON_NON_METHOD, final_decorator)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "`@final` cannot be applied to non-method function `{name}`",
            ));
            diagnostic.info("`@final` is only meaningful on methods and classes");
        }

        let has_defaults = parameters
            .iter_non_variadic_params()
            .any(|param| param.default.is_some());

        // If there are type params, parameters and returns are evaluated in that scope. Otherwise,
        // we always defer the inference of the parameters and returns. That ensures that we do not
        // add any spurious salsa cycles when applying decorators below. (Applying a decorator
        // requires getting the signature of this function definition, which in turn requires
        // (lazily) inferring the parameter and return types.) If defaults exist, we also defer so
        // they can be inferred once with type context in the enclosing scope.
        if type_params.is_none() || has_defaults {
            self.deferred.insert(definition);
        }

        let known_function = KnownFunction::try_from_definition_and_name(db, definition, name);

        // `type_check_only` is itself not available at runtime
        if known_function == Some(KnownFunction::TypeCheckOnly) {
            function_decorators |= FunctionDecorators::TYPE_CHECK_ONLY;
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Function(function))
            .to_scope_id(db, self.file());

        let overload_literal = OverloadLiteral::new(
            db,
            &name.id,
            known_function,
            body_scope,
            function_decorators,
            deprecated,
            dataclass_transformer_params,
        );
        let function_literal = FunctionLiteral {
            last_definition: overload_literal,
        };

        let mut inferred_ty =
            Type::FunctionLiteral(FunctionType::new(db, function_literal, None, None));
        self.undecorated_type = Some(inferred_ty);

        // Check that the function's own type parameters don't shadow
        // type variables from enclosing scopes (by name).
        if let Some(type_params) = &function.type_params {
            let current_scope = self.scope().file_scope_id(db);
            for type_param in type_params.iter() {
                let param_name = type_param.name();
                for enclosing in enclosing_generic_contexts(db, self.index, current_scope) {
                    if let Some(other_typevar) = enclosing.binds_named_typevar(db, &param_name.id) {
                        report_shadowed_type_variable(
                            &self.context,
                            &param_name.id,
                            "function",
                            &function.name.id,
                            function.name.range(),
                            other_typevar,
                        );
                    }
                }
            }
        }

        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            inferred_ty = self.apply_decorator(*decorator_ty, inferred_ty, decorator_node);
        }

        self.add_declaration_with_binding(
            function.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(inferred_ty),
        );

        if function_decorators.contains(FunctionDecorators::OVERLOAD) {
            for stmt in &function.body {
                match stmt {
                    ast::Stmt::Pass(_) => continue,
                    ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                        if matches!(
                            &**value,
                            ast::Expr::StringLiteral(_) | ast::Expr::EllipsisLiteral(_)
                        ) {
                            continue;
                        }
                    }
                    _ => {}
                }
                let Some(builder) = self.context.report_lint(&USELESS_OVERLOAD_BODY, stmt) else {
                    continue;
                };
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Useless body for `@overload`-decorated function `{}`",
                    &function.name
                ));
                diagnostic.set_primary_message("This statement will never be executed");
                diagnostic.info(
                    "`@overload`-decorated functions are solely for type checkers \
                    and must be overwritten at runtime by a non-`@overload`-decorated implementation",
                );
                diagnostic.help("Consider replacing this function body with `...` or `pass`");
                break;
            }
        }
    }

    pub(super) fn infer_function_deferred(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        let db = self.db();
        let mut prev_in_no_type_check = self.context.set_in_no_type_check(InNoTypeCheck::Yes);
        for decorator in &function.decorator_list {
            let decorator_type = self.infer_decorator(decorator);
            if let Type::FunctionLiteral(function) = decorator_type
                && let Some(KnownFunction::NoTypeCheck) = function.known(db)
            {
                // If the function is decorated with the `no_type_check` decorator,
                // we need to suppress any errors that come after the decorators.
                prev_in_no_type_check = InNoTypeCheck::Yes;
                break;
            }
        }
        self.context.set_in_no_type_check(prev_in_no_type_check);

        let has_type_params = function.type_params.is_some();
        let has_defaults = function
            .parameters
            .iter_non_variadic_params()
            .any(|param| param.default.is_some());

        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);

        if !has_type_params {
            self.infer_return_type_annotation(function.returns.as_deref());
            self.infer_parameters(function.parameters.as_ref());
        }

        if has_defaults {
            // In stub files, default values may reference names that are defined later in the file.
            let in_stub = self.in_stub();
            let previous_deferred_state =
                std::mem::replace(&mut self.deferred_state, in_stub.into());

            // For generic functions, only defaults are inferred here; annotation types come from
            // the type-params scope.
            if has_type_params {
                let type_params_scope = self
                    .index
                    .node_scope(NodeWithScopeRef::FunctionTypeParameters(function))
                    .to_scope_id(db, self.file());
                let type_params_inference =
                    infer_scope_types(db, type_params_scope, TypeContext::default());

                for param_with_default in function.parameters.iter_non_variadic_params() {
                    let Some(default) = param_with_default.default.as_deref() else {
                        continue;
                    };
                    let tcx = param_with_default
                        .parameter
                        .annotation
                        .as_deref()
                        .map(|annotation| {
                            TypeContext::new(Some(
                                type_params_inference.expression_type(annotation),
                            ))
                        })
                        .unwrap_or_else(TypeContext::default);
                    self.infer_expression(default, tcx);
                }
            } else {
                for param_with_default in function.parameters.iter_non_variadic_params() {
                    let Some(default) = param_with_default.default.as_deref() else {
                        continue;
                    };
                    let tcx = param_with_default
                        .parameter
                        .annotation
                        .as_deref()
                        .map(|annotation| TypeContext::new(Some(self.expression_type(annotation))))
                        .unwrap_or_else(TypeContext::default);
                    self.infer_expression(default, tcx);
                }
            }

            self.deferred_state = previous_deferred_state;
        }

        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_return_type_annotation(&mut self, returns: Option<&ast::Expr>) {
        if let Some(returns) = returns {
            self.inference_flags |= InferenceFlags::IN_RETURN_TYPE;
            self.infer_type_expression_with_state(
                returns,
                DeferredExpressionState::from(self.defer_annotations()),
            );
            self.inference_flags.remove(InferenceFlags::IN_RETURN_TYPE);
        }
    }

    pub(super) fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let type_params = function
            .type_params
            .as_deref()
            .expect("function type params scope without type params");

        let binding_context = self.index.expect_single_definition(function);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);
        self.infer_return_type_annotation(function.returns.as_deref());
        self.infer_type_parameters(type_params);
        self.infer_parameters(&function.parameters);
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn infer_parameters(&mut self, parameters: &ast::Parameters) {
        let ast::Parameters {
            range: _,
            node_index: _,
            posonlyargs: _,
            args: _,
            vararg,
            kwonlyargs: _,
            kwarg,
        } = parameters;

        self.inference_flags |= InferenceFlags::IN_PARAMETER_ANNOTATION;
        for param_with_default in parameters.iter_non_variadic_params() {
            self.infer_parameter_with_default(param_with_default);
        }
        if let Some(vararg) = vararg {
            self.inference_flags |= InferenceFlags::IN_VARARG_ANNOTATION;
            self.infer_parameter(vararg);
            self.inference_flags
                .remove(InferenceFlags::IN_VARARG_ANNOTATION);
        }
        if let Some(kwarg) = kwarg {
            self.inference_flags |= InferenceFlags::IN_KWARG_ANNOTATION;
            self.infer_parameter(kwarg);
            self.inference_flags
                .remove(InferenceFlags::IN_KWARG_ANNOTATION);
        }
        self.inference_flags
            .remove(InferenceFlags::IN_PARAMETER_ANNOTATION);
    }

    fn validate_unpacked_typed_dict_kwargs(&mut self, parameters: &ast::Parameters) {
        let Some(kwargs) = parameters.kwarg.as_ref() else {
            return;
        };
        let Some(annotation) = kwargs.annotation.as_deref() else {
            return;
        };
        let annotated_type = self.file_expression_type(annotation);
        let Some(unpacked_keys) = extract_unpacked_typed_dict_keys_from_kwargs_annotation(
            self.db(),
            self.file(),
            annotation,
            annotated_type,
            |expr| self.file_expression_type(expr),
        ) else {
            return;
        };

        let overlapping = parameters
            .iter_non_variadic_params()
            .skip(parameters.posonlyargs.len())
            // Legacy PEP 484 positional-only parameters like `def f(__x: int, **kwargs:
            // Unpack[TD])` are not callable by keyword, so they do not overlap with keys
            // accepted through `**kwargs`.
            .filter(|parameter| !parameter.uses_pep_484_positional_only_convention())
            .map(|parameter| &parameter.parameter)
            .filter(|parameter| unpacked_keys.contains_key(&parameter.name.id))
            .collect::<Vec<_>>();

        if overlapping.is_empty() {
            return;
        }

        let overlapping_names = overlapping
            .iter()
            .map(|parameter| format!("`{}`", parameter.name.id))
            .collect::<Vec<_>>()
            .join(", ");

        if let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPE_FORM, kwargs.as_ref())
        {
            if overlapping.len() == 1 {
                builder.into_diagnostic(format_args!(
                    "Parameter {overlapping_names} overlaps with unpacked TypedDict key in \
                     `**kwargs` annotation",
                ));
            } else {
                builder.into_diagnostic(format_args!(
                    "Parameters {overlapping_names} overlap with unpacked TypedDict keys in \
                     `**kwargs` annotation",
                ));
            }
        }
    }

    fn infer_parameter_with_default(&mut self, parameter_with_default: &ast::ParameterWithDefault) {
        let ast::ParameterWithDefault {
            range: _,
            node_index: _,
            parameter,
            default: _,
        } = parameter_with_default;

        if let Some(annotation) = parameter.annotation.as_deref() {
            self.infer_type_expression_with_state(
                annotation,
                DeferredExpressionState::from(self.defer_annotations()),
            );
        }
    }

    fn infer_parameter(&mut self, parameter: &ast::Parameter) {
        let ast::Parameter {
            range: _,
            node_index: _,
            name: _,
            annotation,
        } = parameter;

        if let Some(annotation) = annotation.as_deref() {
            self.infer_type_expression_with_state(
                annotation,
                DeferredExpressionState::from(self.defer_annotations()),
            );
        }
    }

    /// Set initial declared type (if annotated) and inferred type for a function-parameter symbol,
    /// in the function body scope.
    ///
    /// The declared type is the annotated type, if any, or `Unknown`.
    ///
    /// The inferred type is the annotated type, if any. If there is no annotation, it is the union
    /// of `Unknown` and the type of the default value, if any.
    ///
    /// Parameter definitions are odd in that they define a symbol in the function-body scope, so
    /// the Definition belongs to the function body scope, but the expressions (annotation and
    /// default value) both belong to outer scopes. (The default value always belongs to the outer
    /// scope in which the function is defined, the annotation belongs either to the outer scope,
    /// or maybe to an intervening type-params scope, if it's a generic function.) So we don't use
    /// `self.infer_expression` or store any expression types here, we just query for the types of
    /// the expressions from their respective scopes.
    ///
    /// It is safe (non-cycle-causing) to query the annotation type via `file_expression_type`
    /// here, because an outer scope can't depend on a definition from an inner scope, so we
    /// shouldn't be in-process of inferring the outer scope here.
    pub(super) fn infer_parameter_definition(
        &mut self,
        parameter_with_default: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) {
        let ast::ParameterWithDefault {
            parameter,
            default,
            range: _,
            node_index: _,
        } = parameter_with_default;

        let db = self.db();

        let default_expr = default.as_ref();
        if let Some(annotation) = parameter.annotation.as_ref() {
            let declared_ty = self.file_expression_type(annotation);

            // P.args and P.kwargs are only valid as annotations on *args and **kwargs,
            // not on regular parameters.
            if let Type::TypeVar(typevar) = declared_ty
                && typevar.is_paramspec(db)
                && let Some(attr) = typevar.paramspec_attr(db)
            {
                let name = typevar.name(db);
                let (attr_name, variadic) = match attr {
                    ParamSpecAttrKind::Args => ("args", "*args"),
                    ParamSpecAttrKind::Kwargs => ("kwargs", "**kwargs"),
                };
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_PARAMSPEC, annotation.as_ref())
                {
                    builder.into_diagnostic(format_args!(
                        "`{name}.{attr_name}` is only valid for annotating `{variadic}`",
                    ));
                }
            }

            if let Some(default_expr) = default_expr {
                let default_expr = default_expr.as_ref();
                let default_ty = self.file_expression_type(default_expr);

                // Avoid duplicate diagnostics: invalid TypedDict literals already emit specific errors.
                let suppress_invalid_default =
                    is_invalid_typed_dict_literal(db, declared_ty, default_expr.into());
                if !default_ty.is_assignable_to(db, declared_ty)
                    && !suppress_invalid_default
                    && !((self.in_stub()
                        || self.in_function_overload_or_abstractmethod()
                        || self.is_in_type_checking_block(self.scope(), default_expr)
                        || self
                            .class_context_of_current_method()
                            .is_some_and(|class| class.is_protocol(db)))
                        && default
                            .as_ref()
                            .is_some_and(|d| d.is_ellipsis_literal_expr()))
                {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_PARAMETER_DEFAULT, parameter_with_default)
                    {
                        builder.into_diagnostic(format_args!(
                            "Default value of type `{}` is not assignable \
                             to annotated parameter type `{}`",
                            default_ty.display(db),
                            declared_ty.display(db)
                        ));
                    }
                }
            }

            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(declared_ty),
            );
        } else {
            let ty = if let Some(default_expr) = default_expr {
                let default_ty = self.file_expression_type(default_expr);
                UnionType::from_two_elements(db, Type::unknown(), default_ty)
            } else if let Some(ty) = self.special_first_method_parameter_type(parameter) {
                ty
            } else {
                Type::unknown()
            };

            self.add_binding(parameter.into(), definition)
                .insert(self, ty);
        }
    }

    /// Set initial declared/inferred types for a `*args` variadic positional parameter.
    ///
    /// The annotated type is implicitly wrapped in a homogeneous tuple.
    ///
    /// See [`infer_parameter_definition`] doc comment for some relevant observations about scopes.
    ///
    /// [`infer_parameter_definition`]: Self::infer_parameter_definition
    pub(super) fn infer_variadic_positional_parameter_definition(
        &mut self,
        parameter: &'ast ast::Parameter,
        definition: Definition<'db>,
    ) {
        let db = self.db();

        if let Some(annotation) = parameter.annotation() {
            let ty = if annotation.is_starred_expr() {
                todo_type!("PEP 646")
            } else {
                let annotated_type = self.file_expression_type(annotation);
                if let Type::TypeVar(typevar) = annotated_type
                    && typevar.is_paramspec(db)
                {
                    match typevar.paramspec_attr(db) {
                        // `*args: P.args`
                        Some(ParamSpecAttrKind::Args) => annotated_type,

                        // `*args: P.kwargs`
                        Some(ParamSpecAttrKind::Kwargs) => {
                            // TODO: Should this diagnostic be raised as part of
                            // `ArgumentTypeChecker`?
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                            {
                                let name = typevar.name(db);
                                let mut diag = builder.into_diagnostic(format_args!(
                                    "`{name}.kwargs` is valid only in `**kwargs` annotation",
                                ));
                                diag.set_primary_message(format_args!(
                                    "Did you mean `{name}.args`?"
                                ));
                                add_type_expression_reference_link(diag);
                            }
                            Type::homogeneous_tuple(db, Type::unknown())
                        }

                        // `*args: P`
                        None => {
                            // The diagnostic for this case is handled in `in_type_expression`.
                            Type::homogeneous_tuple(db, Type::unknown())
                        }
                    }
                } else {
                    Type::homogeneous_tuple(db, annotated_type)
                }
            };

            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            let inferred_ty = Type::homogeneous_tuple(db, Type::unknown());
            self.add_binding(parameter.into(), definition)
                .insert(self, inferred_ty);
        }
    }

    /// Special case for unannotated `cls` and `self` arguments to class methods and instance methods.
    fn special_first_method_parameter_type(
        &mut self,
        parameter: &ast::Parameter,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let file = self.file();

        let function_scope_id = self.scope();
        let function_scope = function_scope_id.scope(db);
        let function = function_scope.node().as_function()?;

        let parent_file_scope_id = function_scope.parent()?;
        let mut parent_scope_id = parent_file_scope_id.to_scope_id(db, file);

        // Skip type parameter scopes, if the method itself is generic.
        if parent_scope_id.is_annotation(db) {
            let parent_scope = parent_scope_id.scope(db);
            parent_scope_id = parent_scope.parent()?.to_scope_id(db, file);
        }

        // Return early if this is not a method inside a class.
        let class = parent_scope_id.scope(db).node().as_class()?;

        let method_definition = self.index.expect_single_definition(function);
        let DefinitionKind::Function(function_definition) = method_definition.kind(db) else {
            return None;
        };

        if function_definition
            .node(self.module())
            .parameters
            .index(parameter.name())
            .is_none_or(|index| index != 0)
        {
            return None;
        }

        let function_node = function_definition.node(self.module());
        let function_name = &function_node.name;

        let mut is_classmethod = is_implicit_classmethod(function_name);
        let inference = infer_definition_types(db, method_definition);
        for decorator in &function_node.decorator_list {
            let decorator_ty = inference.expression_type(&decorator.expression);
            if let Some(known_class) = decorator_ty
                .as_class_literal()
                .and_then(|class| class.known(db))
            {
                if known_class == KnownClass::Staticmethod {
                    return None;
                }

                is_classmethod |= known_class == KnownClass::Classmethod;
            }
        }

        let class_definition = self.index.expect_single_definition(class);
        let class_literal = infer_definition_types(db, class_definition)
            .declaration_type(class_definition)
            .inner_type()
            .as_class_literal()?;

        let typing_self = typing_self(db, self.scope(), Some(method_definition), class_literal);
        if is_classmethod || function_name == "__new__" {
            typing_self
                .map(|typing_self| SubclassOfType::from(db, SubclassOfInner::TypeVar(typing_self)))
        } else {
            typing_self.map(Type::TypeVar)
        }
    }

    /// Set initial declared/inferred types for a `**kwargs` keyword-variadic parameter.
    ///
    /// The annotated type is implicitly wrapped in a string-keyed dictionary.
    ///
    /// See [`infer_parameter_definition`] doc comment for some relevant observations about scopes.
    ///
    /// [`infer_parameter_definition`]: Self::infer_parameter_definition
    pub(super) fn infer_variadic_keyword_parameter_definition(
        &mut self,
        parameter: &'ast ast::Parameter,
        definition: Definition<'db>,
    ) {
        let db = self.db();

        if let Some(annotation) = parameter.annotation() {
            let annotated_type = self.file_expression_type(annotation);
            let ty = if let Type::TypeVar(typevar) = annotated_type
                && typevar.is_paramspec(db)
            {
                match typevar.paramspec_attr(db) {
                    // `**kwargs: P.args`
                    Some(ParamSpecAttrKind::Args) => {
                        // TODO: Should this diagnostic be raised as part of `ArgumentTypeChecker`?
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_TYPE_FORM, annotation)
                        {
                            let name = typevar.name(db);
                            let mut diag = builder.into_diagnostic(format_args!(
                                "`{name}.args` is valid only in `*args` annotation",
                            ));
                            diag.set_primary_message(format_args!("Did you mean `{name}.kwargs`?"));
                            add_type_expression_reference_link(diag);
                        }
                        KnownClass::Dict.to_specialized_instance(
                            db,
                            &[KnownClass::Str.to_instance(db), Type::unknown()],
                        )
                    }

                    // `**kwargs: P.kwargs`
                    Some(ParamSpecAttrKind::Kwargs) => annotated_type,

                    // `**kwargs: P`
                    None => {
                        // The diagnostic for this case is handled in `in_type_expression`.
                        KnownClass::Dict.to_specialized_instance(
                            db,
                            &[KnownClass::Str.to_instance(db), Type::unknown()],
                        )
                    }
                }
            } else if extract_unpacked_typed_dict_keys_from_kwargs_annotation(
                db,
                self.file(),
                annotation,
                annotated_type,
                |expr| self.file_expression_type(expr),
            )
            .is_some()
            {
                annotated_type
            } else {
                KnownClass::Dict
                    .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), annotated_type])
            };
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            let inferred_ty = KnownClass::Dict
                .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::unknown()]);

            self.add_binding(parameter.into(), definition)
                .insert(self, inferred_ty);
        }
    }
}
