use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::definition::Definition;

use super::super::{BoundOrConstraintsNodes, TypeInferenceBuilder};
use crate::types::diagnostic::{INVALID_TYPE_VARIABLE_BOUND, INVALID_TYPE_VARIABLE_CONSTRAINTS};
use crate::types::infer::InferenceRegion;
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    KnownClass, KnownFunction, SpecialFormType, Type, TypeContext, TypeVarBoundOrConstraints,
    TypingModule,
};

#[cfg(test)]
mod tests;

pub(in crate::types::infer) struct DeferredAssignmentFacts;
pub(in crate::types::infer) struct OrdinaryDeferredAssignmentEffects;

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum DeferredAssignmentChild<'db> {
    NamedTuple,
    NewType,
    TypeAliasType(Definition<'db>),
    BuiltinType(Definition<'db>),
    TypedDict,
    NewClass(Definition<'db>),
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDeferredAssignmentEffects)]
    pub(in crate::types::infer) trait DeferredAssignmentEffects<'db, 'ast> {
        type Error;

        #[operation(local)]
        async fn cached_expression(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn deferred_definition(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(source)]
        async fn typed_dict_module(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<TypingModule>, Self::Error>;
        #[operation(source)]
        async fn is_new_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn child(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, value: &'ast ast::Expr, call: &'ast ast::ExprCall, child: DeferredAssignmentChild<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_constraints(&self) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_constraint(&self, call: &'ast ast::ExprCall, cursor: &mut usize) -> Result<Option<&'ast ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn push_constraint(&self, constraints: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn intern_constraints(&self, builder: &TypeInferenceBuilder<'db, 'ast>, constraints: Vec<Type<'db>>) -> Result<TypeVarConstraints<'db>, Self::Error>;
        #[operation(child)]
        async fn type_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn has_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn generic_constraint(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn generic_bound(&self, builder: &TypeInferenceBuilder<'db, 'ast>, keyword: &ast::Keyword) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn find_keyword(&self, call: &'ast ast::ExprCall, name: &str) -> Result<Option<&'ast ast::Keyword>, Self::Error>;
        #[operation(child)]
        async fn paramspec_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn typevartuple_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn validate_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: Option<TypeVarBoundOrConstraints<'db>>, default_ty: Type<'db>, default_node: &ast::Expr, bound_nodes: Option<BoundOrConstraintsNodes<'ast>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn bounded_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: TypeVarBoundOrConstraints<'db>, default_ty: Type<'db>, default_node: &ast::Expr, bound_nodes: Option<BoundOrConstraintsNodes<'ast>>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl DeferredAssignmentFacts {
        fn call<'ast>(&self, value: &'ast ast::Expr) -> Option<&'ast ast::ExprCall> {
            value.as_call_expr()
        }

        fn callee<'ast>(&self, call: &'ast ast::ExprCall) -> &'ast ast::Expr {
            &call.func
        }

        fn named_tuple(&self, ty: Type<'_>) -> bool {
            ty == Type::SpecialForm(SpecialFormType::NamedTuple)
        }

        fn has_constraints(&self, constraints: &[Type<'_>]) -> bool {
            !constraints.is_empty()
        }

        fn keyword_value<'ast>(&self, keyword: &'ast ast::Keyword) -> &'ast ast::Expr {
            &keyword.value
        }

        fn target_name<'target>(&self, target: &'target ast::Expr) -> Option<&'target str> {
            target.as_name_expr().map(|name| &*name.id)
        }

        fn bound_nodes<'ast>(&self, call: &'ast ast::ExprCall, bound: Option<&'ast ast::Keyword>) -> Option<BoundOrConstraintsNodes<'ast>> {
            bound
                .map(|kw| BoundOrConstraintsNodes::Bound(&kw.value))
                .or_else(|| {
                    if call.arguments.args.len() < 3 {
                        return None;
                    }
                    Some(BoundOrConstraintsNodes::Constraints(&call.arguments.args[1..]))
                })
        }
    }

    #[synchronous(infer_assignment_deferred_sync)]
    #[capabilities(effects = DeferredAssignmentEffects, facts = DeferredAssignmentFacts)]
    #[passive_values(DeferredAssignmentChild::NamedTuple, DeferredAssignmentChild::NewType, DeferredAssignmentChild::TypeAliasType, DeferredAssignmentChild::BuiltinType, DeferredAssignmentChild::TypedDict, DeferredAssignmentChild::NewClass, TypeVarBoundOrConstraints::Constraints, TypeVarBoundOrConstraints::UpperBound)]
    pub(in crate::types::infer) async fn infer_assignment_deferred_with<'db, 'ast, E: DeferredAssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: &'ast ast::Expr,
        facts: DeferredAssignmentFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        // Infer deferred bounds/constraints/defaults of a legacy TypeVar / ParamSpec / NewType,
        // and field types for functional TypedDict.
        let Some(call) = facts.call(value) else {
            return Ok(());
        };
        let func = facts.callee(call);
        let func_ty = match effects.cached_expression(builder, func).await? {
            Some(ty) => ty,
            None => effects.expression(builder, func).await?,
        };
        if facts.named_tuple(func_ty) {
            // Only the `fields` argument is deferred for `NamedTuple`;
            // other arguments are inferred eagerly.
            return effects.child(builder, target, value, call, DeferredAssignmentChild::NamedTuple).await;
        }
        let known_class = effects.known_class(builder, func_ty).await?;
        let definition = effects.deferred_definition(builder).await?;
        match (known_class, definition) {
            (Some(KnownClass::NewType), _) => {
                return effects.child(builder, target, value, call, DeferredAssignmentChild::NewType).await;
            }
            (Some(KnownClass::TypeAliasType | KnownClass::ExtensionsTypeAliasType), Some(definition)) => {
                return effects.child(builder, target, value, call, DeferredAssignmentChild::TypeAliasType(definition)).await;
            }
            (Some(KnownClass::Type), Some(definition)) => {
                return effects.child(builder, target, value, call, DeferredAssignmentChild::BuiltinType(definition)).await;
            }
            _ => {}
        }
        if let Some(_) = effects.typed_dict_module(builder, func_ty).await? {
            return effects.child(builder, target, value, call, DeferredAssignmentChild::TypedDict).await;
        }
        if let Some(definition) = definition
            && effects.is_new_class(builder, func_ty).await?
        {
            return effects.child(builder, target, value, call, DeferredAssignmentChild::NewClass(definition)).await;
        }
        let mut constraint_tys = effects.new_constraints().await?;
        let mut cursor = 1;
        #[cursor_loop]
        while let Some(arg) = effects.next_constraint(call, &mut cursor).await? {
            let constraint = effects.type_expression(builder, arg).await?;
            effects.push_constraint(&mut constraint_tys, constraint).await?;
            if effects.has_typevar(builder, constraint).await? {
                effects.generic_constraint(builder, arg).await?;
            }
        }
        #[passive_state]
        let mut bound_or_constraints = if facts.has_constraints(&constraint_tys) {
            Some(TypeVarBoundOrConstraints::Constraints(effects.intern_constraints(builder, constraint_tys).await?))
        } else {
            None
        };
        if let Some(bound) = effects.find_keyword(call, "bound").await? {
            let bound_type = effects.type_expression(builder, facts.keyword_value(bound)).await?;
            bound_or_constraints = Some(TypeVarBoundOrConstraints::UpperBound(bound_type));
            if effects.has_typevar(builder, bound_type).await? {
                effects.generic_bound(builder, bound).await?;
            }
        }
        if let Some(default) = effects.find_keyword(call, "default").await? {
            match known_class {
                Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple) => {
                    effects.typevartuple_default(builder, facts.keyword_value(default)).await?;
                }
                Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec) => {
                    // Pass `None` for the name: the outer-scope typevar check inside
                    // `infer_paramspec_default` is only relevant for PEP 695 type parameter
                    // scopes. Legacy ParamSpec definitions live at module/class-body scope,
                    // so the check would be a no-op here. Out-of-scope defaults for legacy
                    // typevars are instead validated by `check_legacy_typevar_defaults`
                    // (for functions) and `report_invalid_typevar_default_reference`
                    // (for classes).
                    effects.paramspec_default(builder, facts.keyword_value(default)).await?;
                }
                _ => {
                    let default_ty = effects.type_expression(builder, facts.keyword_value(default)).await?;
                    let bound = effects.find_keyword(call, "bound").await?;
                    effects.validate_default(builder, facts.target_name(target), bound_or_constraints, default_ty, facts.keyword_value(default), facts.bound_nodes(call, bound)).await?;
                }
            }
        }
        Ok(())
    }

    #[synchronous(validate_typevar_default_sync)]
    #[capabilities(effects = DeferredAssignmentEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn validate_typevar_default_with<'db, 'ast, E: DeferredAssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: Option<&str>,
        bound_or_constraints: Option<TypeVarBoundOrConstraints<'db>>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_or_constraints_nodes: Option<BoundOrConstraintsNodes<'ast>>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let Some(bound_or_constraints) = bound_or_constraints else {
            return Ok(());
        };
        effects.bounded_default(builder, name, bound_or_constraints, default_ty, default_node, bound_or_constraints_nodes).await
    }
}

impl<'db, 'ast> SynchronousDeferredAssignmentEffects<'db, 'ast>
    for OrdinaryDeferredAssignmentEffects
{
    type Error = Infallible;

    fn cached_expression(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(builder.try_expression_type(expression))
    }

    fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_expression(expression, TypeContext::default()))
    }

    fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<KnownClass>, Infallible> {
        Ok(ty
            .as_class_literal()
            .and_then(|class| class.known(builder.db())))
    }

    fn deferred_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(match builder.region {
            InferenceRegion::Deferred(definition) => Some(definition),
            _ => None,
        })
    }

    fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<TypingModule>, Infallible> {
        Ok(TypingModule::from_typed_dict_type(builder.db(), ty))
    }

    fn is_new_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty
            .as_function_literal()
            .is_some_and(|function| function.is_known(builder.db(), KnownFunction::NewClass)))
    }

    fn child(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: &'ast ast::Expr,
        call: &'ast ast::ExprCall,
        child: DeferredAssignmentChild<'db>,
    ) -> Result<(), Infallible> {
        match child {
            DeferredAssignmentChild::NamedTuple => {
                builder.infer_typing_namedtuple_fields(&call.arguments.args[1]);
            }
            DeferredAssignmentChild::NewType => {
                builder.infer_newtype_assignment_deferred(&call.arguments)
            }
            DeferredAssignmentChild::TypeAliasType(definition) => {
                builder.infer_typealiastype_assignment_deferred(definition, target, &call.arguments)
            }
            DeferredAssignmentChild::BuiltinType(definition) => {
                builder.infer_builtins_type_deferred(definition, value)
            }
            DeferredAssignmentChild::TypedDict => {
                builder.infer_functional_typeddict_deferred(&call.arguments)
            }
            DeferredAssignmentChild::NewClass(definition) => {
                builder.infer_new_class_deferred(definition, value)
            }
        }
        Ok(())
    }

    fn new_constraints(&self) -> Result<Vec<Type<'db>>, Infallible> {
        Ok(Vec::new())
    }

    fn next_constraint(
        &self,
        call: &'ast ast::ExprCall,
        cursor: &mut usize,
    ) -> Result<Option<&'ast ast::Expr>, Infallible> {
        let argument = call.arguments.args.get(*cursor);
        if argument.is_some() {
            *cursor += 1;
        }
        Ok(argument)
    }

    fn push_constraint(
        &self,
        constraints: &mut Vec<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        constraints.push(ty);
        Ok(())
    }

    fn intern_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: Vec<Type<'db>>,
    ) -> Result<TypeVarConstraints<'db>, Infallible> {
        Ok(TypeVarConstraints::new(
            builder.db(),
            constraints.into_boxed_slice(),
        ))
    }

    fn type_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_type_expression(expression))
    }

    fn has_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.has_typevar_or_typevar_instance(builder.db(), builder.program_environment()))
    }

    fn generic_constraint(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        if let Some(builder) = builder
            .context
            .report_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS, expression)
        {
            builder.into_diagnostic("TypeVar constraint cannot be generic");
        }
        Ok(())
    }

    fn generic_bound(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        keyword: &ast::Keyword,
    ) -> Result<(), Infallible> {
        if let Some(builder) = builder
            .context
            .report_lint(&INVALID_TYPE_VARIABLE_BOUND, keyword)
        {
            builder.into_diagnostic("TypeVar upper bound cannot be generic");
        }
        Ok(())
    }

    fn find_keyword(
        &self,
        call: &'ast ast::ExprCall,
        name: &str,
    ) -> Result<Option<&'ast ast::Keyword>, Infallible> {
        Ok(call.arguments.find_keyword(name))
    }

    fn paramspec_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.infer_paramspec_default(expression, None);
        Ok(())
    }

    fn typevartuple_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.infer_typevartuple_default(expression, None);
        Ok(())
    }

    fn validate_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: Option<&str>,
        bounds: Option<TypeVarBoundOrConstraints<'db>>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> Result<(), Infallible> {
        validate_typevar_default_sync(
            builder,
            name,
            bounds,
            default_ty,
            default_node,
            bound_nodes,
            self,
        )
    }

    fn bounded_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: Option<&str>,
        bounds: TypeVarBoundOrConstraints<'db>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> Result<(), Infallible> {
        builder.validate_bound_typevar_default(name, bounds, default_ty, default_node, bound_nodes);
        Ok(())
    }
}
