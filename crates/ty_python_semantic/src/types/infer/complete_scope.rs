use std::convert::Infallible;

use ty_python_core::scope::ScopeId;
use ty_python_core::semantic_index;

use super::{InferScope, ScopeInference, infer_scope_types_impl};
use crate::Db;
use crate::types::TypeContext;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousCompleteScopeEffects)]
    pub(in crate::types) trait CompleteScopeEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn next_scope(&self, pending: &mut Option<ScopeId<'db>>) -> Result<Option<ScopeId<'db>>, Self::Error>;
        #[operation(child)]
        async fn accepts_type_context(&self, scope: ScopeId<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn parent_scope(&self, scope: ScopeId<'db>) -> Result<Option<ScopeId<'db>>, Self::Error>;
        #[operation(child)]
        async fn infer_scope(&self, scope: ScopeId<'db>) -> Result<&'db ScopeInference<'db>, Self::Error>;
    }

    #[synchronous(complete_scope_sync)]
    #[capabilities(effects = CompleteScopeEffects)]
    #[passive_values()]
    pub(in crate::types) async fn complete_scope_with<'db, E: CompleteScopeEffects<'db>>(
        scope: ScopeId<'db>,
        effects: &E,
    ) -> Result<&'db ScopeInference<'db>, E::Error> {
        #[passive_state]
        let mut pending = Some(scope);
        #[passive_state]
        let mut selected = scope;
        #[cursor_loop]
        while let Some(current) = effects.next_scope(&mut pending).await? {
            selected = current;
            // Scopes that may require type context are inferred during the inference of
            // their outer scope.
            if effects.accepts_type_context(current).await? {
                // Note that nested lambdas or comprehensions may require recursing until we reach
                // an outer scope that is independent of any type context.
                pending = effects.parent_scope(current).await?;
            }
        }
        effects.infer_scope(selected).await
    }
}

pub(super) struct OrdinaryCompleteScopeEffects<'db>(pub(super) &'db dyn Db);

impl<'db> SynchronousCompleteScopeEffects<'db> for OrdinaryCompleteScopeEffects<'db> {
    type Error = Infallible;

    fn next_scope(
        &self,
        pending: &mut Option<ScopeId<'db>>,
    ) -> Result<Option<ScopeId<'db>>, Infallible> {
        Ok(pending.take())
    }

    fn accepts_type_context(&self, scope: ScopeId<'db>) -> Result<bool, Infallible> {
        Ok(scope.accepts_type_context(self.0))
    }

    fn parent_scope(&self, scope: ScopeId<'db>) -> Result<Option<ScopeId<'db>>, Infallible> {
        let program_file = scope.program_file(self.0);
        let index = semantic_index(self.0, program_file);
        Ok(index
            .parent_scope_id(scope.file_scope_id(self.0))
            .map(|parent| parent.to_scope_id(self.0, program_file)))
    }

    fn infer_scope(&self, scope: ScopeId<'db>) -> Result<&'db ScopeInference<'db>, Infallible> {
        Ok(infer_scope_types_impl(
            self.0,
            InferScope::new(self.0, scope, TypeContext::default()),
        ))
    }
}
