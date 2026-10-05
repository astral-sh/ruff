//! Admitted base shadowing compares identities in canonical enclosing contexts.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::ClassCheckEffects;
use crate::types::generics::binding::{BindingFacts, find_in_context_with};
use crate::types::infer::builder::post_inference::static_class::generic_checks::base_shadowing::{
    ClassBaseShadowEffects, check_class_base_shadowing_with,
};
use crate::types::infer::builder::post_inference::static_class::generic_checks::default_references::ClassDefaultReferenceEffects;
use crate::types::infer::builder::post_inference::static_class::generic_checks::own_shadowing::ClassOwnShadowEffects;
use crate::types::infer::builder::source_definition::controlled::SourceAccess;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_generic_validation::{
    self as observations, ValidationStage,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Checks one collected base variable against every enclosing generic context by identity.
    pub(super) async fn check_class_base_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let bytes = size_of::<
            [(
                &SemanticIndex<'db>,
                StaticClassLiteral<'db>,
                &ast::StmtClassDef,
                FileScopeId,
                BoundTypeVarInstance<'db>,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((22, bytes)), || {
                check_class_base_shadowing_with(
                    self.builder.index,
                    class,
                    class_node,
                    parent,
                    variable,
                    self,
                )
            })
            .await?
            .await?;
        #[cfg(test)]
        observations::validation_completed(
            self.builder.context.file(),
            class,
            ValidationStage::BaseShadowing,
            &self.builder.context.retained_diagnostics(),
        );
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassBaseShadowEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn typevar(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarInstance<'db>> {
        ClassDefaultReferenceEffects::bound_typevar(self, variable).await
    }

    async fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        parent: FileScopeId,
    ) -> RunResult<AncestorsIter<'index>> {
        ClassOwnShadowEffects::ancestors(self, index, parent).await
    }

    async fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> RunResult<Option<(FileScopeId, &'index Scope)>> {
        ClassOwnShadowEffects::next_ancestor(self, ancestors).await
    }

    async fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> RunResult<Option<GenericContext<'db>>> {
        ClassOwnShadowEffects::scope_context(self, index, scope).await
    }

    async fn find_identity(
        &self,
        context: GenericContext<'db>,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let bytes = size_of::<[(GenericContext<'db>, TypeVarInstance<'db>, BindingFacts, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((20, bytes)), || {
                find_in_context_with(context, typevar, BindingFacts, self.source)
            })
            .await?
            .await
    }

    async fn report(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        variable: BoundTypeVarInstance<'db>,
        other: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let bytes = size_of::<
            [(
                StaticClassLiteral<'db>,
                &ast::StmtClassDef,
                BoundTypeVarInstance<'db>,
                BoundTypeVarInstance<'db>,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((19, bytes)), || {
                self.report_typevar_shadow(
                    class,
                    class_node,
                    variable,
                    other,
                    #[cfg(test)]
                    ValidationStage::BaseShadowing,
                )
            })
            .await?
            .await
    }
}
