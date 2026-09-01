//! Definition-resolution support for IDE refactors.

#![allow(
    dead_code,
    reason = "source-backed definition resolution is retained for IDE consumers"
)]

use ty_module_resolver::Module;

use crate::SemanticModel;
use crate::place::definitions::{DefinitionResolution, definitions_for_module_global};

use super::user_visible_definitions;

impl<'db> SemanticModel<'db> {
    /// Resolves the definitions for an explicit module global.
    pub fn definitions_for_module_global(
        &self,
        module: Module<'db>,
        name: &str,
    ) -> Option<DefinitionResolution<'db>> {
        definitions_for_module_global(self.db(), self.program(), module, name)
            .map(|resolution| source_backed_resolution(self.db(), resolution))
    }
}

pub(super) fn source_backed_resolution<'db>(
    db: &'db dyn crate::Db,
    resolution: DefinitionResolution<'db>,
) -> DefinitionResolution<'db> {
    resolution.project_definitions(|definition| user_visible_definitions(db, [definition]))
}
