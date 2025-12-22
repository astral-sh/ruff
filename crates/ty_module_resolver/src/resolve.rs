/*!
This module principally provides several routines for resolving a particular module
name to a `Module`:

* [`file_to_module`][]: resolves the module `.<self>` (often as the first step in resolving `.`)
* [`resolve_module`][]: resolves an absolute module name

You may notice that we actually provide `resolve_(real)_(shadowable)_module_(confident)`.
You almost certainly just want [`resolve_module`][]. The other variations represent
restrictions to answer specific kinds of questions, usually to empower IDE features.

* The `real` variation disallows all stub files, including the vendored typeshed.
  This enables the goto-definition ("real") vs goto-declaration ("stub or real") distinction.

* The `confident` variation disallows "desperate resolution", which is a fallback
  mode where we start trying to use ancestor directories of the importing file
  as search-paths, but only if we failed to resolve it with the normal search-paths.
  This is mostly just a convenience for cases where we don't want to try to define
  the importing file (resolving a `KnownModule` and tests).

* The `shadowable` variation disables some guards that prevents third-party code
  from shadowing any vendored non-stdlib `KnownModule`. In particular `typing_extensions`,
  which we vendor and heavily assume the contents of (and so don't ever want to shadow).
  This enables checking if the user *actually* has `typing_extensions` installed,
  in which case it's ok to suggest it in features like auto-imports.

There is some awkwardness to the structure of the code to specifically enable caching
of queries, as module resolution happens a lot and involves a lot of disk access.

For implementors, see `import-resolution-diagram.svg` for a flow diagram that
specifies ty's implementation of Python's import resolution algorithm.
*/

use std::borrow::Cow;
use std::fmt;
use std::iter::FusedIterator;
use std::str::Split;

use compact_str::format_compact;
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_db::files::{File, FilePath, FileRootKind};
use ruff_db::system::{DirectoryEntry, System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::{
    self as ast, PySourceType, PythonVersion,
    visitor::{Visitor, walk_body},
};

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::{ModulePath, SearchPath, SystemOrVendoredPathRef};
use crate::typeshed::{TypeshedVersions, vendored_typeshed_versions};
use crate::{MisconfigurationMode, SearchPathSettings, SearchPathSettingsError};

/// Resolves a module name to a module.
pub fn resolve_module<'db>(
    db: &'db dyn Db,
    importing_file: File,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name = ModuleNameIngredient::new(db, module_name, ModuleResolveMode::StubsAllowed);

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file, interned_name))
}

/// Resolves a module name to a module, without desperate resolution available.
///
/// This is appropriate for resolving a `KnownModule`, or cases where for whatever reason
/// we don't have a well-defined importing file.
pub fn resolve_module_confident<'db>(
    db: &'db dyn Db,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name = ModuleNameIngredient::new(db, module_name, ModuleResolveMode::StubsAllowed);

    resolve_module_query(db, interned_name)
}

/// Resolves a module name to a module (stubs not allowed).
pub fn resolve_real_module<'db>(
    db: &'db dyn Db,
    importing_file: File,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name =
        ModuleNameIngredient::new(db, module_name, ModuleResolveMode::StubsNotAllowed);

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file, interned_name))
}

/// Resolves a module name to a module, without desperate resolution available (stubs not allowed).
///
/// This is appropriate for resolving a `KnownModule`, or cases where for whatever reason
/// we don't have a well-defined importing file.
pub fn resolve_real_module_confident<'db>(
    db: &'db dyn Db,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name =
        ModuleNameIngredient::new(db, module_name, ModuleResolveMode::StubsNotAllowed);

    resolve_module_query(db, interned_name)
}

/// Resolves a module name to a module (stubs not allowed, some shadowing is
/// allowed).
///
/// In particular, this allows `typing_extensions` to be shadowed by a
/// non-standard library module. This is useful in the context of the LSP
/// where we don't want to pretend as if these modules are always available at
/// runtime.
///
/// This should generally only be used within the context of the LSP. Using it
/// within ty proper risks being unable to resolve builtin modules since they
/// are involved in an import cycle with `builtins`.
pub fn resolve_real_shadowable_module<'db>(
    db: &'db dyn Db,
    importing_file: File,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::StubsNotAllowedSomeShadowingAllowed,
    );

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file, interned_name))
}

/// Which files should be visible when doing a module query
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, get_size2::GetSize)]
#[allow(clippy::enum_variant_names)]
pub enum ModuleResolveMode {
    /// Stubs are allowed to appear.
    ///
    /// This is the "normal" mode almost everything uses, as type checkers are in fact supposed
    /// to *prefer* stubs over the actual implementations.
    StubsAllowed,
    /// Stubs are not allowed to appear.
    ///
    /// This is the "goto definition" mode, where we need to ignore the typing spec and find actual
    /// implementations. When querying searchpaths this also notably replaces typeshed with
    /// the "real" stdlib.
    StubsNotAllowed,
    /// Like `StubsNotAllowed`, but permits some modules to be shadowed.
    ///
    /// In particular, this allows `typing_extensions` to be shadowed by a
    /// non-standard library module. This is useful in the context of the LSP
    /// where we don't want to pretend as if these modules are always available
    /// at runtime.
    StubsNotAllowedSomeShadowingAllowed,
}

#[salsa::interned(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub(crate) struct ModuleResolveModeIngredient<'db> {
    mode: ModuleResolveMode,
}

impl ModuleResolveMode {
    fn stubs_allowed(self) -> bool {
        matches!(self, Self::StubsAllowed)
    }

    /// Returns `true` if the module name refers to a standard library module
    /// which can't be shadowed by a first-party module.
    ///
    /// This includes "builtin" modules, which can never be shadowed at runtime
    /// either. Additionally, certain other modules that are involved in an
    /// import cycle with `builtins` (`types`, `typing_extensions`, etc.) are
    /// also considered non-shadowable, unless the module resolution mode
    /// specifically opts into allowing some of them to be shadowed. This
    /// latter set of modules cannot be allowed to be shadowed by first-party
    /// or "extra-path" modules in ty proper, or we risk panics in unexpected
    /// places due to being unable to resolve builtin symbols. This is similar
    /// behaviour to other type checkers such as mypy:
    /// <https://github.com/python/mypy/blob/3807423e9d98e678bf16b13ec8b4f909fe181908/mypy/build.py#L104-L117>
    pub(super) fn is_non_shadowable(self, minor_version: u8, module_name: &str) -> bool {
        // Builtin modules are never shadowable, no matter what.
        if ruff_python_stdlib::sys::is_builtin_module(minor_version, module_name) {
            return true;
        }
        // Similarly for `types`, which is always available at runtime.
        if module_name == "types" {
            return true;
        }

        // Otherwise, some modules should only be conditionally allowed
        // to be shadowed, depending on the module resolution mode.
        match self {
            ModuleResolveMode::StubsAllowed | ModuleResolveMode::StubsNotAllowed => {
                module_name == "typing_extensions"
            }
            ModuleResolveMode::StubsNotAllowedSomeShadowingAllowed => false,
        }
    }
}

/// Salsa query that resolves an interned [`ModuleNameIngredient`] to a module.
///
/// This query should not be called directly. Instead, use [`resolve_module`]. It only exists
/// because Salsa requires the module name to be an ingredient.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn resolve_module_query<'db>(
    db: &'db dyn Db,
    module_name: ModuleNameIngredient<'db>,
) -> Option<Module<'db>> {
    let name = module_name.name(db);
    let mode = module_name.mode(db);
    let _span = tracing::trace_span!("resolve_module", %name).entered();

    let Some(resolved) = resolve_name(db, name, mode) else {
        tracing::debug!("Module `{name}` not found in search paths");
        return None;
    };

    let module = match resolved {
        ResolvedName::FileModule(module) => {
            tracing::trace!(
                "Resolved module `{name}` to `{path}`",
                path = module.file.path(db)
            );
            Module::file_module(
                db,
                name.clone(),
                module.kind,
                module.search_path,
                module.file,
            )
        }
        ResolvedName::NamespacePackage => {
            tracing::trace!("Module `{name}` is a namespace package");
            Module::namespace_package(db, name.clone())
        }
    };

    Some(module)
}

/// Like `resolve_module_query` but for cases where it failed to resolve the module
/// and we are now Getting Desperate and willing to try the ancestor directories of
/// the `importing_file` as potential temporary search paths that are private
/// to this import.
///
/// The reason this is split out is because in 99.9% of cases `resolve_module_query`
/// will find the right answer (or no valid answer exists), and we want it to be
/// aggressively cached. Including the `importing_file` as part of that query would
/// trash the caching of import resolution between files.
///
/// TODO: should (some) of this also be cached? If an entire directory of python files
/// is misunderstood we'll end up in here a lot.
fn desperately_resolve_module<'db>(
    db: &'db dyn Db,
    importing_file: File,
    module_name: ModuleNameIngredient<'db>,
) -> Option<Module<'db>> {
    let name = module_name.name(db);
    let mode = module_name.mode(db);
    let _span = tracing::trace_span!("desperately_resolve_module", %name).entered();

    let Some(resolved) = desperately_resolve_name(db, importing_file, name, mode) else {
        let extra = match module_name.mode(db) {
            ModuleResolveMode::StubsAllowed => "neither stub nor real module file",
            ModuleResolveMode::StubsNotAllowed => "stubs not allowed",
            ModuleResolveMode::StubsNotAllowedSomeShadowingAllowed => {
                "stubs not allowed but some shadowing allowed"
            }
        };
        tracing::debug!("Module `{name}` not found while looking in parent dirs ({extra})");
        return None;
    };

    let module = match resolved {
        ResolvedName::FileModule(module) => {
            tracing::trace!(
                "Resolved module `{name}` to `{path}`",
                path = module.file.path(db)
            );
            Module::file_module(
                db,
                name.clone(),
                module.kind,
                module.search_path,
                module.file,
            )
        }
        ResolvedName::NamespacePackage => {
            tracing::trace!("Module `{name}` is a namespace package");
            Module::namespace_package(db, name.clone())
        }
    };

    Some(module)
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via any of the known search paths.
#[allow(unused)]
pub(crate) fn path_to_module<'db>(db: &'db dyn Db, path: &FilePath) -> Option<Module<'db>> {
    // It's not entirely clear on first sight why this method calls `file_to_module` instead of
    // it being the other way round, considering that the first thing that `file_to_module` does
    // is to retrieve the file's path.
    //
    // The reason is that `file_to_module` is a tracked Salsa query and salsa queries require that
    // all arguments are Salsa ingredients (something stored in Salsa). `Path`s aren't salsa ingredients but
    // `VfsFile` is. So what we do here is to retrieve the `path`'s `VfsFile` so that we can make
    // use of Salsa's caching and invalidation.
    let file = path.to_file(db)?;
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via any of the known search paths.
///
/// This function can be understood as essentially resolving `import .<self>` in the file itself,
/// and indeed, one of its primary jobs is resolving `.<self>` to derive the module name of `.`.
/// This intuition is particularly useful for understanding why it's correct that we pass
/// the file itself as `importing_file` to various subroutines.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
pub fn file_to_module(db: &dyn Db, file: File) -> Option<Module<'_>> {
    let _span = tracing::trace_span!("file_to_module", ?file).entered();

    let path = SystemOrVendoredPathRef::try_from_file(db, file)?;

    file_to_module_impl(
        db,
        file,
        path,
        search_paths(db, ModuleResolveMode::StubsAllowed),
    )
    .or_else(|| {
        file_to_module_impl(
            db,
            file,
            path,
            relative_desperate_search_paths(db, file).iter(),
        )
    })
}

fn file_to_module_impl<'db, 'a>(
    db: &'db dyn Db,
    file: File,
    path: SystemOrVendoredPathRef<'a>,
    mut search_paths: impl Iterator<Item = &'a SearchPath>,
) -> Option<Module<'db>> {
    let module_name = search_paths.find_map(|candidate: &SearchPath| {
        let relative_path = match path {
            SystemOrVendoredPathRef::System(path) => candidate.relativize_system_path(path),
            SystemOrVendoredPathRef::Vendored(path) => candidate.relativize_vendored_path(path),
        }?;
        relative_path.to_module_name()
    })?;

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same name in different
    // root paths, but that the module corresponding to `path` is in a lower priority search path,
    // in which case we ignore it.
    let module = resolve_module(db, file, &module_name)?;
    let module_file = module.file(db)?;

    if file.path(db) == module_file.path(db) {
        return Some(module);
    } else if file.source_type(db) == PySourceType::Python
        && module_file.source_type(db) == PySourceType::Stub
    {
        // If a .py and .pyi are both defined, the .pyi will be the one returned by `resolve_module().file`,
        // which would make us erroneously believe the `.py` is *not* also this module (breaking things
        // like relative imports). So here we try `resolve_real_module().file` to cover both cases.
        let module = resolve_real_module(db, file, &module_name)?;
        let module_file = module.file(db)?;
        if file.path(db) == module_file.path(db) {
            return Some(module);
        }
    }
    // This path is for a module with the same name but with a different precedence. For example:
    // ```
    // src/foo.py
    // src/foo/__init__.py
    // ```
    // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
    // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
    None
}

pub fn search_paths(db: &dyn Db, resolve_mode: ModuleResolveMode) -> SearchPathIterator<'_> {
    db.search_paths().iter(db, resolve_mode)
}

/// Get the search-paths for desperate resolution of absolute imports in this file.
///
/// Currently this is "all ancestor directories that don't contain an `__init__.py(i)`"
/// (from closest-to-importing-file to farthest).
///
/// (For paranoia purposes, all relative desperate search-paths are also absolute
/// valid desperate search-paths, but don't worry about that.)
///
/// We exclude `__init__.py(i)` dirs to avoid truncating packages.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn absolute_desperate_search_paths(db: &dyn Db, importing_file: File) -> Option<Vec<SearchPath>> {
    let system = db.system();
    let importing_path = importing_file.path(db).as_system_path()?;

    // Only allow this if the importing_file is under the first-party search path
    let (base_path, rel_path) =
        search_paths(db, ModuleResolveMode::StubsAllowed).find_map(|search_path| {
            if !search_path.is_first_party() {
                return None;
            }
            Some((
                search_path.as_system_path()?,
                search_path.relativize_system_path_only(importing_path)?,
            ))
        })?;

    // Read the revision on the corresponding file root to
    // register an explicit dependency on this directory. When
    // the revision gets bumped, the cache that Salsa creates
    // for this routine will be invalidated.
    //
    // (This is conditional because ruff uses this code too and doesn't set roots)
    if let Some(root) = db.files().root(db, base_path) {
        let _ = root.revision(db);
    }

    // Only allow searching up to the first-party path's root
    let mut search_paths = Vec::new();
    for rel_dir in rel_path.ancestors() {
        let candidate_path = base_path.join(rel_dir);
        if !system.is_directory(&candidate_path) {
            continue;
        }
        // Any dir that isn't a proper package is plausibly some test/script dir that could be
        // added as a search-path at runtime. Notably this reflects pytest's default mode where
        // it adds every dir with a .py to the search-paths (making all test files root modules),
        // unless they see an `__init__.py`, in which case they assume you don't want that.
        let isnt_regular_package = !system.is_file(&candidate_path.join("__init__.py"))
            && !system.is_file(&candidate_path.join("__init__.pyi"));
        // Any dir with a pyproject.toml or ty.toml is a valid relative desperate search-path and
        // we want all of those to also be valid absolute desperate search-paths. It doesn't
        // make any sense for a folder to have `pyproject.toml` and `__init__.py` but let's
        // not let something cursed and spooky happen, ok? d
        if isnt_regular_package
            || system.is_file(&candidate_path.join("pyproject.toml"))
            || system.is_file(&candidate_path.join("ty.toml"))
        {
            let search_path = SearchPath::first_party(system, candidate_path).ok()?;
            search_paths.push(search_path);
        }
    }

    if search_paths.is_empty() {
        None
    } else {
        Some(search_paths)
    }
}

/// Get the search-paths for desperate resolution of relative imports in this file.
///
/// Currently this is "the closest ancestor dir that contains a pyproject.toml (or ty.toml)",
/// which is a completely arbitrary decision. However it's farily important that relative
/// desperate search-paths pick a single "best" answer because every one is *valid* but one
/// that's too long or too short may cause problems.
///
/// For now this works well in common cases where we have some larger workspace that contains
/// one or more python projects in sub-directories, and those python projects assume that
/// absolute imports resolve relative to the pyproject.toml they live under.
///
/// Being so strict minimizes concerns about this going off a lot and doing random
/// chaotic things. In particular, all files under a given pyproject.toml will currently
/// agree on this being their desperate search-path, which is really nice.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn relative_desperate_search_paths(db: &dyn Db, importing_file: File) -> Option<SearchPath> {
    let system = db.system();
    let importing_path = importing_file.path(db).as_system_path()?;

    // Only allow this if the importing_file is under the first-party search path
    let (base_path, rel_path) =
        search_paths(db, ModuleResolveMode::StubsAllowed).find_map(|search_path| {
            if !search_path.is_first_party() {
                return None;
            }
            Some((
                search_path.as_system_path()?,
                search_path.relativize_system_path_only(importing_path)?,
            ))
        })?;

    // Read the revision on the corresponding file root to
    // register an explicit dependency on this directory. When
    // the revision gets bumped, the cache that Salsa creates
    // for this routine will be invalidated.
    //
    // (This is conditional because ruff uses this code too and doesn't set roots)
    if let Some(root) = db.files().root(db, base_path) {
        let _ = root.revision(db);
    }

    // Only allow searching up to the first-party path's root
    for rel_dir in rel_path.ancestors() {
        let candidate_path = base_path.join(rel_dir);
        // Any dir with a pyproject.toml or ty.toml might be a project root
        if system.is_file(&candidate_path.join("pyproject.toml"))
            || system.is_file(&candidate_path.join("ty.toml"))
        {
            let search_path = SearchPath::first_party(system, candidate_path).ok()?;
            return Some(search_path);
        }
    }

    None
}
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub struct SearchPaths {
    /// Search paths that have been statically determined purely from reading
    /// ty's configuration settings. These shouldn't ever change unless the
    /// config settings themselves change.
    static_paths: Vec<SearchPath>,

    /// Path to typeshed, which should come immediately after static paths.
    ///
    /// This can currently only be None if the `SystemPath` this points to is already in `static_paths`.
    stdlib_path: Option<SearchPath>,

    /// Path to the real stdlib, this replaces typeshed (`stdlib_path`) for goto-definition searches
    /// ([`ModuleResolveMode::StubsNotAllowed`]).
    real_stdlib_path: Option<SearchPath>,

    /// site-packages paths are not included in the above fields:
    /// if there are multiple site-packages paths, editable installations can appear
    /// *between* the site-packages paths on `sys.path` at runtime.
    /// That means we can't know where a second or third `site-packages` path should sit
    /// in terms of module-resolution priority until we've discovered the editable installs
    /// for the first `site-packages` path
    site_packages: Vec<SearchPath>,

    typeshed_versions: TypeshedVersions,
}

impl SearchPaths {
    /// Validate and normalize the raw settings given by the user
    /// into settings we can use for module resolution
    ///
    /// This method also implements the typing spec's [module resolution order].
    ///
    /// [module resolution order]: https://typing.python.org/en/latest/spec/distributing.html#import-resolution-ordering
    pub fn from_settings(
        settings: &SearchPathSettings,
        system: &dyn System,
        vendored: &VendoredFileSystem,
    ) -> Result<Self, SearchPathSettingsError> {
        fn canonicalize(path: &SystemPath, system: &dyn System) -> SystemPathBuf {
            system
                .canonicalize_path(path)
                .unwrap_or_else(|_| path.to_path_buf())
        }

        let SearchPathSettings {
            extra_paths,
            src_roots,
            custom_typeshed: typeshed,
            site_packages_paths,
            real_stdlib_path,
            misconfiguration_mode,
        } = settings;

        let mut static_paths = vec![];

        for path in extra_paths {
            let path = canonicalize(path, system);
            tracing::debug!("Adding extra search-path `{path}`");

            match SearchPath::extra(system, path) {
                Ok(path) => static_paths.push(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid extra search-path: {err}");
                    } else {
                        return Err(err.into());
                    }
                }
            }
        }

        for src_root in src_roots {
            tracing::debug!("Adding first-party search path `{src_root}`");
            match SearchPath::first_party(system, src_root.to_path_buf()) {
                Ok(path) => static_paths.push(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid first-party search-path: {err}");
                    } else {
                        return Err(err.into());
                    }
                }
            }
        }

        let (typeshed_versions, stdlib_path) = if let Some(typeshed) = typeshed {
            let typeshed = canonicalize(typeshed, system);
            tracing::debug!("Adding custom-stdlib search path `{typeshed}`");

            let versions_path = typeshed.join("stdlib/VERSIONS");

            let results = system
                .read_to_string(&versions_path)
                .map_err(|error| SearchPathSettingsError::FailedToReadVersionsFile {
                    path: versions_path,
                    error,
                })
                .and_then(|versions_content| Ok(versions_content.parse()?))
                .and_then(|parsed| Ok((parsed, SearchPath::custom_stdlib(system, &typeshed)?)));

            match results {
                Ok(results) => results,
                Err(err) => {
                    if settings.misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping custom-stdlib search-path: {err}");
                        (
                            vendored_typeshed_versions(vendored),
                            SearchPath::vendored_stdlib(),
                        )
                    } else {
                        return Err(err);
                    }
                }
            }
        } else {
            tracing::debug!("Using vendored stdlib");
            (
                vendored_typeshed_versions(vendored),
                SearchPath::vendored_stdlib(),
            )
        };

        let real_stdlib_path = if let Some(path) = real_stdlib_path {
            match SearchPath::real_stdlib(system, path.clone()) {
                Ok(path) => Some(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid real-stdlib search-path: {err}");
                        None
                    } else {
                        return Err(err.into());
                    }
                }
            }
        } else {
            None
        };

        let mut site_packages: Vec<_> = Vec::with_capacity(site_packages_paths.len());

        for path in site_packages_paths {
            tracing::debug!("Adding site-packages search path `{path}`");
            match SearchPath::site_packages(system, path.clone()) {
                Ok(path) => site_packages.push(path),
                Err(err) => {
                    if settings.misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid site-packages search-path: {err}");
                    } else {
                        return Err(err.into());
                    }
                }
            }
        }

        // TODO vendor typeshed's third-party stubs as well as the stdlib and
        // fallback to them as a final step?
        //
        // See: <https://github.com/astral-sh/ruff/pull/19620#discussion_r2240609135>

        // Filter out module resolution paths that point to the same directory
        // on disk (the same invariant maintained by [`sys.path` at runtime]).
        // (Paths may, however, *overlap* -- e.g. you could have both `src/`
        // and `src/foo` as module resolution paths simultaneously.)
        //
        // This code doesn't use an `IndexSet` because the key is the system
        // path and not the search root.
        //
        // [`sys.path` at runtime]: https://docs.python.org/3/library/site.html#module-site
        let mut seen_paths = FxHashSet::with_capacity_and_hasher(static_paths.len(), FxBuildHasher);

        static_paths.retain(|path| {
            if let Some(path) = path.as_system_path() {
                seen_paths.insert(path.to_path_buf())
            } else {
                true
            }
        });

        // Users probably shouldn't do this but... if they've shadowed their stdlib we should deduplicate it away.
        // This notably will mess up anything that checks if a search path "is the standard library" as we won't
        // "remember" that fact for static paths.
        //
        // (We used to shove these into static_paths, so the above retain implicitly did this. I am opting to
        // preserve this behaviour to avoid getting into the weeds of corner cases.)
        let stdlib_path_is_shadowed = stdlib_path
            .as_system_path()
            .is_some_and(|path| seen_paths.contains(path));
        let real_stdlib_path_is_shadowed = real_stdlib_path
            .as_ref()
            .and_then(SearchPath::as_system_path)
            .is_some_and(|path| seen_paths.contains(path));

        let stdlib_path = if stdlib_path_is_shadowed {
            None
        } else {
            Some(stdlib_path)
        };
        let real_stdlib_path = if real_stdlib_path_is_shadowed {
            None
        } else {
            real_stdlib_path
        };

        Ok(SearchPaths {
            static_paths,
            stdlib_path,
            real_stdlib_path,
            site_packages,
            typeshed_versions,
        })
    }

    /// Returns a new `SearchPaths` with no search paths configured.
    ///
    /// This is primarily useful for testing.
    pub fn empty(vendored: &VendoredFileSystem) -> Self {
        Self {
            static_paths: vec![],
            stdlib_path: Some(SearchPath::vendored_stdlib()),
            real_stdlib_path: None,
            site_packages: vec![],
            typeshed_versions: vendored_typeshed_versions(vendored),
        }
    }

    /// Registers the file roots for all non-dynamically discovered search paths that aren't first-party.
    pub fn try_register_static_roots(&self, db: &dyn Db) {
        let files = db.files();
        for path in self
            .static_paths
            .iter()
            .chain(self.site_packages.iter())
            .chain(&self.stdlib_path)
        {
            if let Some(system_path) = path.as_system_path() {
                if !path.is_first_party() {
                    files.try_add_root(db, system_path, FileRootKind::LibrarySearchPath);
                }
            }
        }
    }

    pub(super) fn iter<'a>(
        &'a self,
        db: &'a dyn Db,
        mode: ModuleResolveMode,
    ) -> SearchPathIterator<'a> {
        let stdlib_path = self.stdlib(mode);
        SearchPathIterator {
            db,
            static_paths: self.static_paths.iter(),
            stdlib_path,
            dynamic_paths: None,
            mode: ModuleResolveModeIngredient::new(db, mode),
        }
    }

    pub(crate) fn stdlib(&self, mode: ModuleResolveMode) -> Option<&SearchPath> {
        match mode {
            ModuleResolveMode::StubsAllowed => self.stdlib_path.as_ref(),
            ModuleResolveMode::StubsNotAllowed
            | ModuleResolveMode::StubsNotAllowedSomeShadowingAllowed => {
                self.real_stdlib_path.as_ref()
            }
        }
    }

    pub fn custom_stdlib(&self) -> Option<&SystemPath> {
        self.stdlib_path
            .as_ref()
            .and_then(SearchPath::as_system_path)
    }

    pub fn typeshed_versions(&self) -> &TypeshedVersions {
        &self.typeshed_versions
    }
}

/// Collect all dynamic search paths. For each `site-packages` path:
/// - Collect that `site-packages` path
/// - Collect any search paths listed in `.pth` files in that `site-packages` directory
///   due to editable installations of third-party packages.
///
/// The editable-install search paths for the first `site-packages` directory
/// should come between the two `site-packages` directories when it comes to
/// module-resolution priority.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn dynamic_resolution_paths<'db>(
    db: &'db dyn Db,
    mode: ModuleResolveModeIngredient<'db>,
) -> Vec<SearchPath> {
    tracing::debug!("Resolving dynamic module resolution paths");

    let SearchPaths {
        static_paths,
        stdlib_path,
        site_packages,
        typeshed_versions: _,
        real_stdlib_path,
    } = db.search_paths();

    let mut dynamic_paths = Vec::new();

    if site_packages.is_empty() {
        return dynamic_paths;
    }

    let mut existing_paths: FxHashSet<_> = static_paths
        .iter()
        .filter_map(|path| path.as_system_path())
        .map(Cow::Borrowed)
        .collect();

    // Use the `ModuleResolveMode` to determine which stdlib (if any) to mark as existing
    let stdlib = match mode.mode(db) {
        ModuleResolveMode::StubsAllowed => stdlib_path,
        ModuleResolveMode::StubsNotAllowed
        | ModuleResolveMode::StubsNotAllowedSomeShadowingAllowed => real_stdlib_path,
    };
    if let Some(path) = stdlib.as_ref().and_then(SearchPath::as_system_path) {
        existing_paths.insert(Cow::Borrowed(path));
    }

    let files = db.files();
    let system = db.system();

    for site_packages_search_path in site_packages {
        let site_packages_dir = site_packages_search_path
            .as_system_path()
            .expect("Expected site package path to be a system path");

        if !existing_paths.insert(Cow::Borrowed(site_packages_dir)) {
            continue;
        }

        let site_packages_root = files.expect_root(db, site_packages_dir);

        // This query needs to be re-executed each time a `.pth` file
        // is added, modified or removed from the `site-packages` directory.
        // However, we don't use Salsa queries to read the source text of `.pth` files;
        // we use the APIs on the `System` trait directly. As such, add a dependency on the
        // site-package directory's revision.
        site_packages_root.revision(db);

        dynamic_paths.push(site_packages_search_path.clone());

        // As well as modules installed directly into `site-packages`,
        // the directory may also contain `.pth` files.
        // Each `.pth` file in `site-packages` may contain one or more lines
        // containing a (relative or absolute) path.
        // Each of these paths may point to an editable install of a package,
        // so should be considered an additional search path.
        let pth_file_iterator = match PthFileIterator::new(db, site_packages_dir) {
            Ok(iterator) => iterator,
            Err(error) => {
                tracing::warn!(
                    "Failed to search for editable installation in {site_packages_dir}: {error}"
                );
                continue;
            }
        };

        // The Python documentation specifies that `.pth` files in `site-packages`
        // are processed in alphabetical order, so collecting and then sorting is necessary.
        // https://docs.python.org/3/library/site.html#module-site
        let mut all_pth_files: Vec<PthFile> = pth_file_iterator.collect();
        all_pth_files.sort_unstable_by(|a, b| a.path.cmp(&b.path));

        let installations = all_pth_files.iter().flat_map(PthFile::items);

        for installation in installations {
            let installation = system
                .canonicalize_path(&installation)
                .unwrap_or(installation);

            if existing_paths.insert(Cow::Owned(installation.clone())) {
                match SearchPath::editable(system, installation.clone()) {
                    Ok(search_path) => {
                        tracing::debug!(
                            "Adding editable installation to module resolution path {path}",
                            path = installation
                        );

                        // Register a file root for editable installs that are outside any other root
                        // (Most importantly, don't register a root for editable installations from the project
                        // directory as that would change the durability of files within those folders).
                        // Not having an exact file root for editable installs just means that
                        // some queries (like `list_modules_in`) will run slightly more frequently
                        // than they would otherwise.
                        if let Some(dynamic_path) = search_path.as_system_path() {
                            if files.root(db, dynamic_path).is_none() {
                                files.try_add_root(
                                    db,
                                    dynamic_path,
                                    FileRootKind::LibrarySearchPath,
                                );
                            }
                        }

                        dynamic_paths.push(search_path);
                    }

                    Err(error) => {
                        tracing::debug!("Skipping editable installation: {error}");
                    }
                }
            }
        }
    }

    dynamic_paths
}

/// Iterate over the available module-resolution search paths,
/// following the invariants maintained by [`sys.path` at runtime]:
/// "No item is added to `sys.path` more than once."
/// Dynamic search paths (required for editable installs into `site-packages`)
/// are only calculated lazily.
///
/// [`sys.path` at runtime]: https://docs.python.org/3/library/site.html#module-site
pub struct SearchPathIterator<'db> {
    db: &'db dyn Db,
    static_paths: std::slice::Iter<'db, SearchPath>,
    stdlib_path: Option<&'db SearchPath>,
    dynamic_paths: Option<std::slice::Iter<'db, SearchPath>>,
    mode: ModuleResolveModeIngredient<'db>,
}

impl<'db> Iterator for SearchPathIterator<'db> {
    type Item = &'db SearchPath;

    fn next(&mut self) -> Option<Self::Item> {
        let SearchPathIterator {
            db,
            static_paths,
            stdlib_path,
            mode,
            dynamic_paths,
        } = self;

        static_paths
            .next()
            .or_else(|| stdlib_path.take())
            .or_else(|| {
                dynamic_paths
                    .get_or_insert_with(|| dynamic_resolution_paths(*db, *mode).iter())
                    .next()
            })
    }
}

impl FusedIterator for SearchPathIterator<'_> {}

/// Represents a single `.pth` file in a `site-packages` directory.
/// One or more lines in a `.pth` file may be a (relative or absolute)
/// path that represents an editable installation of a package.
struct PthFile<'db> {
    path: SystemPathBuf,
    contents: String,
    site_packages: &'db SystemPath,
}

impl<'db> PthFile<'db> {
    /// Yield paths in this `.pth` file that appear to represent editable installations,
    /// and should therefore be added as module-resolution search paths.
    fn items(&'db self) -> impl Iterator<Item = SystemPathBuf> + 'db {
        let PthFile {
            path: _,
            contents,
            site_packages,
        } = self;

        // Empty lines or lines starting with '#' are ignored by the Python interpreter.
        // Lines that start with "import " or "import\t" do not represent editable installs at all;
        // instead, these are lines that are executed by Python at startup.
        // https://docs.python.org/3/library/site.html#module-site
        contents.lines().filter_map(move |line| {
            let line = line.trim_end();
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with("import ")
                || line.starts_with("import\t")
            {
                return None;
            }

            Some(SystemPath::absolute(line, site_packages))
        })
    }
}

/// Iterator that yields a [`PthFile`] instance for every `.pth` file
/// found in a given `site-packages` directory.
struct PthFileIterator<'db> {
    db: &'db dyn Db,
    directory_iterator: Box<dyn Iterator<Item = std::io::Result<DirectoryEntry>> + 'db>,
    site_packages: &'db SystemPath,
}

impl<'db> PthFileIterator<'db> {
    fn new(db: &'db dyn Db, site_packages: &'db SystemPath) -> std::io::Result<Self> {
        Ok(Self {
            db,
            directory_iterator: db.system().read_directory(site_packages)?,
            site_packages,
        })
    }
}

impl<'db> Iterator for PthFileIterator<'db> {
    type Item = PthFile<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        let PthFileIterator {
            db,
            directory_iterator,
            site_packages,
        } = self;

        let system = db.system();

        loop {
            let entry_result = directory_iterator.next()?;
            let Ok(entry) = entry_result else {
                continue;
            };
            let file_type = entry.file_type();
            if file_type.is_directory() {
                continue;
            }
            let path = entry.into_path();
            if path.extension() != Some("pth") {
                continue;
            }

            let contents = match system.read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) => {
                    tracing::warn!("Failed to read .pth file `{path}`: {error}");
                    continue;
                }
            };

            return Some(PthFile {
                path,
                contents,
                site_packages,
            });
        }
    }
}

/// A thin wrapper around `ModuleName` to make it a Salsa ingredient.
///
/// This is needed because Salsa requires that all query arguments are salsa ingredients.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ModuleNameIngredient<'db> {
    #[returns(ref)]
    pub(super) name: ModuleName,
    pub(super) mode: ModuleResolveMode,
}

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
fn resolve_name(db: &dyn Db, name: &ModuleName, mode: ModuleResolveMode) -> Option<ResolvedName> {
    let search_paths = search_paths(db, mode);
    resolve_name_impl(db, name, mode, search_paths)
}

/// Like `resolve_name` but for cases where it failed to resolve the module
/// and we are now Getting Desperate and willing to try the ancestor directories of
/// the `importing_file` as potential temporary search paths that are private
/// to this import.
fn desperately_resolve_name(
    db: &dyn Db,
    importing_file: File,
    name: &ModuleName,
    mode: ModuleResolveMode,
) -> Option<ResolvedName> {
    let search_paths = absolute_desperate_search_paths(db, importing_file);
    resolve_name_impl(db, name, mode, search_paths.iter().flatten())
}

fn resolve_name_impl<'a>(
    db: &dyn Db,
    name: &ModuleName,
    mode: ModuleResolveMode,
    search_paths: impl Iterator<Item = &'a SearchPath>,
) -> Option<ResolvedName> {
    let python_version = db.python_version();
    let resolver_state = ResolverContext::new(db, python_version, mode);
    let is_non_shadowable = mode.is_non_shadowable(python_version.minor, name.as_str());

    let name = RelaxedModuleName::new(name);
    let stub_name = name.to_stub_package();
    let mut is_namespace_package = false;

    for search_path in search_paths {
        // When a builtin module is imported, standard module resolution is bypassed:
        // the module name always resolves to the stdlib module,
        // even if there's a module of the same name in the first-party root
        // (which would normally result in the stdlib module being overridden).
        // TODO: offer a diagnostic if there is a first-party module of the same name
        if is_non_shadowable && !search_path.is_standard_library() {
            continue;
        }

        if !search_path.is_standard_library() && resolver_state.mode.stubs_allowed() {
            match resolve_name_in_search_path(&resolver_state, &stub_name, search_path) {
                Ok((package_kind, _, ResolvedName::FileModule(module))) => {
                    if package_kind.is_root() && module.kind.is_module() {
                        tracing::trace!(
                            "Search path `{search_path}` contains a module \
                             named `{stub_name}` but a standalone module isn't a valid stub."
                        );
                    } else {
                        return Some(ResolvedName::FileModule(module));
                    }
                }
                Ok((_, _, ResolvedName::NamespacePackage)) => {
                    is_namespace_package = true;
                }
                Err((PackageKind::Root, _)) => {
                    tracing::trace!(
                        "Search path `{search_path}` contains no stub package named `{stub_name}`."
                    );
                }
                Err((PackageKind::Regular, PyTyped::Partial)) => {
                    tracing::trace!(
                        "Stub-package in `{search_path}` doesn't contain module: \
                         `{name}` but it is a partial package, keep going."
                    );
                    // stub exists, but the module doesn't. But this is a partial package,
                    // fall through to looking for a non-stub package
                }
                Err((PackageKind::Regular, _)) => {
                    tracing::trace!(
                        "Stub-package in `{search_path}` doesn't contain module: `{name}`"
                    );
                    // stub exists, but the module doesn't.
                    return None;
                }
                Err((PackageKind::Namespace, _)) => {
                    tracing::trace!(
                        "Stub-package in `{search_path}` doesn't contain module: \
                         `{name}` but it is a namespace package, keep going."
                    );
                    // stub exists, but the module doesn't. But this is a namespace package,
                    // fall through to looking for a non-stub package
                }
            }
        }

        match resolve_name_in_search_path(&resolver_state, &name, search_path) {
            Ok((_, _, ResolvedName::FileModule(module))) => {
                return Some(ResolvedName::FileModule(module));
            }
            Ok((_, _, ResolvedName::NamespacePackage)) => {
                is_namespace_package = true;
            }
            Err(kind) => match kind {
                (PackageKind::Root, _) => {
                    tracing::trace!(
                        "Search path `{search_path}` contains no package named `{name}`."
                    );
                }
                (PackageKind::Regular, PyTyped::Partial) => {
                    tracing::trace!(
                        "Package in `{search_path}` doesn't contain module: \
                         `{name}` but it is a partial package, keep going."
                    );
                }
                (PackageKind::Regular, _) => {
                    // For regular packages, don't search the next search path. All files of that
                    // package must be in the same location
                    tracing::trace!("Package in `{search_path}` doesn't contain module: `{name}`");
                    return None;
                }
                (PackageKind::Namespace, _) => {
                    tracing::trace!(
                        "Package in `{search_path}` doesn't contain module: \
                         `{name}` but it is a namespace package, keep going."
                    );
                }
            },
        }
    }

    if is_namespace_package {
        return Some(ResolvedName::NamespacePackage);
    }

    None
}

#[derive(Debug)]
enum ResolvedName {
    /// A module that resolves to a file.
    FileModule(ResolvedFileModule),

    /// The module name resolved to a namespace package.
    ///
    /// For example, `from opentelemetry import trace, metrics` where
    /// `opentelemetry` is a namespace package (and `trace` and `metrics` are
    /// sub packages).
    NamespacePackage,
}

#[derive(Debug)]
struct ResolvedFileModule {
    kind: ModuleKind,
    search_path: SearchPath,
    file: File,
}

/// Attempts to resolve a module name in a particular search path.
///
/// `search_path` should be the directory to start looking for the module.
///
/// `name` should be a complete non-empty module name, e.g, `foo` or
/// `foo.bar.baz`.
///
/// Upon success, this returns the kind of the parent package (root, regular
/// package or namespace package) along with the resolved details of the
/// module: its kind (single-file module or package), the search path in
/// which it was found (guaranteed to be equal to the one given) and the
/// corresponding `File`.
///
/// Upon error, the kind of the parent package is returned.
fn resolve_name_in_search_path(
    context: &ResolverContext,
    name: &RelaxedModuleName,
    search_path: &SearchPath,
) -> Result<(PackageKind, PyTyped, ResolvedName), (PackageKind, PyTyped)> {
    let mut components = name.components();
    let module_name = components.next_back().unwrap();

    let resolved_package = resolve_package(search_path, components, context)?;

    let mut package_path = resolved_package.path;

    package_path.push(module_name);

    // Check for a regular package first (highest priority)
    package_path.push("__init__");
    if let Some(regular_package) = resolve_file_module(&package_path, context) {
        return Ok((
            resolved_package.kind,
            resolved_package.typed,
            ResolvedName::FileModule(ResolvedFileModule {
                search_path: search_path.clone(),
                kind: ModuleKind::Package,
                file: regular_package,
            }),
        ));
    }

    // Check for a file module next
    package_path.pop();

    if let Some(file_module) = resolve_file_module(&package_path, context) {
        return Ok((
            resolved_package.kind,
            resolved_package.typed,
            ResolvedName::FileModule(ResolvedFileModule {
                file: file_module,
                kind: ModuleKind::Module,
                search_path: search_path.clone(),
            }),
        ));
    }

    // Last resort, check if a folder with the given name exists. If so,
    // then this is a namespace package. We need to skip this check for
    // typeshed because the `resolve_file_module` can also return `None` if the
    // `__init__.py` exists but isn't available for the current Python version.
    // Let's assume that the `xml` module is only available on Python 3.11+ and
    // we're resolving for Python 3.10:
    //
    // * `resolve_file_module("xml/__init__.pyi")` returns `None` even though
    //   the file exists but the module isn't available for the current Python
    //   version.
    // * The check here would now return `true` because the `xml` directory
    //   exists, resulting in a false positive for a namespace package.
    //
    // Since typeshed doesn't use any namespace packages today (May 2025),
    // simply skip this check which also helps performance. If typeshed
    // ever uses namespace packages, ensure that this check also takes the
    // `VERSIONS` file into consideration.
    if !search_path.is_standard_library() && package_path.is_directory(context) {
        if let Some(path) = package_path.to_system_path() {
            let system = context.db.system();
            if system.case_sensitivity().is_case_sensitive()
                || system.path_exists_case_sensitive(
                    &path,
                    package_path.search_path().as_system_path().unwrap(),
                )
            {
                return Ok((
                    resolved_package.kind,
                    resolved_package.typed,
                    ResolvedName::NamespacePackage,
                ));
            }
        }
    }

    Err((resolved_package.kind, resolved_package.typed))
}

/// If `module` exists on disk with either a `.pyi` or `.py` extension,
/// return the [`File`] corresponding to that path.
///
/// `.pyi` files take priority, as they always have priority when
/// resolving modules.
pub(super) fn resolve_file_module(
    module: &ModulePath,
    resolver_state: &ResolverContext,
) -> Option<File> {
    // Stubs have precedence over source files
    let stub_file = if resolver_state.mode.stubs_allowed() {
        module.with_pyi_extension().to_file(resolver_state)
    } else {
        None
    };
    let file = stub_file.or_else(|| {
        module
            .with_py_extension()
            .and_then(|path| path.to_file(resolver_state))
    })?;

    // For system files, test if the path has the correct casing.
    // We can skip this step for vendored files or virtual files because
    // those file systems are case sensitive (we wouldn't get to this point).
    if let Some(path) = file.path(resolver_state.db).as_system_path() {
        let system = resolver_state.db.system();
        if !system.case_sensitivity().is_case_sensitive()
            && !system
                .path_exists_case_sensitive(path, module.search_path().as_system_path().unwrap())
        {
            return None;
        }
    }

    Some(file)
}

/// Attempt to resolve the parent package of a module.
///
/// `module_search_path` should be the directory to start looking for the
/// parent package.
///
/// `components` should be the full module name of the parent package. This
/// specifically should not include the basename of the module. So e.g.,
/// for `foo.bar.baz`, `components` should be `[foo, bar]`. It follows that
/// `components` may be empty (in which case, the parent package is the root).
///
/// Upon success, the path to the package and its "kind" (root, regular or
/// namespace) is returned. Upon error, the kind of the package is still
/// returned based on how many components were found and whether `__init__.py`
/// is present.
fn resolve_package<'a, 'db, I>(
    module_search_path: &SearchPath,
    components: I,
    resolver_state: &ResolverContext<'db>,
) -> Result<ResolvedPackage, (PackageKind, PyTyped)>
where
    I: Iterator<Item = &'a str>,
{
    let mut package_path = module_search_path.to_module_path();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    let mut typed = package_path.py_typed(resolver_state);

    // For `foo.bar.baz`, test that `foo` and `bar` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);
        typed = package_path.py_typed(resolver_state).inherit_parent(typed);

        let is_regular_package = package_path.is_regular_package(resolver_state);

        if is_regular_package {
            // This is the only place where we need to consider the existence of legacy namespace
            // packages, as we are explicitly searching for the *parent* package of the module
            // we actually want. Here, such a package should be treated as a PEP-420 ("modern")
            // namespace package. In all other contexts it acts like a normal package and needs
            // no special handling.
            in_namespace_package = is_legacy_namespace_package(&package_path, resolver_state);
        } else if package_path.is_directory(resolver_state)
            // Pure modules hide namespace packages with the same name
            && resolve_file_module(&package_path, resolver_state).is_none()
        {
            // A directory without an `__init__.py(i)` is a namespace package,
            // continue with the next folder.
            in_namespace_package = true;
        } else if in_namespace_package {
            // Package not found but it is part of a namespace package.
            return Err((PackageKind::Namespace, typed));
        } else if in_sub_package {
            // A regular sub package wasn't found.
            return Err((PackageKind::Regular, typed));
        } else {
            // We couldn't find `foo` for `foo.bar.baz`, search the next search path.
            return Err((PackageKind::Root, typed));
        }

        in_sub_package = true;
    }

    let kind = if in_namespace_package {
        PackageKind::Namespace
    } else if in_sub_package {
        PackageKind::Regular
    } else {
        PackageKind::Root
    };

    Ok(ResolvedPackage {
        kind,
        path: package_path,
        typed,
    })
}

/// Determines whether a package is a legacy namespace package.
///
/// Before PEP 420 introduced implicit namespace packages, the ecosystem developed
/// its own form of namespace packages. These legacy namespace packages continue to persist
/// in modern codebases because they work with ancient Pythons and if it ain't broke, don't fix it.
///
/// A legacy namespace package is distinguished by having an `__init__.py` that contains an
/// expression to the effect of:
///
/// ```python
/// __path__ = __import__("pkgutil").extend_path(__path__, __name__)
/// ```
///
/// The resulting package simultaneously has properties of both regular packages and namespace ones:
///
/// * Like regular packages, `__init__.py` is defined and can contain items other than submodules
/// * Like implicit namespace packages, multiple copies of the package may exist with different
///   submodules, and they will be merged into one namespace at runtime by the interpreter
///
/// Now, you may rightly wonder: "What if the `__init__.py` files have different contents?"
/// The apparent official answer is: "Don't do that!"
/// And the reality is: "Of course people do that!"
///
/// In practice we think it's fine to, just like with regular packages, use the first one
/// we find on the search paths. To the extent that the different copies "need" to have the same
/// contents, they all "need" to have the legacy namespace idiom (we do nothing to enforce that,
/// we will just get confused if you mess it up).
fn is_legacy_namespace_package(
    package_path: &ModulePath,
    resolver_state: &ResolverContext,
) -> bool {
    // Just an optimization, the stdlib and typeshed are never legacy namespace packages
    if package_path.search_path().is_standard_library() {
        return false;
    }

    let mut package_path = package_path.clone();
    package_path.push("__init__");
    let Some(init) = resolve_file_module(&package_path, resolver_state) else {
        return false;
    };

    // This is all syntax-only analysis so it *could* be fooled but it's really unlikely.
    //
    // The benefit of being syntax-only is speed and avoiding circular dependencies
    // between module resolution and semantic analysis.
    //
    // The downside is if you write slightly different syntax we will fail to detect the idiom,
    // but hey, this is better than nothing!
    let parsed = ruff_db::parsed::parsed_module(resolver_state.db, init);
    let mut visitor = LegacyNamespacePackageVisitor::default();
    visitor.visit_body(parsed.load(resolver_state.db).suite());

    visitor.is_legacy_namespace_package
}

#[derive(Debug)]
struct ResolvedPackage {
    path: ModulePath,
    kind: PackageKind,
    typed: PyTyped,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum PackageKind {
    /// A root package or module. E.g. `foo` in `foo.bar.baz` or just `foo`.
    Root,

    /// A regular sub-package where the parent contains an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` when the `foo` directory contains an `__init__.py`.
    Regular,

    /// A sub-package in a namespace package. A namespace package is a package
    /// without an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` if the `foo` directory contains no
    /// `__init__.py`.
    Namespace,
}

impl PackageKind {
    pub(crate) const fn is_root(self) -> bool {
        matches!(self, PackageKind::Root)
    }
}

/// Info about the `py.typed` file for this package
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum PyTyped {
    /// No `py.typed` was found
    Untyped,
    /// A `py.typed` was found containing "partial"
    Partial,
    /// A `py.typed` was found (not partial)
    Full,
}

impl PyTyped {
    /// Inherit py.typed info from the parent package
    ///
    /// > This marker applies recursively: if a top-level package includes it,
    /// > all its sub-packages MUST support type checking as well.
    ///
    /// This implementation implies that once a `py.typed` is specified
    /// all child packages inherit it, so they can never become Untyped.
    /// However they can override whether that's Full or Partial by
    /// redeclaring a `py.typed` file of their own.
    fn inherit_parent(self, parent: Self) -> Self {
        if self == Self::Untyped { parent } else { self }
    }
}

pub(super) struct ResolverContext<'db> {
    pub(super) db: &'db dyn Db,
    pub(super) python_version: PythonVersion,
    pub(super) mode: ModuleResolveMode,
}

impl<'db> ResolverContext<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        python_version: PythonVersion,
        mode: ModuleResolveMode,
    ) -> Self {
        Self {
            db,
            python_version,
            mode,
        }
    }

    pub(super) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
    }
}

/// A [`ModuleName`] but with relaxed semantics to allow `<package>-stubs.path`
#[derive(Debug)]
struct RelaxedModuleName(compact_str::CompactString);

impl RelaxedModuleName {
    fn new(name: &ModuleName) -> Self {
        Self(name.as_str().into())
    }

    fn components(&self) -> Split<'_, char> {
        self.0.split('.')
    }

    fn to_stub_package(&self) -> Self {
        if let Some((package, rest)) = self.0.split_once('.') {
            Self(format_compact!("{package}-stubs.{rest}"))
        } else {
            Self(format_compact!("{package}-stubs", package = self.0))
        }
    }
}

impl fmt::Display for RelaxedModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Detects if a module contains a statement of the form:
/// ```python
/// __path__ = pkgutil.extend_path(__path__, __name__)
/// ```
/// or
/// ```python
/// __path__ = __import__("pkgutil").extend_path(__path__, __name__)
/// ```
#[derive(Default)]
struct LegacyNamespacePackageVisitor {
    is_legacy_namespace_package: bool,
    in_body: bool,
}

impl Visitor<'_> for LegacyNamespacePackageVisitor {
    fn visit_body(&mut self, body: &[ruff_python_ast::Stmt]) {
        if self.is_legacy_namespace_package {
            return;
        }

        // Don't traverse into nested bodies.
        if self.in_body {
            return;
        }

        self.in_body = true;

        walk_body(self, body);
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        if self.is_legacy_namespace_package {
            return;
        }

        let ast::Stmt::Assign(ast::StmtAssign { value, targets, .. }) = stmt else {
            return;
        };

        let [ast::Expr::Name(maybe_path)] = &**targets else {
            return;
        };

        if &*maybe_path.id != "__path__" {
            return;
        }

        let ast::Expr::Call(ast::ExprCall {
            func: extend_func,
            arguments: extend_arguments,
            ..
        }) = &**value
        else {
            return;
        };

        let ast::Expr::Attribute(ast::ExprAttribute {
            value: maybe_pkg_util,
            attr: maybe_extend_path,
            ..
        }) = &**extend_func
        else {
            return;
        };

        // Match if the left side of the attribute access is either `__import__("pkgutil")` or `pkgutil`
        match &**maybe_pkg_util {
            // __import__("pkgutil").extend_path(__path__, __name__)
            ast::Expr::Call(ruff_python_ast::ExprCall {
                func: maybe_import,
                arguments: import_arguments,
                ..
            }) => {
                let ast::Expr::Name(maybe_import) = &**maybe_import else {
                    return;
                };

                if maybe_import.id() != "__import__" {
                    return;
                }

                let Some(ast::Expr::StringLiteral(name)) =
                    import_arguments.find_argument_value("name", 0)
                else {
                    return;
                };

                if name.value.to_str() != "pkgutil" {
                    return;
                }
            }
            // "pkgutil.extend_path(__path__, __name__)"
            ast::Expr::Name(name) => {
                if name.id() != "pkgutil" {
                    return;
                }
            }
            _ => {
                return;
            }
        }

        // Test that this is an `extend_path(__path__, __name__)` call
        if maybe_extend_path != "extend_path" {
            return;
        }

        let Some(ast::Expr::Name(path)) = extend_arguments.find_argument_value("path", 0) else {
            return;
        };
        let Some(ast::Expr::Name(name)) = extend_arguments.find_argument_value("name", 1) else {
            return;
        };

        self.is_legacy_namespace_package = path.id() == "__path__" && name.id() == "__name__";
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "These are tests, so it's fine to do I/O by-passing System."
    )]
    use ruff_db::Db;
    use ruff_db::files::{File, FilePath, system_path_to_file};
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem as _};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::PythonVersion;

    use crate::db::tests::TestDb;
    use crate::module::ModuleKind;
    use crate::module_name::ModuleName;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    #[test]
    fn first_party_module() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "print('Hello, world!')")])
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();

        assert_eq!(
            Some(&foo_module),
            resolve_module_confident(&db, &foo_module_name).as_ref()
        );

        assert_eq!("foo", foo_module.name(&db));
        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(ModuleKind::Module, foo_module.kind(&db));

        let expected_foo_path = src.join("foo.py");
        assert_eq!(&expected_foo_path, foo_module.file(&db).unwrap().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(expected_foo_path))
        );
    }

    #[test]
    fn stubs_over_module_source() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", ""), ("foo.pyi", "")])
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();

        assert_eq!(
            Some(&foo_module),
            resolve_module_confident(&db, &foo_module_name).as_ref()
        );

        assert_eq!("foo", foo_module.name(&db));
        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(ModuleKind::Module, foo_module.kind(&db));

        let expected_foo_path = src.join("foo.pyi");
        assert_eq!(&expected_foo_path, foo_module.file(&db).unwrap().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(expected_foo_path))
        );
    }

    /// Tests precedence when there is a package and a sibling stub file.
    ///
    /// NOTE: I am unsure if this is correct. I wrote this test to match
    /// behavior while implementing "list modules." Notably, in this case, the
    /// regular source file gets priority. But in `stubs_over_module_source`
    /// above, the stub file gets priority.
    #[test]
    fn stubs_over_package_source() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", ""), ("foo.pyi", "")])
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();

        assert_eq!(
            Some(&foo_module),
            resolve_module_confident(&db, &foo_module_name).as_ref()
        );

        assert_eq!("foo", foo_module.name(&db));
        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(ModuleKind::Package, foo_module.kind(&db));

        let expected_foo_path = src.join("foo/__init__.py");
        assert_eq!(&expected_foo_path, foo_module.file(&db).unwrap().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(expected_foo_path))
        );
    }

    #[test]
    fn builtins_vendored() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_vendored_typeshed()
            .with_src_files(&[("builtins.py", "FOOOO = 42")])
            .build();

        let builtins_module_name = ModuleName::new_static("builtins").unwrap();
        let builtins =
            resolve_module_confident(&db, &builtins_module_name).expect("builtins to resolve");

        assert_eq!(
            builtins.file(&db).unwrap().path(&db),
            &stdlib.join("builtins.pyi")
        );
    }

    #[test]
    fn builtins_custom() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("builtins.pyi", "def min(a, b): ...")],
            versions: "builtins: 3.8-",
        };

        const SRC: &[FileSpec] = &[("builtins.py", "FOOOO = 42")];

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let builtins_module_name = ModuleName::new_static("builtins").unwrap();
        let builtins =
            resolve_module_confident(&db, &builtins_module_name).expect("builtins to resolve");

        assert_eq!(
            builtins.file(&db).unwrap().path(&db),
            &stdlib.join("builtins.pyi")
        );
    }

    #[test]
    fn stdlib() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module_confident(&db, &functools_module_name).as_ref()
        );

        assert_eq!(&stdlib, functools_module.search_path(&db).unwrap());
        assert_eq!(ModuleKind::Module, functools_module.kind(&db));

        let expected_functools_path = stdlib.join("functools.pyi");
        assert_eq!(
            &expected_functools_path,
            functools_module.file(&db).unwrap().path(&db)
        );

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &FilePath::System(expected_functools_path))
        );
    }

    fn create_module_names(raw_names: &[&str]) -> Vec<ModuleName> {
        raw_names
            .iter()
            .map(|raw| ModuleName::new(raw).unwrap())
            .collect()
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            functools: 3.8-             # Top-level single-file module
            xml: 3.8-3.8                # Namespace package on py38 only
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("functools.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let existing_modules = create_module_names(&["asyncio", "functools", "xml.etree"]);
        for module_name in existing_modules {
            let resolved_module =
                resolve_module_confident(&db, &module_name).unwrap_or_else(|| {
                    panic!("Expected module {module_name} to exist in the mock stdlib")
                });
            let search_path = resolved_module.search_path(&db).unwrap();
            assert_eq!(
                &stdlib, search_path,
                "Search path for {module_name} was unexpectedly {search_path:?}"
            );
            assert!(
                search_path.is_standard_library(),
                "Expected a stdlib search path, but got {search_path:?}"
            );
        }
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_nonexisting_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            importlib: 3.9-             # Namespace package on py39+
            xml: 3.8-3.8                # Namespace package on 3.8 only
        ";

        const STDLIB: &[FileSpec] = &[
            ("collections/__init__.pyi", ""),
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("importlib/abc.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let nonexisting_modules = create_module_names(&[
            "collections",
            "importlib",
            "importlib.abc",
            "xml",
            "asyncio.tasks",
        ]);

        for module_name in nonexisting_modules {
            assert!(
                resolve_module_confident(&db, &module_name).is_none(),
                "Unexpectedly resolved a module for {module_name}"
            );
        }
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py39_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            functools: 3.8-             # Top-level single-file module
            importlib: 3.9-             # Namespace package on py39+
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("collections/__init__.pyi", ""),
            ("functools.pyi", ""),
            ("importlib/abc.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY39)
            .build();

        let existing_modules = create_module_names(&[
            "asyncio",
            "functools",
            "importlib.abc",
            "collections",
            "asyncio.tasks",
        ]);

        for module_name in existing_modules {
            let resolved_module =
                resolve_module_confident(&db, &module_name).unwrap_or_else(|| {
                    panic!("Expected module {module_name} to exist in the mock stdlib")
                });
            let search_path = resolved_module.search_path(&db).unwrap();
            assert_eq!(
                &stdlib, search_path,
                "Search path for {module_name} was unexpectedly {search_path:?}"
            );
            assert!(
                search_path.is_standard_library(),
                "Expected a stdlib search path, but got {search_path:?}"
            );
        }
    }
    #[test]
    fn stdlib_resolution_respects_versions_file_py39_nonexisting_modules() {
        const VERSIONS: &str = "\
            importlib: 3.9-   # Namespace package on py39+
            xml: 3.8-3.8      # Namespace package on 3.8 only
        ";

        const STDLIB: &[FileSpec] = &[("importlib/abc.pyi", ""), ("xml/etree.pyi", "")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY39)
            .build();

        let nonexisting_modules = create_module_names(&["importlib", "xml", "xml.etree"]);
        for module_name in nonexisting_modules {
            assert!(
                resolve_module_confident(&db, &module_name).is_none(),
                "Unexpectedly resolved a module for {module_name}"
            );
        }
    }

    #[test]
    fn first_party_precedence_over_stdlib() {
        const SRC: &[FileSpec] = &[("functools.py", "def update_wrapper(): ...")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module_confident(&db, &functools_module_name).as_ref()
        );
        assert_eq!(&src, functools_module.search_path(&db).unwrap());
        assert_eq!(ModuleKind::Module, functools_module.kind(&db));
        assert_eq!(
            &src.join("functools.py"),
            functools_module.file(&db).unwrap().path(&db)
        );

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &FilePath::System(src.join("functools.py")))
        );
    }

    #[test]
    fn stdlib_uses_vendored_typeshed_when_no_custom_typeshed_supplied() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_vendored_typeshed()
            .with_python_version(PythonVersion::default())
            .build();

        let pydoc_data_topics_name = ModuleName::new_static("pydoc_data.topics").unwrap();
        let pydoc_data_topics = resolve_module_confident(&db, &pydoc_data_topics_name).unwrap();

        assert_eq!("pydoc_data.topics", pydoc_data_topics.name(&db));
        assert_eq!(pydoc_data_topics.search_path(&db).unwrap(), &stdlib);
        assert_eq!(
            pydoc_data_topics.file(&db).unwrap().path(&db),
            &stdlib.join("pydoc_data/topics.pyi")
        );
    }

    #[test]
    fn resolve_package() {
        let TestCase { src, db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", "print('Hello, world!'")])
            .build();

        let foo_path = src.join("foo/__init__.py");
        let foo_module =
            resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();

        assert_eq!("foo", foo_module.name(&db));
        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(&foo_path, foo_module.file(&db).unwrap().path(&db));

        assert_eq!(
            Some(&foo_module),
            path_to_module(&db, &FilePath::System(foo_path)).as_ref()
        );

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(src.join("foo")))
        );
    }

    #[test]
    fn package_priority_over_module() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", "print('Hello, world!')"),
            ("foo.py", "print('Hello, world!')"),
        ];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo_module =
            resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_init_path = src.join("foo/__init__.py");

        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(&foo_init_path, foo_module.file(&db).unwrap().path(&db));
        assert_eq!(ModuleKind::Package, foo_module.kind(&db));

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo_init_path))
        );
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(src.join("foo.py")))
        );
    }

    #[test]
    fn typing_stub_over_module() {
        const SRC: &[FileSpec] = &[("foo.py", "print('Hello, world!')"), ("foo.pyi", "x: int")];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo = resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_real =
            resolve_real_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_stub = src.join("foo.pyi");

        assert_eq!(&src, foo.search_path(&db).unwrap());
        assert_eq!(&foo_stub, foo.file(&db).unwrap().path(&db));

        assert_eq!(Some(foo), path_to_module(&db, &FilePath::System(foo_stub)));
        assert_eq!(
            Some(foo_real),
            path_to_module(&db, &FilePath::System(src.join("foo.py")))
        );
        assert!(foo_real != foo);
    }

    #[test]
    fn sub_packages() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", ""),
            ("foo/bar/__init__.py", ""),
            ("foo/bar/baz.py", "print('Hello, world!)'"),
        ];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let baz_module =
            resolve_module_confident(&db, &ModuleName::new_static("foo.bar.baz").unwrap()).unwrap();
        let baz_path = src.join("foo/bar/baz.py");

        assert_eq!(&src, baz_module.search_path(&db).unwrap());
        assert_eq!(&baz_path, baz_module.file(&db).unwrap().path(&db));

        assert_eq!(
            Some(baz_module),
            path_to_module(&db, &FilePath::System(baz_path))
        );
    }

    #[test]
    fn module_search_path_priority() {
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("foo.py", "")])
            .build();

        let foo_module =
            resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_src_path = src.join("foo.py");

        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(&foo_src_path, foo_module.file(&db).unwrap().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo_src_path))
        );

        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(site_packages.join("foo.py")))
        );
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        use anyhow::Context;
        use ruff_db::system::{OsSystem, SystemPath};

        use crate::db::tests::TestDb;

        let mut db = TestDb::new().with_python_version(PythonVersion::PY38);

        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir
            .path()
            .canonicalize()
            .context("Failed to canonicalize temp dir")?;
        let root = SystemPath::from_std_path(&root).unwrap();
        db.use_system(OsSystem::new(root));

        let src = root.join("src");
        let site_packages = root.join("site-packages");
        let custom_typeshed = root.join("typeshed");

        let foo = src.join("foo.py");
        let bar = src.join("bar.py");

        std::fs::create_dir_all(src.as_std_path())?;
        std::fs::create_dir_all(site_packages.as_std_path())?;
        std::fs::create_dir_all(custom_typeshed.join("stdlib").as_std_path())?;
        std::fs::File::create(custom_typeshed.join("stdlib/VERSIONS").as_std_path())?;

        std::fs::write(foo.as_std_path(), "")?;
        std::os::unix::fs::symlink(foo.as_std_path(), bar.as_std_path())?;

        db.set_search_paths(
            SearchPathSettings {
                src_roots: vec![src.clone()],
                custom_typeshed: Some(custom_typeshed),
                site_packages_paths: vec![site_packages],
                ..SearchPathSettings::empty()
            }
            .to_search_paths(db.system(), db.vendored())
            .expect("Valid search path settings"),
        );

        let foo_module =
            resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        let bar_module =
            resolve_module_confident(&db, &ModuleName::new_static("bar").unwrap()).unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.search_path(&db).unwrap());
        assert_eq!(&foo, foo_module.file(&db).unwrap().path(&db));

        // `foo` and `bar` shouldn't resolve to the same file

        assert_eq!(&src, bar_module.search_path(&db).unwrap());
        assert_eq!(&bar, bar_module.file(&db).unwrap().path(&db));
        assert_eq!(&foo, foo_module.file(&db).unwrap().path(&db));

        assert_ne!(&foo_module, &bar_module);

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo))
        );
        assert_eq!(
            Some(bar_module),
            path_to_module(&db, &FilePath::System(bar))
        );

        Ok(())
    }

    #[test]
    fn deleting_an_unrelated_file_doesnt_change_module_resolution() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "x = 1"), ("bar.py", "x = 2")])
            .with_python_version(PythonVersion::PY38)
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        let foo_pieces = (
            foo_module.name(&db).clone(),
            foo_module.file(&db),
            foo_module.known(&db),
            foo_module.search_path(&db).cloned(),
            foo_module.kind(&db),
        );

        let bar_path = src.join("bar.py");
        let bar = system_path_to_file(&db, &bar_path).expect("bar.py to exist");

        db.clear_salsa_events();

        // Delete `bar.py`
        db.memory_file_system().remove_file(&bar_path).unwrap();
        bar.sync(&mut db);

        // Re-query the foo module. The foo module should still be cached
        // because `bar.py` isn't relevant for resolving `foo`.

        let foo_module2 = resolve_module_confident(&db, &foo_module_name);
        let foo_pieces2 = foo_module2.map(|foo_module2| {
            (
                foo_module2.name(&db).clone(),
                foo_module2.file(&db),
                foo_module2.known(&db),
                foo_module2.search_path(&db).cloned(),
                foo_module2.kind(&db),
            )
        });

        assert!(
            !db.take_salsa_events()
                .iter()
                .any(|event| { matches!(event.kind, salsa::EventKind::WillExecute { .. }) })
        );

        assert_eq!(Some(foo_pieces), foo_pieces2);
    }

    #[test]
    fn adding_file_on_which_module_resolution_depends_invalidates_previously_failing_query_that_now_succeeds()
    -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new().build();
        let foo_path = src.join("foo.py");

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        assert_eq!(resolve_module_confident(&db, &foo_module_name), None);

        // Now write the foo file
        db.write_file(&foo_path, "x = 1")?;

        let foo_file = system_path_to_file(&db, &foo_path).expect("foo.py to exist");

        let foo_module =
            resolve_module_confident(&db, &foo_module_name).expect("Foo module to resolve");
        assert_eq!(foo_file, foo_module.file(&db).unwrap());

        Ok(())
    }

    #[test]
    fn removing_file_on_which_module_resolution_depends_invalidates_previously_successful_query_that_now_fails()
    -> anyhow::Result<()> {
        const SRC: &[FileSpec] = &[("foo.py", "x = 1"), ("foo/__init__.py", "x = 2")];

        let TestCase { mut db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module =
            resolve_module_confident(&db, &foo_module_name).expect("foo module to exist");
        let foo_init_path = src.join("foo/__init__.py");

        assert_eq!(&foo_init_path, foo_module.file(&db).unwrap().path(&db));

        // Delete `foo/__init__.py` and the `foo` folder. `foo` should now resolve to `foo.py`
        db.memory_file_system().remove_file(&foo_init_path)?;
        db.memory_file_system()
            .remove_directory(foo_init_path.parent().unwrap())?;
        File::sync_path(&mut db, &foo_init_path);
        File::sync_path(&mut db, foo_init_path.parent().unwrap());

        let foo_module =
            resolve_module_confident(&db, &foo_module_name).expect("Foo module to resolve");
        assert_eq!(&src.join("foo.py"), foo_module.file(&db).unwrap().path(&db));

        Ok(())
    }

    #[test]
    fn adding_file_to_search_path_with_lower_priority_does_not_invalidate_query() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let stdlib_functools_path = stdlib.join("functools.pyi");

        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        assert_eq!(functools_module.search_path(&db).unwrap(), &stdlib);
        assert_eq!(
            Ok(functools_module.file(&db).unwrap()),
            system_path_to_file(&db, &stdlib_functools_path)
        );

        // Adding a file to site-packages does not invalidate the query,
        // since site-packages takes lower priority in the module resolution
        db.clear_salsa_events();
        let site_packages_functools_path = site_packages.join("functools.py");
        db.write_file(&site_packages_functools_path, "f: int")
            .unwrap();
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        let functools_file = functools_module.file(&db).unwrap();
        let functools_search_path = functools_module.search_path(&db).unwrap().clone();
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(
            &db,
            resolve_module_query,
            ModuleNameIngredient::new(&db, functools_module_name, ModuleResolveMode::StubsAllowed),
            &events,
        );
        assert_eq!(&functools_search_path, &stdlib);
        assert_eq!(
            Ok(functools_file),
            system_path_to_file(&db, &stdlib_functools_path)
        );
    }

    #[test]
    fn adding_file_to_search_path_with_higher_priority_invalidates_the_query() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            src,
            ..
        } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        assert_eq!(functools_module.search_path(&db).unwrap(), &stdlib);
        assert_eq!(
            Ok(functools_module.file(&db).unwrap()),
            system_path_to_file(&db, stdlib.join("functools.pyi"))
        );

        // Adding a first-party file invalidates the query,
        // since first-party files take higher priority in module resolution:
        let src_functools_path = src.join("functools.py");
        db.write_file(&src_functools_path, "FOO: int").unwrap();
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        assert_eq!(functools_module.search_path(&db).unwrap(), &src);
        assert_eq!(
            Ok(functools_module.file(&db).unwrap()),
            system_path_to_file(&db, &src_functools_path)
        );
    }

    #[test]
    fn deleting_file_from_higher_priority_search_path_invalidates_the_query() {
        const SRC: &[FileSpec] = &[("functools.py", "FOO: int")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            src,
            ..
        } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let src_functools_path = src.join("functools.py");

        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        assert_eq!(functools_module.search_path(&db).unwrap(), &src);
        assert_eq!(
            Ok(functools_module.file(&db).unwrap()),
            system_path_to_file(&db, &src_functools_path)
        );

        // If we now delete the first-party file,
        // it should resolve to the stdlib:
        db.memory_file_system()
            .remove_file(&src_functools_path)
            .unwrap();
        File::sync_path(&mut db, &src_functools_path);
        let functools_module = resolve_module_confident(&db, &functools_module_name).unwrap();
        assert_eq!(functools_module.search_path(&db).unwrap(), &stdlib);
        assert_eq!(
            Ok(functools_module.file(&db).unwrap()),
            system_path_to_file(&db, stdlib.join("functools.pyi"))
        );
    }

    #[test]
    fn editable_install_absolute_path() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo/__init__.py", ""), ("/x/src/foo/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(x_directory).unwrap();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_bar_module_name = ModuleName::new_static("foo.bar").unwrap();

        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        let foo_bar_module = resolve_module_confident(&db, &foo_bar_module_name).unwrap();

        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/src/foo/__init__.py")
        );
        assert_eq!(
            foo_bar_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/src/foo/bar.py")
        );
    }

    #[test]
    fn editable_install_pth_file_with_whitespace() {
        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "        /x/src"),
            ("_bar.pth", "/y/src        "),
        ];
        let external_files = [("/x/src/foo.py", ""), ("/y/src/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(external_files).unwrap();

        // Lines with leading whitespace in `.pth` files do not parse:
        let foo_module_name = ModuleName::new_static("foo").unwrap();
        assert_eq!(resolve_module_confident(&db, &foo_module_name), None);

        // Lines with trailing whitespace in `.pth` files do:
        let bar_module_name = ModuleName::new_static("bar").unwrap();
        let bar_module = resolve_module_confident(&db, &bar_module_name).unwrap();
        assert_eq!(
            bar_module.file(&db).unwrap().path(&db),
            &FilePath::system("/y/src/bar.py")
        );
    }

    #[test]
    fn editable_install_relative_path() {
        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "../../x/../x/y/src"),
            ("../x/y/src/foo.pyi", ""),
        ];

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();

        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/y/src/foo.pyi")
        );
    }

    #[test]
    fn editable_install_multiple_pth_files_with_multiple_paths() {
        const COMPLEX_PTH_FILE: &str = "\
/

# a comment
/baz

import not_an_editable_install; do_something_else_crazy_dynamic()

# another comment
spam

not_a_directory
";

        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "../../x/../x/y/src"),
            ("_lots_of_others.pth", COMPLEX_PTH_FILE),
            ("../x/y/src/foo.pyi", ""),
            ("spam/spam.py", ""),
        ];

        let root_files = [("/a.py", ""), ("/baz/b.py", "")];

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(root_files).unwrap();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let a_module_name = ModuleName::new_static("a").unwrap();
        let b_module_name = ModuleName::new_static("b").unwrap();
        let spam_module_name = ModuleName::new_static("spam").unwrap();

        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        let a_module = resolve_module_confident(&db, &a_module_name).unwrap();
        let b_module = resolve_module_confident(&db, &b_module_name).unwrap();
        let spam_module = resolve_module_confident(&db, &spam_module_name).unwrap();

        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/y/src/foo.pyi")
        );
        assert_eq!(
            a_module.file(&db).unwrap().path(&db),
            &FilePath::system("/a.py")
        );
        assert_eq!(
            b_module.file(&db).unwrap().path(&db),
            &FilePath::system("/baz/b.py")
        );
        assert_eq!(
            spam_module.file(&db).unwrap().path(&db),
            &FilePath::System(site_packages.join("spam/spam.py"))
        );
    }

    #[test]
    fn module_resolution_paths_cached_between_different_module_resolutions() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src"), ("_bar.pth", "/y/src")];
        let external_directories = [("/x/src/foo.py", ""), ("/y/src/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(external_directories).unwrap();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let bar_module_name = ModuleName::new_static("bar").unwrap();

        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/src/foo.py")
        );

        db.clear_salsa_events();
        let bar_module = resolve_module_confident(&db, &bar_module_name).unwrap();
        assert_eq!(
            bar_module.file(&db).unwrap().path(&db),
            &FilePath::system("/y/src/bar.py")
        );
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(
            &db,
            dynamic_resolution_paths,
            ModuleResolveModeIngredient::new(&db, ModuleResolveMode::StubsAllowed),
            &events,
        );
    }

    #[test]
    fn deleting_pth_file_on_which_module_resolution_depends_invalidates_cache() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo.py", "")];

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(x_directory).unwrap();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::system("/x/src/foo.py")
        );

        db.memory_file_system()
            .remove_file(site_packages.join("_foo.pth"))
            .unwrap();

        File::sync_path(&mut db, &site_packages.join("_foo.pth"));

        assert_eq!(resolve_module_confident(&db, &foo_module_name), None);
    }

    #[test]
    fn deleting_editable_install_on_which_module_resolution_depends_invalidates_cache() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        db.write_files(x_directory).unwrap();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module_confident(&db, &foo_module_name).unwrap();
        let src_path = SystemPathBuf::from("/x/src");
        assert_eq!(
            foo_module.file(&db).unwrap().path(&db),
            &FilePath::System(src_path.join("foo.py"))
        );

        db.memory_file_system()
            .remove_file(src_path.join("foo.py"))
            .unwrap();
        db.memory_file_system().remove_directory(&src_path).unwrap();
        File::sync_path(&mut db, &src_path.join("foo.py"));
        File::sync_path(&mut db, &src_path);
        assert_eq!(resolve_module_confident(&db, &foo_module_name), None);
    }

    #[test]
    fn no_duplicate_search_paths_added() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("_foo.pth", "/src")])
            .build();

        let search_paths: Vec<&SearchPath> =
            search_paths(&db, ModuleResolveMode::StubsAllowed).collect();

        assert!(search_paths.contains(
            &&SearchPath::first_party(db.system(), SystemPathBuf::from("/src")).unwrap()
        ));
        assert!(
            !search_paths.contains(
                &&SearchPath::editable(db.system(), SystemPathBuf::from("/src")).unwrap()
            )
        );
    }

    #[test]
    fn multiple_site_packages_with_editables() {
        let mut db = TestDb::new();

        let venv_site_packages = SystemPathBuf::from("/venv-site-packages");
        let site_packages_pth = venv_site_packages.join("foo.pth");
        let system_site_packages = SystemPathBuf::from("/system-site-packages");
        let editable_install_location = SystemPathBuf::from("/x/y/a.py");
        let system_site_packages_location = system_site_packages.join("a.py");

        db.memory_file_system()
            .create_directory_all("/src")
            .unwrap();
        db.write_files([
            (&site_packages_pth, "/x/y"),
            (&editable_install_location, ""),
            (&system_site_packages_location, ""),
        ])
        .unwrap();

        db.set_search_paths(
            SearchPathSettings {
                site_packages_paths: vec![venv_site_packages, system_site_packages],
                ..SearchPathSettings::new(vec![SystemPathBuf::from("/src")])
            }
            .to_search_paths(db.system(), db.vendored())
            .expect("Valid search path settings"),
        );

        // The editable installs discovered from the `.pth` file in the first `site-packages` directory
        // take precedence over the second `site-packages` directory...
        let a_module_name = ModuleName::new_static("a").unwrap();
        let a_module = resolve_module_confident(&db, &a_module_name).unwrap();
        assert_eq!(
            a_module.file(&db).unwrap().path(&db),
            &editable_install_location
        );

        db.memory_file_system()
            .remove_file(&site_packages_pth)
            .unwrap();
        File::sync_path(&mut db, &site_packages_pth);

        // ...But now that the `.pth` file in the first `site-packages` directory has been deleted,
        // the editable install no longer exists, so the module now resolves to the file in the
        // second `site-packages` directory
        let a_module = resolve_module_confident(&db, &a_module_name).unwrap();
        assert_eq!(
            a_module.file(&db).unwrap().path(&db),
            &system_site_packages_location
        );
    }

    #[test]
    #[cfg(unix)]
    fn case_sensitive_resolution_with_symlinked_directory() -> anyhow::Result<()> {
        use anyhow::Context;
        use ruff_db::system::OsSystem;

        let temp_dir = tempfile::TempDir::new()?;
        let root = SystemPathBuf::from_path_buf(
            temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalized path")?,
        )
        .expect("UTF8 path for temp dir");

        let mut db = TestDb::new();

        let src = root.join("src");
        let a_package_target = root.join("a-package");
        let a_src = src.join("a");

        db.use_system(OsSystem::new(&root));

        db.write_file(
            a_package_target.join("__init__.py"),
            "class Foo: x: int = 4",
        )
        .context("Failed to write `a-package/__init__.py`")?;

        db.write_file(src.join("main.py"), "print('Hy')")
            .context("Failed to write `main.py`")?;

        // The symlink triggers the slow-path in the `OsSystem`'s
        // `exists_path_case_sensitive` code because canonicalizing the path
        // for `a/__init__.py` results in `a-package/__init__.py`
        std::os::unix::fs::symlink(a_package_target.as_std_path(), a_src.as_std_path())
            .context("Failed to symlink `src/a` to `a-package`")?;

        db.set_search_paths(
            SearchPathSettings::new(vec![src])
                .to_search_paths(db.system(), db.vendored())
                .expect("Valid search path settings"),
        );

        // Now try to resolve the module `A` (note the capital `A` instead of `a`).
        let a_module_name = ModuleName::new_static("A").unwrap();
        assert_eq!(resolve_module_confident(&db, &a_module_name), None);

        // Now lookup the same module using the lowercase `a` and it should
        // resolve to the file in the system site-packages
        let a_module_name = ModuleName::new_static("a").unwrap();
        let a_module = resolve_module_confident(&db, &a_module_name).expect("a.py to resolve");
        assert!(
            a_module
                .file(&db)
                .unwrap()
                .path(&db)
                .as_str()
                .ends_with("src/a/__init__.py"),
        );

        Ok(())
    }

    #[test]
    fn file_to_module_where_one_search_path_is_subdirectory_of_other() {
        let project_directory = SystemPathBuf::from("/project");
        let site_packages = project_directory.join(".venv/lib/python3.13/site-packages");
        let installed_foo_module = site_packages.join("foo/__init__.py");

        let mut db = TestDb::new();
        db.write_file(&installed_foo_module, "").unwrap();

        let search_paths = SearchPathSettings {
            src_roots: vec![project_directory],
            site_packages_paths: vec![site_packages.clone()],
            ..SearchPathSettings::empty()
        }
        .to_search_paths(db.system(), db.vendored())
        .expect("Valid search path settings");
        db.set_search_paths(search_paths);

        let foo_module_file = File::new(&db, FilePath::System(installed_foo_module));
        let module = file_to_module(&db, foo_module_file).unwrap();
        assert_eq!(module.search_path(&db).unwrap(), &site_packages);
    }
}
