//! Admitted class shadowing uses canonical enclosing contexts and the shared name lookup.

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::ClassCheckEffects;
use super::scan_cost::binding_node_quote;
use crate::types::generics::binding::{BindingFacts, TypeVarBindingEffects};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::shadowing::{TypeVariableNameEffects, find_named_typevar_with};
use crate::types::infer::builder::post_inference::static_class::generic_checks::default_references::{
    ClassDefaultReferenceEffects, VariableCursor, VariableRange,
};
use crate::types::infer::builder::post_inference::static_class::generic_checks::own_shadowing::{
    ClassOwnShadowEffects, check_class_own_shadowing_with,
};
use crate::types::infer::builder::source_definition::controlled::class_selection::FixedFieldBorrow;
use crate::types::infer::builder::source_definition::controlled::SourceAccess;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_generic_validation::{self as observations, ValidationStage};
use crate::types::local_transfer::generated_field_quote;
use crate::types::local_transfer::names::{name_comparison_preparation_quote, name_comparison_quote};
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::typevar::TypeVarIdentity;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    /// Checks every own variable against all enclosing contexts before proceeding to base shadowing.
    pub(super) async fn check_class_own_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        let bytes = size_of::<
            [(
                &SemanticIndex<'db>,
                StaticClassLiteral<'db>,
                &ast::StmtClassDef,
                FileScopeId,
                GenericContext<'db>,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((22, bytes)), || {
                check_class_own_shadowing_with(
                    self.builder.index,
                    class,
                    class_node,
                    parent,
                    context,
                    self,
                )
            })
            .await?
            .await?;
        #[cfg(test)]
        observations::validation_completed(
            self.builder.context.file(),
            class,
            ValidationStage::OwnShadowing,
            &self.builder.context.retained_diagnostics(),
        );
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassOwnShadowEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        ClassDefaultReferenceEffects::variables(self, context).await
    }

    async fn cursor<'a>(
        &self,
        variables: &'a ContextVariables<'db>,
    ) -> RunResult<VariableCursor<'a, 'db>> {
        ClassDefaultReferenceEffects::cursor(self, variables, VariableRange::All).await
    }

    async fn next_variable(
        &self,
        cursor: &mut VariableCursor<'_, 'db>,
    ) -> RunResult<Option<(usize, BoundTypeVarInstance<'db>)>> {
        ClassDefaultReferenceEffects::next_variable(self, cursor).await
    }

    async fn name(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<&'db Name> {
        let typevar = ClassDefaultReferenceEffects::bound_typevar(self, variable).await?;
        let bytes = size_of::<[(crate::types::typevar::TypeVarInstance<'db>, &Self); 2]>();
        let identity = self
            .source
            .boxed_future_with_fixed_transfers(Ok((10, bytes)), || {
                TypeVarBindingEffects::typevar_identity(self.source, typevar)
            })
            .await?
            .await?;
        // `name` is a direct interned borrowed field; its generated request has the same
        // construction shape as the borrowed context-variables field, without a thin wrapper.
        let quote = generated_field_quote(
            |identity: TypeVarIdentity<'db>, fields| identity.field_requests(fields),
            |identity: TypeVarIdentity<'db>, fields| identity.field_requests(fields).name(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = identity
                    .field_requests(endpoint.field_request_context())
                    .name();
                endpoint.read_field(request, &FixedFieldBorrow)
            })
            .await?;
        Ok(read.await)
    }

    async fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        parent: FileScopeId,
    ) -> RunResult<AncestorsIter<'index>> {
        let bytes = size_of::<[(&SemanticIndex<'db>, FileScopeId, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((13, bytes)), || {
                TypeVarBindingEffects::ancestors(self.source, index, parent)
            })
            .await?
            .await
    }

    async fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> RunResult<Option<(FileScopeId, &'index Scope)>> {
        // The underlying step pays scope indexing and parent selection; this wrapper pays
        // the shared scan's destructuring, context branch, report branch and back edge.
        let bytes = size_of::<[(Option<(FileScopeId, &Scope)>, Option<GenericContext<'db>>); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((16, bytes)), || {
                TypeVarBindingEffects::next_ancestor(self.source, ancestors)
            })
            .await?
            .await
    }

    async fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let (work, bytes) = const { binding_node_quote() }?;
        let node = self
            .source
            .local_with_fixed_transfers(work, bytes, || BindingFacts.node(scope))
            .await?;
        let bytes = size_of::<
            [(
                &SemanticIndex<'db>,
                crate::types::generics::binding::BindingNode,
                ReturnCallableTypeVarScope,
                &Self,
            ); 2],
        >();
        self.source
            .boxed_future_with_fixed_transfers(Ok((17, bytes)), || {
                TypeVarBindingEffects::scope_context(
                    self.source,
                    index,
                    node,
                    ReturnCallableTypeVarScope::Public,
                )
            })
            .await?
            .await
    }

    async fn find_named(
        &self,
        context: GenericContext<'db>,
        name: &Name,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let bytes = size_of::<[(GenericContext<'db>, &Name, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((15, bytes)), || {
                find_named_typevar_with(context, name, self)
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
                    ValidationStage::OwnShadowing,
                )
            })
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVariableNameEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        ClassDefaultReferenceEffects::variables(self, context).await
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let bytes = size_of::<[(&ContextVariables<'db>, &mut usize, &Self); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((13, bytes)), || {
                TypeVarBindingEffects::next_variable(self.source, variables, cursor)
            })
            .await?
            .await
    }

    async fn has_name(&self, variable: BoundTypeVarInstance<'db>, name: &Name) -> RunResult<bool> {
        let candidate = ClassOwnShadowEffects::name(self, variable).await?;
        let (work, bytes) = const { name_comparison_preparation_quote() }?;
        let (work, bytes) = self
            .source
            .local_with_fixed_transfers(work, bytes, || {
                name_comparison_quote(candidate.as_str().len().min(name.as_str().len()))
            })
            .await??;
        self.source
            .local_with_fixed_transfers(work, bytes, || candidate == name)
            .await
    }
}
