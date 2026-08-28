//! Direct dependencies and module ownership supplied by a package manager.

use std::collections::{BTreeMap, BTreeSet};

use compact_str::CompactString;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, editable_search_paths, file_to_module, resolve_real_module,
};
use ty_python_core::ProgramFile;

use crate::Db;

/// Returns the missing dependency for this import, if one can be identified.
///
/// Cache the diagnostic details so metadata changes do not invalidate import inference when
/// the result for this importing file and module is unchanged.
#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn missing_direct_dependency<'db>(
    db: &'db dyn Db,
    importing_file: ProgramFile<'db>,
    imported_module: Module<'db>,
) -> Option<MissingDependency> {
    let metadata = db.dependency_metadata(importing_file.file(db))?;
    metadata.missing_dependency(db, importing_file, imported_module)
}

/// The dependency information needed to check imports, without source ranges or lockfile details.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyMetadata {
    pub projects: Box<[DependencyProject]>,
    /// Installable packages, keyed by opaque package-manager IDs rather than names.
    /// A distribution can provide several Python modules; its name need not match their import names.
    /// IDs distinguish distributions with the same name but different sources.
    pub distributions: BTreeMap<CompactString, DependencyDistribution>,
    /// Maps module names to the IDs in [`Self::distributions`] that provide those modules.
    /// A module can have several owners, for example when distributions share a namespace package.
    pub module_owners: BTreeMap<ModuleName, Box<[CompactString]>>,
}

impl DependencyMetadata {
    /// Check whether `importing_file` is allowed to import `imported_module`.
    ///
    /// Use the nearest containing project's declarations. Imports of its own distribution
    /// and its runtime or optional dependencies are allowed. Dependency groups are also
    /// allowed for files not identified as package code.
    ///
    /// Return the missing dependency, or `None` if the import is allowed or its project or
    /// owning distribution cannot be identified.
    fn missing_dependency<'db>(
        &self,
        db: &'db dyn Db,
        importing_file: ProgramFile<'db>,
        imported_module: Module<'db>,
    ) -> Option<MissingDependency> {
        let path = importing_file.file(db).path(db).as_system_path()?;
        let project = self
            .projects
            .iter()
            .filter(|project| path.starts_with(&project.path))
            .max_by_key(|project| project.path.as_str().len())?;

        // Stubs can belong to a different distribution, so prefer the runtime module.
        // Fall back to the resolved stub for native modules that ty cannot resolve at runtime.
        let runtime_module = resolve_real_module(
            db,
            ImportingFile::File(
                importing_file.file(db),
                importing_file.resolver_environment(db),
            ),
            imported_module.name(db),
        )
        .unwrap_or(imported_module);
        let id = self.owner(db, runtime_module)?;

        // Runtime and optional declarations take precedence when a dependency is also in a group.
        if project.distribution.as_ref() == Some(id) || project.dependencies.contains(id) {
            return None;
        }

        let group_dependency = project.group_dependencies.contains(id);
        if group_dependency && !self.is_package_file(db, importing_file, project) {
            return None;
        }

        Some(MissingDependency {
            distribution_name: self.distributions.get(id)?.name.clone(),
            group_dependency,
        })
    }

    fn owner<'db>(&self, db: &'db dyn Db, module: Module<'db>) -> Option<&CompactString> {
        // A namespace can also contain local modules that the package manager doesn't know about.
        // Only attribute concrete modules; inference checks the children of `from ns import x`.
        let search_path = module.search_path(db)?;
        if search_path.is_standard_library() {
            // ty bundles typing_extensions stubs, but the runtime module is third-party.
            if module.name(db).first_component() != "typing_extensions" {
                return None;
            }
        } else if !search_path.is_site_packages() {
            if let Some(path) = module
                .file(db)
                .and_then(|file| file.path(db).as_system_path())
                && let Some(owner) = self.editable_owner(path)
            {
                return Some(owner);
            }

            // A local module can shadow an installed distribution with the same import name.
            // Its name alone is not evidence that the import uses that distribution.
            if !search_path.is_editable() {
                return None;
            }
        }

        self.module_owner(module.name(db))
    }

    fn module_owner(&self, module: &ModuleName) -> Option<&CompactString> {
        let owners = module
            .ancestors()
            .find_map(|name| self.module_owners.get(&name))?;

        // In particular, importing a namespace shared by several distributions doesn't establish
        // which of them is required. A more specific submodule may have an unambiguous owner.
        match owners.as_ref() {
            [owner] => Some(owner),
            _ => None,
        }
    }

    fn editable_owner(&self, path: &SystemPath) -> Option<&CompactString> {
        let mut owner = None;
        let mut longest_root = 0;

        for (id, distribution) in &self.distributions {
            let Some(root) = &distribution.editable_path else {
                continue;
            };
            if !path.starts_with(root) {
                continue;
            }

            match root.as_str().len().cmp(&longest_root) {
                std::cmp::Ordering::Greater => {
                    longest_root = root.as_str().len();
                    owner = Some(id);
                }
                std::cmp::Ordering::Equal => owner = None,
                std::cmp::Ordering::Less => {}
            }
        }

        owner
    }

    fn is_package_file(
        &self,
        db: &dyn Db,
        file: ProgramFile<'_>,
        project: &DependencyProject,
    ) -> bool {
        let Some(id) = &project.distribution else {
            return false;
        };

        if let Some(module) = file_to_module(db, file.resolver_file(db))
            && self.module_owner(module.name(db)) == Some(id)
        {
            return true;
        }

        // Editable installs may only record a .pth file, not their Python modules. Use the
        // resolver's editable roots before deduplication against first-party search paths. A root
        // narrower than the project directory separates package code from sibling tests/scripts.
        // A flat install exposing the whole project does not establish that distinction.
        let Some(root) = self
            .distributions
            .get(id)
            .and_then(|distribution| distribution.editable_path.as_ref())
        else {
            return false;
        };
        let Some(path) = file.file(db).path(db).as_system_path() else {
            return false;
        };

        editable_search_paths(db, file.resolver_environment(db)).any(|search_root| {
            search_root != root.as_path()
                && search_root.starts_with(root)
                && path.starts_with(search_root)
        })
    }
}

/// The direct dependency declarations of a workspace member or a virtual workspace root.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyProject {
    pub path: SystemPathBuf,
    pub distribution: Option<CompactString>,
    pub dependencies: BTreeSet<CompactString>,
    pub group_dependencies: BTreeSet<CompactString>,
}

/// A distribution's display name and, for editable installs, its source directory.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyDistribution {
    pub name: CompactString,
    pub editable_path: Option<SystemPathBuf>,
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct MissingDependency {
    pub(crate) distribution_name: CompactString,
    pub(crate) group_dependency: bool,
}
