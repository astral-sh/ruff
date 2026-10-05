pub(super) mod legacy;
pub(in crate::types::infer) mod pep695;

use crate::types::{
    BindingContext, KnownClass, KnownInstanceType, LintDiagnosticGuard, Truthiness, Type,
    TypeContext, TypeVarBoundOrConstraints, TypeVarKind, TypeVarVariance,
    context::InferContext,
    diagnostic::{
        INVALID_LEGACY_TYPE_VARIABLE, INVALID_PARAMSPEC, INVALID_TYPE_VARIABLE_DEFAULT,
        report_mismatched_type_name,
    },
    infer::{
        TypeInferenceBuilder,
        builder::BoundOrConstraintsNodes,
    },
    typevar::{TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance},
    visitor::find_over_type,
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
        pep695::infer_type_parameter_definition(
            self,
            definition,
            ast::TypeParamRef::from(node),
        );
    }

    pub(super) fn infer_typevar_deferred(&mut self, node: &'ast ast::TypeParamTypeVar) {
        match super::deferred::type_parameter::infer_type_parameter_deferred_sync(
            self,
            ast::TypeParamRef::TypeVar(node),
            super::deferred::type_parameter::DeferredTypeParameterFacts,
            &super::deferred::type_parameter::OrdinaryDeferredTypeParameterEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
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
        let Ok(()) = super::deferred::assignment::validate_typevar_default_sync(
            self,
            name,
            bound_or_constraints,
            default_ty,
            default_node,
            bound_or_constraints_nodes,
            &super::deferred::assignment::OrdinaryDeferredAssignmentEffects,
        );
    }

    pub(super) fn validate_bound_typevar_default(
        &mut self,
        name: Option<&str>,
        bound_or_constraints: TypeVarBoundOrConstraints<'db>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_or_constraints_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) {
        let env = self.program_environment();
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
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(definition.full_range(
                            db,
                            &parsed_module(db, definition.python_file(db)).load(db),
                        )))
                        .message(format_args!("`{default_name}` defined here")),
                    );
                }
            };

            match bound_or_constraints {
                TypeVarBoundOrConstraints::UpperBound(outer_bound) => {
                    // Default TypeVar's upper bound must be assignable to outer's bound.
                    // If the default has constraints, all constraints must be assignable
                    // to the outer bound.
                    if let Some(default_constraints) = default_typevar.constraints(db, env) {
                        for constraint in default_constraints {
                            if !constraint.is_assignable_to(db, env, outer_bound) {
                                if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                                    annotate_default_definition(&mut diagnostic);
                                    if let Some(name) = name {
                                        diagnostic.set_primary_annotation_message(format_args!(
                                            "Constraint `{constraint}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of `{name}`",
                                            constraint = constraint.display(db, env),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of TypeVar `{name}` \
                                            is not assignable to upper bound `{bound}` \
                                            of `{name}` because constraint `{constraint}` \
                                            of `{default_name}` is not assignable to \
                                            `{bound}`",
                                            bound = outer_bound.display(db, env),
                                            constraint = constraint.display(db, env),
                                        ));
                                    } else {
                                        diagnostic.set_primary_annotation_message(format_args!(
                                            "Constraint `{constraint}` of `{default_name}` is \
                                            not assignable to upper bound `{bound}` of \
                                            outer TypeVar",
                                            constraint = constraint.display(db, env),
                                            bound = outer_bound.display(db, env),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default of TypeVar is not assignable its upper \
                                            bound `{bound}` because constraint `{constraint}` \
                                            of `{default_name}` is not assignable to `{bound}`",
                                            bound = outer_bound.display(db, env),
                                            constraint = constraint.display(db, env),
                                        ));
                                    }
                                }
                                break;
                            }
                        }
                    } else {
                        let default_bound = default_typevar
                            .upper_bound(db, env)
                            .unwrap_or_else(Type::object);
                        if !default_bound.is_assignable_to(db, env, outer_bound) {
                            if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                                annotate_default_definition(&mut diagnostic);
                                if let Some(name) = name {
                                    diagnostic.set_primary_annotation_message(format_args!(
                                        "Upper bound `{default_bound}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of `{name}`",
                                        default_bound = default_bound.display(db, env),
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Default `{default_name}` of TypeVar `{name}` \
                                            is not assignable to upper bound `{bound}` \
                                            of `{name}` because its upper bound \
                                            `{default_bound}` is not assignable to \
                                            `{bound}`",
                                        bound = outer_bound.display(db, env),
                                        default_bound = default_bound.display(db, env),
                                    ));
                                } else {
                                    diagnostic.set_primary_annotation_message(format_args!(
                                        "Upper bound `{default_bound}` of default \
                                            `{default_name}` is not assignable to upper \
                                            bound of outer TypeVar",
                                        default_bound = default_bound.display(db, env),
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "TypeVar default `{default_name}` is not \
                                            assignable to upper bound `{bound}` \
                                            because upper bound of `{default_name}`
                                            (`{default_bound}`) is not assignable
                                            to `{bound}`",
                                        bound = outer_bound.display(db, env),
                                        default_bound = default_bound.display(db, env),
                                    ));
                                }
                            }
                        }
                    }
                }
                TypeVarBoundOrConstraints::Constraints(outer_constraints) => {
                    // TypeVar default with constrained outer.
                    let outer = outer_constraints.elements(db);
                    if let Some(default_constraints) = default_typevar.constraints(db, env) {
                        // Default has constraints: outer constraints must be a superset.
                        for default_constraint in default_constraints {
                            if !outer
                                .iter()
                                .any(|o| default_constraint.is_equivalent_to(db, env, *o))
                            {
                                if let Some(mut diagnostic) = inconsistent_with_constraints() {
                                    annotate_default_definition(&mut diagnostic);
                                    if let Some(name) = name {
                                        diagnostic.set_primary_annotation_message(format_args!(
                                            "Constraint `{constraint}` of default \
                                                `{default_name}` is not one of the constraints \
                                                of `{name}`",
                                            constraint = default_constraint.display(db, env),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of TypeVar `{name}` \
                                                is inconsistent with its constraints \
                                                `{name}` because constraint `{constraint}` of \
                                                `{default_name}` is not one of the constraints \
                                                of `{name}`",
                                            constraint = default_constraint.display(db, env),
                                        ));
                                    } else {
                                        diagnostic.set_primary_annotation_message(format_args!(
                                            "Constraint `{constraint}` of outer TypeVar default \
                                                `{default_name}` is not one of the constraints \
                                                of the outer TypeVar",
                                            constraint = default_constraint.display(db, env),
                                        ));
                                        diagnostic.set_concise_message(format_args!(
                                            "Default `{default_name}` of outer TypeVar is \
                                            inconsistent with the constraints of the outer \
                                            TypeVar because constraint `{constraint}` of \
                                            default `{default_name}` is not one of the \
                                            constraints of the outer TypeVar",
                                            constraint = default_constraint.display(db, env),
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
                            if let Some(default_bound) = default_typevar.upper_bound(db, env) {
                                diagnostic.set_primary_annotation_message(
                                    "Bounded TypeVar cannot be used as the default \
                                    for a constrained TypeVar",
                                );
                                diagnostic.info(format_args!(
                                    "`{default_name}` has bound `{default_bound}` but is not constrained",
                                    default_bound = default_bound.display(db, env),
                                ));
                            } else {
                                diagnostic.set_primary_annotation_message(
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
                if !default_ty.is_assignable_to(db, env, bound) {
                    if let Some(mut diagnostic) = not_assignable_to_upper_bound() {
                        if let Some(name) = name {
                            diagnostic.set_primary_annotation_message(format_args!(
                                "Default of `{name}`"
                            ));
                        } else {
                            diagnostic.set_primary_annotation_message("TypeVar default");
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
                        .any(|c| default_ty.is_equivalent_to(db, env, *c))
                {
                    if let Some(mut diagnostic) = inconsistent_with_constraints() {
                        if let Some(name) = name {
                            diagnostic.set_primary_annotation_message(format_args!(
                                "`{default}` is not one of the constraints of `{name}`",
                                default = default_ty.display(db, env),
                            ));
                        } else {
                            diagnostic.set_primary_annotation_message(format_args!(
                                "`{default}` is not one of the constraints",
                                default = default_ty.display(db, env),
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
    pub(super) fn check_default_for_outer_scope_typevars(
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

        let outer_tv = find_over_type(db, self.program_environment(), default_ty, false, |ty| {
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
        diagnostic.set_primary_annotation_message(format_args!(
            "`{outer_name}` is a type parameter bound in an outer scope"
        ));
        diagnostic.set_concise_message(format_args!(
            "Type parameter `{typevar_name}` cannot use \
                outer-scope type parameter `{outer_name}` as its default"
        ));
        if let Some(definition) = outer_typevar.definition(db) {
            diagnostic.annotate(
                Annotation::secondary(Span::from(
                    definition
                        .full_range(db, &parsed_module(db, definition.python_file(db)).load(db)),
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
        pep695::infer_type_parameter_definition(
            self,
            definition,
            ast::TypeParamRef::from(node),
        );
    }

    pub(super) fn infer_paramspec_deferred(&mut self, node: &'ast ast::TypeParamParamSpec) {
        match super::deferred::type_parameter::infer_type_parameter_deferred_sync(
            self,
            ast::TypeParamRef::ParamSpec(node),
            super::deferred::type_parameter::DeferredTypeParameterFacts,
            &super::deferred::type_parameter::OrdinaryDeferredTypeParameterEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    pub(super) fn infer_paramspec_default(
        &mut self,
        default_expr: &ast::Expr,
        paramspec_name: Option<&str>,
    ) {
        match super::deferred::type_parameter::infer_paramspec_default_sync(
            self,
            default_expr,
            paramspec_name,
            &super::deferred::type_parameter::OrdinaryDeferredTypeParameterEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }



    pub(super) fn infer_typevartuple_definition(
        &mut self,
        node: &ast::TypeParamTypeVarTuple,
        definition: Definition<'db>,
    ) {
        pep695::infer_type_parameter_definition(
            self,
            definition,
            ast::TypeParamRef::from(node),
        );
    }

    pub(super) fn infer_typevartuple_deferred(&mut self, node: &'ast ast::TypeParamTypeVarTuple) {
        match super::deferred::type_parameter::infer_type_parameter_deferred_sync(
            self,
            ast::TypeParamRef::TypeVarTuple(node),
            super::deferred::type_parameter::DeferredTypeParameterFacts,
            &super::deferred::type_parameter::OrdinaryDeferredTypeParameterEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    pub(super) fn infer_typevartuple_default(
        &mut self,
        default_expr: &ast::Expr,
        typevartuple_name: Option<&str>,
    ) {
        match super::deferred::type_parameter::infer_typevartuple_default_sync(
            self,
            default_expr,
            typevartuple_name,
            &super::deferred::type_parameter::OrdinaryDeferredTypeParameterEffects,
        ) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    pub(super) fn infer_legacy_typevartuple(
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
            let db = context.db();
            if let Some(builder) = context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, node) {
                builder.into_diagnostic(message);
            }
            KnownClass::TypeVarTuple.to_instance(db, context.program_environment())
        }

        let env = self.program_environment();
        let db = self.db();
        let arguments = &call_expr.arguments;
        let is_typing_extensions = known_class == KnownClass::ExtensionsTypeVarTuple;
        let assume_all_features = self.in_stub() || is_typing_extensions;

        let mut default = None;
        let mut covariant = false;
        let mut contravariant = false;
        let mut infer_variance = false;
        let mut name_param_ty = None;
        let mut name_param_node = None;

        if arguments.args.len() > 1 {
            return error(
                &self.context,
                "`TypeVarTuple` can only have one positional argument",
                call_expr,
            );
        }

        if let Some(starred) = arguments.args.iter().find(|arg| arg.is_starred_expr()) {
            return error(
                &self.context,
                "Starred arguments are not supported in `TypeVarTuple` creation",
                starred,
            );
        }

        for kwarg in &arguments.keywords {
            let Some(identifier) = kwarg.arg.as_ref() else {
                return error(
                    &self.context,
                    "Starred arguments are not supported in `TypeVarTuple` creation",
                    kwarg,
                );
            };
            match identifier.id().as_str() {
                "name" => {
                    if !arguments.args.is_empty() {
                        return error(
                            &self.context,
                            "The `name` parameter of `TypeVarTuple` can only be provided once",
                            kwarg,
                        );
                    }
                    name_param_node = Some(&kwarg.value);
                    name_param_ty =
                        Some(self.infer_expression(&kwarg.value, TypeContext::default()));
                }
                "default" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY313
                    {
                        error(
                            &self.context,
                            "The `default` parameter of `typing.TypeVarTuple` was added in Python 3.13",
                            kwarg,
                        );
                    }
                    default = Some(TypeVarDefaultEvaluation::Lazy);
                }
                "bound" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY315
                    {
                        return error(
                            &self.context,
                            "The `bound` parameter of `typing.TypeVarTuple` was added in Python 3.15",
                            kwarg,
                        );
                    }
                    return error(
                        &self.context,
                        "The `bound` argument for `TypeVarTuple` is not supported",
                        call_expr,
                    );
                }
                "covariant" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY315
                    {
                        error(
                            &self.context,
                            "The `covariant` parameter of `typing.TypeVarTuple` was added in Python 3.15",
                            kwarg,
                        );
                    }
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => covariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `covariant` parameter of `TypeVarTuple` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "contravariant" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY315
                    {
                        error(
                            &self.context,
                            "The `contravariant` parameter of `typing.TypeVarTuple` was added in Python 3.15",
                            kwarg,
                        );
                    }
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => contravariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `contravariant` parameter of `TypeVarTuple` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "infer_variance" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY315
                    {
                        error(
                            &self.context,
                            "The `infer_variance` parameter of `typing.TypeVarTuple` was added in Python 3.15",
                            kwarg,
                        );
                    }
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => infer_variance = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `infer_variance` parameter of `TypeVarTuple` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                name => {
                    error(
                        &self.context,
                        format_args!(
                            "Unknown keyword argument `{name}` in `TypeVarTuple` creation"
                        ),
                        kwarg,
                    );
                    self.infer_expression(&kwarg.value, TypeContext::default());
                }
            }
        }

        let variance = match (covariant, contravariant, infer_variance) {
            (true, true, _) => {
                return error(
                    &self.context,
                    "A `TypeVarTuple` cannot be both covariant and contravariant",
                    call_expr,
                );
            }
            (true, false, true) | (false, true, true) => {
                return error(
                    &self.context,
                    "A `TypeVarTuple` cannot specify variance when `infer_variance=True`",
                    call_expr,
                );
            }
            (true, false, false) => Some(TypeVarVariance::Covariant),
            (false, true, false) => Some(TypeVarVariance::Contravariant),
            (false, false, false) => Some(TypeVarVariance::Invariant),
            (false, false, true) => None,
        };

        let Some(name_param_ty) = name_param_ty.or_else(|| {
            arguments
                .find_positional(0)
                .map(|arg| self.infer_expression(arg, TypeContext::default()))
        }) else {
            return error(
                &self.context,
                "The `name` parameter of `TypeVarTuple` is required.",
                call_expr,
            );
        };

        let Some(name_param) = name_param_ty.as_string_literal().map(|name| name.value(db)) else {
            return error(
                &self.context,
                "The first argument to `TypeVarTuple` must be a string literal",
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
                "A `TypeVarTuple` definition must be a simple variable assignment",
                target,
            );
        };

        if name_param != target_name {
            report_mismatched_type_name(
                &self.context,
                name_param_node
                    .map(Ranged::range)
                    .unwrap_or_else(|| call_expr.range()),
                "TypeVarTuple",
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
            TypeVarKind::LegacyTypeVarTuple,
        );
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db, identity, None, variance, default,
        )))
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
            let db = context.db();
            if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, node) {
                builder.into_diagnostic(message);
            }
            // If the call doesn't create a valid paramspec, we'll emit diagnostics and fall back to
            // just creating a regular instance of `typing.ParamSpec`.
            KnownClass::ParamSpec.to_instance(db, context.program_environment())
        }

        let env = self.program_environment();
        let db = self.db();
        let arguments = &call_expr.arguments;
        let is_typing_extensions = known_class == KnownClass::ExtensionsParamSpec;
        let assume_all_features = self.in_stub() || is_typing_extensions;

        let mut default = None;
        let mut covariant = false;
        let mut contravariant = false;
        let mut infer_variance = false;
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
                "bound" => {
                    return error(
                        &self.context,
                        "The `bound` argument for `ParamSpec` is not supported",
                        call_expr,
                    );
                }
                "infer_variance" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY312
                    {
                        error(
                            &self.context,
                            "The `infer_variance` parameter of `typing.ParamSpec` was added in Python 3.12",
                            kwarg,
                        );
                    }
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => infer_variance = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `infer_variance` parameter of `ParamSpec` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "covariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => covariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `covariant` parameter of `ParamSpec` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "contravariant" => {
                    match self
                        .infer_expression(&kwarg.value, TypeContext::default())
                        .bool(db, env)
                    {
                        Truthiness::AlwaysTrue => contravariant = true,
                        Truthiness::AlwaysFalse => {}
                        Truthiness::Ambiguous => {
                            return error(
                                &self.context,
                                "The `contravariant` parameter of `ParamSpec` \
                                cannot have an ambiguous truthiness",
                                &kwarg.value,
                            );
                        }
                    }
                }
                "default" => {
                    if !assume_all_features
                        && self.program_environment().python_version(db) < PythonVersion::PY313
                    {
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

        let variance = match (covariant, contravariant, infer_variance) {
            (true, true, _) => {
                return error(
                    &self.context,
                    "A `ParamSpec` cannot be both covariant and contravariant",
                    call_expr,
                );
            }
            (true, false, true) | (false, true, true) => {
                return error(
                    &self.context,
                    "A `ParamSpec` cannot specify variance when `infer_variance=True`",
                    call_expr,
                );
            }
            (true, false, false) => Some(TypeVarVariance::Covariant),
            (false, true, false) => Some(TypeVarVariance::Contravariant),
            (false, false, false) => Some(TypeVarVariance::Invariant),
            (false, false, true) => None,
        };

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
            target_name,
            Some(definition),
            TypeVarKind::LegacyParamSpec,
        );
        Type::KnownInstance(KnownInstanceType::TypeVar(TypeVarInstance::new(
            db, identity, None, variance, default,
        )))
    }
}
