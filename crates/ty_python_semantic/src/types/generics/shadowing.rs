//! Checks explicit function type parameters against the names bound by enclosing generic scopes.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::GenericContext;
use super::binding::{BindingFacts, InlineBinding, SynchronousTypeVarBindingEffects};
use super::context_construction::ContextVariables;
use crate::types::context::InferContext;
use crate::types::diagnostic::report_shadowed_type_variable;
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::{BoundTypeVarInstance, TypeVarKind};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVariableNameEffects)]
    pub(in crate::types) trait TypeVariableNameEffects<'db> {
        type Error;

        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn has_name(&self, variable: BoundTypeVarInstance<'db>, name: &Name) -> Result<bool, Self::Error>;
    }

    /// Returns the first variable with this name in the context's stored order.
    /// Distinct type-variable identities can have the same name, so identity lookup is insufficient.
    #[synchronous(find_named_typevar_sync)]
    #[capabilities(effects = TypeVariableNameEffects)]
    #[passive_values()]
    pub(in crate::types) async fn find_named_typevar_with<'db, E: TypeVariableNameEffects<'db>>(
        context: GenericContext<'db>,
        name: &Name,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(variable) = effects.next_variable(variables, &mut cursor).await? {
            if effects.has_name(variable, name).await? {
                return Ok(Some(variable));
            }
        }
        Ok(None)
    }

    #[synchronous(SynchronousFunctionShadowEffects)]
    pub(in crate::types) trait FunctionShadowEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn next_parameter<'ast>(&self, function: &'ast ast::StmtFunctionDef, cursor: &mut usize) -> Result<Option<&'ast ast::TypeParam>, Self::Error>;
        #[operation(local)]
        async fn parameter_name<'ast>(&self, parameter: &'ast ast::TypeParam) -> Result<&'ast Name, Self::Error>;
        #[operation(source)]
        async fn ancestors<'index>(&self, index: &'index SemanticIndex<'db>, scope: FileScopeId) -> Result<AncestorsIter<'index>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(&self, ancestors: &mut AncestorsIter<'index>) -> Result<Option<(FileScopeId, &'index Scope)>, Self::Error>;
        #[operation(child)]
        async fn scope_context(&self, index: &SemanticIndex<'db>, scope: &Scope) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn find_named(&self, context: GenericContext<'db>, name: &Name) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn parameter_kind(&self, parameter: &ast::TypeParam) -> Result<TypeVarKind, Self::Error>;
        #[operation(child)]
        async fn report(&self, function: &ast::StmtFunctionDef, name: &Name, kind: TypeVarKind, other: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
    }

    /// Reports each enclosing generic context that binds a function type parameter's name.
    /// Each parameter restarts the ancestor walk at the scope containing the function definition;
    /// continuing beyond the nearest match also reports shadowing of more distant bindings.
    #[synchronous(check_function_type_parameter_shadowing_sync)]
    #[capabilities(effects = FunctionShadowEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_function_type_parameter_shadowing_with<'db, E: FunctionShadowEffects<'db>>(
        index: &SemanticIndex<'db>,
        current_scope: FileScopeId,
        function: &ast::StmtFunctionDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        let mut parameter_cursor = 0;
        #[cursor_loop]
        while let Some(parameter) = effects.next_parameter(function, &mut parameter_cursor).await? {
            let name = effects.parameter_name(parameter).await?;
            let mut ancestors = effects.ancestors(index, current_scope).await?;
            #[cursor_loop]
            while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
                let (_, scope) = ancestor;
                let Some(context) = effects.scope_context(index, scope).await? else {
                    continue;
                };
                if let Some(other) = effects.find_named(context, name).await? {
                    let kind = effects.parameter_kind(parameter).await?;
                    effects.report(function, name, kind, other).await?;
                }
            }
        }
        Ok(())
    }
}

/// Advances through explicit parameters without allocating a separate parameter list.
pub(in crate::types) fn next_function_type_parameter<'ast>(
    function: &'ast ast::StmtFunctionDef,
    cursor: &mut usize,
) -> Option<&'ast ast::TypeParam> {
    let parameter = function.type_params.as_deref()?.get(*cursor)?;
    *cursor += 1;
    Some(parameter)
}

/// Classifies the explicit parameter only once a same-name enclosing binding is found.
pub(in crate::types) const fn function_type_parameter_kind(
    parameter: &ast::TypeParam,
) -> TypeVarKind {
    match parameter {
        ast::TypeParam::TypeVar(_) => TypeVarKind::Pep695TypeVar,
        ast::TypeParam::ParamSpec(_) => TypeVarKind::Pep695ParamSpec,
        ast::TypeParam::TypeVarTuple(_) => TypeVarKind::Pep695TypeVarTuple,
    }
}

impl<'db> SynchronousTypeVariableNameEffects<'db> for InlineBinding<'db> {
    type Error = Infallible;

    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        SynchronousTypeVarBindingEffects::variables(self, context)
    }

    fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        SynchronousTypeVarBindingEffects::next_variable(self, variables, cursor)
    }

    fn has_name(
        &self,
        variable: BoundTypeVarInstance<'db>,
        name: &Name,
    ) -> Result<bool, Infallible> {
        Ok(variable.typevar(self.0).name(self.0) == name)
    }
}

/// Uses canonical ordinary contexts and the caller's diagnostic context for the shared scan.
#[derive(Debug)]
pub(in crate::types) struct OrdinaryFunctionShadowEffects<'a, 'db, 'ast> {
    pub(in crate::types) context: &'a InferContext<'db, 'ast>,
}

impl<'db> SynchronousFunctionShadowEffects<'db> for OrdinaryFunctionShadowEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn next_parameter<'ast>(
        &self,
        function: &'ast ast::StmtFunctionDef,
        cursor: &mut usize,
    ) -> Result<Option<&'ast ast::TypeParam>, Infallible> {
        Ok(next_function_type_parameter(function, cursor))
    }

    fn parameter_name<'ast>(
        &self,
        parameter: &'ast ast::TypeParam,
    ) -> Result<&'ast Name, Infallible> {
        Ok(&parameter.name().id)
    }

    fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> Result<AncestorsIter<'index>, Infallible> {
        SynchronousTypeVarBindingEffects::ancestors(&InlineBinding(self.context.db()), index, scope)
    }

    fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> Result<Option<(FileScopeId, &'index Scope)>, Infallible> {
        SynchronousTypeVarBindingEffects::next_ancestor(
            &InlineBinding(self.context.db()),
            ancestors,
        )
    }

    fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        SynchronousTypeVarBindingEffects::scope_context(
            &InlineBinding(self.context.db()),
            index,
            BindingFacts.node(scope),
            ReturnCallableTypeVarScope::Public,
        )
    }

    fn find_named(
        &self,
        context: GenericContext<'db>,
        name: &Name,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        find_named_typevar_sync(context, name, &InlineBinding(self.context.db()))
    }

    fn parameter_kind(&self, parameter: &ast::TypeParam) -> Result<TypeVarKind, Infallible> {
        Ok(function_type_parameter_kind(parameter))
    }

    fn report(
        &self,
        function: &ast::StmtFunctionDef,
        name: &Name,
        kind: TypeVarKind,
        other: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        report_shadowed_type_variable(
            self.context,
            name,
            "function",
            &function.name.id,
            function.name.range(),
            kind,
            other,
        );
        Ok(())
    }
}
