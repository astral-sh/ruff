//! Checks variables referenced by class bases against enclosing bindings of the same identity.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::OrdinaryClassGenericCheckEffects;
use super::own_shadowing::SynchronousClassOwnShadowEffects;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassBaseShadowEffects)]
    pub(in crate::types::infer::builder) trait ClassBaseShadowEffects<'db> {
        type Error;

        #[operation(source)]
        async fn typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn ancestors<'index>(&self, index: &'index SemanticIndex<'db>, parent: FileScopeId) -> Result<AncestorsIter<'index>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(&self, ancestors: &mut AncestorsIter<'index>) -> Result<Option<(FileScopeId, &'index Scope)>, Self::Error>;
        #[operation(child)]
        async fn scope_context(&self, index: &SemanticIndex<'db>, scope: &Scope) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn find_identity(&self, context: GenericContext<'db>, typevar: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn report(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, variable: BoundTypeVarInstance<'db>, other: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
    }

    /// Reports the first binding with the base variable's identity in every enclosing context.
    /// A match does not end the ancestor walk, so each enclosing definition receives its report.
    #[synchronous(check_class_base_shadowing_sync)]
    #[capabilities(effects = ClassBaseShadowEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_class_base_shadowing_with<'db, E: ClassBaseShadowEffects<'db>>(
        index: &SemanticIndex<'db>,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        variable: BoundTypeVarInstance<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let typevar = effects.typevar(variable).await?;
        let mut ancestors = effects.ancestors(index, parent).await?;
        #[cursor_loop]
        while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
            let (_, scope) = ancestor;
            let Some(context) = effects.scope_context(index, scope).await? else {
                continue;
            };
            if let Some(other) = effects.find_identity(context, typevar).await? {
                effects.report(class, class_node, variable, other).await?;
            }
        }
        Ok(())
    }
}

impl<'db> SynchronousClassBaseShadowEffects<'db>
    for OrdinaryClassGenericCheckEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn typevar(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(variable.typevar(self.context.db()))
    }

    fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        parent: FileScopeId,
    ) -> Result<AncestorsIter<'index>, Infallible> {
        SynchronousClassOwnShadowEffects::ancestors(self, index, parent)
    }

    fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> Result<Option<(FileScopeId, &'index Scope)>, Infallible> {
        SynchronousClassOwnShadowEffects::next_ancestor(self, ancestors)
    }

    fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        SynchronousClassOwnShadowEffects::scope_context(self, index, scope)
    }

    fn find_identity(
        &self,
        context: GenericContext<'db>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(context.binds_typevar(self.context.db(), typevar))
    }

    fn report(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        variable: BoundTypeVarInstance<'db>,
        other: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        SynchronousClassOwnShadowEffects::report(self, class, class_node, variable, other)
    }
}
