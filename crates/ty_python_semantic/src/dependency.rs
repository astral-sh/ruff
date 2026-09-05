//! Direct dependencies and module ownership supplied by a package manager.

use std::collections::{BTreeMap, BTreeSet};

use compact_str::CompactString;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, editable_search_paths, file_to_module,
    resolve_real_shadowable_module,
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

/// Whether package-manager metadata identifies this module as a direct dependency available to
/// this file.
///
/// A distribution is an installable Python project: for example, the `typing-extensions`
/// distribution provides the `typing_extensions` module. The package manager supplies both the
/// project's dependency declarations and the distributions that own its importable modules.
/// This query uses that information to check whether the project declares the imported distribution
/// as a dependency. A project is not its own direct dependency, so this returns `false` when
/// `importing_file` and `imported_module` belong to the same distribution. Such imports are valid;
/// this query does not answer the broader question of whether an import is allowed.
///
/// It uses a standalone script's own declarations or the nearest containing project's declarations;
/// a parent project's dependencies do not apply to a nested project.
///
/// For example, consider a project with this `pyproject.toml`:
///
/// ```toml
/// [project]
/// name = "widget-tools"
/// dependencies = ["typing-extensions"]
///
/// [project.optional-dependencies]
/// images = ["Pillow"]
///
/// [dependency-groups]
/// test = ["pytest"]
/// ```
///
/// Assume the metadata identifies `src/widgets/` as package code belonging to `widget-tools`, and
/// identifies the distributions providing each imported module below. Runtime dependencies and
/// optional dependencies are both direct dependencies: the project explicitly declares them.
/// The `images` extra is an optional feature selected by installing `widget-tools[images]`. This
/// query recognizes its dependency declarations even when that extra is not enabled. A dependency
/// installed only because another distribution requires it is an indirect dependency, and does
/// not qualify:
///
/// ```python
/// # src/widgets/images.py
/// import typing_extensions  # true: declared in project.dependencies
/// from PIL import Image  # true: provided by the optional dependency Pillow
/// import urllib3  # false: no direct dependency declaration, even if installed
/// ```
///
/// Imports within the project's own distribution return `false`: they use the project itself,
/// rather than a declared dependency. The same applies when its tests or development scripts
/// import its package. Neither case requires a dependency declaration:
///
/// ```python
/// # src/widgets/images.py
/// from widgets import helpers  # false: both modules belong to widget-tools
///
/// # tests/test_images.py
/// from widgets import images  # false: importing the containing project's own distribution
/// ```
///
/// Dependency groups describe dependencies for working on the project, and are not installed for
/// users of its distribution. They therefore qualify only for files outside the project's package
/// code. A dependency also declared in `project.dependencies` or `project.optional-dependencies`
/// qualifies regardless of whether it appears in a group:
///
/// ```python
/// # tests/test_images.py
/// import pytest  # true: a test may use the project's test dependency group
///
/// # src/widgets/images.py
/// import pytest  # false: package code cannot rely on a dependency group
/// ```
///
/// A stub (`.pyi` file) describes a module's types without providing its runtime implementation.
/// When an import resolves to a stub, ownership follows the runtime module if it can be resolved.
/// For example, a project declaring `requests` may use the separate `types-requests` distribution
/// for type information:
///
/// ```python
/// import requests  # Check the declaration for requests, not types-requests.
/// ```
///
/// If runtime resolution fails, we use the resolved stub's ownership instead. If a runtime module
/// resolves but its owner is unknown or ambiguous, the stub's owner does not substitute for it.
///
/// Missing metadata or unknown or ambiguous module ownership also leads us to return `false`,
/// including for standard-library modules and local modules with no known owning distribution. A
/// `false` result does not by itself justify a missing-dependency diagnostic; see
/// [`missing_direct_dependency`]. Conversely, `true` establishes a direct dependency declaration,
/// but does not guarantee that the runtime module is installed or exports a particular member.
/// Callers proposing an import fix, for example, need to check those conditions separately.
#[salsa::tracked]
pub(crate) fn is_direct_dependency<'db>(
    db: &'db dyn Db,
    importing_file: ProgramFile<'db>,
    imported_module: Module<'db>,
) -> bool {
    let Some(metadata) = db.dependency_metadata(importing_file.file(db)) else {
        return false;
    };
    let Some(project) = metadata.project_for_file(db, importing_file) else {
        return false;
    };
    let Some(id) = metadata.import_owner(db, importing_file, imported_module) else {
        return false;
    };

    project.dependencies.contains(id)
        || (project.group_dependencies.contains(id)
            && !metadata.is_package_file(db, importing_file, project))
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
    /// Find a script's declarations or the nearest containing project's declarations.
    fn project_for_file(&self, db: &dyn Db, file: ProgramFile<'_>) -> Option<&DependencyProject> {
        let path = file.file(db).path(db).as_system_path()?;
        self.projects
            .iter()
            .filter(|project| match project.kind {
                DependencyProjectKind::Project => path.starts_with(&project.path),
                DependencyProjectKind::Script => path == project.path.as_path(),
            })
            .max_by_key(|project| project.path.as_str().len())
    }

    /// Check whether `importing_file` is allowed to import `imported_module`.
    ///
    /// Use the script's own declarations or the nearest containing project's declarations.
    /// Imports of its own distribution and its runtime or optional dependencies are allowed.
    /// Dependency groups are also allowed for files not identified as package code.
    ///
    /// Return the missing dependency, or `None` if the import is allowed or its project or
    /// owning distribution cannot be identified.
    fn missing_dependency<'db>(
        &self,
        db: &'db dyn Db,
        importing_file: ProgramFile<'db>,
        imported_module: Module<'db>,
    ) -> Option<MissingDependency> {
        let project = self.project_for_file(db, importing_file)?;
        let id = self.import_owner(db, importing_file, imported_module)?;

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
            project_kind: project.kind,
        })
    }

    /// Returns the distribution ID to use when checking the dependency required by an import.
    ///
    /// The returned string is an opaque package-manager ID from [`Self::distributions`], not a
    /// Python module name or a distribution's display name. Callers compare this ID with the
    /// importing project's dependency declarations.
    ///
    /// `imported_module` comes from type-checking resolution, which can prefer a stub package over
    /// the runtime implementation. For example, `import widgets` might resolve to
    /// `widgets-stubs/__init__.pyi`, installed by a separate stub distribution. The dependency
    /// needed at runtime is the distribution providing `widgets/__init__.py`, rather than the one
    /// providing those stubs. Resolve the same module name again, with stubs disabled and using
    /// `importing_file`'s Python environment, then look up that module's owner in the metadata.
    /// Allow runtime modules to shadow bundled backports such as `typing_extensions`.
    ///
    /// If runtime resolution fails, use `imported_module` for the ownership lookup. This allows
    /// dependency checks for native extension modules that ty can resolve only through their
    /// stubs. The fallback applies only to module resolution: if a runtime module is found but
    /// its owner cannot be identified unambiguously, return `None` rather than attributing the
    /// import to the stub distribution. For example, a local module shadowing an installed package
    /// does not establish a dependency on that package merely because their import names match.
    fn import_owner<'db>(
        &self,
        db: &'db dyn Db,
        importing_file: ProgramFile<'db>,
        imported_module: Module<'db>,
    ) -> Option<&CompactString> {
        let runtime_module = resolve_real_shadowable_module(
            db,
            ImportingFile::File(
                importing_file.file(db),
                importing_file.resolver_environment(db),
            ),
            imported_module.name(db),
        )
        .unwrap_or(imported_module);
        self.owner(db, runtime_module)
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

/// The direct dependency declarations of a workspace member, virtual workspace root, or script.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct DependencyProject {
    /// The project directory or the exact path of a standalone script.
    pub path: SystemPathBuf,
    pub kind: DependencyProjectKind,
    pub distribution: Option<CompactString>,
    pub dependencies: BTreeSet<CompactString>,
    pub group_dependencies: BTreeSet<CompactString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub enum DependencyProjectKind {
    Project,
    Script,
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
    pub(crate) project_kind: DependencyProjectKind,
}
