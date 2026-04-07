use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::SystemPath;
use ty_module_resolver::{Module, ModuleName, resolve_module, resolve_module_confident};

use crate::collector::CollectedImport;
use crate::{ImportDb, ResolvedImport, ResolvedPathKind};

/// Resolve collected imports for a given Python file.
pub(crate) struct Resolver<'a> {
    db: &'a ImportDb,
    file: Option<File>,
}

impl<'a> Resolver<'a> {
    /// Initialize a [`Resolver`] with a given [`ImportDb`].
    pub(crate) fn new(db: &'a ImportDb, path: &SystemPath) -> Self {
        // If we know the importing file we can potentially resolve more imports.
        let file = system_path_to_file(db, path).ok();
        Self { db, file }
    }

    /// Resolve a single collected import occurrence.
    pub(crate) fn resolve(&self, import: CollectedImport) -> ResolvedImport {
        let occurrence = import.occurrence;
        match occurrence.kind {
            crate::ImportKind::Import => self
                .resolve_module(&occurrence.requested)
                .map(|module| self.to_resolved_import(module, occurrence.clone()))
                .unwrap_or_else(|| ResolvedImport::unresolved(occurrence)),
            crate::ImportKind::ImportFrom => self
                .resolve_module(&occurrence.requested)
                .or_else(|| {
                    occurrence
                        .requested
                        .parent()
                        .as_ref()
                        .and_then(|parent| self.resolve_module(parent))
                })
                .map(|module| self.to_resolved_import(module, occurrence.clone()))
                .unwrap_or_else(|| ResolvedImport::unresolved(occurrence)),
            crate::ImportKind::StringImport => {
                let min_dots = import.string_import_min_dots.unwrap_or_default();
                let requested = occurrence.requested.clone();
                let count = requested.components().count();
                requested
                    .ancestors()
                    .take(count.saturating_sub(min_dots))
                    .find_map(|name| self.resolve_module(&name))
                    .map(|module| self.to_resolved_import(module, occurrence.clone()))
                    .unwrap_or_else(|| ResolvedImport::unresolved(occurrence))
            }
        }
    }

    fn resolve_module(&self, module_name: &ModuleName) -> Option<Module<'a>> {
        if let Some(file) = self.file {
            resolve_module(self.db, file, module_name)
        } else {
            resolve_module_confident(self.db, module_name)
        }
    }

    fn to_resolved_import(
        &self,
        module: Module<'a>,
        occurrence: crate::ImportOccurrence,
    ) -> ResolvedImport {
        let resolved_module = Some(module.name(self.db).clone());
        let resolved_path = module
            .file(self.db)
            .and_then(|file| file.path(self.db).as_system_path())
            .map(SystemPath::to_path_buf);
        let (winning_root, resolved_path_kind) =
            module
                .search_path(self.db)
                .map_or((None, None), |search_path| {
                    let kind = if search_path.is_standard_library() {
                        ResolvedPathKind::StandardLibrary
                    } else if search_path.is_site_packages() {
                        ResolvedPathKind::SitePackages
                    } else if search_path.is_first_party() {
                        ResolvedPathKind::FirstParty
                    } else {
                        ResolvedPathKind::Unknown
                    };
                    (self.db.winning_root_index(search_path), Some(kind))
                });

        ResolvedImport {
            occurrence,
            resolved_module,
            resolved_path,
            winning_root,
            resolved_path_kind,
        }
    }
}
