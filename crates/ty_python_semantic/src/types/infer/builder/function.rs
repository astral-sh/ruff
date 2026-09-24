use crate::{
    Db, DisplaySettings, FxIndexMap, ProgramEnvironment,
    reachability::ReachabilityConstraintsExtension,
    types::{
        Binding, CallArguments, CallableType, ClassBase, ClassLiteral, ClassType, DynamicType,
        IntersectionBuilder, KnownClass, KnownInstanceType, ParamSpecAttrKind, SpecialFormType,
        SubclassOfInner, SubclassOfType, Type, TypeContext, TypeVarBoundOrConstraints, TypeVarKind,
        UnionBuilder, UnionType,
        callable::CallableTypeKind,
        constraints::ConstraintSetBuilder,
        context::InferContext,
        cyclic::ActiveRecursionDetector,
        diagnostic::{
            ABSTRACT_AND_FINAL_METHOD, ASSERT_TYPE_UNSPELLABLE_SUBTYPE, DISJOINT_CAST,
            FINAL_ON_NON_METHOD, INVALID_ARGUMENT_TYPE, INVALID_PARAMETER_DEFAULT,
            INVALID_PARAMSPEC, INVALID_TYPE_FORM, REDUNDANT_CAST, STATIC_ASSERT_ERROR,
            TYPE_ASSERTION_FAILURE, UNSOUND_RETURN_STATEMENT, USELESS_OVERLOAD_BODY,
            add_type_expression_reference_link, is_invalid_typed_dict_literal,
            report_bad_argument_to_get_protocol_members, report_bad_argument_to_protocol_interface,
            report_implicit_return_type, report_invalid_generator_function_return_type,
            report_invalid_return_type, report_invalid_total_ordering_call,
            report_issubclass_check_against_protocol_with_non_method_members,
            report_runtime_check_against_non_runtime_checkable_protocol,
            report_runtime_check_against_typed_dict, report_shadowed_type_variable,
            report_unsound_return_statement,
        },
        function::{
            FunctionBodyKind, FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction,
            OverloadLiteral, function_body_kind, is_implicit_classmethod, report_revealed_type,
            same_module_uncached_raw_signature,
        },
        generics::{enclosing_generic_contexts, typing_self},
        infer::{
            InferenceFlags, TypeExpressionFlags, TypeInferenceBuilder,
            builder::{
                DeclaredAndInferredType, DeferredExpressionState, TypeAndRange,
                validate_paramspec_components,
            },
            function_known_decorator_flags, function_known_decorators, infer_deferred_types,
            infer_function_default_types, infer_statement_types, nearest_enclosing_function,
            original_class_type,
        },
        infer_definition_types,
        list_members::all_members,
        relation::TypeRelation,
        signatures::{ReturnCallableTypeVarScope, function_signature_expression_type},
        tuple::{TupleSpec, TupleSpecBuilder, TupleType},
        typed_dict::extract_unpacked_typed_dict_keys_from_kwargs_annotation,
        typevar::TypeVarSet,
        visitor::non_any_dynamic_content,
    },
};
use ruff_db::{
    diagnostic::{Annotation, DiagnosticId, Severity, Span},
    parsed::parsed_module,
    source::source_text,
};
use ruff_diagnostics::{Edit, Fix};
use ruff_python_edits::unwrapped_call_argument;
use ty_module_resolver::{ImportingFile, ModuleName, resolve_module};
use ty_python_core::{
    Truthiness, UseDefMap,
    definition::{Definition, DefinitionKind},
    scope::NodeWithScopeRef,
};

use ruff_python_ast::{self as ast, find_node::covering_node};
use ruff_text_size::Ranged;

fn parameters_have_defaults(parameters: &ast::Parameters) -> bool {
    parameters
        .iter_non_variadic_params()
        .any(|param| param.default.is_some())
}

fn function_has_deferred_annotations(function: &ast::StmtFunctionDef) -> bool {
    function.type_params.is_none()
        && (function.returns.is_some()
            || function
                .parameters
                .iter()
                .any(|param| param.annotation().is_some()))
}

/// Whether a non-static method receives an instance or the class itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodReceiverKind {
    Instance,
    Class,
}

impl MethodReceiverKind {
    /// Classifies methods by their decorators and implicit class-receiver rules.
    ///
    /// Free functions and ordinary static methods have no receiver; `__new__` receives the class.
    ///
    /// ```python
    /// class Example:
    ///     def instance(self): ...
    ///     @classmethod
    ///     def class_method(cls): ...
    ///     @staticmethod
    ///     def static_method(): ...
    /// ```
    fn from_function<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> Option<Self> {
        if !definition.scope(db).scope(db).kind().is_class() {
            return None;
        }

        let decorators = if function.decorator_list.is_empty() {
            FunctionDecorators::empty()
        } else {
            function_known_decorator_flags(db, definition)
        };
        if decorators.contains(FunctionDecorators::STATICMETHOD) && function.name.id != "__new__" {
            return None;
        }

        if decorators.contains(FunctionDecorators::CLASSMETHOD)
            || is_implicit_classmethod(&function.name)
            || function.name.id == "__new__"
        {
            Some(Self::Class)
        } else {
            Some(Self::Instance)
        }
    }

    /// Accepts only `Self` for an instance receiver and `type[Self]` for a class receiver.
    fn accepts_annotation(self, db: &dyn Db, annotation: Type<'_>) -> bool {
        match (self, annotation) {
            (Self::Instance, Type::TypeVar(typevar)) => typevar.typevar(db).is_self(db),
            (Self::Class, Type::SubclassOf(subclass)) => {
                matches!(
                    subclass.subclass_of(),
                    SubclassOfInner::TypeVar(typevar) if typevar.typevar(db).is_self(db)
                )
            }
            _ => false,
        }
    }
}

/// Return type policy for checking explicit `return` statements in a function body.
#[derive(Debug, Copy, Clone)]
struct ExpectedReturnType<'db> {
    /// The externally-visible return type.
    public: Type<'db>,
    /// The return type as seen from inside the function body.
    lexical: Type<'db>,
}

impl<'db> ExpectedReturnType<'db> {
    /// Creates the expected return type policy for `function`.
    fn from_function(db: &'db dyn Db, function: FunctionType<'db>) -> Self {
        /// Normalizes special return annotations to the type actually returned by expressions.
        fn normalize<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
        ) -> Type<'db> {
            match ty.resolve_type_alias(db) {
                Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_instance(db, env),
                _ => ty,
            }
        }

        let env = ProgramEnvironment::from_file(function.program_file(db));
        let public = normalize(
            db,
            &env,
            same_module_uncached_raw_signature(db, function, ReturnCallableTypeVarScope::Public)
                .return_ty,
        );
        let lexical = normalize(
            db,
            &env,
            same_module_uncached_raw_signature(db, function, ReturnCallableTypeVarScope::Lexical)
                .return_ty,
        );

        Self { public, lexical }
    }

    /// Returns the externally-visible return type.
    fn public(self) -> Type<'db> {
        self.public
    }

    /// Returns `true` if `ty` is accepted by either the public return type or the lexical return
    /// type.
    fn accepts(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        relation: TypeRelation,
    ) -> bool {
        let builder = ConstraintSetBuilder::new();

        let check =
            |target| ty.has_relation_to(db, env, target, &builder, TypeVarSet::None, relation);

        check(self.public)
            .or(db, &builder, || check(self.lexical))
            .is_always_satisfied(db, env)
    }
}

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

        let env = self.program_environment();
        let db = self.db();

        // Parameters are odd: they are Definitions in the function body scope, but have no
        // constituent nodes that are part of the function body. In order to get diagnostics
        // merged/emitted for them, we need to explicitly infer their definitions here.
        for parameter in &function.parameters {
            self.infer_definition(parameter);
        }

        validate_paramspec_components(&self.context, self.index, &function.parameters, |expr| {
            self.file_expression_type(expr)
        });
        self.validate_unpacked_typed_dict_kwargs(&function.parameters);

        self.infer_body(&function.body);

        if let Some(returns) = function.returns.as_deref() {
            let has_empty_body = self.return_types_and_ranges.is_empty()
                && function_body_kind(db, env, function, |expr| self.expression_type(expr))
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
            let declared_ty = same_module_uncached_raw_signature(
                db,
                enclosing_function,
                ReturnCallableTypeVarScope::Public,
            )
            .return_ty;
            let expected_return = ExpectedReturnType::from_function(db, enclosing_function);
            let expected_ty = expected_return.public();

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
                    .to_instance_unknown(db, env)
                    .is_assignable_to(db, env, expected_ty)
                {
                    report_invalid_generator_function_return_type(
                        &self.context,
                        returns.range(),
                        inferred_return,
                        declared_ty,
                    );
                }

                if let Some(expected_return_ty) = declared_ty.generator_return_type(db, env) {
                    for &return_statement in &self.return_types_and_ranges {
                        if !return_statement
                            .ty
                            .is_assignable_to(db, env, expected_return_ty)
                        {
                            report_invalid_return_type(
                                &self.context,
                                return_statement.range,
                                returns.range(),
                                expected_return_ty,
                                return_statement.ty,
                            );
                        } else if self.context.is_lint_enabled(&UNSOUND_RETURN_STATEMENT)
                            && expected_return_ty.is_fully_static(db, env)
                            && !return_statement.ty.is_pure_redundant_with(
                                db,
                                env,
                                expected_return_ty,
                            )
                        {
                            // N.B. the implementation here is the ~same as for `UNSOUND_YIELD` and `UNSOUND_ASSIGNMENT`;
                            // update those too if updating this!
                            report_unsound_return_statement(
                                &self.context,
                                return_statement.range,
                                returns.range(),
                                expected_return_ty,
                                return_statement.ty,
                            );
                        }
                    }

                    let use_def = self.index.use_def_map(scope_id);

                    if can_implicitly_return_none(db, use_def)
                        && !Type::none(db, env).is_assignable_to(db, env, expected_return_ty)
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

            for return_statement in
                self.return_types_and_ranges
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
            {
                if !expected_return.accepts(
                    db,
                    env,
                    return_statement.ty,
                    TypeRelation::Assignability,
                ) {
                    report_invalid_return_type(
                        &self.context,
                        return_statement.range,
                        returns.range(),
                        declared_ty,
                        return_statement.ty,
                    );
                } else if self.context.is_lint_enabled(&UNSOUND_RETURN_STATEMENT)
                    && expected_return.public.is_fully_static(db, env)
                    && !expected_return.accepts(
                        db,
                        env,
                        return_statement.ty,
                        TypeRelation::Redundancy { pure: true },
                    )
                {
                    // N.B. the implementation here is the ~same as for `UNSOUND_YIELD` and `UNSOUND_ASSIGNMENT`;
                    // update those too if updating this!
                    report_unsound_return_statement(
                        &self.context,
                        return_statement.range,
                        returns.range(),
                        declared_ty,
                        return_statement.ty,
                    );
                }
            }

            let use_def = self.index.use_def_map(scope_id);
            if can_implicitly_return_none(db, use_def)
                && !Type::none(db, env).is_assignable_to(db, env, expected_ty)
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
            type_params: _,
            parameters,
            returns: _,
            body: _,
            decorator_list,
        } = function;

        let db = self.db();

        let decorator_inference =
            (!decorator_list.is_empty()).then(|| function_known_decorators(self.db(), definition));
        if let Some(decorator_inference) = decorator_inference.as_ref() {
            self.context.extend(decorator_inference.diagnostics());
            self.expressions
                .extend(decorator_inference.expression_types());
            self.bindings.extend(decorator_inference.bindings());
            self.called_functions
                .extend(decorator_inference.called_functions().iter().copied());
            self.implicit_aliases
                .extend(decorator_inference.implicit_aliases().iter().copied());
        }

        let mut decorator_types_and_nodes = Vec::with_capacity(decorator_list.len());
        let mut has_transforming_decorators = false;
        let mut function_decorators = FunctionDecorators::empty();
        let mut dataclass_transformer_params = None;
        let mut final_decorator = None;

        for decorator in decorator_list {
            let decorator_type = decorator_inference
                .as_ref()
                .and_then(|decorator_inference| {
                    decorator_inference.expression_type(&decorator.expression)
                })
                .unwrap_or_else(Type::unknown);
            let decorator_function_decorator =
                FunctionDecorators::from_decorator_type(db, decorator_type);
            function_decorators |= decorator_function_decorator;

            match decorator_type {
                Type::FunctionLiteral(function) => match function.known(db) {
                    Some(KnownFunction::NoTypeCheck) => {
                        // If the function is decorated with the `no_type_check` decorator,
                        // we need to suppress any errors that come after the decorators.
                        self.context.inference_flags |= InferenceFlags::IN_NO_TYPE_CHECK;
                        continue;
                    }
                    Some(KnownFunction::Final) => {
                        final_decorator = Some(decorator);
                        continue;
                    }
                    _ => {}
                },
                Type::DataclassTransformer(params) => {
                    dataclass_transformer_params = Some(params);
                }
                _ => {}
            }
            if !decorator_function_decorator.is_empty()
                && !decorator_function_decorator
                    .intersects(FunctionDecorators::CLASSMETHOD | FunctionDecorators::STATICMETHOD)
            {
                continue;
            }

            has_transforming_decorators |= decorator_function_decorator.is_empty();
            decorator_types_and_nodes.push((decorator_type, decorator));
        }
        if !has_transforming_decorators {
            // With only known decorators, use the complete overload set's declaration
            // flags, including its recovery for inconsistently decorated overloads.
            decorator_types_and_nodes.clear();
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

        if function_decorators
            .contains(FunctionDecorators::ABSTRACT_METHOD | FunctionDecorators::FINAL)
            && self
                .index
                .scope(self.scope().file_scope_id(db))
                .kind()
                .is_class()
            && let Some(builder) = self.context.report_lint(&ABSTRACT_AND_FINAL_METHOD, name)
        {
            builder.into_diagnostic(format_args!(
                "Method `{name}` cannot be both `@abstractmethod` and `@final`",
            ));
        }

        // If there are type params, parameters and returns are evaluated in that scope. Otherwise,
        // we defer the inference of any parameter and return annotations. That ensures that we do
        // not add any spurious salsa cycles when applying decorators below. (Applying a decorator
        // requires getting the signature of this function definition, which in turn requires
        // (lazily) inferring the parameter and return types.) If defaults exist, we also defer so
        // they can be inferred once with type context in the enclosing scope.
        if function_has_deferred_annotations(function) || parameters_have_defaults(parameters) {
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
            .to_scope_id(db, self.program_file());

        let overload_literal = OverloadLiteral::new(
            db,
            &name.id,
            known_function,
            body_scope,
            function_decorators,
            None,
            dataclass_transformer_params,
            function.returns.is_some(),
        );
        let function_literal = FunctionLiteral::new(db, overload_literal);
        let function_type = FunctionType::new(db, function_literal, None);
        let is_decorated_overload_implementation =
            has_transforming_decorators && function_literal.has_separate_implementation(db);
        let is_decorated_overload = has_transforming_decorators && overload_literal.is_overload(db);

        let mut inferred_ty = Type::FunctionLiteral(
            if is_decorated_overload_implementation || is_decorated_overload {
                FunctionType::new(db, function_literal.without_overloads(), None)
            } else {
                function_type
            },
        );
        // Explicit method wrappers apply in decorator order. Their declaration flags
        // must not make the input to an inner decorator look already wrapped.
        if has_transforming_decorators
            && function_decorators
                .intersects(FunctionDecorators::STATICMETHOD | FunctionDecorators::CLASSMETHOD)
        {
            inferred_ty = inferred_ty.underlying_function(db);
        }
        if !decorator_list.is_empty() {
            self.undecorated_type = Some(inferred_ty);
        }

        // Check that the function's own type parameters don't shadow
        // type variables from enclosing scopes (by name).
        if let Some(type_params) = &function.type_params {
            let current_scope = self.scope().file_scope_id(db);
            for type_param in type_params.iter() {
                let param_name = type_param.name();
                for enclosing in enclosing_generic_contexts(self.db(), self.index, current_scope) {
                    if let Some(other_typevar) = enclosing.binds_named_typevar(db, &param_name.id) {
                        let kind = match type_param {
                            ast::TypeParam::TypeVar(_) => TypeVarKind::Pep695TypeVar,
                            ast::TypeParam::ParamSpec(_) => TypeVarKind::Pep695ParamSpec,
                            ast::TypeParam::TypeVarTuple(_) => TypeVarKind::Pep695TypeVarTuple,
                        };
                        report_shadowed_type_variable(
                            &self.context,
                            &param_name.id,
                            "function",
                            &function.name.id,
                            function.name.range(),
                            kind,
                            other_typevar,
                        );
                    }
                }
            }
        }

        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            let descriptor_kind = match decorator_ty {
                Type::ClassLiteral(class) => match class.known(db) {
                    Some(KnownClass::Staticmethod) => Some(CallableTypeKind::StaticMethodLike),
                    Some(KnownClass::Classmethod) => Some(CallableTypeKind::ClassMethodLike),
                    _ => None,
                },
                _ => None,
            };
            if let Some(kind) = descriptor_kind {
                let wrap = |ty: Type<'db>| match ty.resolve_type_alias(db) {
                    Type::FunctionLiteral(function)
                        if function.callable_type_kind(db) == CallableTypeKind::FunctionLike
                            || function.callable_type_kind(db) == kind =>
                    {
                        Some(Type::FunctionLiteral(
                            function.with_descriptor_kind(db, kind),
                        ))
                    }
                    Type::Callable(callable)
                        if matches!(
                            callable.kind(db),
                            CallableTypeKind::Regular | CallableTypeKind::FunctionLike
                        ) || callable.kind(db) == kind =>
                    {
                        Some(Type::Callable(callable.with_kind(db, kind)))
                    }
                    _ => None,
                };
                let wrapped = if let Some(union) = inferred_ty.as_union_like(db) {
                    union.try_map(db, self.program_environment(), |ty| wrap(*ty))
                } else {
                    wrap(inferred_ty)
                };
                if let Some(wrapped) = wrapped {
                    inferred_ty = wrapped;
                    continue;
                }
            }
            if let Type::KnownInstance(KnownInstanceType::Deprecated(deprecated)) = decorator_ty {
                match inferred_ty {
                    Type::FunctionLiteral(function) => {
                        inferred_ty =
                            Type::FunctionLiteral(function.with_deprecated(db, *deprecated));
                        continue;
                    }
                    Type::Callable(callable) => {
                        inferred_ty = Type::Callable(callable.with_deprecated(
                            db,
                            overload_literal.with_deprecated(db, *deprecated),
                        ));
                        continue;
                    }
                    _ => {}
                }
            }
            let decorated_ty = inferred_ty;
            inferred_ty = self.apply_decorator(
                *decorator_ty,
                inferred_ty,
                decorator_node,
                (!is_decorated_overload_implementation).then_some(function),
            );
            if let Type::PropertyInstance(property) = inferred_ty {
                inferred_ty = Type::PropertyInstance(property.with_accessor_definition(
                    db,
                    *decorator_ty,
                    decorated_ty,
                    definition,
                ));
            }
        }

        if is_decorated_overload_implementation {
            // Overloads describe the exposed function. The implementation check compares the
            // unbound callable, before classmethod or staticmethod descriptor binding.
            let unwrap_method = |ty| match ty {
                Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
                    wrapper.wrapped(db)
                }
                _ => ty,
            };
            let implementation_ty = match inferred_ty {
                Type::Union(union) => {
                    union.map(db, self.program_environment(), |ty| unwrap_method(*ty))
                }
                _ => unwrap_method(inferred_ty),
            };
            let last_definition = match inferred_ty {
                Type::FunctionLiteral(function) => Some(function.literal(db).last_definition),
                Type::Callable(callable) => callable.deprecated(db),
                _ => None,
            };
            let function_type = if let Some(last_definition) = last_definition {
                FunctionType::new(
                    db,
                    function_literal.with_last_definition_metadata(db, last_definition),
                    None,
                )
            } else {
                function_type
            };
            let implementation_callables = implementation_ty
                .try_upcast_to_callable(db, self.program_environment())
                .map_or_else(Box::default, |callables| {
                    callables.iter().copied().collect()
                });
            inferred_ty = Type::FunctionLiteral(
                function_type.with_implementation_callables(db, implementation_callables),
            );
        } else if is_decorated_overload && let Type::FunctionLiteral(function) = inferred_ty {
            inferred_ty = Type::FunctionLiteral(FunctionType::new(
                db,
                function_literal
                    .with_last_definition_metadata(db, function.literal(db).last_definition),
                None,
            ));
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
                    function.name
                ));
                diagnostic.set_primary_annotation_message("This statement will never be executed");
                diagnostic.info(
                    "`@overload`-decorated functions are solely for type checkers \
                    and must be overwritten at runtime by a non-`@overload`-decorated implementation",
                );
                diagnostic.help("Consider replacing this function body with `...` or `pass`");
                break;
            }
        }
    }

    pub(super) fn extend_function_deferred(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        let db = self.db();
        if function_has_deferred_annotations(function) {
            self.extend_definition(definition, infer_deferred_types(db, definition));
        }
        if parameters_have_defaults(&function.parameters) {
            self.extend_definition(definition, infer_function_default_types(db, definition));
        }
    }

    pub(super) fn infer_function_annotations(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        // PEP 695 annotations are inferred in the function's type-parameter scope.
        if !function_has_deferred_annotations(function) {
            return;
        }

        self.suppress_errors_for_no_type_check(definition, function);
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);
        self.infer_function_signature_annotations(function, definition);
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    pub(super) fn infer_function_defaults(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        let db = self.db();
        if !parameters_have_defaults(&function.parameters) {
            return;
        }

        self.suppress_errors_for_no_type_check(definition, function);
        let previous_typevar_binding_context = self.typevar_binding_context.replace(definition);

        // In stub files, default values may reference names that are defined later in the file.
        let previous_deferred_state = self.replace_deferred_state(self.in_stub().into());

        // Borrow annotation types from their own inference result instead of copying that result
        // into this query. Scope inference merges both regions when checking the whole function.
        for param_with_default in function.parameters.iter_non_variadic_params() {
            let Some(default) = param_with_default.default() else {
                continue;
            };
            let annotation = param_with_default
                .annotation()
                .map(|annotation| function_signature_expression_type(db, definition, annotation));
            self.infer_expression(default, TypeContext::new(annotation));
        }

        self.deferred_state = previous_deferred_state;
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    fn suppress_errors_for_no_type_check(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        if !function.decorator_list.is_empty()
            && function_known_decorator_flags(self.db(), definition)
                .contains(FunctionDecorators::NO_TYPE_CHECK)
        {
            // Decorator expressions and their diagnostics belong to their own inference query.
            // Signature and default inference only need to know whether errors are suppressed.
            self.context.inference_flags |= InferenceFlags::IN_NO_TYPE_CHECK;
        }
    }

    fn infer_return_type_annotation(&mut self, returns: Option<&ast::Expr>) {
        if let Some(returns) = returns {
            self.context.inference_flags |= InferenceFlags::IN_RETURN_TYPE;
            self.infer_type_expression_with_state(
                returns,
                DeferredExpressionState::from(self.defer_annotations()),
            );
            self.context
                .inference_flags
                .remove(InferenceFlags::IN_RETURN_TYPE);
        }
    }

    pub(super) fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let binding_context = self.index.expect_single_definition(function);
        let previous_typevar_binding_context =
            self.typevar_binding_context.replace(binding_context);
        self.infer_function_signature_annotations(function, binding_context);
        self.typevar_binding_context = previous_typevar_binding_context;
    }

    /// Infer an annotated method receiver before the rest of its signature so `Self` can be
    /// validated where it occurs, including inside parsed string annotations.
    ///
    /// ```python
    /// class Example:
    ///     def method(self: object) -> "Self | object": ...
    /// ```
    fn infer_function_signature_annotations(
        &mut self,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) {
        let receiver_is_incompatible = self.infer_method_receiver_annotation(function, definition);
        let previous_incompatible_receiver = self.context.inference_flags.replace(
            InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER,
            receiver_is_incompatible == Some(true),
        );

        self.infer_return_type_annotation(function.returns.as_deref());
        if let Some(type_params) = function.type_params.as_deref() {
            self.infer_type_parameters(type_params);
        }
        self.infer_parameters(&function.parameters, receiver_is_incompatible.is_some());

        self.context.inference_flags.set(
            InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER,
            previous_incompatible_receiver,
        );
    }

    /// Infers an explicitly annotated method receiver before the rest of its signature.
    ///
    /// Returns whether the annotation is incompatible with `Self`, or `None` for functions without
    /// an annotated instance or class receiver.
    fn infer_method_receiver_annotation(
        &mut self,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> Option<bool> {
        let receiver = function
            .parameters
            .posonlyargs
            .first()
            .or_else(|| function.parameters.args.first())?;
        let annotation = receiver.parameter.annotation.as_deref()?;
        let receiver_kind = MethodReceiverKind::from_function(self.db(), definition, function)?;

        let previously_in_parameter_annotation = self
            .context
            .inference_flags
            .replace(InferenceFlags::IN_PARAMETER_ANNOTATION, true);
        let previously_in_init_receiver = self.context.inference_flags.replace(
            InferenceFlags::IN_INIT_RECEIVER_ANNOTATION,
            function.name.id == "__init__" && receiver_kind == MethodReceiverKind::Instance,
        );
        let annotation_type = self.infer_type_expression_with_state(
            annotation,
            DeferredExpressionState::from(self.defer_annotations()),
        );
        self.context.inference_flags.set(
            InferenceFlags::IN_PARAMETER_ANNOTATION,
            previously_in_parameter_annotation,
        );

        self.context.inference_flags.set(
            InferenceFlags::IN_INIT_RECEIVER_ANNOTATION,
            previously_in_init_receiver,
        );

        Some(!receiver_kind.accepts_annotation(self.db(), annotation_type))
    }

    fn infer_parameters(
        &mut self,
        parameters: &ast::Parameters,
        first_annotation_already_inferred: bool,
    ) {
        let ast::Parameters {
            range: _,
            node_index: _,
            posonlyargs: _,
            args: _,
            vararg,
            kwonlyargs: _,
            kwarg,
        } = parameters;

        self.context.inference_flags |= InferenceFlags::IN_PARAMETER_ANNOTATION;
        for param_with_default in parameters
            .iter_non_variadic_params()
            .skip(usize::from(first_annotation_already_inferred))
        {
            self.infer_parameter_with_default(param_with_default);
        }
        if let Some(vararg) = vararg {
            self.context.inference_flags |= InferenceFlags::IN_VARARG_ANNOTATION;
            self.infer_parameter(vararg);
            self.context
                .inference_flags
                .remove(InferenceFlags::IN_VARARG_ANNOTATION);
        }
        if let Some(kwarg) = kwarg {
            self.context.inference_flags |= InferenceFlags::IN_KWARG_ANNOTATION;
            self.infer_parameter(kwarg);
            self.context
                .inference_flags
                .remove(InferenceFlags::IN_KWARG_ANNOTATION);
        }
        self.context
            .inference_flags
            .remove(InferenceFlags::IN_PARAMETER_ANNOTATION);
    }

    fn validate_unpacked_typed_dict_kwargs(&mut self, parameters: &ast::Parameters) {
        let db = self.db();
        let env = self.program_environment();
        let Some(kwargs) = parameters.kwarg.as_ref() else {
            return;
        };
        let Some(annotation) = kwargs.annotation.as_deref() else {
            return;
        };
        let annotation_flags = self.file_type_expression_flags(annotation);
        if !annotation_flags.contains(TypeExpressionFlags::UNPACK) {
            return;
        }

        let annotated_type = self.file_expression_type(annotation);
        let Some(unpacked_keys) = extract_unpacked_typed_dict_keys_from_kwargs_annotation(
            db,
            annotated_type,
            annotation_flags,
        ) else {
            if !annotated_type.is_unknown()
                && let Some(builder) = self.context.report_lint(&INVALID_TYPE_FORM, annotation)
            {
                let diag = builder.into_diagnostic(format_args!(
                    "Unpacked value for `**kwargs` must be a TypedDict, not `{}`",
                    annotated_type.display(db, env)
                ));
                add_type_expression_reference_link(diag);
            }
            return;
        };

        // Legacy PEP 484 positional-only parameters like `def f(__x: int, **kwargs:
        // Unpack[TD])` are not callable by keyword, so they do not overlap with keys
        // accepted through `**kwargs`. The convention only applies to the leading
        // positional-or-keyword parameters that are actually converted to positional-only
        // parameters by `Parameters::from_parameters`.
        let pep_484_positional_only_count = if parameters.posonlyargs.is_empty() {
            parameters
                .args
                .iter()
                .take_while(|parameter| parameter.uses_pep_484_positional_only_convention())
                .count()
        } else {
            0
        };

        let overlapping = parameters
            .iter_non_variadic_params()
            .skip(parameters.posonlyargs.len() + pep_484_positional_only_count)
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
        let env = self.program_environment();
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
                    is_invalid_typed_dict_literal(db, env, declared_ty, default_expr.into());
                if !default_ty.is_assignable_to(db, env, declared_ty)
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
                            default_ty.display(db, env),
                            declared_ty.display(db, env)
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
                UnionType::from_two_elements(db, env, Type::unknown(), default_ty)
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
            let annotated_type = self.file_expression_type(annotation);
            let has_unpacked_annotation = self
                .file_type_expression_flags(annotation)
                .contains(TypeExpressionFlags::UNPACK);
            let ty = match annotated_type {
                Type::TypeVar(typevar)
                    if has_unpacked_annotation && typevar.is_typevartuple(db) =>
                {
                    Type::tuple(TupleType::new(
                        db,
                        self.program_environment(),
                        &TupleSpecBuilder::with_capacity(0)
                            .concat_variadic_typevar(db, self.program_environment(), typevar)
                            .build(),
                    ))
                }
                _ if has_unpacked_annotation => annotated_type,
                Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
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
                                diag.set_primary_annotation_message(format_args!(
                                    "Did you mean `{name}.args`?"
                                ));
                                add_type_expression_reference_link(diag);
                            }
                            Type::homogeneous_tuple(db, self.program_environment(), Type::unknown())
                        }

                        // `*args: P`
                        None => {
                            // The diagnostic for this case is handled in `in_type_expression`.
                            Type::homogeneous_tuple(db, self.program_environment(), Type::unknown())
                        }
                    }
                }
                _ => Type::homogeneous_tuple(db, self.program_environment(), annotated_type),
            };

            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            let inferred_ty =
                Type::homogeneous_tuple(db, self.program_environment(), Type::unknown());
            self.add_binding(parameter.into(), definition)
                .insert(self, inferred_ty);
        }
    }

    /// Special case for unannotated `cls` and `self` arguments to class methods and instance methods.
    fn special_first_method_parameter_type(
        &mut self,
        parameter: &ast::Parameter,
    ) -> Option<Type<'db>> {
        let env = self.program_environment();
        let db = self.db();
        let file = self.program_file();

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
        let receiver_kind =
            MethodReceiverKind::from_function(db, method_definition, function_node)?;

        let class_definition = self.index.expect_single_definition(class);
        let class_literal = original_class_type(db, class_definition)?;
        let typing_self = typing_self(db, self.scope(), Some(method_definition), class_literal);
        match receiver_kind {
            MethodReceiverKind::Class => typing_self.map(|typing_self| {
                SubclassOfType::from(db, env, SubclassOfInner::TypeVar(typing_self))
            }),
            MethodReceiverKind::Instance => typing_self.map(Type::TypeVar),
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
        let env = self.program_environment();
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
                            diag.set_primary_annotation_message(format_args!(
                                "Did you mean `{name}.kwargs`?"
                            ));
                            add_type_expression_reference_link(diag);
                        }
                        KnownClass::Dict.to_specialized_instance(
                            db,
                            env,
                            &[KnownClass::Str.to_instance(db, env), Type::unknown()],
                        )
                    }

                    // `**kwargs: P.kwargs`
                    Some(ParamSpecAttrKind::Kwargs) => annotated_type,

                    // `**kwargs: P`
                    None => {
                        // The diagnostic for this case is handled in `in_type_expression`.
                        KnownClass::Dict.to_specialized_instance(
                            db,
                            env,
                            &[KnownClass::Str.to_instance(db, env), Type::unknown()],
                        )
                    }
                }
            } else if extract_unpacked_typed_dict_keys_from_kwargs_annotation(
                db,
                annotated_type,
                self.file_type_expression_flags(annotation),
            )
            .is_some()
            {
                annotated_type
            } else {
                KnownClass::Dict.to_specialized_instance(
                    db,
                    env,
                    &[KnownClass::Str.to_instance(db, env), annotated_type],
                )
            };
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(ty),
            );
        } else {
            let inferred_ty = KnownClass::Dict.to_specialized_instance(
                db,
                env,
                &[KnownClass::Str.to_instance(db, env), Type::unknown()],
            );

            self.add_binding(parameter.into(), definition)
                .insert(self, inferred_ty);
        }
    }

    /// Set initial declared type (if annotated) and inferred type for a lambda-parameter symbol,
    /// in the lambda body scope.
    pub(super) fn infer_lambda_parameter_definition(
        &mut self,
        index: u32,
        parameter_with_default: &'ast ast::ParameterWithDefault,
        lambda: &'ast ast::ExprLambda,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let ast::ParameterWithDefault {
            parameter,
            default,
            range: _,
            node_index: _,
        } = parameter_with_default;

        let ty = if let Some(parameter_type) = self.annotated_lambda_parameter_type(index, lambda) {
            parameter_type
        } else if let Some(default_expr) = default {
            let default_ty = self.file_expression_type(default_expr);
            UnionType::from_two_elements(
                db,
                self.program_environment(),
                Type::Dynamic(DynamicType::UnknownLambdaParameter),
                default_ty,
            )
        } else {
            Type::Dynamic(DynamicType::UnknownLambdaParameter)
        };

        self.add_binding(parameter.into(), definition)
            .insert(self, ty);
    }

    /// Set initial declared/inferred types for a `*args` variadic positional parameter
    /// in a lambda expression.
    pub(super) fn infer_variadic_positional_lambda_parameter_definition(
        &mut self,
        index: u32,
        parameter: &'ast ast::Parameter,
        lambda: &'ast ast::ExprLambda,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        // Note that this currently always returns `None` because we do not support `Unpack`
        // annotations for callable types.
        let ty = if let Some(parameter_type) = self.annotated_lambda_parameter_type(index, lambda) {
            parameter_type
        } else {
            Type::homogeneous_tuple(
                db,
                self.program_environment(),
                Type::Dynamic(DynamicType::UnknownLambdaParameter),
            )
        };
        self.add_binding(parameter.into(), definition)
            .insert(self, ty);
    }

    /// Set initial declared/inferred types for a `**kwargs` keyword-variadic parameter
    /// in a lambda expression.
    pub(super) fn infer_variadic_keyword_lambda_parameter_definition(
        &mut self,
        parameter: &'ast ast::Parameter,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let env = self.program_environment();
        let inferred_ty = KnownClass::Dict.to_specialized_instance(
            db,
            env,
            &[
                KnownClass::Str.to_instance(db, env),
                Type::Dynamic(DynamicType::UnknownLambdaParameter),
            ],
        );

        self.add_binding(parameter.into(), definition)
            .insert(self, inferred_ty);
    }

    /// Returns the annotated type of the lambda parameter at the given index in the provided
    /// lambda expression, based on a `Callable` type annotation, if present.
    fn annotated_lambda_parameter_type(
        &mut self,
        index: u32,
        lambda: &'ast ast::ExprLambda,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let enclosing_stmt = infer_statement_types(
            self.db(),
            self.index.enclosing_lambda_statement(lambda.into())?,
        );
        let callable = enclosing_stmt.expression_type(lambda).as_callable()?;
        let [signature] = callable.signatures(self.db()).overloads.as_slice() else {
            // TODO: If there are multiple applicable overloads, we could attempt multi-inference.
            return None;
        };

        let parameter_type = signature.parameters().as_slice()[index as usize].annotated_type();
        (!parameter_type.has_provisional_marker(db, self.program_environment()))
            .then_some(parameter_type)
    }
}

impl KnownFunction {
    /// Evaluate a call to this known function, and emit any diagnostics that are necessary
    /// as a result of the call.
    pub(super) fn check_call<'db>(
        self,
        builder: &TypeInferenceBuilder<'db, '_>,
        overload: &mut Binding<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression: &ast::ExprCall,
    ) {
        let db = builder.db();
        let parameter_types = overload.parameter_types();

        match self {
            KnownFunction::RevealType => {
                let env = builder.program_environment();
                let revealed_type = overload
                    .arguments_for_parameter(call_arguments, 0)
                    .fold(UnionBuilder::new(db, env), |builder, (_, ty)| {
                        builder.add(ty)
                    })
                    .build();
                report_revealed_type(
                    &builder.context,
                    revealed_type,
                    call_argument_node(call_expression, "obj", 0)
                        .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                );
            }

            KnownFunction::HasMember => {
                let [Some(ty), Some(Type::LiteralValue(literal))] = parameter_types else {
                    return;
                };
                let Some(member) = literal.as_string() else {
                    return;
                };
                let env = builder.program_environment();
                let ty_members = all_members(db, env, *ty);
                overload.set_return_type(Type::bool_literal(
                    ty_members.iter().any(|m| m.name == member.value(db)),
                ));
            }

            KnownFunction::AssertType => {
                let [Some(actual_ty), Some(asserted_ty)] = parameter_types else {
                    return;
                };
                let env = builder.program_environment();
                let asserted_ty = asserted_ty.project_type_form(db, env);
                if actual_ty.is_equivalent_to(db, env, asserted_ty) {
                    return;
                }
                let diagnostic = if actual_ty.is_spellable(db)
                    || !actual_ty.is_subtype_of(db, env, asserted_ty)
                {
                    &TYPE_ASSERTION_FAILURE
                } else {
                    &ASSERT_TYPE_UNSPELLABLE_SUBTYPE
                };
                if let Some(diagnostic) = builder.context.report_lint(diagnostic, call_expression) {
                    let settings = DisplaySettings::from_possibly_ambiguous_types(
                        db,
                        env,
                        [*actual_ty, asserted_ty],
                    );
                    let mut diagnostic = diagnostic.into_diagnostic(format_args!(
                        "Argument does not have asserted type `{}`",
                        asserted_ty.display_with(db, env, settings.clone()),
                    ));

                    diagnostic.annotate(
                        Annotation::secondary(
                            builder.context.span(
                                call_argument_node(call_expression, "val", 0)
                                    .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                            ),
                        )
                        .message(format_args!(
                            "Inferred type is `{}`",
                            actual_ty.display_with(db, env, settings.clone())
                        )),
                    );

                    if actual_ty.is_subtype_of(db, env, asserted_ty) {
                        diagnostic.info(format_args!(
                            "`{inferred_type}` is a subtype of `{asserted_type}`, but they are not equivalent",
                            asserted_type = asserted_ty.display_with(db, env, settings.clone()),
                            inferred_type = actual_ty.display_with(db, env, settings.clone()),
                        ));
                    } else {
                        diagnostic.info(format_args!(
                            "`{asserted_type}` and `{inferred_type}` are not equivalent types",
                            asserted_type = asserted_ty.display_with(db, env, settings.clone()),
                            inferred_type = actual_ty.display_with(db, env, settings.clone()),
                        ));
                    }

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` does not match asserted type `{}`",
                        actual_ty.display_with(db, env, settings.clone()),
                        asserted_ty.display_with(db, env, settings),
                    ));
                }
            }

            KnownFunction::AssertNever => {
                let [Some(actual_ty)] = parameter_types else {
                    return;
                };
                let env = builder.program_environment();
                if actual_ty.is_equivalent_to(db, env, Type::Never) {
                    return;
                }
                if let Some(diagnostic) = builder
                    .context
                    .report_lint(&TYPE_ASSERTION_FAILURE, call_expression)
                {
                    let mut diagnostic =
                        diagnostic.into_diagnostic("Argument does not have asserted type `Never`");
                    diagnostic.annotate(
                        Annotation::secondary(
                            builder.context.span(
                                call_argument_node(call_expression, "arg", 0)
                                    .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                            ),
                        )
                        .message(format_args!(
                            "Inferred type of argument is `{}`",
                            actual_ty.display(db, env)
                        )),
                    );
                    diagnostic.info(format_args!(
                        "`Never` and `{inferred_type}` are not equivalent types",
                        inferred_type = actual_ty.display(db, env),
                    ));

                    diagnostic.set_concise_message(format_args!(
                        "Type `{}` is not equivalent to `Never`",
                        actual_ty.display(db, env),
                    ));
                }
            }

            KnownFunction::StaticAssert => {
                let [Some(parameter_ty), message] = parameter_types else {
                    return;
                };
                let env = builder.program_environment();
                let truthiness = match parameter_ty.try_bool(db, env) {
                    Ok(truthiness) => truthiness,
                    Err(err) => {
                        err.report_diagnostic(
                            &builder.context,
                            call_argument_node(call_expression, "condition", 0)
                                .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                        );

                        return;
                    }
                };

                if let Some(diagnostic) = builder
                    .context
                    .report_lint(&STATIC_ASSERT_ERROR, call_expression)
                {
                    if truthiness.is_always_true() {
                        return;
                    }
                    let mut diagnostic = if let Some(message) = message
                        .and_then(Type::as_string_literal)
                        .map(|s| s.value(db))
                    {
                        diagnostic
                            .into_diagnostic(format_args!("Static assertion error: {message}"))
                    } else if *parameter_ty == Type::bool_literal(false) {
                        diagnostic.into_diagnostic(
                            "Static assertion error: argument evaluates to `False`",
                        )
                    } else if truthiness.is_always_false() {
                        diagnostic.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            is always falsy",
                            parameter_ty = parameter_ty.display(db, env)
                        ))
                    } else {
                        diagnostic.into_diagnostic(format_args!(
                            "Static assertion error: argument of type `{parameter_ty}` \
                            has an ambiguous static truthiness",
                            parameter_ty = parameter_ty.display(db, env)
                        ))
                    };
                    if let Some(condition) = call_argument_node(call_expression, "condition", 0) {
                        diagnostic.annotate(
                            Annotation::secondary(builder.context.span(condition)).message(
                                format_args!(
                                    "Inferred type of argument is `{}`",
                                    parameter_ty.display(db, env)
                                ),
                            ),
                        );
                    }
                }
            }

            KnownFunction::Cast => {
                let [Some(casted_type), Some(source_type)] = parameter_types else {
                    return;
                };
                let env = builder.context.program_environment();
                let casted_type = casted_type.project_type_form(db, env);
                if source_type.is_equivalent_to(db, env, casted_type)
                    && non_any_dynamic_content(db, env, *source_type).is_absent()
                    && non_any_dynamic_content(db, env, casted_type).is_absent()
                {
                    if let Some(diagnostic) = builder
                        .context
                        .report_lint(&REDUNDANT_CAST, call_expression)
                    {
                        let source_display = source_type.display(db, env).to_string();
                        let casted_display = casted_type.display(db, env).to_string();
                        let mut diagnostic = diagnostic.into_diagnostic(format_args!(
                            "Value is already of type `{casted_display}`",
                        ));
                        if source_display != casted_display {
                            diagnostic.info(format_args!(
                                "`{casted_display}` is equivalent to `{source_display}`",
                            ));
                        }
                        if let Some(value) = call_expression.arguments.find_argument_value("val", 1)
                        {
                            let source = source_text(db, builder.file());
                            let covering = covering_node(
                                builder.context.module().syntax().into(),
                                call_expression.range(),
                            );
                            let replacement = unwrapped_call_argument(
                                call_expression,
                                value,
                                covering.parent(),
                                builder.module().tokens(),
                                &source,
                            );
                            diagnostic.help("Remove the redundant `cast`");
                            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                                replacement,
                                call_expression.range(),
                            )));
                        }
                    }
                } else if builder.context.is_lint_enabled(&DISJOINT_CAST)
                    && !builder.file().is_stub(db)
                    && !builder.index.is_in_type_checking_block(
                        builder.scope().file_scope_id(db),
                        call_expression.range(),
                    )
                    && source_type.is_disjoint_from(db, env, casted_type)
                    && !casted_type.is_equivalent_to(db, env, Type::Never)
                    && !source_type.is_equivalent_to(db, env, Type::Never)
                    && call_expression
                        .arguments
                        .find_argument_value("val", 1)
                        .is_none_or(|value_expr| {
                            builder
                                .speculate_without_diagnostics()
                                .infer_expression(value_expr, TypeContext::new(Some(casted_type)))
                                .is_disjoint_from(db, env, casted_type)
                        })
                    && let Some(diagnostic) =
                        builder.context.report_lint(&DISJOINT_CAST, call_expression)
                {
                    let types = [*source_type, casted_type];
                    let settings = DisplaySettings::from_possibly_ambiguous_types(db, env, types);
                    let source_display = source_type.display_with(db, env, settings.clone());
                    let casted_display = casted_type.display_with(db, env, settings.clone());
                    let mut diagnostic = diagnostic.into_diagnostic("Cast to a disjoint type");
                    diagnostic.set_concise_message(format_args!(
                        "Cast from `{source_display}` to disjoint type `{casted_display}`",
                    ));
                    if let Some(arg) = call_expression.arguments.find_argument_value("typ", 0) {
                        diagnostic.annotate(
                            builder
                                .context
                                .secondary(arg)
                                .message("Disjoint from the inferred type"),
                        );
                    }
                    if let Some(arg) = call_expression.arguments.find_argument_value("val", 1) {
                        diagnostic.annotate(
                            builder
                                .context
                                .secondary(arg)
                                .message(format_args!("Inferred as `{source_display}`")),
                        );
                    }

                    // deduplicate definitions before attaching a subdiagnostic to each definition,
                    // or we'd have multiple subdiagnostics pointing to a single definition
                    // if the two types are specializations of the same generic class.
                    let definitions: FxIndexMap<Definition<'db>, String> = types
                        .into_iter()
                        .filter_map(|ty| ty.definition(db, env))
                        .filter_map(|definition| definition.definition())
                        .filter_map(|definition| Some((definition, definition.name(db)?)))
                        .collect();

                    for (definition, name) in definitions {
                        let file = definition.python_file(db);
                        let module = parsed_module(db, file).load(db);
                        let mut range = definition.focus_range(db, &module);
                        if let DefinitionKind::Class(class) = definition.kind(db) {
                            let definition_types = infer_definition_types(db, definition);
                            if let Some(decorator) =
                                class.node(&module).decorator_list.iter().find(|decorator| {
                                    definition_types
                                        .expression_type(&decorator.expression)
                                        .as_function_literal()
                                        .is_some_and(|func| func.is_known(db, KnownFunction::Final))
                                })
                            {
                                range = range.cover_range(decorator.range());
                            }
                        }
                        diagnostic.annotate(
                            Annotation::secondary(Span::from(range))
                                .message(format_args!("`{name}` defined here")),
                        );
                    }

                    if casted_type.is_protocol_instance() {
                        if source_type.is_protocol_instance() {
                            diagnostic.info(format_args!(
                                "protocol `{casted_display}` is disjoint \
                                from protocol `{source_display}`"
                            ));
                        } else {
                            diagnostic.info(format_args!(
                                "protocol `{casted_display}` is disjoint \
                                from `{source_display}`"
                            ));
                        }
                    } else if source_type.is_protocol_instance() {
                        diagnostic.info(format_args!(
                            "`{casted_display}` is disjoint \
                            from protocol `{source_display}`"
                        ));
                    } else {
                        diagnostic.info(format_args!(
                            "`{casted_display}` is disjoint from `{source_display}`"
                        ));
                    }

                    source_type
                        .disjointness_error_context(db, env, casted_type)
                        .attach_to(db, env, &mut diagnostic);
                }
            }

            KnownFunction::GetProtocolMembers => {
                let [Some(Type::ClassLiteral(class))] = parameter_types else {
                    return;
                };
                if class.is_protocol(builder.db()) {
                    return;
                }
                report_bad_argument_to_get_protocol_members(
                    &builder.context,
                    call_expression,
                    *class,
                );
            }

            KnownFunction::RevealProtocolInterface => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let env = builder.program_environment();
                let Some(protocol_class) = param_type
                    .to_class_type(db)
                    .and_then(|class| class.into_protocol_class(db))
                else {
                    report_bad_argument_to_protocol_interface(
                        &builder.context,
                        call_expression,
                        *param_type,
                    );
                    return;
                };
                if let Some(diagnostic) = builder
                    .context
                    .report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let mut diag = diagnostic.into_diagnostic("Revealed protocol interface");
                    let span = builder.context.span(
                        call_argument_node(call_expression, "protocol", 0)
                            .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                    );
                    diag.annotate(Annotation::primary(span).message(format_args!(
                        "`{}`",
                        protocol_class.interface(db).display(db, env)
                    )));
                }
            }

            KnownFunction::RevealMro => {
                let [Some(param_type)] = parameter_types else {
                    return;
                };
                let mut good_argument = true;
                let classes = match param_type {
                    Type::ClassLiteral(class) => vec![ClassType::NonGeneric(*class)],
                    Type::GenericAlias(generic_alias) => vec![ClassType::Generic(*generic_alias)],
                    Type::Union(union) => {
                        let elements = union.elements(db);
                        let mut classes = Vec::with_capacity(elements.len());
                        for element in elements {
                            match element {
                                Type::ClassLiteral(class) => {
                                    classes.push(ClassType::NonGeneric(*class));
                                }
                                Type::GenericAlias(generic_alias) => {
                                    classes.push(ClassType::Generic(*generic_alias));
                                }
                                _ => {
                                    good_argument = false;
                                    break;
                                }
                            }
                        }
                        classes
                    }
                    _ => {
                        good_argument = false;
                        vec![]
                    }
                };
                if !good_argument {
                    let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_ARGUMENT_TYPE, call_expression)
                    else {
                        return;
                    };
                    let mut diagnostic =
                        builder.into_diagnostic("Invalid argument to `reveal_mro`");
                    diagnostic.set_primary_annotation_message(format_args!(
                        "Can only pass a class object, generic alias or a union thereof"
                    ));
                    return;
                }
                if let Some(diagnostic) = builder
                    .context
                    .report_diagnostic(DiagnosticId::RevealedType, Severity::Info)
                {
                    let env = builder.program_environment();
                    let mut diag = diagnostic.into_diagnostic("Revealed MRO");
                    let span = builder.context.span(
                        call_argument_node(call_expression, "cls", 0)
                            .unwrap_or_else(|| ast::AnyNodeRef::from(call_expression)),
                    );
                    let mut message = String::new();
                    let display_settings = DisplaySettings::from_possibly_ambiguous_types(
                        db,
                        env,
                        classes
                            .iter()
                            .flat_map(|class| class.iter_mro(db))
                            .filter_map(ClassBase::into_class),
                    );
                    for (i, class) in classes.iter().enumerate() {
                        message.push('(');
                        for class in class.iter_mro(db) {
                            message.push_str(
                                &class
                                    .display_with(db, env, display_settings.clone())
                                    .to_string(),
                            );
                            // Omit the comma for the last element (which is always `object`)
                            if class
                                .into_class()
                                .is_none_or(|base| !base.is_object(builder.db()))
                            {
                                message.push_str(", ");
                            }
                        }
                        // If the last element was also the first element
                        // (i.e., it's a length-1 tuple -- which can only happen if we're revealing
                        // the MRO for `object` itself), add a trailing comma so that it's still a
                        // valid tuple display.
                        if class.is_object(db) {
                            message.push(',');
                        }
                        message.push(')');
                        if i < classes.len() - 1 {
                            message.push_str(" | ");
                        }
                    }
                    diag.annotate(Annotation::primary(span).message(message));
                }
            }

            KnownFunction::IsInstance | KnownFunction::IsSubclass => {
                let [Some(first_arg), Some(second_argument)] = parameter_types else {
                    return;
                };

                check_classinfo_in_isinstance(
                    db,
                    &builder.context,
                    call_expression,
                    self,
                    *second_argument,
                    call_expression.arguments.args.get(1),
                );

                if self == KnownFunction::IsInstance {
                    let env = builder.program_environment();
                    let truthiness = match second_argument {
                        Type::ClassLiteral(class) => {
                            is_instance_truthiness(db, env, *first_arg, *class)
                        }
                        Type::SpecialForm(
                            SpecialFormType::TypingCallable
                            | SpecialFormType::CollectionsAbcCallable,
                        ) => {
                            let callable_top = Type::Callable(CallableType::top(db));
                            if first_arg.is_subtype_of(db, env, callable_top) {
                                Truthiness::AlwaysTrue
                            } else {
                                Truthiness::Ambiguous
                            }
                        }
                        _ if is_instance_tuple_exhaustive(
                            db,
                            env,
                            *first_arg,
                            *second_argument,
                        ) =>
                        {
                            Truthiness::AlwaysTrue
                        }
                        _ => Truthiness::Ambiguous,
                    };
                    overload.set_return_type(Type::from_truthiness(db, env, truthiness));
                }
            }

            known @ (KnownFunction::DunderImport | KnownFunction::ImportModule) => {
                let [Some(first), rest @ ..] = parameter_types else {
                    return;
                };
                let Some(full_module_name) = first.as_string_literal() else {
                    return;
                };

                if rest.iter().any(Option::is_some) {
                    return;
                }

                let module_name = full_module_name.value(db);

                if known == KnownFunction::DunderImport && module_name.contains('.') {
                    // `__import__("collections.abc")` returns the `collections` module.
                    // `importlib.import_module("collections.abc")` returns the `collections.abc` module.
                    // ty doesn't have a way to represent the return type of the former yet.
                    // https://github.com/astral-sh/ruff/pull/19008#discussion_r2173481311
                    return;
                }

                let Some(module_name) = ModuleName::new(module_name) else {
                    return;
                };
                let importing_file = ImportingFile::File(
                    builder.file(),
                    builder.program_environment().resolver_environment(db),
                );
                let Some(module) = resolve_module(db, importing_file, &module_name) else {
                    return;
                };

                overload.set_return_type(Type::module_literal(db, builder.program_file(), module));
            }

            KnownFunction::TotalOrdering => {
                // When `total_ordering(cls)` is called as a function (not as a decorator),
                // check that the class defines at least one ordering method.
                let [Some(class_type)] = parameter_types else {
                    return;
                };

                let class = match class_type {
                    Type::ClassLiteral(class) => ClassType::NonGeneric(*class),
                    Type::GenericAlias(generic) => ClassType::Generic(*generic),
                    _ => return,
                };

                if !class.has_ordering_method_in_mro(db) {
                    report_invalid_total_ordering_call(
                        &builder.context,
                        class.class_literal(db),
                        call_expression,
                    );
                }
            }

            _ => {}
        }
    }
}

fn call_argument_node<'a>(
    call_expression: &'a ast::ExprCall,
    name: &str,
    position: usize,
) -> Option<ast::AnyNodeRef<'a>> {
    call_expression
        .arguments
        .find_argument(name, position)
        .map(|argument| match argument {
            ast::ArgOrKeyword::Arg(expr) => ast::AnyNodeRef::from(expr),
            ast::ArgOrKeyword::Keyword(keyword) => ast::AnyNodeRef::from(keyword),
        })
}

/// Check the second argument to `isinstance()` or `issubclass()` for types that cannot be used
/// at runtime (protocol classes, typed dicts, `typing.Any` in `isinstance`, and invalid
/// `UnionType` elements). Handles class literals, tuples (including nested tuples), and
/// recursively validates each element.
///
/// `classinfo_expr` is the AST expression corresponding to `classinfo`, if available. It is
/// used for precise annotation spans (e.g., highlighting just the `UnionType` inside a tuple
/// rather than the whole tuple). It may be `None` when the tuple is not a literal in the AST
/// (e.g., when it's stored in a variable).
fn check_classinfo_in_isinstance<'db>(
    db: &'db dyn Db,
    context: &InferContext<'db, '_>,
    call_expression: &ast::ExprCall,
    function: KnownFunction,
    classinfo: Type<'db>,
    classinfo_expr: Option<&ast::Expr>,
) {
    match classinfo {
        Type::ClassLiteral(class) => {
            if class.is_typed_dict(db) {
                report_runtime_check_against_typed_dict(context, call_expression, class, function);
            } else if let Some(protocol_class) = class.into_protocol_class(db) {
                if !protocol_class.is_runtime_checkable(db) {
                    report_runtime_check_against_non_runtime_checkable_protocol(
                        context,
                        call_expression,
                        protocol_class,
                        function,
                    );
                } else if function == KnownFunction::IsSubclass {
                    let non_method_members = protocol_class.interface(db).non_method_members(db);
                    if !non_method_members.is_empty() {
                        report_issubclass_check_against_protocol_with_non_method_members(
                            context,
                            call_expression,
                            protocol_class,
                            &non_method_members,
                        );
                    }
                }
            }
        }
        Type::SpecialForm(SpecialFormType::Any) if function == KnownFunction::IsInstance => {
            let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression) else {
                return;
            };
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "`typing.Any` cannot be used with `isinstance()`"
            ));
            diagnostic
                .set_primary_annotation_message("This call will raise `TypeError` at runtime");
        }
        Type::KnownInstance(KnownInstanceType::UnionType(_)) => {
            report_invalid_union_type_elements(
                db,
                context,
                call_expression,
                function,
                classinfo,
                classinfo_expr,
            );
        }
        Type::NominalInstance(nominal)
            if let Some(tuple_spec) = nominal.tuple_spec(db, context.program_environment()) =>
        {
            let element_exprs = match classinfo_expr {
                Some(ast::Expr::Tuple(tuple_expr)) => Some(&tuple_expr.elts),
                _ => None,
            };
            for (index, element) in tuple_spec.iter_element_types(db).enumerate() {
                let element_expr = element_exprs.and_then(|elts| elts.get(index));
                check_classinfo_in_isinstance(
                    db,
                    context,
                    call_expression,
                    function,
                    element,
                    element_expr,
                );
            }
        }

        _ => {}
    }
}

/// Return whether a fixed `isinstance` tuple covers every possible type of an input.
///
/// Each class in the tuple uses the same truthiness inference as a single-class `isinstance` check.
///
/// ```python
/// def f(x: A | B) -> bool:
///     if isinstance(x, (A, B)):
///         return True
/// ```
fn is_instance_tuple_exhaustive<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    classinfo: Type<'db>,
) -> bool {
    let Some(tuple) = classinfo.tuple_instance_spec(db, env) else {
        return false;
    };
    if tuple.is_variadic() {
        return false;
    }

    is_instance_tuple_covers(db, env, &tuple, ty, &ActiveRecursionDetector::default())
}

fn is_instance_tuple_covers<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tuple: &TupleSpec<'db>,
    ty: Type<'db>,
    recursion_guard: &ActiveRecursionDetector<Type<'db>>,
) -> bool {
    match ty {
        Type::TypeAlias(_) | Type::Recursive(_) => recursion_guard.visit(
            &ty,
            || true,
            || is_instance_tuple_covers(db, env, tuple, ty.resolve_type_alias(db), recursion_guard),
        ),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| is_instance_tuple_covers(db, env, tuple, *element, recursion_guard)),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| is_instance_tuple_covers(db, env, tuple, *element, recursion_guard)),
        Type::TypeVar(typevar) => match typevar.require_bound_or_constraints(db, env) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                is_instance_tuple_covers(db, env, tuple, bound, recursion_guard)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                constraints.elements(db).iter().all(|constraint| {
                    is_instance_tuple_covers(db, env, tuple, *constraint, recursion_guard)
                })
            }
        },
        ty => tuple.fixed_elements().any(|element| {
            let Type::ClassLiteral(class) = element else {
                return false;
            };
            is_instance_truthiness(db, env, ty, *class).is_always_true()
        }),
    }
}

/// Evaluate an `isinstance` call. Return `Truthiness::AlwaysTrue` if we can definitely infer that
/// this will return `True` at runtime, `Truthiness::AlwaysFalse` if we can definitely infer
/// that this will return `False` at runtime, or `Truthiness::Ambiguous` if we should infer `bool`
/// instead.
fn is_instance_truthiness<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    class: ClassLiteral<'db>,
) -> Truthiness {
    let is_instance = |ty: &Type<'_>| {
        ty.as_nominal_instance().is_some_and(|instance| {
            instance
                .class(db, env)
                .is_subtype_of_class_literal(db, class)
        })
    };

    let always_true_if = |test: bool| {
        if test {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        }
    };

    match ty {
        Type::Recursive(recursive) => recursive
            .unfold(db, env)
            .map(|unfolded| is_instance_truthiness(db, env, unfolded, class))
            .unwrap_or(Truthiness::Ambiguous),
        Type::RecursiveVar(_) => {
            unreachable!("semantic operation on an unbound recursive variable")
        }
        Type::Union(..) => {
            // We do not handle unions specifically here, because something like `A | SubclassOfA` would
            // have been simplified to `A` anyway
            Truthiness::Ambiguous
        }

        // Create a new intersection that maps type variables to their upper bounds,
        // and evaluate the truthiness of the `isinstance()` check with that type.
        // Along the way, short-circuit to `AlwaysTrue` if we find any positive element
        // that is always true.
        Type::Intersection(intersection) => {
            let mut effective = IntersectionBuilder::new(db, env);
            let mut found_tvars_or_newtypes = false;

            for &positive in intersection.positive(db) {
                if is_instance_truthiness(db, env, positive, class).is_always_true() {
                    return Truthiness::AlwaysTrue;
                } else if let Type::TypeVar(tvar) = positive {
                    match tvar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            effective.add_positive_in_place(bound);
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            effective.add_positive_in_place(constraints.as_type(db, env));
                        }
                    }
                    found_tvars_or_newtypes = true;
                } else if let Type::NewTypeInstance(newtype) = positive {
                    found_tvars_or_newtypes = true;
                    effective.add_positive_in_place(newtype.concrete_base_type(db));
                } else {
                    effective.add_positive_in_place(positive);
                }
            }

            if !found_tvars_or_newtypes {
                return Truthiness::Ambiguous;
            }

            for &negative in intersection.negative(db) {
                if is_instance_truthiness(db, env, negative, class).is_always_true() {
                    return Truthiness::AlwaysFalse;
                }
                effective.add_negative_in_place(negative);
            }

            let effective = effective.build();

            if effective == ty {
                Truthiness::Ambiguous
            } else {
                is_instance_truthiness(db, env, effective, class)
            }
        }

        Type::EnumComplement(complement) => {
            is_instance_truthiness(db, env, complement.to_intersection(db, env), class)
        }

        Type::NominalInstance(..) => always_true_if(is_instance(&ty)),

        Type::NewTypeInstance(newtype) => {
            always_true_if(is_instance(&newtype.concrete_base_type(db)))
        }

        Type::LiteralValue(..) | Type::ModuleLiteral(..) | Type::FunctionLiteral(..) => {
            always_true_if(
                ty.literal_fallback_instance(db, env)
                    .as_ref()
                    .is_some_and(is_instance),
            )
        }

        Type::ClassLiteral(..) => {
            always_true_if(is_instance(&KnownClass::Type.to_instance(db, env)))
        }

        Type::TypeAlias(alias) => is_instance_truthiness(db, env, alias.value_type(db), class),

        Type::TypeVar(bound_typevar) => match bound_typevar.require_bound_or_constraints(db, env) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                is_instance_truthiness(db, env, bound, class)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => always_true_if(
                constraints
                    .elements(db)
                    .iter()
                    .all(|c| is_instance_truthiness(db, env, *c, class).is_always_true()),
            ),
        },

        Type::BoundMethod(..)
        | Type::KnownBoundMethod(..)
        | Type::WrapperDescriptor(..)
        | Type::DataclassDecorator(..)
        | Type::DataclassTransformer(..)
        | Type::GenericAlias(..)
        | Type::SubclassOf(..)
        | Type::ProtocolInstance(..)
        | Type::SpecialForm(..)
        | Type::KnownInstance(..)
        | Type::PropertyInstance(..)
        | Type::SlotDescriptor(..)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::BoundSuper(..)
        | Type::TypeIs(..)
        | Type::TypeGuard(..)
        | Type::TypeForm(..)
        | Type::Callable(..)
        | Type::Dynamic(..)
        | Type::Divergent(_)
        | Type::Never
        | Type::TypedDict(_) => {
            // We could probably try to infer more precise types in some of these cases, but it's unclear
            // if it's worth the effort.
            Truthiness::Ambiguous
        }
    }
}

/// Report an error if a `types.UnionType` instance passed to `isinstance()`/`issubclass()`
/// contains elements that are not class objects.
fn report_invalid_union_type_elements<'db>(
    db: &'db dyn Db,
    context: &InferContext<'db, '_>,
    call_expression: &ast::ExprCall,
    function: KnownFunction,
    union_type: Type<'db>,
    union_type_expr: Option<&ast::Expr>,
) {
    fn find_invalid_elements<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: KnownFunction,
        ty: Type<'db>,
        invalid_elements: &mut Vec<Type<'db>>,
    ) {
        match ty {
            Type::ClassLiteral(_) => {}
            Type::NominalInstance(instance)
                if instance.has_known_class(db, KnownClass::NoneType) => {}
            Type::SpecialForm(special_form) if special_form.is_valid_isinstance_target() => {}
            // `Any` can be used in `issubclass()` calls but not `isinstance()` calls
            Type::SpecialForm(SpecialFormType::Any) if function == KnownFunction::IsSubclass => {}
            Type::KnownInstance(KnownInstanceType::UnionType(instance)) => {
                match instance.value_expression_types(db, env) {
                    Ok(value_expression_types) => {
                        for element in value_expression_types {
                            find_invalid_elements(db, env, function, element, invalid_elements);
                        }
                    }
                    Err(_) => {
                        invalid_elements.push(ty);
                    }
                }
            }
            _ => invalid_elements.push(ty),
        }
    }

    let mut invalid_elements = vec![];
    let env = context.program_environment();
    find_invalid_elements(db, env, function, union_type, &mut invalid_elements);

    let Some((first_invalid_element, other_invalid_elements)) = invalid_elements.split_first()
    else {
        return;
    };

    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call_expression) else {
        return;
    };

    let function_name: &str = function.into();

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid second argument to `{function_name}`"));
    diagnostic.info(format_args!(
        "A `UnionType` instance can only be used as the second argument to \
        `{function_name}` if all elements are class objects"
    ));
    if let Some(union_type_expr) = union_type_expr {
        diagnostic.annotate(
            Annotation::secondary(context.span(union_type_expr))
                .message("This `UnionType` instance contains non-class elements"),
        );
    }

    // When we have a secondary annotation pointing at the UnionType expression,
    // "the union" is unambiguous. Otherwise, spell out the union type in the message.
    let env = context.program_environment();
    let union_suffix = match (&union_type_expr, union_type) {
        (None, Type::KnownInstance(KnownInstanceType::UnionType(instance))) => {
            match instance.union_type(db) {
                Ok(ty) => format!(" `{}`", ty.display(db, env)),
                Err(_) => String::new(),
            }
        }
        _ => String::new(),
    };

    match other_invalid_elements {
        [] => diagnostic.info(format_args!(
            "Element `{}` in the union{union_suffix} is not a class object",
            first_invalid_element.display(db, env)
        )),
        [single] => diagnostic.info(format_args!(
            "Elements `{}` and `{}` in the union{union_suffix} are not class objects",
            first_invalid_element.display(db, env),
            single.display(db, env),
        )),
        _ => diagnostic.info(format_args!(
            "Element `{}` in the union{union_suffix}, and {} more elements, are not class objects",
            first_invalid_element.display(db, env),
            other_invalid_elements.len(),
        )),
    }
}
