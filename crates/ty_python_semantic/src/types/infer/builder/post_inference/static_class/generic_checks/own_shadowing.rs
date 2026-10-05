//! Checks each class-owned variable against every enclosing generic context by name.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::OrdinaryClassGenericCheckEffects;
use super::default_references::{VariableCursor, VariableRange};
use crate::types::diagnostic::report_shadowed_type_variable;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::shadowing::{
    SynchronousTypeVariableNameEffects, find_named_typevar_sync,
};
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassOwnShadowEffects)]
    pub(in crate::types::infer::builder) trait ClassOwnShadowEffects<'db> {
        type Error;

        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        async fn cursor<'a>(&self, variables: &'a ContextVariables<'db>) -> Result<VariableCursor<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, cursor: &mut VariableCursor<'_, 'db>) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Self::Error>;
        #[operation(source)]
        async fn name(&self, variable: BoundTypeVarInstance<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(source)]
        async fn ancestors<'index>(&self, index: &'index SemanticIndex<'db>, parent: FileScopeId) -> Result<AncestorsIter<'index>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(&self, ancestors: &mut AncestorsIter<'index>) -> Result<Option<(FileScopeId, &'index Scope)>, Self::Error>;
        #[operation(child)]
        async fn scope_context(&self, index: &SemanticIndex<'db>, scope: &Scope) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn find_named(&self, context: GenericContext<'db>, name: &Name) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn report(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, variable: BoundTypeVarInstance<'db>, other: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
    }

    /// Reports every enclosing context that binds a class-owned variable's name.
    /// Each own variable restarts at the class's parent scope, and a match does not end that walk.
    #[synchronous(check_class_own_shadowing_sync)]
    #[capabilities(effects = ClassOwnShadowEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_class_own_shadowing_with<'db, E: ClassOwnShadowEffects<'db>>(
        index: &SemanticIndex<'db>,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = effects.cursor(variables).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut cursor).await? {
            let (_, variable) = entry;
            let name = effects.name(variable).await?;
            let mut ancestors = effects.ancestors(index, parent).await?;
            #[cursor_loop]
            while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
                let (_, scope) = ancestor;
                let Some(context) = effects.scope_context(index, scope).await? else {
                    continue;
                };
                if let Some(other) = effects.find_named(context, name).await? {
                    effects.report(class, class_node, variable, other).await?;
                }
            }
        }
        Ok(())
    }
}

impl<'db> SynchronousTypeVariableNameEffects<'db>
    for OrdinaryClassGenericCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_with_fields(salsa::FieldReads::new(self.context.db())))
    }

    fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let variable = GenericContext::variable_at_in(variables, *cursor);
        if variable.is_some() {
            *cursor += 1;
        }
        Ok(variable)
    }

    fn has_name(
        &self,
        variable: BoundTypeVarInstance<'db>,
        name: &Name,
    ) -> Result<bool, Infallible> {
        Ok(variable.typevar(self.context.db()).name(self.context.db()) == name)
    }
}

impl<'db> SynchronousClassOwnShadowEffects<'db> for OrdinaryClassGenericCheckEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        SynchronousTypeVariableNameEffects::variables(self, context)
    }

    fn cursor<'a>(
        &self,
        variables: &'a ContextVariables<'db>,
    ) -> Result<VariableCursor<'a, 'db>, Infallible> {
        Ok(VariableCursor::new(variables, VariableRange::All))
    }

    fn next_variable(
        &self,
        cursor: &mut VariableCursor<'_, 'db>,
    ) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Infallible> {
        Ok(cursor.next())
    }

    fn name(&self, variable: BoundTypeVarInstance<'db>) -> Result<&'db Name, Infallible> {
        Ok(variable.typevar(self.context.db()).name(self.context.db()))
    }

    fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        parent: FileScopeId,
    ) -> Result<AncestorsIter<'index>, Infallible> {
        Ok(index.ancestor_scopes(parent))
    }

    fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> Result<Option<(FileScopeId, &'index Scope)>, Infallible> {
        Ok(ancestors.next())
    }

    fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(GenericContext::of_node(
            self.context.db(),
            scope.node(),
            index,
        ))
    }

    fn find_named(
        &self,
        context: GenericContext<'db>,
        name: &Name,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        find_named_typevar_sync(context, name, self)
    }

    fn report(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        variable: BoundTypeVarInstance<'db>,
        other: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        let db = self.context.db();
        report_shadowed_type_variable(
            self.context,
            variable.typevar(db).name(db),
            "class",
            &class_node.name.id,
            class.header_range(db),
            variable.kind(db),
            other,
        );
        Ok(())
    }
}
