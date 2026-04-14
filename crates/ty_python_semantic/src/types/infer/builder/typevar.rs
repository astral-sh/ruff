use crate::{
    Program,
    types::{
        BindingContext, KnownClass, KnownInstanceType, LintDiagnosticGuard, Truthiness, Type,
        TypeContext, TypeVarBoundOrConstraints, TypeVarKind, TypeVarVariance,
        context::InferContext,
        diagnostic::{
            INVALID_LEGACY_TYPE_VARIABLE, INVALID_PARAMSPEC, INVALID_TYPE_VARIABLE_BOUND,
            INVALID_TYPE_VARIABLE_CONSTRAINTS, INVALID_TYPE_VARIABLE_DEFAULT,
            report_mismatched_type_name,
        },
        infer::{
            InferenceFlags, TypeInferenceBuilder,
            builder::{BoundOrConstraintsNodes, DeclaredAndInferredType, DeferredExpressionState},
        },
        todo_type,
        typevar::{
            TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
            TypeVarIdentity, TypeVarInstance,
        },
        visitor::find_over_type,
    },
};
use ruff_db::{
    diagnostic::{Annotation, Span},
    parsed::parsed_module,
};
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::{definition::Definition, scope::NodeWithScopeKind};

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(super) fn infer_typevar_definition(
        &mut self,
        node: &ast::TypeParamTypeVar,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVar {
            range: _,
            node_index: _,
            name,
            bound,
            default,
        } = node;

        let db = self.db();

        let bound_or_constraint = match bound.as_deref() {
            Some(expr @ ast::Expr::Tuple(ast::ExprTuple { elts, .. })) => {
                if elts.len() < 2 {
                    if let Some(builder) = self
                        .context
                        .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, expr)
                    {
                        builder.into_diagnostic("TypeVar must have at least two constrained types");
                    }
                    None
                } else {
                    Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints)
                }
            }
            Some(_) => Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            None => None,
        };
        if bound_or_constraint.is_some() || default.is_some() {
            self.deferred.insert(definition);
        }
        let identity = TypeVarIdentity::new(db, &name.id, Some(definition), TypeVarKind::Pep695);
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db,
            identity,
            bound_or_constraint,
            None, // explicit_variance
            default.as_deref().map(|_| TypeVarDefaultEvaluation::Lazy),
        )));
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
    }

    pub(super) fn infer_typevar_deferred(&mut self, node: &'ast ast::TypeParamTypeVar) {
        let ast::TypeParamTypeVar {
            range: _,
            node_index: _,
            name,
            bound,
            default,
        } = node;

        let db = self.db();

        let previous_deferred_state =
            std::mem::replace(&mut self.deferred_state, DeferredExpressionState::Deferred);
        let bound_node = bound.as_deref();
        let bound_or_constraints = match bound_node {
            Some(expr @ ast::Expr::Tuple(ast::ExprTuple { elts, .. })) => {
                // Here, we interpret `bound` as a heterogeneous tuple and convert it to `TypeVarConstraints`
                // in `TypeVarInstance::lazy_constraints`.
                let constraint_tys: Box<[Type<'_>]> = elts
                    .iter()
                    .map(|expr| {
                        let constraint = self.infer_type_expression(expr);
                        if constraint.has_typevar_or_typevar_instance(db)
                            && let Some(builder) = self
                                .context
                                .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, expr)
                        {
                            builder.into_diagnostic("TypeVar constraint cannot be generic");
                        }
                        constraint
                    })
                    .collect();

                let tuple_ty = Type::heterogeneous_tuple(db, constraint_tys.clone());
                self.store_expression_type(expr, tuple_ty);
                // Mirror the `< 2` guard from `infer_typevar_definition` to avoid
                // a cascading `invalid-type-variable-default` diagnostic for tuples
                // that have already been flagged as invalid constraints.
                if elts.len() < 2 {
                    None
                } else {
                    Some(TypeVarBoundOrConstraints::Constraints(
                        TypeVarConstraints::new(db, constraint_tys),
                    ))
                }
            }
            Some(expr) => {
                let bound_ty = self.infer_type_expression(expr);
                if bound_ty.has_typevar_or_typevar_instance(db)
                    && let Some(builder) =
                        self.context.report_lint(&INVALID_TYPE_VARIABLE_BOUND, expr)
                {
                    builder.into_diagnostic("TypeVar upper bound cannot be generic");
                }

                Some(TypeVarBoundOrConstraints::UpperBound(bound_ty))
            }
            None => None,
        };
        if let Some(default_expr) = default.as_deref() {
            let default_ty = self.infer_type_expression(default_expr);
            if !self.check_default_for_outer_scope_typevars(default_ty, default_expr, &name.id) {
                let bound_node = bound_node.map(|n| match n {
                    ast::Expr::Tuple(tuple) => BoundOrConstraintsNodes::Constraints(&tuple.elts),
                    _ => BoundOrConstraintsNodes::Bound(n),
                });
                self.validate_typevar_default(
                    Some(&name.id),
                    bound_or_constraints,
                    default_ty,
                    default_expr,
                    bound_node,
                );
            }
        }
        self.deferred_state = previous_deferred_state;
    }

    /// Validate that a `TypeVar`'s default is compatible with its bound or constraints.
    pub(super) fn validate_typevar_default(
        &mut self,
        name: Option<&str>,
        bound_or_constraints: Option<TypeVarBoundOrConstraints<'db>>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_or_constraints_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) {
        let Some(bound_or_constraints) = bound_or_constraints else {
            return;
        };

        let db = self.db();

        // Normalize both typevar representations into a `TypeVarInstance` so they
        // follow the same compatibility rules:
        // - `Type::KnownInstance(TypeVar(..))` for legacy `typing.TypeVar(...)` values
        // - `Type::TypeVar(..)` for bound in-scope type parameters (for example, PEP 695)
        let default_typevar = match default_ty {
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => Some(typevar),
            Type::TypeVar(bound_typevar) => Some(bound_typevar.typevar(db)),
            _ => None,
        };

        let not_assignable_message =
            "TypeVar default is not assignable to the TypeVar's upper bound";

        let not_assignable_to_upper_bound = || {
            self.context
                .report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, default_node)
                .map(|builder| {
                    let mut diagnostic = builder.into_diagnostic(not_assignable_message);
                    if let Some(BoundOrConstraintsNodes::Bound(bound)) = bound_or_constraints_nodes
                    {
                        let secondary = self.context.secondary(bound);
                        let secondary = if let Some(name) = name {
                            secondary.message(format_args!("Upper bound of `{name}`"))
                        } else {
                            secondary.message("Upper bound of outer TypeVar")
                        };
                        diagnostic.annotate(secondary);
                    }
                    diagnostic
                })
        };

        let inconsistent_with_constraints = || {
            self.context
                .report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, default_node)
                .map(|builder| {
                    let mut diagnostic = builder.into_diagnostic(
                        "TypeVar default is inconsistent \
                                    with the TypeVar's constraints",
                    );
                    if let Some(BoundOrConstraintsNodes::Constraints([first, .., last])) =
                        bound_or_constraints_nodes
                    {
                        let secondary = self
                            .context
                            .secondary(TextRange::new(first.start(), last.end()));
                        let secondary = if let Some(name) = name {
                            secondary.message(format_args!("Constraints of `{name}`"))
                        } else {
                            secondary.message("Constraints of outer TypeVar")
                        };
                        diagnostic.annotate(secondary);
                    }
                    diagnostic
                })
        };

        if let Some(default_typevar) = default_typevar {
            let default_name = default_typevar.name(db);

            // Annotate the diagnostic with the definition span of the default TypeVar.
            let annotate_default_definition = |diagnostic: &mut LintDiagnosticGuard<'_, '_>| {
                if let Some(definition) = default_typevar.definition(db) {
                    let file = definition.file(db);
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(
                            definition.full_range(db, &parsed_module(db, file).load(db)),
                        ))
                        .message(format_args!("`{default_name}` defined here")),
                    );
                }
            };

            match bound_or_constraints {
                TypeVarBoundOrConstraints::UpperBound(outer_bound) => {
                    // Default TypeVar's upper bound must be assignable to outer's bound.
                    // If the default has constraints, all constraints must be assignable
                    // to the outer bound.
                    if let Some(default_constraints) = default_typevar.constraints(db) {
                        for constraint in default_constraints {
                            if !constraint.is_assignable_to(db, outer_bound) {
                                if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                                    annotate_default_definition(&mut diagnostic);
                                    if let Some(name) = name {
                                        diagnostic.set_primary_message(format_args!(
                                            "Constraint `{constraint}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of `{name}`",
                                            constraint = constraint.display(db),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of TypeVar `{name}` \
                                            is not assignable to upper bound `{bound}` \
                                            of `{name}` because constraint `{constraint}` \
                                            of `{default_name}` is not assignable to \
                                            `{bound}`",
                                            bound = outer_bound.display(db),
                                            constraint = constraint.display(db),
                                        ));
                                    } else {
                                        diagnostic.set_primary_message(format_args!(
                                            "Constraint `{constraint}` of `{default_name}` is \
                                            not assignable to upper bound `{bound}` of \
                                            outer TypeVar",
                                            constraint = constraint.display(db),
                                            bound = outer_bound.display(db),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default of TypeVar is not assignable its upper \
                                            bound `{bound}` because constraint `{constraint}` \
                                            of `{default_name}` is not assignable to `{bound}`",
                                            bound = outer_bound.display(db),
                                            constraint = constraint.display(db),
                                        ));
                                    }
                                }
                                break;
                            }
                        }
                    } else {
                        let default_bound =
                            default_typevar.upper_bound(db).unwrap_or_else(Type::object);
                        if !default_bound.is_assignable_to(db, outer_bound) {
                            if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                                annotate_default_definition(&mut diagnostic);
                                if let Some(name) = name {
                                    diagnostic.set_primary_message(format_args!(
                                        "Upper bound `{default_bound}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of `{name}`",
                                        default_bound = default_bound.display(db),
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Default `{default_name}` of TypeVar `{name}` \
                                            is not assignable to upper bound `{bound}` \
                                            of `{name}` because its upper bound \
                                            `{default_bound}` is not assignable to \
                                            `{bound}`",
                                        bound = outer_bound.display(db),
                                        default_bound = default_bound.display(db),
                                    ));
                                } else {
                                    diagnostic.set_primary_message(format_args!(
                                        "Upper bound `{default_bound}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of outer TypeVar",
                                        default_bound = default_bound.display(db),
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "TypeVar default `{default_name}` is not \
                                            assignable to upper bound `{bound}` \
                                            because upper bound of `{default_name}`
                                            (`{default_bound}`) is not assignable
                                            to `{bound}`",
                                        bound = outer_bound.display(db),
                                        default_bound = default_bound.display(db),
                                    ));
                                }
                            }
                        }
                    }
                }
                TypeVarBoundOrConstraints::Constraints(outer_constraints) => {
                    // TypeVar default with constrained outer.
                    let outer = outer_constraints.elements(db);
                    if let Some(default_constraints) = default_typevar.constraints(db) {
                        // Default has constraints: outer constraints must be a superset.
                        for default_constraint in default_constraints {
                            if !outer
                                .iter()
                                .any(|o| default_constraint.is_equivalent_to(db, *o))
                            {
                                if let Some(mut diagnostic) = inconsistent_with_constraints() {
                                    annotate_default_definition(&mut diagnostic);
                                    if let Some(name) = name {
                                        diagnostic.set_primary_message(format_args!(
                                            "Constraint `{constraint}` of default \
                                                `{default_name}` is not one of the constraints \
                                                of `{name}`",
                                            constraint = default_constraint.display(db),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of TypeVar `{name}` \
                                                is inconsistent with its constraints \
                                                `{name}` because constraint `{constraint}` of \
                                                `{default_name}` is not one of the constraints \
                                                of `{name}`",
                                            constraint = default_constraint.display(db),
                                        ));
                                    } else {
                                        diagnostic.set_primary_message(format_args!(
                                            "Constraint `{constraint}` of outer TypeVar default \
                                                `{default_name}` is not one of the constraints \
                                                of the outer TypeVar",
                                            constraint = default_constraint.display(db),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of outer TypeVar is \
                                            inconsistent with the constraints of the outer \
                                            TypeVar because constraint `{constraint}` of \
                                            default `{default_name}` is not one of the \
                                            constraints of the outer TypeVar",
                                            constraint = default_constraint.display(db),
                                        ));
                                    }
                                }
                                break;
                            }
                        }
                    } else {
                        // A non-constrained default TypeVar (bounded or unbounded) is
                        // incompatible with a constrained outer TypeVar per the typing spec.
                        if let Some(mut diagnostic) = inconsistent_with_constraints() {
                            annotate_default_definition(&mut diagnostic);
                            if let Some(default_bound) = default_typevar.upper_bound(db) {
                                diagnostic.set_primary_message(
                                    "Bounded TypeVar cannot be used as the default \
                                    for a constrained TypeVar",
                                );
                                diagnostic.info(format_args!(
                                    "`{default_name}` has bound `{default_bound}` but is not constrained",
                                    default_bound = default_bound.display(db),
                                ));
                            } else {
                                diagnostic.set_primary_message(
                                    "Unbounded TypeVar cannot be used as the default \
                                    for a constrained TypeVar",
                                );
                                diagnostic.info(format_args!(
                                    "`{default_name}` has no bound or constraints",
                                ));
                            }
                        }
                    }
                }
            }
            return;
        }

        // Concrete default type checks.
        match bound_or_constraints {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                if !default_ty.is_assignable_to(db, bound) {
                    if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                        if let Some(name) = name {
                            diagnostic.set_primary_message(format_args!("Default of `{name}`"));
                        } else {
                            diagnostic.set_primary_message("TypeVar default");
                        }
                        diagnostic.set_concise_message(not_assignable_message);
                    }
                }
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                if default_ty != Type::any()
                    && !constraints
                        .elements(db)
                        .iter()
                        .any(|c| default_ty.is_equivalent_to(db, *c))
                {
                    if let Some(mut diagnostic) = inconsistent_with_constraints() {
                        if let Some(name) = name {
                            diagnostic.set_primary_message(format_args!(
                                "`{default}` is not one of the constraints of `{name}`",
                                default = default_ty.display(db),
                            ));
                        } else {
                            diagnostic.set_primary_message(format_args!(
                                "`{default}` is not one of the constraints",
                                default = default_ty.display(db),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Check if a PEP 695 type parameter's default references type variables from an outer scope.
    ///
    /// Returns `true` if such a reference was found and a diagnostic was emitted,
    /// indicating that further default validation should be skipped.
    ///
    /// Note: this only handles PEP 695 type parameters in function and type alias scopes.
    /// Class type parameter scopes are skipped here because out-of-scope references
    /// are validated at the class level via `report_invalid_typevar_default_reference`.
    /// Legacy `TypeVar`s are validated by `check_legacy_typevar_defaults`.
    fn check_default_for_outer_scope_typevars(
        &self,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        typevar_name: &str,
    ) -> bool {
        let db = self.db();

        // Determine the expected binding context from the current type parameter scope.
        // Only check function and type alias scopes; class scopes are handled separately
        // when processing the class definition.
        let expected_binding_def = match self.scope().node(db) {
            NodeWithScopeKind::FunctionTypeParameters(function) => {
                self.index.expect_single_definition(function)
            }
            NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => {
                self.index.expect_single_definition(type_alias)
            }
            _ => return false,
        };
        let expected_binding = BindingContext::Definition(expected_binding_def);

        let outer_tv = find_over_type(db, default_ty, false, |ty| {
            if let Type::TypeVar(bound_tv) = ty
                && bound_tv.binding_context(db) != expected_binding
            {
                Some(bound_tv)
            } else {
                None
            }
        });

        let Some(outer_tv) = outer_tv else {
            return false;
        };
        let outer_typevar = outer_tv.typevar(db);
        let outer_name = outer_typevar.name(db);
        let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, default_node)
        else {
            return false;
        };
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Invalid default for type parameter `{typevar_name}`"
        ));
        diagnostic.set_primary_message(format_args!(
            "`{outer_name}` is a type parameter bound in an outer scope"
        ));
        diagnostic.set_concise_message(format_args!(
            "Type parameter `{typevar_name}` cannot use \
                outer-scope type parameter `{outer_name}` as its default"
        ));
        if let Some(definition) = outer_typevar.definition(db) {
            let file = definition.file(db);
            diagnostic.annotate(
                Annotation::secondary(Span::from(
                    definition.full_range(db, &parsed_module(db, file).load(db)),
                ))
                .message(format_args!("`{outer_name}` defined here")),
            );
        }
        diagnostic.info("See https://typing.python.org/en/latest/spec/generics.html#scoping-rules");

        true
    }

    pub(super) fn infer_paramspec_definition(
        &mut self,
        node: &ast::TypeParamParamSpec,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamParamSpec {
            range: _,
            node_index: _,
            name,
            default,
        } = node;

        let db = self.db();

        if default.is_some() {
            self.deferred.insert(definition);
        }
        let identity =
            TypeVarIdentity::new(db, &name.id, Some(definition), TypeVarKind::Pep695ParamSpec);
        let ty = Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db,
            identity,
            None, // ParamSpec, when declared using PEP 695 syntax, has no bounds or constraints
            None, // explicit_variance
            default.as_deref().map(|_| TypeVarDefaultEvaluation::Lazy),
        )));
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
    }

    pub(super) fn infer_paramspec_deferred(&mut self, node: &ast::TypeParamParamSpec) {
        let ast::TypeParamParamSpec {
            range: _,
            node_index: _,
            name,
            default: Some(default),
        } = node
        else {
            return;
        };
        let previous_deferred_state =
            std::mem::replace(&mut self.deferred_state, DeferredExpressionState::Deferred);
        self.infer_paramspec_default(default, Some(&name.id));
        self.deferred_state = previous_deferred_state;
    }

    pub(super) fn infer_paramspec_default(
        &mut self,
        default_expr: &ast::Expr,
        paramspec_name: Option<&str>,
    ) {
        let previously_allowed_paramspec = self
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true);
        self.infer_paramspec_default_impl(default_expr, paramspec_name);
        self.inference_flags.set(
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            previously_allowed_paramspec,
        );
    }

    fn infer_paramspec_default_impl(
        &mut self,
        default_expr: &ast::Expr,
        paramspec_name: Option<&str>,
    ) {
        let db = self.db();

        match default_expr {
            ast::Expr::EllipsisLiteral(ellipsis) => {
                let ty = self.infer_ellipsis_literal_expression(ellipsis);
                self.store_expression_type(default_expr, ty);
                return;
            }
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                let previously_allowed_paramspec = self
                    .inference_flags
                    .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false);
                let types = elts
                    .iter()
                    .map(|elt| self.infer_type_expression(elt))
                    .collect::<Vec<_>>();
                self.inference_flags.set(
                    InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
                    previously_allowed_paramspec,
                );
                // N.B. We cannot represent a heterogeneous list of types in our type system, so we
                // use a heterogeneous tuple type to represent the list of types instead.
                self.store_expression_type(default_expr, Type::heterogeneous_tuple(db, types));
                return;
            }
            ast::Expr::Name(_) => {
                let ty = self.infer_type_expression(default_expr);
                if let Some(name) = paramspec_name
                    && self.check_default_for_outer_scope_typevars(ty, default_expr, name)
                {
                    return;
                }
                let is_paramspec = match ty {
                    Type::TypeVar(typevar) => typevar.is_paramspec(db),
                    Type::KnownInstance(known_instance) => {
                        known_instance.class(db) == KnownClass::ParamSpec
                    }
                    _ => false,
                };
                if is_paramspec {
                    return;
                }
            }
            _ => {}
        }
        if let Some(builder) = self.context.report_lint(&INVALID_PARAMSPEC, default_expr) {
            builder.into_diagnostic(
                "The default value to `ParamSpec` must be either \
                    a list of types, `ParamSpec`, or `...`",
            );
        }
    }

    pub(super) fn infer_typevartuple_definition(
        &mut self,
        node: &ast::TypeParamTypeVarTuple,
        definition: Definition<'db>,
    ) {
        let ast::TypeParamTypeVarTuple {
            range: _,
            node_index: _,
            name: _,
            default,
        } = node;
        self.infer_optional_expression(default.as_deref(), TypeContext::default());
        let pep_695_todo = todo_type!("PEP-695 TypeVarTuple definition types");
        self.add_declaration_with_binding(
            node.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(pep_695_todo),
        );
    }

    pub(super) fn infer_legacy_paramspec(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
        known_class: KnownClass,
    ) -> Type<'db> {
        fn error<'db>(
            context: &InferContext<'db, '_>,
            message: impl std::fmt::Display,
            node: impl Ranged,
        ) -> Type<'db> {
            if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, node) {
                builder.into_diagnostic(message);
            }
            // If the call doesn't create a valid paramspec, we'll emit diagnostics and fall back to
            // just creating a regular instance of `typing.ParamSpec`.
            KnownClass::ParamSpec.to_instance(context.db())
        }

        let db = self.db();
        let arguments = &call_expr.arguments;
        let is_typing_extensions = known_class == KnownClass::ExtensionsParamSpec;
        let assume_all_features = self.in_stub() || is_typing_extensions;
        let python_version = Program::get(db).python_version(db);
        let have_features_from =
            |version: PythonVersion| assume_all_features || python_version >= version;

        let mut default = None;
        let mut name_param_ty = None;
        let mut name_param_node = None;

        if arguments.args.len() > 1 {
            return error(
                &self.context,
                "`ParamSpec` can only have one positional argument",
                call_expr,
            );
        }

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `ParamSpec` creation",
                starred,
            );
        }

        for kwarg in &arguments.keywords {
            let Some(identifier) = kwarg.arg.as_ref() else {
                return error(
                    &self.context,
                    "Starred arguments are not supported in `ParamSpec` creation",
                    kwarg,
                );
            };
            match identifier.id().as_str() {
                "name" => {
                    // Duplicate keyword argument is a syntax error, so we don't have to check if
                    // `name_param_ty.is_some()` here.
                    if !arguments.args.is_empty() {
                        return error(
                            &self.context,
                            "The `name` parameter of `ParamSpec` can only be provided once",
                            kwarg,
                        );
                    }
                    name_param_node = Some(&kwarg.value);
                    name_param_ty =
                        Some(self.infer_expression(&kwarg.value, TypeContext::default()));
                }
                "bound" | "covariant" | "contravariant" | "infer_variance" => {
                    return error(
                        &self.context,
                        "The variance and bound arguments for `ParamSpec` do not have defined semantics yet",
                        call_expr,
                    );
                }
                "default" => {
                    if !have_features_from(PythonVersion::PY313) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `default` parameter of `typing.ParamSpec` was added in Python 3.13",
                            kwarg,
                        );
                    }
                    default = Some(TypeVarDefaultEvaluation::Lazy);
                }
                name => {
                    // We don't return here; this error is informational since this will error
                    // at runtime, but it will likely cause fewer cascading errors if we just
                    // ignore the unknown keyword and still understand as much of the typevar as we
                    // can.
                    error(
                        &self.context,
                        format_args!("Unknown keyword argument `{name}` in `ParamSpec` creation"),
                        kwarg,
                    );
                    self.infer_expression(&kwarg.value, TypeContext::default());
                }
            }
        }

        let Some(name_param_ty) = name_param_ty.or_else(|| {
            arguments
                .find_positional(0)
                .map(|arg| self.infer_expression(arg, TypeContext::default()))
        }) else {
            return error(
                &self.context,
                "The `name` parameter of `ParamSpec` is required.",
                call_expr,
            );
        };

        let Some(name_param) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `ParamSpec` must be a string literal",
                call_expr,
            );
        };
        let name_param_node = name_param_node.or_else(|| arguments.find_positional(0));

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `ParamSpec` definition must be a simple variable assignment",
                target,
            );
        };

        if name_param != target_name {
            report_mismatched_type_name(
                &self.context,
                name_param_node
                    .map(Ranged::range)
                    .unwrap_or_else(|| call_expr.range()),
                "ParamSpec",
                target_name,
                Some(name_param),
                name_param_ty,
            );
        }

        if default.is_some() {
            self.deferred.insert(definition);
        }

        let identity = TypeVarIdentity::new(
            db,
            target_name.clone(),
            Some(definition),
            TypeVarKind::ParamSpec,
        );
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db, identity, None, None, default,
        )))
    }

    pub(super) fn infer_legacy_typevar(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
        known_class: KnownClass,
    ) -> Type<'db> {
        fn error<'db>(
            context: &InferContext<'db, '_>,
            message: impl std::fmt::Display,
            node: impl Ranged,
        ) -> Type<'db> {
            if let Some(builder) = context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, node) {
                builder.into_diagnostic(message);
            }
            // If the call doesn't create a valid typevar, we'll emit diagnostics and fall back to
            // just creating a regular instance of `typing.TypeVar`.
            KnownClass::TypeVar.to_instance(context.db())
        }

        let db = self.db();
        let arguments = &call_expr.arguments;
        let is_typing_extensions = known_class == KnownClass::ExtensionsTypeVar;
        let assume_all_features = self.in_stub() || is_typing_extensions;
        let python_version = Program::get(db).python_version(db);
        let have_features_from =
            |version: PythonVersion| assume_all_features || python_version >= version;

        let mut has_bound = false;
        let mut default = None;
        let mut covariant = false;
        let mut contravariant = false;
        let mut name_param_ty = None;
        let mut name_param_node = None;

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `TypeVar` creation",
                starred,
            );
        }

        for kwarg in &arguments.keywords {
            let Some(identifier) = kwarg.arg.as_ref() else {
                return error(
                    &self.context,
                    "Starred arguments are not supported in `TypeVar` creation",
                    kwarg,
                );
            };
            match identifier.id().as_str() {
                "name" => {
                    // Duplicate keyword argument is a syntax error, so we don't have to check if
                    // `name_param_ty.is_some()` here.
                    if !arguments.args.is_empty() {
                        return error(
                            &self.context,
                            "The `name` parameter of `TypeVar` can only be provided once.",
                            kwarg,
                        );
                    }
                    name_param_node = Some(&kwarg.value);
                    name_param_ty =
                        Some(self.infer_expression(&kwarg.value, TypeContext::default()));
                }
                "bound" => has_bound = true,
                "covariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                    {
                        Truthiness::AlwaysTrue => covariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `covariant` parameter of `TypeVar` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "contravariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                    {
                        Truthiness::AlwaysTrue => contravariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `contravariant` parameter of `TypeVar` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "default" => {
                    if !have_features_from(PythonVersion::PY313) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `default` parameter of `typing.TypeVar` was added in Python 3.13",
                            kwarg,
                        );
                    }

                    default = Some(TypeVarDefaultEvaluation::Lazy);
                }
                "infer_variance" => {
                    if !have_features_from(PythonVersion::PY312) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        error(
                            &self.context,
                            "The `infer_variance` parameter of `typing.TypeVar` was added in Python 3.12",
                            kwarg,
                        );
                    }
                    // TODO support `infer_variance` in legacy TypeVars
                    if self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db)
                        .is_ambiguous()
                    {
                        return error(
                            &self.context,
                            "The `infer_variance` parameter of `TypeVar` \
                            cannot have an ambiguous truthiness",
                            &kwarg.value,
                        );
                    }
                }
                name => {
                    // We don't return here; this error is informational since this will error
                    // at runtime, but it will likely cause fewer cascading errors if we just
                    // ignore the unknown keyword and still understand as much of the typevar as we
                    // can.
                    error(
                        &self.context,
                        format_args!("Unknown keyword argument `{name}` in `TypeVar` creation",),
                        kwarg,
                    );
                    self.infer_expression(&kwarg.value, TypeContext::default());
                }
            }
        }

        let variance = match (covariant, contravariant) {
            (true, true) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot be both covariant and contravariant",
                    call_expr,
                );
            }
            (true, false) => TypeVarVariance::Covariant,
            (false, true) => TypeVarVariance::Contravariant,
            (false, false) => TypeVarVariance::Invariant,
        };

        let Some(name_param_ty) = name_param_ty.or_else(|| {
            arguments
                .find_positional(0)
                .map(|arg| self.infer_expression(arg, TypeContext::default()))
        }) else {
            return error(
                &self.context,
                "The `name` parameter of `TypeVar` is required.",
                call_expr,
            );
        };

        let Some(name_param) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `TypeVar` must be a string literal.",
                call_expr,
            );
        };
        let name_param_node = name_param_node.or_else(|| arguments.find_positional(0));

        let ast::Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = target
        else {
            return error(
                &self.context,
                "A `TypeVar` definition must be a simple variable assignment",
                target,
            );
        };

        if name_param != target_name {
            report_mismatched_type_name(
                &self.context,
                name_param_node
                    .map(Ranged::range)
                    .unwrap_or_else(|| call_expr.range()),
                "TypeVar",
                target_name,
                Some(name_param),
                name_param_ty,
            );
        }

        // Inference of bounds, constraints, and defaults must be deferred, to avoid cycles. So we
        // only check presence/absence/number here.

        let num_constraints = arguments.args.len().saturating_sub(1);

        let bound_or_constraints = match (has_bound, num_constraints) {
            (false, 0) => None,
            (true, 0) => Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            (true, _) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot have both a bound and constraints",
                    call_expr,
                );
            }
            (_, 1) => {
                return error(
                    &self.context,
                    "A `TypeVar` cannot have exactly one constraint",
                    &arguments.args[1],
                );
            }
            (false, _) => Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
        };

        if bound_or_constraints.is_some() || default.is_some() {
            self.deferred.insert(definition);
        }

        let identity = TypeVarIdentity::new(
            db,
            target_name.clone(),
            Some(definition),
            TypeVarKind::Legacy,
        );
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db,
            identity,
            bound_or_constraints,
            Some(variance),
            default,
        )))
    }
}
