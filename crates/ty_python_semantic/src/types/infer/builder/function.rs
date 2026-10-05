use std::ops::ControlFlow;

use crate::{
    Db, ProgramEnvironment,
    reachability::ReachabilityConstraintsExtension,
    types::{
        DynamicType, KnownClass, KnownInstanceType, ParamSpecAttrKind, SubclassOfInner,
        Type, TypeContext, UnionType,
        constraints::ConstraintSetBuilder,
        diagnostic::{
            ABSTRACT_AND_FINAL_METHOD, FINAL_ON_NON_METHOD, INVALID_PARAMETER_DEFAULT,
            INVALID_PARAMSPEC, INVALID_TYPE_FORM, UNSOUND_RETURN_STATEMENT, USELESS_OVERLOAD_BODY,
            add_type_expression_reference_link, is_invalid_typed_dict_literal,
            report_implicit_return_type, report_invalid_generator_function_return_type,
            report_invalid_return_type,
            report_unsound_return_statement,
        },
        function::{
            FunctionBodyKind, FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction,
            function_body_kind, is_implicit_classmethod, same_module_uncached_raw_signature,
        },
        generics::shadowing::{
            OrdinaryFunctionShadowEffects, check_function_type_parameter_shadowing_sync,
        },
        infer::{
            FunctionDecoratorInference, InferenceFlags, TypeExpressionFlags, TypeInferenceBuilder,
            builder::{DeclaredAndInferredType, TypeAndRange},
            function_known_decorator_flags,
            infer_statement_types, nearest_enclosing_function,
        },
        relation::TypeRelation,
        signatures::effects::legacy_inline,
        signatures::{ReturnCallableTypeVarScope, function_signature_expression_type},
        tuple::{TupleSpecBuilder, TupleType},
        typed_dict::extract_unpacked_typed_dict_keys_from_kwargs_annotation,
        typevar::TypeVarSet,
    },
};
use ty_python_core::{
    UseDefMap,
    ast_node_ref::AstNodeRef,
    definition::Definition,
    scope::NodeWithScopeRef,
};

use ruff_python_ast as ast;
use ruff_text_size::Ranged;

pub(in crate::types::infer) mod annotations;
pub(in crate::types::infer) mod application;
pub(in crate::types::infer) mod decorators;
pub(in crate::types::infer) mod receiver;
pub(in crate::types::infer) mod source_effects;

use source_effects::{
    FunctionDecoratorRequest, FunctionDefinitionEffects, FunctionDefinitionWork,
    LegacyFunctionDefinitionEffects, OverloadIdentity,
};

pub(super) fn parameters_have_defaults(parameters: &ast::Parameters) -> bool {
    parameters
        .iter_non_variadic_params()
        .any(|param| param.default.is_some())
}

pub(super) fn function_has_deferred_annotations(function: &ast::StmtFunctionDef) -> bool {
    function.type_params.is_none()
        && (function.returns.is_some()
            || function
                .parameters
                .iter()
                .any(|param| param.annotation().is_some()))
}

/// Whether a non-static method receives an instance or the class itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types::infer) enum MethodReceiverKind {
    Instance,
    Class,
}

impl MethodReceiverKind {
    pub(in crate::types::infer) fn from_decorators(
        function: &ast::StmtFunctionDef,
        decorators: FunctionDecorators,
    ) -> Option<Self> {
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
        match super::source_function_body::infer_function_body_sync(
            self,
            function,
            super::source_function_body::FunctionBodyFacts,
            &super::source_function_body::OrdinaryFunctionBodyEffects,
        ) {
            Ok(()) => {}
            Err(error) => match error {},
        }
    }

    pub(super) fn infer_function_return_types(&mut self, function: &ast::StmtFunctionDef) {
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
        legacy_inline(self.infer_function_definition_with(
            &LegacyFunctionDefinitionEffects,
            function,
            definition,
        ));
    }

    pub(in crate::types::infer) async fn infer_function_definition_with<
        E: FunctionDefinitionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        let previous_flags = self.context.inference_flags;
        let result = self
            .infer_function_definition_inner_with(effects, function, definition)
            .await;
        if result.is_err() {
            self.context.inference_flags = previous_flags;
        }
        result
    }

    async fn infer_function_definition_inner_with<E: FunctionDefinitionEffects<'db>>(
        &mut self,
        effects: &E,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
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

        effects
            .checkpoint(FunctionDefinitionWork::Definition)
            .await?;
        let decorator_inference = if decorator_list.is_empty() {
            None
        } else {
            Some(effects.known_decorators(self, definition).await?)
        };
        if let Some(decorator_inference) = decorator_inference.as_ref() {
            effects
                .merge_decorator_results(self, decorator_inference)
                .await?;
        }

        let mut decorator_types_and_nodes =
            effects.decorator_candidates(decorator_list.len()).await?;
        let mut has_transforming_decorators = false;
        let mut function_decorators = FunctionDecorators::empty();
        let mut dataclass_transformer_params = None;
        let mut final_decorator = None;

        for decorator in decorator_list {
            effects
                .checkpoint(FunctionDefinitionWork::ClassifyDecorator)
                .await?;
            let decorator_type = effects
                .decorator_type(decorator_inference, decorator)
                .await?;
            let decorator_function_decorator =
                effects.classify_decorator(self, decorator_type).await?;
            function_decorators |= decorator_function_decorator;

            if decorator_function_decorator.contains(FunctionDecorators::NO_TYPE_CHECK) {
                // If the function is decorated with the `no_type_check` decorator,
                // we need to suppress any errors that come after the decorators.
                self.context.inference_flags |= InferenceFlags::IN_NO_TYPE_CHECK;
                continue;
            }
            if decorator_function_decorator.contains(FunctionDecorators::FINAL) {
                final_decorator = Some(decorator);
                continue;
            }
            if let Type::DataclassTransformer(params) = decorator_type {
                dataclass_transformer_params = Some(params);
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
            effects
                .checkpoint(FunctionDefinitionWork::DecoratorStorage(
                    decorator_types_and_nodes.len(),
                ))
                .await?;
            decorator_types_and_nodes.clear();
        }

        effects
            .checkpoint(FunctionDefinitionWork::DecoratorsClassified {
                definition,
                decorators: function_decorators,
                inference_flags: self.context.inference_flags,
                has_transforming_decorators,
                candidates: &decorator_types_and_nodes,
            })
            .await?;
        effects
            .check_final_decorators(self, function, final_decorator, function_decorators)
            .await?;

        // If there are type params, parameters and returns are evaluated in that scope. Otherwise,
        // we defer the inference of any parameter and return annotations. That ensures that we do
        // not add any spurious salsa cycles when applying decorators below. (Applying a decorator
        // requires getting the signature of this function definition, which in turn requires
        // (lazily) inferring the parameter and return types.) If defaults exist, we also defer so
        // they can be inferred once with type context in the enclosing scope.
        effects
            .checkpoint(FunctionDefinitionWork::Parameters(parameters.len()))
            .await?;
        if function_has_deferred_annotations(function) || parameters_have_defaults(parameters) {
            effects.record_deferred(self, definition).await?;
        }

        let known_function = effects.known_function(db, definition, name).await?;

        // `type_check_only` is itself not available at runtime
        if known_function == Some(KnownFunction::TypeCheckOnly) {
            function_decorators |= FunctionDecorators::TYPE_CHECK_ONLY;
        }

        let body_scope = effects
            .function_body_scope(
                db,
                self.program_file(),
                self.index.node_scope(NodeWithScopeRef::Function(function)),
            )
            .await?;

        let overload_literal = effects
            .overload_literal(
                db,
                OverloadIdentity {
                    name: &name.id,
                    known: known_function,
                    body_scope,
                    decorators: function_decorators,
                    dataclass_transformer: dataclass_transformer_params,
                    has_return_annotation: function.returns.is_some(),
                },
            )
            .await?;
        let function_literal = effects.function_literal(db, overload_literal).await?;
        let function_type = effects.function_type(db, function_literal).await?;
        let is_decorated_overload_implementation = has_transforming_decorators
            && effects
                .has_separate_implementation(db, function_literal)
                .await?;
        let is_decorated_overload =
            has_transforming_decorators && effects.is_overload(db, overload_literal).await?;

        let mut inferred_ty = Type::FunctionLiteral(
            if is_decorated_overload_implementation || is_decorated_overload {
                effects
                    .function_type(db, function_literal.without_overloads())
                    .await?
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
            inferred_ty = effects.underlying_function(db, inferred_ty).await?;
        }
        if !decorator_list.is_empty() {
            self.undecorated_type = Some(inferred_ty);
        }

        if function.type_params.is_some() {
            effects
                .check_type_parameter_shadowing(self, function)
                .await?;
        }

        for (decorator_ty, decorator_node) in decorator_types_and_nodes.iter().rev() {
            effects
                .checkpoint(FunctionDefinitionWork::ApplyDecorator)
                .await?;
            inferred_ty = effects
                .apply_function_decorator(
                    self,
                    FunctionDecoratorRequest {
                        function,
                        definition,
                        overload_literal,
                        decorator_ty: *decorator_ty,
                        decorator_node,
                        inferred_ty,
                        is_decorated_overload_implementation,
                    },
                )
                .await?;
        }

        if is_decorated_overload_implementation || is_decorated_overload {
            inferred_ty = effects
                .finish_decorated_overload(
                    self,
                    function_literal,
                    function_type,
                    inferred_ty,
                    is_decorated_overload_implementation,
                    is_decorated_overload,
                )
                .await?;
        }

        effects
            .bind_function(self, function, definition, inferred_ty)
            .await?;

        if function_decorators.contains(FunctionDecorators::OVERLOAD) {
            for stmt in &function.body {
                effects
                    .checkpoint(FunctionDefinitionWork::OverloadStatement {
                        definition,
                        statement: stmt,
                    })
                    .await?;
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
                if effects
                    .report_useless_overload_body(self, function, stmt)
                    .await?
                    .is_break()
                {
                    break;
                }
            }
        }
        effects
            .checkpoint(FunctionDefinitionWork::DecoratorStorage(
                decorator_types_and_nodes.len(),
            ))
            .await?;
        Ok(())
    }

    fn report_useless_overload_body(
        &mut self,
        function: &ast::StmtFunctionDef,
        statement: &ast::Stmt,
    ) -> ControlFlow<()> {
        let Some(builder) = self.context.report_lint(&USELESS_OVERLOAD_BODY, statement) else {
            return ControlFlow::Continue(());
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
        ControlFlow::Break(())
    }

    pub(in crate::types::infer) fn extend_function_decorator_inference(
        &mut self,
        inference: &FunctionDecoratorInference<'db>,
    ) {
        self.context.extend(inference.diagnostics());
        self.expressions.extend(inference.expression_types());
        self.bindings.extend(inference.bindings());
        self.called_functions
            .extend(inference.called_functions().iter().copied());
        self.implicit_aliases
            .extend(inference.implicit_aliases().iter().copied());
    }

    fn check_function_final_decorators(
        &mut self,
        function: &ast::StmtFunctionDef,
        final_decorator: Option<&ast::Decorator>,
        function_decorators: FunctionDecorators,
    ) {
        let db = self.db();
        let name = &function.name;

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
    }

    fn check_function_type_parameter_shadowing(&mut self, function: &ast::StmtFunctionDef) {
        let db = self.db();
        // Check that the function's own type parameters don't shadow
        // type variables from enclosing scopes (by name).
        match check_function_type_parameter_shadowing_sync(
            self.index,
            self.scope().file_scope_id(db),
            function,
            &OrdinaryFunctionShadowEffects { context: &self.context },
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    fn apply_function_definition_decorator(
        &mut self,
        request: FunctionDecoratorRequest<'_, 'db>,
    ) -> Type<'db> {
        let effects = application::OrdinaryDecoratorApplicationEffects {
            db: self.db(),
            env: self.program_environment(),
        };
        let Ok(result) = application::apply_function_decorator_sync(
            self,
            request,
            application::DecoratorApplicationFacts,
            &effects,
        );
        result
    }

    fn finish_function_definition_overload(
        &mut self,
        function_literal: FunctionLiteral<'db>,
        function_type: FunctionType<'db>,
        mut inferred_ty: Type<'db>,
        is_decorated_overload_implementation: bool,
        is_decorated_overload: bool,
    ) -> Type<'db> {
        let db = self.db();
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

        inferred_ty
    }

    pub(super) fn infer_function_annotations(
        &mut self,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) {
        match annotations::function_annotations_sync(
            self,
            definition,
            function,
            annotations::AnnotationFacts,
            &annotations::OrdinaryAnnotationEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
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

    /// Infers the complete function signature's annotations in its PEP 695 type-parameter scope.
    pub(super) fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let Ok(()) = annotations::function_type_parameters_sync(
            self,
            function,
            annotations::AnnotationFacts,
            &annotations::OrdinaryAnnotationEffects,
        );
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
        match annotations::signature_annotations_sync(
            self,
            definition,
            function,
            annotations::AnnotationFacts,
            &annotations::OrdinaryAnnotationEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
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
        match annotations::receiver_annotation_sync(
            self,
            definition,
            function,
            annotations::AnnotationFacts,
            &annotations::OrdinaryAnnotationEffects,
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    pub(super) fn validate_unpacked_typed_dict_kwargs(&mut self, parameters: &ast::Parameters) {
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
        match super::source_parameter::infer_parameter_definition_sync(
            self,
            parameter_with_default,
            definition,
            super::source_parameter::ParameterFacts,
            &super::source_parameter::OrdinaryParameterEffects,
        ) {
            Ok(()) => {}
            Err(error) => match error {},
        }
    }

    pub(super) fn infer_annotated_parameter_definition(
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
    pub(super) fn infer_method_receiver_parameter_type(
        &mut self,
        parameter: &ast::Parameter,
        function: &AstNodeRef<ast::StmtFunctionDef>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> Option<Type<'db>> {
        legacy_inline(receiver::infer_method_receiver_with(
            self,
            parameter,
            function,
            class,
            &receiver::OrdinaryMethodReceiverEffects,
        ))
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
