//! Select the nearest original static class by walking ordinary ancestor scopes, including the supplied scope.

use std::convert::Infallible;

use ty_python_core::definition::{Definition, DefinitionNodeKey};
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use crate::Db;
use crate::types::class::{ClassLiteral, StaticClassLiteral};
use crate::types::infer::original_class_type;

/// Resolves ancestor class definitions through ordinary canonical inference.
pub(super) struct InlineEnclosingClassEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl std::fmt::Debug for InlineEnclosingClassEffects<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InlineEnclosingClassEffects")
            .finish_non_exhaustive()
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies ordered lexical traversal and undecorated class identities to both inference modes.
    #[synchronous(SynchronousEnclosingClassEffects)]
    pub(super) trait EnclosingClassEffects<'db> {
        type Error;
        #[operation(source)]
        async fn ancestors<'index>(&self, index: &'index SemanticIndex<'db>, scope: FileScopeId) -> Result<AncestorsIter<'index>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(&self, ancestors: &mut AncestorsIter<'index>) -> Result<Option<(FileScopeId, &'index Scope)>, Self::Error>;
        #[operation(source)]
        async fn class_key(&self, scope: &Scope) -> Result<Option<DefinitionNodeKey>, Self::Error>;
        #[operation(source)]
        async fn definition(&self, index: &SemanticIndex<'db>, key: DefinitionNodeKey) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn original_class(&self, definition: Definition<'db>) -> Result<Option<ClassLiteral<'db>>, Self::Error>;
    }

    /// Returns the nearest original static class, including the supplied scope and crossing
    /// function and type-parameter scopes. Missing or non-static classes do not hide an outer class.
    #[synchronous(nearest_enclosing_class_sync)]
    #[capabilities(effects = EnclosingClassEffects)]
    #[passive_values()]
    pub(super) async fn nearest_enclosing_class_with<'db, E: EnclosingClassEffects<'db>>(
        index: &SemanticIndex<'db>, scope: FileScopeId, effects: &E,
    ) -> Result<Option<StaticClassLiteral<'db>>, E::Error> {
        let mut ancestors = effects.ancestors(index, scope).await?;
        #[cursor_loop]
        while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
            let (_, ancestor) = ancestor;
            let Some(key) = effects.class_key(ancestor).await? else { continue; };
            let definition = effects.definition(index, key).await?;
            if let Some(ClassLiteral::Static(class)) = effects.original_class(definition).await? {
                return Ok(Some(class));
            }
        }
        Ok(None)
    }
}

impl<'db> SynchronousEnclosingClassEffects<'db> for InlineEnclosingClassEffects<'db> {
    type Error = Infallible;

    fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> Result<AncestorsIter<'index>, Infallible> {
        Ok(index.ancestor_scopes(scope))
    }

    fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> Result<Option<(FileScopeId, &'index Scope)>, Infallible> {
        Ok(ancestors.next())
    }

    fn class_key(&self, scope: &Scope) -> Result<Option<DefinitionNodeKey>, Infallible> {
        Ok(scope.node().as_class().map(Into::into))
    }

    fn definition(
        &self,
        index: &SemanticIndex<'db>,
        key: DefinitionNodeKey,
    ) -> Result<Definition<'db>, Infallible> {
        Ok(index.expect_single_definition(key))
    }

    fn original_class(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<ClassLiteral<'db>>, Infallible> {
        Ok(original_class_type(self.db, definition))
    }
}
