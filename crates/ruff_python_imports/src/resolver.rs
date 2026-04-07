use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::SystemPath;
use ty_module_resolver::{
    Module, ModuleName, resolve_module, resolve_module_confident, resolve_real_module,
    resolve_real_module_confident,
};

use crate::collector::CollectedImport;
use crate::db::ImportDb;
use crate::{
    ImportKind, RawImportOccurrence as ImportOccurrence, RawResolvedImport as ResolvedImport,
};

/// Resolve imports for a given Python file.
pub(crate) struct Resolver<'a> {
    db: &'a ImportDb,
    file: Option<File>,
}

impl<'a> Resolver<'a> {
    /// Initialize a [`Resolver`] with a given [`ImportDb`].
    pub(crate) fn new(db: &'a ImportDb, path: &SystemPath) -> Self {
        let file = system_path_to_file(db, path).ok();
        Self { db, file }
    }

    pub(crate) fn resolve_all(
        &self,
        imports: impl IntoIterator<Item = CollectedImport>,
    ) -> Vec<ResolvedImport> {
        let mut resolved = Vec::new();
        for import in imports {
            resolved.extend(self.resolve(import));
        }
        resolved
    }

    fn resolve(&self, import: CollectedImport) -> Vec<ResolvedImport> {
        let CollectedImport { occurrence } = import;
        let mut resolved = match occurrence.kind {
            ImportKind::Import => self.resolve_import(&occurrence),
            ImportKind::ImportFrom => self.resolve_import_from(&occurrence),
            ImportKind::StringImport { min_dots } => {
                self.resolve_string_import(&occurrence, min_dots)
            }
        };

        if resolved.is_empty() {
            resolved.push(ResolvedImport {
                occurrence,
                resolved_module: None,
                resolved_path: None,
                winning_root: None,
            });
        }

        resolved
    }

    fn resolve_import(&self, occurrence: &ImportOccurrence) -> Vec<ResolvedImport> {
        let mut resolved = Vec::new();

        if let Some(module) = self.resolve_module(&occurrence.requested) {
            resolved.push(self.resolved_import(occurrence, module));

            if self
                .resolved_path(&module)
                .is_some_and(|path| path.extension() == Some("pyi"))
                && let Some(source_module) = self.resolve_real_module(&occurrence.requested)
            {
                resolved.push(self.resolved_import(occurrence, source_module));
            }
        }

        resolved
    }

    fn resolve_import_from(&self, occurrence: &ImportOccurrence) -> Vec<ResolvedImport> {
        let mut resolved = Vec::new();

        if let Some(module) = self.resolve_module(&occurrence.requested) {
            let resolved_path = self.resolved_path(&module);
            resolved.push(self.resolved_import(occurrence, module));

            if resolved_path.is_some_and(|path| path.extension() == Some("pyi"))
                && let Some(source_module) = self.resolve_real_module(&occurrence.requested)
            {
                resolved.push(self.resolved_import(occurrence, source_module));
            }

            return resolved;
        }

        if let Some(parent) = occurrence.requested.parent()
            && let Some(module) = self.resolve_module(&parent)
        {
            let resolved_path = self.resolved_path(&module);
            resolved.push(self.resolved_import_with_module(occurrence, module, parent.clone()));

            if resolved_path.is_some_and(|path| path.extension() == Some("pyi"))
                && let Some(source_module) = self.resolve_real_module(&parent)
            {
                resolved.push(self.resolved_import_with_module(occurrence, source_module, parent));
            }
        }

        resolved
    }

    fn resolve_string_import(
        &self,
        occurrence: &ImportOccurrence,
        min_dots: usize,
    ) -> Vec<ResolvedImport> {
        let count = occurrence.requested.components().count();
        for name in occurrence
            .requested
            .ancestors()
            .take(count.saturating_sub(min_dots))
        {
            if let Some(module) = self.resolve_module(&name) {
                let mut resolved =
                    vec![self.resolved_import_with_module(occurrence, module, name.clone())];

                if self
                    .resolved_path(&module)
                    .is_some_and(|path| path.extension() == Some("pyi"))
                    && let Some(source_module) = self.resolve_real_module(&name)
                {
                    resolved.push(self.resolved_import_with_module(
                        occurrence,
                        source_module,
                        name,
                    ));
                }

                return resolved;
            }
        }

        Vec::new()
    }

    fn resolved_import(&self, occurrence: &ImportOccurrence, module: Module<'a>) -> ResolvedImport {
        self.resolved_import_with_module(occurrence, module, module.name(self.db).clone())
    }

    fn resolved_import_with_module(
        &self,
        occurrence: &ImportOccurrence,
        module: Module<'a>,
        resolved_module: ModuleName,
    ) -> ResolvedImport {
        let winning_root = module
            .search_path(self.db)
            .and_then(|search_path| self.db.winning_root_index(search_path));

        ResolvedImport {
            occurrence: occurrence.clone(),
            resolved_module: Some(resolved_module),
            resolved_path: self.resolved_path(&module),
            winning_root,
        }
    }

    fn resolved_path(&self, module: &Module<'a>) -> Option<ruff_db::system::SystemPathBuf> {
        module
            .file(self.db)?
            .path(self.db)
            .as_system_path()
            .map(SystemPath::to_path_buf)
    }

    fn resolve_module(&self, module_name: &ModuleName) -> Option<Module<'a>> {
        if let Some(file) = self.file {
            resolve_module(self.db, file, module_name)
        } else {
            resolve_module_confident(self.db, module_name)
        }
    }

    fn resolve_real_module(&self, module_name: &ModuleName) -> Option<Module<'a>> {
        if let Some(file) = self.file {
            resolve_real_module(self.db, file, module_name)
        } else {
            resolve_real_module_confident(self.db, module_name)
        }
    }
}
