//! Deferred bounds and defaults for PEP 695 parameters and legacy parameter defaults.

use std::convert::Infallible;

use ruff_python_ast as ast;

use super::super::{BoundOrConstraintsNodes, DeferredExpressionState, TypeInferenceBuilder};
use crate::types::diagnostic::{
    INVALID_LEGACY_TYPE_VARIABLE, INVALID_PARAMSPEC, INVALID_TYPE_VARIABLE_BOUND,
    INVALID_TYPE_VARIABLE_CONSTRAINTS,
};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::typevar::TypeVarConstraints;
use crate::types::{KnownClass, Type, TypeVarBoundOrConstraints};

/// Fixed shared-flow segments admitted independently of expression and collection effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ParameterWork {
    Declaration,
    ParamSpecDefault,
    TypeVarTupleDefault,
    Element,
}

/// The temporary interpretation of a parameter's default expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum DefaultContext {
    ParamSpec,
    ParamSpecList,
    TypeVarTuple,
}

/// The original value of the single flag changed for a default expression.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct DefaultFlagState {
    pub(in crate::types::infer::builder) flag: InferenceFlags,
    pub(in crate::types::infer::builder) enabled: bool,
}

/// Syntax facts used by both ordinary and controlled deferred inference.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct DeferredTypeParameterFacts;

/// Ordinary effects preserve the existing diagnostics and lazy compatibility checks.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct OrdinaryDeferredTypeParameterEffects;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDeferredTypeParameterEffects)]
    pub(in crate::types::infer) trait DeferredTypeParameterEffects<'db, 'ast> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ParameterWork) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn replace_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> Result<DeferredExpressionState, Self::Error>;
        #[operation(local)]
        async fn restore_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn enter_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, context: DefaultContext) -> Result<DefaultFlagState, Self::Error>;
        #[operation(local)]
        async fn restore_flags(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flags: DefaultFlagState) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn type_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn ellipsis(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprEllipsisLiteral) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_types(&self) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element<'expr>(&self, elements: &'expr [ast::Expr], cursor: &mut usize) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn push_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn copy_types(&self, types: &[Type<'db>]) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn tuple_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn intern_constraints(&self, builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Self::Error>;
        #[operation(child)]
        async fn has_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn generic_constraint(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn generic_bound(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn outer_default(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, expression: &ast::Expr, name: &str) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn validate_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: Option<TypeVarBoundOrConstraints<'db>>, ty: Type<'db>, expression: &ast::Expr, nodes: Option<BoundOrConstraintsNodes<'ast>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn paramspec_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn typevartuple_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn is_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn is_typevartuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn is_unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn invalid_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn invalid_typevartuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl DeferredTypeParameterFacts {
        fn bound<'a>(&self, node: ast::TypeParamRef<'a>) -> Option<&'a ast::Expr> {
            match node {
                ast::TypeParamRef::TypeVar(node) => node.bound.as_deref(),
                ast::TypeParamRef::ParamSpec(_) | ast::TypeParamRef::TypeVarTuple(_) => None,
            }
        }

        fn default<'a>(&self, node: ast::TypeParamRef<'a>) -> Option<&'a ast::Expr> {
            match node {
                ast::TypeParamRef::TypeVar(node) => node.default.as_deref(),
                ast::TypeParamRef::ParamSpec(node) => node.default.as_deref(),
                ast::TypeParamRef::TypeVarTuple(node) => node.default.as_deref(),
            }
        }

        fn name<'a>(&self, node: ast::TypeParamRef<'a>) -> &'a str {
            match node {
                ast::TypeParamRef::TypeVar(node) => &node.name.id,
                ast::TypeParamRef::ParamSpec(node) => &node.name.id,
                ast::TypeParamRef::TypeVarTuple(node) => &node.name.id,
            }
        }

        fn tuple_elements<'a>(&self, expression: &'a ast::Expr) -> Option<&'a [ast::Expr]> {
            match expression {
                ast::Expr::Tuple(tuple) => Some(&tuple.elts),
                _ => None,
            }
        }

        fn enough_constraints(&self, elements: &[ast::Expr]) -> bool {
            elements.len() >= 2
        }

        fn bound_nodes<'a>(&self, expression: Option<&'a ast::Expr>) -> Option<BoundOrConstraintsNodes<'a>> {
            expression.map(|expression| match expression {
                ast::Expr::Tuple(tuple) => BoundOrConstraintsNodes::Constraints(&tuple.elts),
                _ => BoundOrConstraintsNodes::Bound(expression),
            })
        }
    }

    /// Infers deferred declarations in bound, default, outer-scope, compatibility order.
    /// The caller must restore temporary builder state if an effect fails before normal restoration.
    #[synchronous(infer_type_parameter_deferred_sync)]
    #[capabilities(effects = DeferredTypeParameterEffects, facts = DeferredTypeParameterFacts)]
    #[passive_values(ParameterWork::Declaration, ParameterWork::Element, DeferredExpressionState::Deferred, TypeVarBoundOrConstraints::Constraints, TypeVarBoundOrConstraints::UpperBound)]
    pub(in crate::types::infer) async fn infer_type_parameter_deferred_with<'db, 'ast, E: DeferredTypeParameterEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: ast::TypeParamRef<'ast>,
        facts: DeferredTypeParameterFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint(ParameterWork::Declaration).await?;
        let default = facts.default(node);
        match (node, default) {
            (ast::TypeParamRef::ParamSpec(_) | ast::TypeParamRef::TypeVarTuple(_), None) => return Ok(()),
            _ => {}
        }
        let previous = effects.replace_deferred(builder, DeferredExpressionState::Deferred).await?;
        let bound = facts.bound(node);
        let bounds = if let Some(expression) = bound {
            if let Some(elements) = facts.tuple_elements(expression) {
                // Here, we interpret `bound` as a heterogeneous tuple and convert it to `TypeVarConstraints`
                // in `TypeVarInstance::lazy_constraints`.
                let mut types = effects.new_types().await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(element) = effects.next_element(elements, &mut cursor).await? {
                    effects.checkpoint(ParameterWork::Element).await?;
                    let ty = effects.type_expression(builder, element).await?;
                    if effects.has_typevar(builder, ty).await? {
                        effects.generic_constraint(builder, element).await?;
                    }
                    effects.push_type(&mut types, ty).await?;
                }
                let tuple_types = effects.copy_types(&types).await?;
                let tuple = effects.tuple_type(builder, tuple_types).await?;
                effects.store(builder, expression, tuple).await?;
                // Mirror the `< 2` guard in `TypeParameterHeaderFacts::annotations` to avoid
                // a cascading `invalid-type-variable-default` diagnostic for tuples
                // that have already been flagged as invalid constraints.
                if facts.enough_constraints(elements) {
                    Some(TypeVarBoundOrConstraints::Constraints(effects.intern_constraints(builder, types).await?))
                } else {
                    None
                }
            } else {
                let ty = effects.type_expression(builder, expression).await?;
                if effects.has_typevar(builder, ty).await? {
                    effects.generic_bound(builder, expression).await?;
                }
                Some(TypeVarBoundOrConstraints::UpperBound(ty))
            }
        } else {
            None
        };
        if let Some(default) = default {
            let name = facts.name(node);
            match node {
                ast::TypeParamRef::TypeVar(_) => {
                    let ty = effects.type_expression(builder, default).await?;
                    if !effects.outer_default(builder, ty, default, name).await? {
                        effects.validate_default(builder, Some(name), bounds, ty, default, facts.bound_nodes(bound)).await?;
                    }
                }
                ast::TypeParamRef::ParamSpec(_) => effects.paramspec_default(builder, default, Some(name)).await?,
                ast::TypeParamRef::TypeVarTuple(_) => effects.typevartuple_default(builder, default, Some(name)).await?,
            }
        }
        effects.restore_deferred(builder, previous).await?;
        Ok(())
    }

    /// Infers a ParamSpec default, using a tuple to store a list of argument types.
    /// The caller must restore temporary builder state if an effect fails before normal restoration.
    #[synchronous(infer_paramspec_default_sync)]
    #[capabilities(effects = DeferredTypeParameterEffects)]
    #[passive_values(ParameterWork::ParamSpecDefault, ParameterWork::Element, DefaultContext::ParamSpec, DefaultContext::ParamSpecList)]
    pub(in crate::types::infer) async fn infer_paramspec_default_with<'db, 'ast, E: DeferredTypeParameterEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        name: Option<&str>,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint(ParameterWork::ParamSpecDefault).await?;
        let previous = effects.enter_default(builder, DefaultContext::ParamSpec).await?;
        let valid = match expression {
            ast::Expr::EllipsisLiteral(ellipsis) => {
                let ty = effects.ellipsis(builder, ellipsis).await?;
                effects.store(builder, expression, ty).await?;
                true
            }
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                let list_previous = effects.enter_default(builder, DefaultContext::ParamSpecList).await?;
                let mut types = effects.new_types().await?;
                let mut cursor = 0;
                #[cursor_loop]
                while let Some(element) = effects.next_element(elts, &mut cursor).await? {
                    effects.checkpoint(ParameterWork::Element).await?;
                    let ty = effects.type_expression(builder, element).await?;
                    effects.push_type(&mut types, ty).await?;
                }
                effects.restore_flags(builder, list_previous).await?;
                // N.B. We cannot represent a heterogeneous list of types in our type system, so we
                // use a heterogeneous tuple type to represent the list of types instead.
                let ty = effects.tuple_type(builder, types).await?;
                effects.store(builder, expression, ty).await?;
                true
            }
            ast::Expr::Name(_) => {
                let ty = effects.type_expression(builder, expression).await?;
                if let Some(name) = name
                    && effects.outer_default(builder, ty, expression, name).await?
                {
                    true
                } else {
                    effects.is_paramspec(builder, ty).await?
                }
            }
            _ => false,
        };
        if !valid {
            effects.invalid_paramspec(builder, expression).await?;
        }
        effects.restore_flags(builder, previous).await
    }

    /// Infers a TypeVarTuple default and checks for unpack syntax or another TypeVarTuple.
    /// The caller must restore temporary builder state if an effect fails before normal restoration.
    #[synchronous(infer_typevartuple_default_sync)]
    #[capabilities(effects = DeferredTypeParameterEffects)]
    #[passive_values(ParameterWork::TypeVarTupleDefault, DefaultContext::TypeVarTuple)]
    pub(in crate::types::infer) async fn infer_typevartuple_default_with<'db, 'ast, E: DeferredTypeParameterEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        name: Option<&str>,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint(ParameterWork::TypeVarTupleDefault).await?;
        let previous = effects.enter_default(builder, DefaultContext::TypeVarTuple).await?;
        let ty = effects.type_expression(builder, expression).await?;
        effects.restore_flags(builder, previous).await?;
        if let Some(name) = name
            && effects.outer_default(builder, ty, expression, name).await?
        {
            return Ok(());
        }
        if !effects.is_unpack(builder, expression).await?
            && !effects.is_typevartuple(builder, ty).await?
        {
            effects.invalid_typevartuple(builder, expression).await?;
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousDeferredTypeParameterEffects<'db, 'ast> for OrdinaryDeferredTypeParameterEffects {
    type Error = Infallible;

    fn checkpoint(&self, _work: ParameterWork) -> Result<(), Infallible> { Ok(()) }

    fn replace_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> Result<DeferredExpressionState, Infallible> {
        Ok(builder.replace_deferred_state(state))
    }

    fn restore_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> Result<(), Infallible> {
        builder.deferred_state = state;
        Ok(())
    }

    fn enter_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, context: DefaultContext) -> Result<DefaultFlagState, Infallible> {
        let (flag, enabled) = match context {
            DefaultContext::ParamSpec => (InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true),
            DefaultContext::ParamSpecList => (InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false),
            DefaultContext::TypeVarTuple => (InferenceFlags::IN_VALID_UNPACK_CONTEXT, true),
        };
        Ok(DefaultFlagState { flag, enabled: builder.context.inference_flags.replace(flag, enabled) })
    }

    fn restore_flags(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flags: DefaultFlagState) -> Result<(), Infallible> {
        builder.context.inference_flags.set(flags.flag, flags.enabled);
        Ok(())
    }

    fn type_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Infallible> { Ok(builder.infer_type_expression(expression)) }

    fn ellipsis(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprEllipsisLiteral) -> Result<Type<'db>, Infallible> { Ok(builder.infer_ellipsis_literal_expression(expression)) }

    fn new_types(&self) -> Result<Vec<Type<'db>>, Infallible> { Ok(Vec::new()) }

    fn next_element<'expr>(&self, elements: &'expr [ast::Expr], cursor: &mut usize) -> Result<Option<&'expr ast::Expr>, Infallible> {
        let element = elements.get(*cursor);
        if element.is_some() { *cursor += 1; }
        Ok(element)
    }

    fn push_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Infallible> { types.push(ty); Ok(()) }

    fn copy_types(&self, types: &[Type<'db>]) -> Result<Vec<Type<'db>>, Infallible> { Ok(types.to_vec()) }

    fn tuple_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> Result<Type<'db>, Infallible> {
        Ok(Type::heterogeneous_tuple(builder.db(), builder.program_environment(), types))
    }

    fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Infallible> { builder.store_expression_type(expression, ty); Ok(()) }

    fn intern_constraints(&self, builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Infallible> { Ok(TypeVarConstraints::new(builder.db(), types.into_boxed_slice())) }

    fn has_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Infallible> { Ok(ty.has_typevar_or_typevar_instance(builder.db(), builder.program_environment())) }

    fn generic_constraint(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, expression) { diagnostic.into_diagnostic("TypeVar constraint cannot be generic"); }
        Ok(())
    }

    fn generic_bound(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_TYPE_VARIABLE_BOUND, expression) { diagnostic.into_diagnostic("TypeVar upper bound cannot be generic"); }
        Ok(())
    }

    fn outer_default(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>, expression: &ast::Expr, name: &str) -> Result<bool, Infallible> { Ok(builder.check_default_for_outer_scope_typevars(ty, expression, name)) }

    fn validate_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: Option<TypeVarBoundOrConstraints<'db>>, ty: Type<'db>, expression: &ast::Expr, nodes: Option<BoundOrConstraintsNodes<'ast>>) -> Result<(), Infallible> {
        super::assignment::validate_typevar_default_sync(builder, name, bounds, ty, expression, nodes, &super::assignment::OrdinaryDeferredAssignmentEffects)
    }

    fn paramspec_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> Result<(), Infallible> { infer_paramspec_default_sync(builder, expression, name, self) }

    fn typevartuple_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> Result<(), Infallible> { infer_typevartuple_default_sync(builder, expression, name, self) }

    fn is_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(match ty {
            Type::TypeVar(variable) => variable.is_paramspec(builder.db()),
            Type::KnownInstance(instance) => instance.class(builder.db()) == KnownClass::ParamSpec,
            _ => false,
        })
    }

    fn is_typevartuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Infallible> { Ok(matches!(ty, Type::TypeVar(variable) if variable.is_typevartuple(builder.db()))) }

    fn is_unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<bool, Infallible> { Ok(builder.type_expression_flags(expression).contains(TypeExpressionFlags::UNPACK)) }

    fn invalid_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_PARAMSPEC, expression) {
            diagnostic.into_diagnostic("The default value to `ParamSpec` must be either a list of types, `ParamSpec`, or `...`");
        }
        Ok(())
    }

    fn invalid_typevartuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_LEGACY_TYPE_VARIABLE, expression) {
            diagnostic.into_diagnostic("The default value for `TypeVarTuple` must be an unpacked tuple type or another TypeVarTuple");
        }
        Ok(())
    }
}
