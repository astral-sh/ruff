//! Declaration-owned class contexts assembled from the shared PEP 695 header algorithm.

#[cfg(test)]
pub(in crate::types) mod observations;

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::{ProgramFile, semantic_index};

use crate::Db;
use crate::types::GenericContext;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Selects a class's parameter list and builds its context from canonical declarations.
    #[synchronous(SynchronousClassHeaderContextEffects)]
    pub(in crate::types) trait ClassHeaderContextEffects<'db, 'ast> {
        type Error;

        #[operation(local)]
        async fn parameters(&self, class: &'ast ast::StmtClassDef) -> Result<Option<&'ast ast::TypeParams>, Self::Error>;
        #[operation(child)]
        async fn context(&self, class: &'ast ast::StmtClassDef, parameters: &'ast ast::TypeParams) -> Result<GenericContext<'db>, Self::Error>;
    }

    /// Returns the context of a present parameter list, preserving an absent list as `None`.
    /// Completed declaration recovery may produce an empty context; it still remains `Some`.
    #[synchronous(class_header_context_sync)]
    #[capabilities(effects = ClassHeaderContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn class_header_context_with<'db, 'ast, E: ClassHeaderContextEffects<'db, 'ast>>(
        class: &'ast ast::StmtClassDef,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        let Some(parameters) = effects.parameters(class).await? else {
            return Ok(None);
        };
        Ok(Some(effects.context(class, parameters).await?))
    }
}

/// Resolves class and parameter definitions only when the class has a parameter list.
pub(in crate::types::class) struct InlineClassHeaderContext<'db> {
    db: &'db dyn Db,
    file: ProgramFile<'db>,
}

impl<'db> InlineClassHeaderContext<'db> {
    pub(in crate::types::class) const fn new(db: &'db dyn Db, file: ProgramFile<'db>) -> Self {
        Self { db, file }
    }
}

impl<'db, 'ast> SynchronousClassHeaderContextEffects<'db, 'ast> for InlineClassHeaderContext<'db> {
    type Error = Infallible;

    fn parameters(
        &self,
        class: &'ast ast::StmtClassDef,
    ) -> Result<Option<&'ast ast::TypeParams>, Infallible> {
        Ok(class.type_params.as_deref())
    }

    fn context(
        &self,
        class: &'ast ast::StmtClassDef,
        parameters: &'ast ast::TypeParams,
    ) -> Result<GenericContext<'db>, Infallible> {
        let index = semantic_index(self.db, self.file);
        let definition = index.expect_single_definition(class);
        Ok(GenericContext::from_type_params(
            self.db, index, definition, parameters,
        ))
    }
}
