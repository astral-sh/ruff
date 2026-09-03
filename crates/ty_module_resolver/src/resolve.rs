/*!
This module principally provides several routines for resolving a particular module
name to a `Module`:

* [`file_to_module`][]: resolves the module `.<self>` (often as the first step in resolving `.`)
* [`stub_file_to_real_module`][]: resolves the runtime module corresponding to a stub file
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
use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::fmt;
use std::iter::FusedIterator;
use std::rc::Rc;

use compact_str::format_compact;
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_db::PythonFile;
use ruff_db::files::{
    DirectoryListing, File, FilePath, FileRootKind, directory_listing, system_path_to_directory,
    system_path_to_file,
};
use ruff_db::source::source_text;
use ruff_db::system::{FileType, System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::{
    self as ast, PySourceType,
    visitor::{Visitor, walk_body},
};

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::{ImportingFile, ModuleName};
use crate::path::{ModulePath, SearchPath, SystemOrVendoredPathRef};
use crate::strategy::MisconfigurationStrategy;
use crate::typeshed::{TypeshedVersions, vendored_typeshed_versions};
use crate::{ResolverEnvironment, ResolverFile, SearchPathSettings, SearchPathSettingsError};

/// Resolves a module name to a module.
pub fn resolve_module<'db>(
    db: &'db dyn Db,
    importing_file: ImportingFile<'db>,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let resolver_environment = importing_file.resolver_environment(db);
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::Typing,
        resolver_environment,
    );

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file.file(db), interned_name))
}

/// Resolves the module referenced by a `from` import statement.
///
/// Returns `None` if the statement does not name a valid module or the module cannot be resolved.
pub fn resolve_module_for_import_from<'db>(
    db: &'db dyn Db,
    importing_file: ImportingFile<'db>,
    import: &ast::StmtImportFrom,
) -> Option<Module<'db>> {
    let module_name = ModuleName::from_import_statement(db, importing_file, import).ok()?;
    resolve_module(db, importing_file, &module_name)
}

/// Resolves a module name to a module, without desperate resolution available.
///
/// This is appropriate for resolving a `KnownModule`, or cases where for whatever reason
/// we don't have a well-defined importing file.
pub fn resolve_module_confident<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::Typing,
        resolver_environment,
    );

    resolve_module_query(db, interned_name)
}

/// Resolves a module name to a module (stubs not allowed).
pub fn resolve_real_module<'db>(
    db: &'db dyn Db,
    importing_file: ImportingFile<'db>,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let resolver_environment = importing_file.resolver_environment(db);
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::Runtime,
        resolver_environment,
    );

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file.file(db), interned_name))
}

/// Resolves a module name to a module, without desperate resolution available (stubs not allowed).
///
/// This is appropriate for resolving a `KnownModule`, or cases where for whatever reason
/// we don't have a well-defined importing file.
pub fn resolve_real_module_confident<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::Runtime,
        resolver_environment,
    );

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
    importing_file: ImportingFile<'db>,
    module_name: &ModuleName,
) -> Option<Module<'db>> {
    let resolver_environment = importing_file.resolver_environment(db);
    let interned_name = ModuleNameIngredient::new(
        db,
        module_name,
        ModuleResolveMode::RuntimeSomeShadowingAllowed,
        resolver_environment,
    );

    resolve_module_query(db, interned_name)
        .or_else(|| desperately_resolve_module(db, importing_file.file(db), interned_name))
}

/// Selects typing or runtime module-resolution semantics.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, get_size2::GetSize)]
pub enum ModuleResolveMode {
    /// Resolve modules for type checking, preferring stubs over runtime implementations.
    ///
    /// This is the "normal" mode almost everything uses, as type checkers are in fact supposed
    /// to *prefer* stubs over the actual implementations.
    Typing,

    /// Resolve modules to their runtime implementations without considering stubs.
    ///
    /// This is the "goto definition" mode, where we need to ignore the typing spec and find actual
    /// implementations. When querying searchpaths this also notably replaces typeshed with
    /// the "real" stdlib.
    Runtime,

    /// Like [`ModuleResolveMode::Runtime`], but permits some modules to be shadowed.
    ///
    /// In particular, this allows `typing_extensions` to be shadowed by a
    /// non-standard library module. This is useful in the context of the LSP
    /// where we don't want to pretend as if these modules are always available
    /// at runtime.
    RuntimeSomeShadowingAllowed,
}

#[salsa::interned(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub(crate) struct ModuleResolveModeIngredient<'db> {
    #[returns(copy)]
    resolver_environment: ResolverEnvironment<'db>,
    #[returns(copy)]
    mode: ModuleResolveMode,
}

impl ModuleResolveMode {
    fn is_typing(self) -> bool {
        matches!(self, Self::Typing)
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
            ModuleResolveMode::Typing | ModuleResolveMode::Runtime => {
                module_name == "typing_extensions"
            }
            ModuleResolveMode::RuntimeSomeShadowingAllowed => false,
        }
    }
}

/// Salsa query that resolves an interned [`ModuleNameIngredient`] to a module.
///
/// This query should not be called directly. Instead, use [`resolve_module`]. It only exists
/// because Salsa requires the module name to be an ingredient.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn resolve_module_query<'db>(
    db: &'db dyn Db,
    module_name: ModuleNameIngredient<'db>,
) -> Option<Module<'db>> {
    let name = module_name.name(db);
    let mode = module_name.mode(db);
    let resolver_environment = module_name.resolver_environment(db);
    let _span = tracing::trace_span!("resolve_module", %name).entered();

    let resolved = NameResolver::new(db, resolver_environment, mode).resolve(name);
    if resolved.is_none() {
        tracing::debug!("Module `{name}` not found in search paths");
    }
    resolved
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
/// Cache desperate resolution because repeated unresolved imports in a project can otherwise
/// re-walk the same importing-file-relative search paths many times.
#[salsa::tracked(returns(copy))]
fn desperately_resolve_module<'db>(
    db: &'db dyn Db,
    importing_file: File,
    module_name: ModuleNameIngredient<'db>,
) -> Option<Module<'db>> {
    let name = module_name.name(db);
    let mode = module_name.mode(db);
    let resolver_environment = module_name.resolver_environment(db);
    let _span = tracing::trace_span!("desperately_resolve_module", %name).entered();

    let Some(resolved) =
        desperately_resolve_name(db, importing_file, resolver_environment, name, mode)
    else {
        let mode = match mode {
            ModuleResolveMode::Typing => "typing mode",
            ModuleResolveMode::Runtime => "runtime mode",
            ModuleResolveMode::RuntimeSomeShadowingAllowed => {
                "runtime mode with some shadowing allowed"
            }
        };
        tracing::debug!("Module `{name}` not found while looking in parent dirs ({mode})");
        return None;
    };

    resolved
        .into_iter()
        .next()
        .map(|candidate| candidate.into_module(db, resolver_environment, name))
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via any of the known search paths.
#[allow(unused)]
pub(crate) fn path_to_module<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    path: &FilePath,
) -> Option<Module<'db>> {
    // It's not entirely clear on first sight why this method calls `file_to_module` instead of
    // it being the other way round, considering that the first thing that `file_to_module` does
    // is to retrieve the file's path.
    //
    // The reason is that `file_to_module` is a tracked Salsa query and salsa queries require that
    // all arguments are Salsa ingredients (something stored in Salsa). `Path`s aren't salsa ingredients but
    // `VfsFile` is. So what we do here is to retrieve the `path`'s `VfsFile` so that we can make
    // use of Salsa's caching and invalidation.
    let file = path.to_file(db)?;
    file_to_module(db, ResolverFile::new(db, file, resolver_environment))
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via any of the known search paths.
///
/// This function can be understood as essentially resolving `import .<self>` in the file itself,
/// and indeed, one of its primary jobs is resolving `.<self>` to derive the module name of `.`.
/// This intuition is particularly useful for understanding why it's correct that we pass
/// the file itself as `importing_file` to various subroutines.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
pub fn file_to_module<'db>(
    db: &'db dyn Db,
    resolver_file: ResolverFile<'db>,
) -> Option<Module<'db>> {
    let resolver_environment = resolver_file.environment(db);
    let file = resolver_file.file(db);
    let _span = tracing::trace_span!("file_to_module", ?file).entered();

    let path = SystemOrVendoredPathRef::try_from_file(db, file)?;

    file_to_module_impl(
        db,
        resolver_file,
        path,
        search_paths(db, resolver_environment, ModuleResolveMode::Typing),
    )
    .or_else(|| {
        file_to_module_impl(
            db,
            resolver_file,
            path,
            relative_desperate_search_paths(db, resolver_file).iter(),
        )
    })
}

/// Resolves the runtime module corresponding to a stub file.
///
/// Modules that are only available as stubs, including built-in modules, return `None`.
pub fn stub_file_to_real_module<'db>(
    db: &'db dyn Db,
    resolver_file: ResolverFile<'db>,
) -> Option<Module<'db>> {
    debug_assert!(resolver_file.file(db).is_stub(db));

    let module = file_to_module(db, resolver_file)?;
    // Built-in modules have no source file to find. Checking here also avoids a failed
    // resolution attempt that would emit misleading logs.
    if ruff_python_stdlib::sys::is_builtin_module(module.python_version(db).minor, module.name(db))
    {
        return None;
    }
    // This lookup is equivalent to resolving `.<self>` from the stub, so the stub is the correct
    // importing file.
    resolve_real_module(
        db,
        ImportingFile::ResolverFile(resolver_file),
        module.name(db),
    )
}

fn file_to_module_impl<'db, 'a>(
    db: &'db dyn Db,
    resolver_file: ResolverFile<'db>,
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
    let module = resolve_module(db, ImportingFile::ResolverFile(resolver_file), &module_name)?;
    let module_file = module.file(db)?;

    let file: File = resolver_file.file(db);
    let file_path = file.path(db);
    if file_path == module_file.path(db) {
        return Some(module);
    } else if file.source_type(db) == PySourceType::Python
        && module_file.source_type(db) == PySourceType::Stub
    {
        // If a .py and .pyi are both defined, the .pyi will be the one returned by `resolve_module().file`,
        // which would make us erroneously believe the `.py` is *not* also this module (breaking things
        // like relative imports). So here we try `resolve_real_module().file` to cover both cases.
        let module =
            resolve_real_module(db, ImportingFile::ResolverFile(resolver_file), &module_name)?;
        let module_file = module.file(db)?;
        if file_path == module_file.path(db) {
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

pub fn search_paths<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    resolve_mode: ModuleResolveMode,
) -> SearchPathIterator<'db> {
    let search_paths = resolver_environment.search_paths(db);

    SearchPathIterator {
        db,
        static_paths: search_paths.static_paths.iter(),
        stdlib_path: search_paths.stdlib(resolve_mode),
        dynamic_paths: None,
        mode: ModuleResolveModeIngredient::new(db, resolver_environment, resolve_mode),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct StubPackagePaths<'a> {
    before_stdlib: &'a [SearchPath],
    after_stdlib: &'a [SearchPath],
}

impl StubPackagePaths<'_> {
    fn is_empty(self) -> bool {
        self.before_stdlib.is_empty() && self.after_stdlib.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
struct StubPackageIndex {
    paths: Box<[SearchPath]>,
    stdlib_offset: usize,
}

impl StubPackageIndex {
    /// Indexes search paths that may contain a stub package, preserving their position relative to
    /// the standard library.
    fn from_search_paths<'a>(
        db: &dyn Db,
        search_paths: impl Iterator<Item = &'a SearchPath>,
    ) -> Self {
        let mut paths = Vec::new();
        let mut stdlib_offset = None;

        for search_path in search_paths {
            if search_path.is_standard_library() {
                stdlib_offset = Some(paths.len());
            } else if search_path_may_contain_stub_package(db, search_path) {
                paths.push(search_path.clone());
            }
        }

        let stdlib_offset = stdlib_offset.unwrap_or(paths.len());
        Self {
            paths: paths.into_boxed_slice(),
            stdlib_offset,
        }
    }

    /// Returns all indexed paths in normal typing-resolution order.
    fn all(&self) -> StubPackagePaths<'_> {
        StubPackagePaths {
            before_stdlib: self.before_stdlib(),
            after_stdlib: self.after_stdlib(),
        }
    }

    /// Splits the indexed paths between the stub-overlay pass and its normal fallback.
    ///
    /// The overlay contains only extra paths, which all precede stdlib. The fallback retains the
    /// remaining paths' positions relative to stdlib.
    fn split_overlay(&self) -> (StubPackagePaths<'_>, StubPackagePaths<'_>) {
        let before_stdlib = self.before_stdlib();
        let (extra, remaining) =
            before_stdlib.split_at(before_stdlib.partition_point(SearchPath::is_extra));

        (
            StubPackagePaths {
                before_stdlib: extra,
                after_stdlib: &[],
            },
            StubPackagePaths {
                before_stdlib: remaining,
                after_stdlib: self.after_stdlib(),
            },
        )
    }

    /// Returns indexed paths that precede stdlib in normal typing resolution.
    fn before_stdlib(&self) -> &[SearchPath] {
        &self.paths[..self.stdlib_offset]
    }

    /// Returns indexed paths that follow stdlib in normal typing resolution.
    fn after_stdlib(&self) -> &[SearchPath] {
        &self.paths[self.stdlib_offset..]
    }
}

/// Returns an index of search paths that may contain a top-level stub package, preserving their
/// resolution order relative to stdlib.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn stub_package_index(
    db: &dyn Db,
    resolver_environment: ResolverEnvironment<'_>,
) -> StubPackageIndex {
    StubPackageIndex::from_search_paths(
        db,
        search_paths(db, resolver_environment, ModuleResolveMode::Typing),
    )
}

fn search_path_may_contain_stub_package(db: &dyn Db, search_path: &SearchPath) -> bool {
    let Some(path) = search_path.as_system_path() else {
        return false;
    };

    directory_listing(db, path)
        .is_ok_and(|listing| listing.iter().any(|(name, _)| name.ends_with("-stubs")))
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
#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn absolute_desperate_search_paths(
    db: &dyn Db,
    importing_file: ResolverFile<'_>,
) -> Option<Box<[SearchPath]>> {
    let resolver_environment = importing_file.environment(db);
    let importing_file = importing_file.file(db);
    let system = db.system();
    let importing_path = importing_file.path(db).as_system_path()?;

    // Only allow this if the importing_file is under the first-party search path
    let (base_path, rel_path) = search_paths(db, resolver_environment, ModuleResolveMode::Typing)
        .find_map(|search_path| {
        if !search_path.is_first_party() {
            return None;
        }
        Some((
            search_path.as_system_path()?,
            search_path.relativize_system_path_only(importing_path)?,
        ))
    })?;

    // Only allow searching up to the first-party path's root
    let mut search_paths = Vec::new();
    for rel_dir in rel_path.ancestors() {
        let candidate_path = base_path.join(rel_dir);
        let Ok(listing) = directory_listing(db, &candidate_path) else {
            continue;
        };
        // Any dir that isn't a proper package is plausibly some test/script dir that could be
        // added as a search-path at runtime. Notably this reflects pytest's default mode where
        // it adds every dir with a .py to the search-paths (making all test files root modules),
        // unless they see an `__init__.py`, in which case they assume you don't want that.
        let isnt_regular_package = !listing.entry_is_file(db, &candidate_path, "__init__.py")
            && !listing.entry_is_file(db, &candidate_path, "__init__.pyi");
        // Any dir with a pyproject.toml or ty.toml is a valid relative desperate search-path and
        // we want all of those to also be valid absolute desperate search-paths. It doesn't
        // make any sense for a folder to have `pyproject.toml` and `__init__.py` but let's
        // not let something cursed and spooky happen, ok? d
        if isnt_regular_package
            || listing.entry_is_file(db, &candidate_path, "pyproject.toml")
            || listing.entry_is_file(db, &candidate_path, "ty.toml")
        {
            let search_path = SearchPath::first_party(system, candidate_path).ok()?;
            search_paths.push(search_path);
        }
    }

    if search_paths.is_empty() {
        None
    } else {
        Some(search_paths.into_boxed_slice())
    }
}

/// Get the search-paths for desperate resolution of relative imports in this file.
///
/// Currently this is "the closest ancestor dir that contains a pyproject.toml (or ty.toml)",
/// which is a completely arbitrary decision. However it's fairly important that relative
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
#[salsa::tracked(returns(clone), heap_size=ruff_memory_usage::heap_size)]
fn relative_desperate_search_paths(
    db: &dyn Db,
    importing_file: ResolverFile<'_>,
) -> Option<SearchPath> {
    let resolver_environment = importing_file.environment(db);
    let importing_file = importing_file.file(db);
    let system = db.system();
    let importing_path = importing_file.path(db).as_system_path()?;

    // Only allow this if the importing_file is under the first-party search path
    let (base_path, rel_path) = search_paths(db, resolver_environment, ModuleResolveMode::Typing)
        .find_map(|search_path| {
        if !search_path.is_first_party() {
            return None;
        }
        Some((
            search_path.as_system_path()?,
            search_path.relativize_system_path_only(importing_path)?,
        ))
    })?;

    // Only allow searching up to the first-party path's root
    for rel_dir in rel_path.ancestors() {
        let candidate_path = base_path.join(rel_dir);
        let Ok(listing) = directory_listing(db, &candidate_path) else {
            continue;
        };
        // Any dir with a pyproject.toml or ty.toml might be a project root
        if listing.entry_is_file(db, &candidate_path, "pyproject.toml")
            || listing.entry_is_file(db, &candidate_path, "ty.toml")
        {
            let search_path = SearchPath::first_party(system, candidate_path).ok()?;
            return Some(search_path);
        }
    }

    None
}
#[derive(Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
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
    /// ([`ModuleResolveMode::Runtime`]).
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

impl fmt::Debug for SearchPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            static_paths,
            stdlib_path,
            real_stdlib_path,
            site_packages,
            // Omit `typeshed_versions` because its debug representation spans thousands of lines,
            // making even simple `Type` debug representations impractically large.
            typeshed_versions: _,
        } = self;

        f.debug_struct("SearchPaths")
            .field("static_paths", static_paths)
            .field("stdlib_path", stdlib_path)
            .field("real_stdlib_path", real_stdlib_path)
            .field("site_packages", site_packages)
            .finish_non_exhaustive()
    }
}

impl SearchPaths {
    /// Validate and normalize the raw settings given by the user
    /// into settings we can use for module resolution
    ///
    /// This method also implements the typing spec's [module resolution order].
    ///
    /// [module resolution order]: https://typing.python.org/en/latest/spec/distributing.html#import-resolution-ordering
    pub(crate) fn from_settings<Strategy: MisconfigurationStrategy>(
        settings: &SearchPathSettings,
        system: &dyn System,
        vendored: &VendoredFileSystem,
        strategy: &Strategy,
    ) -> Result<Self, Strategy::Error<SearchPathSettingsError>> {
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
        } = settings;

        let mut static_paths = vec![];

        for path in extra_paths {
            let path = canonicalize(path, system);
            tracing::debug!("Adding extra search-path `{path}`");

            let path = strategy.fallback_opt(
                SearchPath::extra(system, path).map_err(SearchPathSettingsError::from),
                |err| {
                    tracing::debug!("Skipping invalid extra search-path: {err}");
                },
            )?;
            static_paths.extend(path);
        }

        for src_root in src_roots {
            tracing::debug!("Adding first-party search path `{src_root}`");
            let path = strategy.fallback_opt(
                SearchPath::first_party(system, src_root.to_path_buf())
                    .map_err(SearchPathSettingsError::from),
                |err| {
                    tracing::debug!("Skipping invalid first-party search-path: {err}");
                },
            )?;
            static_paths.extend(path);
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

            strategy.fallback(results, |err| {
                tracing::debug!("Skipping custom-stdlib search-path: {err}");
                (
                    vendored_typeshed_versions(vendored),
                    SearchPath::vendored_stdlib(),
                )
            })?
        } else {
            tracing::debug!("Using vendored stdlib");
            (
                vendored_typeshed_versions(vendored),
                SearchPath::vendored_stdlib(),
            )
        };

        let real_stdlib_path = if let Some(path) = real_stdlib_path {
            strategy.fallback_opt(
                SearchPath::real_stdlib(system, path.clone())
                    .map_err(SearchPathSettingsError::from),
                |err| {
                    tracing::debug!("Skipping invalid real-stdlib search-path: {err}");
                },
            )?
        } else {
            None
        };

        let mut site_packages: Vec<_> = Vec::with_capacity(site_packages_paths.len());

        for path in site_packages_paths {
            tracing::debug!("Adding site-packages search path `{path}`");
            let path = strategy.fallback_opt(
                SearchPath::site_packages(system, path.clone())
                    .map_err(SearchPathSettingsError::from),
                |err| {
                    tracing::debug!("Skipping invalid site-packages search-path: {err}");
                },
            )?;
            site_packages.extend(path);
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
    /// The vendored standard library remains available.
    pub fn empty(vendored: &VendoredFileSystem) -> Self {
        Self {
            static_paths: vec![],
            stdlib_path: Some(SearchPath::vendored_stdlib()),
            real_stdlib_path: None,
            site_packages: vec![],
            typeshed_versions: vendored_typeshed_versions(vendored),
        }
    }

    /// Returns the configured roots for first-party modules.
    pub fn first_party_roots(&self) -> impl Iterator<Item = &SystemPath> {
        self.static_paths
            .iter()
            .filter(|path| path.is_first_party())
            .filter_map(SearchPath::as_system_path)
    }

    /// Registers file roots for all non-dynamically discovered search paths.
    pub fn try_register_static_roots(&self, db: &dyn Db) {
        let files = db.files();
        for path in self
            .static_paths
            .iter()
            .chain(self.site_packages.iter())
            .chain(&self.stdlib_path)
        {
            if let Some(system_path) = path.as_system_path() {
                // Nested first-party paths reuse the project root. Other nested paths, such as
                // site-packages inside `.venv`, need their own search-path root.
                if !path.is_first_party() || files.root(db, system_path).is_none() {
                    files.try_add_root(db, system_path, FileRootKind::SearchPath);
                }
            }
        }
    }

    fn stdlib(&self, mode: ModuleResolveMode) -> Option<&SearchPath> {
        match mode {
            ModuleResolveMode::Typing => self.stdlib_path.as_ref(),
            ModuleResolveMode::Runtime | ModuleResolveMode::RuntimeSomeShadowingAllowed => {
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

/// Returns the validated roots listed in the environment's `.pth` files.
///
/// Unlike [`search_paths`], this includes editable roots that are also first-party search paths.
/// Those `.pth` entries still identify installed source trees, even though adding their paths to
/// module resolution a second time would be redundant.
pub fn editable_search_paths<'db>(
    db: &'db dyn Db,
    environment: ResolverEnvironment<'db>,
) -> impl Iterator<Item = &'db SystemPath> {
    site_packages_editables(db, environment)
        .iter()
        .flat_map(|paths| paths.editables.iter())
        .filter_map(SearchPath::as_system_path)
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
struct SitePackagesEditables {
    site_packages: SearchPath,
    editables: Box<[SearchPath]>,
}

/// Discover editable roots without discarding entries that overlap static search paths.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn site_packages_editables<'db>(
    db: &'db dyn Db,
    environment: ResolverEnvironment<'db>,
) -> Box<[SitePackagesEditables]> {
    let mut paths = Vec::new();
    let system = db.system();

    for site_packages in &environment.search_paths(db).site_packages {
        let site_packages_dir = site_packages
            .as_system_path()
            .expect("Expected site package path to be a system path");

        // As well as modules installed directly into `site-packages`,
        // the directory may also contain `.pth` files.
        // Each `.pth` file in `site-packages` may contain one or more lines
        // containing a (relative or absolute) path.
        // Each of these paths may point to an editable install of a package,
        // so should be considered an additional search path.
        let listing = match directory_listing(db, site_packages_dir) {
            Ok(listing) => listing,
            Err(error) => {
                tracing::warn!(
                    "Failed to search for editable installation in {site_packages_dir}: {error}"
                );
                paths.push(SitePackagesEditables {
                    site_packages: site_packages.clone(),
                    editables: Box::default(),
                });
                continue;
            }
        };

        let mut editables = Vec::new();

        // The Python documentation specifies that `.pth` files in `site-packages`
        // are processed in alphabetical order. `DirectoryListing` is already sorted.
        // https://docs.python.org/3/library/site.html#module-site
        let pth_files = listing.iter().filter(|(name, file_type)| {
            !file_type.is_directory() && SystemPath::new(name).extension() == Some("pth")
        });

        for (name, _) in pth_files {
            let path = site_packages_dir.join(name);
            // Track each `.pth` file independently so content changes invalidate this query.
            let Ok(file) = system_path_to_file(db, &path).inspect_err(|error| {
                tracing::warn!("Failed to open .pth file `{path}`: {error}");
            }) else {
                continue;
            };
            let contents = source_text(db, file);
            if let Some(error) = contents.read_error() {
                tracing::warn!("Failed to read .pth file `{path}`: {error}");
                continue;
            }

            let installations = contents.lines().filter_map(|line| {
                let line = line.trim_end();
                if line.is_empty()
                    || line.starts_with('#')
                    || line.starts_with("import ")
                    || line.starts_with("import\t")
                {
                    return None;
                }

                Some(SystemPath::absolute(line, site_packages_dir))
            });

            for installation in installations {
                let installation = system
                    .canonicalize_path(&installation)
                    .unwrap_or(installation);

                match SearchPath::editable(system, installation) {
                    Ok(search_path) => editables.push(search_path),
                    Err(error) => {
                        tracing::debug!("Skipping editable installation: {error}");
                    }
                }
            }
        }

        paths.push(SitePackagesEditables {
            site_packages: site_packages.clone(),
            editables: editables.into_boxed_slice(),
        });
    }

    paths.into_boxed_slice()
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
) -> Box<[SearchPath]> {
    tracing::debug!("Resolving dynamic module resolution paths");

    let environment = mode.resolver_environment(db);
    let site_packages = site_packages_editables(db, environment);
    if site_packages.is_empty() {
        return Box::default();
    }

    let search_paths = environment.search_paths(db);
    let mut existing_paths: FxHashSet<_> = search_paths
        .static_paths
        .iter()
        .filter_map(SearchPath::as_system_path)
        .collect();

    if let Some(path) = search_paths
        .stdlib(mode.mode(db))
        .and_then(SearchPath::as_system_path)
    {
        existing_paths.insert(path);
    }

    let mut dynamic_paths = Vec::new();
    let files = db.files();

    for paths in site_packages {
        let site_packages_dir = paths
            .site_packages
            .as_system_path()
            .expect("Expected site package path to be a system path");
        if !existing_paths.insert(site_packages_dir) {
            continue;
        }
        dynamic_paths.push(paths.site_packages.clone());

        for search_path in &paths.editables {
            let Some(path) = search_path.as_system_path() else {
                continue;
            };
            if !existing_paths.insert(path) {
                continue;
            }
            tracing::debug!("Adding editable installation to module resolution path {path}");

            // Register a file root for editable installs that are outside any other root
            // (Most importantly, don't register a root for editable installations from the project
            // directory as that would change the durability of files within those folders).
            // Not having an exact file root for editable installs just means that
            // some queries will run slightly more frequently than they would otherwise.
            if files.root(db, path).is_none() {
                files.try_add_root(db, path, FileRootKind::SearchPath);
            }
            dynamic_paths.push(search_path.clone());
        }
    }

    dynamic_paths.into_boxed_slice()
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

/// A thin wrapper around a module name, resolution mode, and resolver environment to make them a Salsa
/// ingredient.
///
/// This is needed because Salsa requires that all query arguments are salsa ingredients.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct ModuleNameIngredient<'db> {
    #[returns(ref)]
    pub(super) name: ModuleName,
    #[returns(copy)]
    pub(super) mode: ModuleResolveMode,
    #[returns(copy)]
    pub(super) resolver_environment: ResolverEnvironment<'db>,
}

/// Like `NameResolver::resolve` but for cases where it failed to resolve the module
/// and we are now Getting Desperate and willing to try the ancestor directories of
/// the `importing_file` as potential temporary search paths that are private
/// to this import.
///
/// These paths can contain PEP 561 stub packages, but never user-provided extra paths, so typing
/// resolution indexes them for stub packages without performing a separate stub-overlay pass.
/// Runtime resolution instead ignores stub packages and `.pyi` files entirely.
fn desperately_resolve_name<'db>(
    db: &'db dyn Db,
    importing_file: File,
    resolver_environment: ResolverEnvironment<'db>,
    name: &ModuleName,
    mode: ModuleResolveMode,
) -> Option<ResolvedNames<'db>> {
    let importing_file = ResolverFile::new(db, importing_file, resolver_environment);
    let search_paths = absolute_desperate_search_paths(db, importing_file).unwrap_or_default();
    let context = ResolverContext::new(db, resolver_environment, mode);
    let stub_packages = mode
        .is_typing()
        .then(|| StubPackageIndex::from_search_paths(db, search_paths.iter()));
    let mut candidates = discover_roots(
        &context,
        name.first_component(),
        mode.is_non_shadowable(resolver_environment.python_version(db).minor, name.as_str()),
        search_paths.iter(),
        stub_packages
            .as_ref()
            .map_or_else(StubPackagePaths::default, StubPackageIndex::all),
    );
    let mut components = name.components().skip(1).peekable();
    candidates = normalize_candidates(db, candidates, components.peek().is_some());
    while let Some(component) = components.next() {
        candidates = advance_candidates(
            &context,
            candidates,
            component,
            ComponentFileFilter::ByMode,
            components.peek().is_some(),
        );
    }

    (!candidates.is_empty()).then_some(candidates)
}

#[derive(Debug, Clone, Copy)]
enum ResolvedModule {
    NamespacePackage,
    LegacyNamespacePackage(File),
    RegularPackage(File),
    Module(File),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComponentFileFilter {
    /// Prefer `.pyi` over `.py` in typing mode, or only accept `.py` in runtime mode.
    ByMode,

    /// Only accept a `.pyi` file.
    StubOnly,
}

/// Where a candidate sits in the typing specification's module-resolution order.
///
/// Variants are declared from highest to lowest precedence so that derived ordering can be used
/// when traversing candidates. This is a precedence tier rather than a total ordering: the stable
/// sorts used by the resolver preserve search-path order between candidates in the same tier.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum CandidatePrecedence {
    /// A PEP 561 stub-only package named `<package>-stubs`.
    ///
    /// Stub packages take precedence over candidates for `<package>` regardless of where those
    /// candidates appear in the search-path order.
    StubPackage,

    /// A candidate whose precedence is determined by search-path order.
    ///
    /// This includes `.pyi` and `.py` packages and modules from extra paths, first-party code,
    /// editable installs, site-packages, and the standard library.
    SearchPathOrder,
}

#[derive(Debug, Clone)]
struct ModuleResolutionCandidate<'db> {
    path: ModulePath,
    /// Borrowed entries at `path`, shared by sibling lookups and cleared when advancing.
    directory: OnceCell<ModuleDirectory<'db>>,
    module: ResolvedModule,
    py_typed: PyTyped,
    precedence: CandidatePrecedence,
}

impl<'db> ModuleResolutionCandidate<'db> {
    fn root(search_path: &SearchPath) -> Self {
        Self::with_precedence(search_path, CandidatePrecedence::SearchPathOrder)
    }

    fn stub(search_path: &SearchPath) -> Self {
        Self::with_precedence(search_path, CandidatePrecedence::StubPackage)
    }

    fn with_precedence(search_path: &SearchPath, precedence: CandidatePrecedence) -> Self {
        Self {
            path: search_path.to_module_path(),
            directory: OnceCell::new(),
            module: ResolvedModule::NamespacePackage,
            py_typed: PyTyped::Untyped,
            precedence,
        }
    }

    fn directory(&self, context: &ResolverContext<'db>) -> ModuleDirectory<'db> {
        *self
            .directory
            .get_or_init(|| ModuleDirectory::new(context, &self.path))
    }

    // Is this some kind of namespace package?
    fn is_any_namespace_package(&self) -> bool {
        match self.module {
            ResolvedModule::NamespacePackage => true,
            ResolvedModule::LegacyNamespacePackage(_) => true,
            ResolvedModule::RegularPackage(_) => false,
            ResolvedModule::Module(_) => false,
        }
    }

    // This is the module we were actually interested in resolving, complete the resolution
    fn into_module(
        self,
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
        name: &ModuleName,
    ) -> Module<'db> {
        match self.module {
            ResolvedModule::NamespacePackage => {
                tracing::trace!("Resolve namespace package `{name}`");
                Module::namespace_package(db, resolver_environment, Cow::Borrowed(name))
            }
            ResolvedModule::LegacyNamespacePackage(file) => {
                // legacy namespace packages behave like regular packages
                // when they're the target of the resolution
                tracing::trace!(
                    "Resolved legacy namespace package `{name}` to `{path}`",
                    path = file.path(db)
                );
                Module::file_module(
                    db,
                    file,
                    resolver_environment,
                    Cow::Borrowed(name),
                    ModuleKind::Package,
                    self.path.into_search_path(),
                )
            }
            ResolvedModule::RegularPackage(file) => {
                tracing::trace!(
                    "Resolved package `{name}` to `{path}`",
                    path = file.path(db)
                );
                Module::file_module(
                    db,
                    file,
                    resolver_environment,
                    Cow::Borrowed(name),
                    ModuleKind::Package,
                    self.path.into_search_path(),
                )
            }
            ResolvedModule::Module(file) => {
                tracing::trace!("Resolved module `{name}` to `{path}`", path = file.path(db));
                Module::file_module(
                    db,
                    file,
                    resolver_environment,
                    Cow::Borrowed(name),
                    ModuleKind::Module,
                    self.path.into_search_path(),
                )
            }
        }
    }

    fn missing_submodule_is_terminal(&self) -> bool {
        if matches!(self.py_typed, PyTyped::Partial) {
            return false;
        }

        // Regular packages and modules are both terminal. A `foo.py`
        // in a higher-priority search path is not shadowed by
        // `foo/__init__.py` in a lower-priority one. Note that both
        // shadow namespace packages.
        matches!(
            self.module,
            ResolvedModule::RegularPackage(_) | ResolvedModule::Module(_)
        )
    }

    fn to_str<'a>(&self, db: &'a dyn Db) -> Cow<'a, str> {
        match self.module {
            ResolvedModule::NamespacePackage => {
                Cow::Owned(self.path.to_system_path().unwrap_or_default().to_string())
            }
            ResolvedModule::LegacyNamespacePackage(file) => Cow::Borrowed(file.path(db).as_str()),
            ResolvedModule::RegularPackage(file) => Cow::Borrowed(file.path(db).as_str()),
            ResolvedModule::Module(file) => Cow::Borrowed(file.path(db).as_str()),
        }
    }
}

/// Directory information reused while resolving children of one candidate.
/// System entries borrow the existing Salsa result; vendored paths use the immutable filesystem.
/// Symlinks remain possible files or directories until normal resolution checks their targets.
#[derive(Debug, Clone, Copy)]
enum ModuleDirectory<'db> {
    System(Option<&'db DirectoryListing>),
    Vendored,
}

impl<'db> ModuleDirectory<'db> {
    fn new(context: &ResolverContext<'db>, path: &ModulePath) -> Self {
        match path.to_system_path() {
            Some(path) => Self::System(directory_listing(context.db, &path).ok()),
            None => Self::Vendored,
        }
    }

    fn may_contain_directory(
        self,
        context: &ResolverContext,
        path: &ModulePath,
        name: &str,
    ) -> bool {
        match self {
            Self::System(listing) => matches!(
                listing.and_then(|listing| listing.file_type(name)),
                Some(FileType::Directory | FileType::Symlink)
            ),
            Self::Vendored => path
                .to_vendored_path()
                .is_some_and(|path| context.vendored().is_directory(path.join(name))),
        }
    }

    fn may_contain_file(self, name: &str, extension: &str) -> bool {
        match self {
            Self::System(listing) => matches!(
                listing.and_then(|listing| {
                    listing.file_type(&format_compact!("{name}.{extension}"))
                }),
                Some(FileType::File | FileType::Symlink)
            ),
            // Vendored paths have no cached listing; file lookup checks the archive directly.
            Self::Vendored => true,
        }
    }
}

/// Resolves module names against an environment's configured search paths.
pub(crate) struct NameResolver<'db> {
    context: ResolverContext<'db>,
}

impl<'db> NameResolver<'db> {
    /// Creates a resolver for the given environment and resolution mode.
    pub(crate) fn new(
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
        mode: ModuleResolveMode,
    ) -> Self {
        Self {
            context: ResolverContext::new(db, resolver_environment, mode),
        }
    }

    /// Resolves a complete module name in one of two modes: typing or runtime.
    ///
    /// For example, consider these files, with `extra` configured as an extra search path:
    ///
    /// ```text
    /// extra
    /// └── acme
    ///     └── patched.pyi
    /// site-packages
    /// ├── acme-stubs
    /// │   ├── __init__.pyi
    /// │   ├── py.typed          # contains "partial"
    /// │   └── stubbed.pyi
    /// └── acme
    ///     ├── __init__.py
    ///     ├── patched.py
    ///     ├── stubbed.py
    ///     └── source_only.py
    /// ```
    ///
    /// - **Typing mode** first tries extra-path stub overlays when resolving a submodule.
    ///   Here, `acme.patched` resolves to `extra/acme/patched.pyi`. While following a dotted name,
    ///   the overlay search may traverse namespace packages or packages defined by `__init__.py`
    ///   for the preceding components (i.e., `acme`). It succeeds only if the requested module
    ///   (`acme.patched`) is defined by a `.pyi` file; finding only `patched.py` would not suffice.
    ///   If no overlay supplies the submodule, a full search considers stubs and runtime modules
    ///   under typing precedence: `acme.stubbed` resolves to `acme-stubs/stubbed.pyi`, while
    ///   `acme.source_only` resolves to `acme/source_only.py`[^1]. Top-level names (e.g., `acme`)
    ///   go directly through the full search.
    ///
    /// - **Runtime mode** searches only runtime modules, ignoring stub packages and `.pyi` files.
    ///   `acme.patched`, `acme.stubbed`, and `acme.source_only` therefore resolve to their `.py`
    ///   files under `site-packages/acme`. It also uses the real standard library instead of
    ///   typeshed's stubs.
    ///
    /// [^1]: The `partial` marker allows the full search to use runtime modules missing from the
    /// stub package. Without this marker, the stub package is treated as complete, so
    /// `acme.source_only` would not resolve.
    fn resolve(&self, name: &ModuleName) -> Option<Module<'db>> {
        let mut search = ModuleSearch::new(self);
        let mut components = name.components().peekable();

        while let Some(component) = components.next() {
            if components.peek().is_none() {
                return search.resolve_child(component);
            }
            search = search.enter_package(component)?;
        }

        None
    }
}

/// Enumerates immediate submodules using the resolver's candidate selection.
///
/// Filesystem entries provide possible names; module resolution determines what each name
/// refers to. Enumeration then applies its eligibility rules without changing precedence.
pub(crate) struct SubmoduleEnumeration<'resolver, 'db> {
    search: ModuleSearch<'resolver, 'db>,
    /// An explicitly resolved package may have been reached through symlinks. Only entries
    /// below its directory need checking; other namespace portions keep their ancestry checks.
    known_package_directory: Option<&'db SystemPath>,
}

impl<'resolver, 'db> SubmoduleEnumeration<'resolver, 'db> {
    /// Starts at a name prefix using configured search paths, or at the roots for `None`.
    pub(crate) fn for_prefix(
        resolver: &'resolver NameResolver<'db>,
        prefix: Option<&ModuleName>,
    ) -> Option<Self> {
        Some(Self {
            search: ModuleSearch::for_prefix(resolver, prefix)?,
            known_package_directory: None,
        })
    }

    /// Starts beneath a resolved module, preserving packages found outside configured paths
    /// and allowing enumeration beneath an already resolved package reached through symlinks.
    pub(crate) fn for_module(
        resolver: &'resolver NameResolver<'db>,
        module: Module<'db>,
    ) -> Option<Self> {
        let db = resolver.context.db;
        let known_package_directory = if module.kind(db) == ModuleKind::Package {
            module
                .file(db)
                .and_then(|file| file.path(db).as_system_path())
                .and_then(SystemPath::parent)
        } else {
            None
        };
        Some(Self {
            search: ModuleSearch::for_module(resolver, module)?,
            known_package_directory,
        })
    }

    /// Collects resolved immediate children and unresolved overlay prefixes to explore.
    pub(crate) fn collect(&self) -> EnumeratedChildren<'db> {
        let db = self.search.resolver.context.db;
        let mut children = EnumeratedChildren::default();
        for component in self.child_names() {
            db.unwind_if_revision_cancelled();
            match self.enumerate_child(&component) {
                Some(EnumeratedChild::Module(module)) => children.modules.push(module),
                Some(EnumeratedChild::OverlayPrefix(prefix)) => {
                    children.overlay_prefixes.push(prefix);
                }
                None => {}
            }
        }
        children
    }

    fn child_names(&self) -> BTreeSet<String> {
        match &self.search.cursor {
            SearchCursor::Root => self.top_level_names(),
            SearchCursor::Prefix(prefix) => self.package_child_names(prefix),
        }
    }

    fn enumerate_child(&self, component: &str) -> Option<EnumeratedChild<'db>> {
        let name = self.search.child_name(component)?;
        let candidates = self.search.resolve_child_candidates(&name);
        if !candidates.is_empty() {
            // A resolved but excluded module still shadows other locations. Its exclusion
            // must not be treated as a failed resolution that permits an overlay prefix.
            return self
                .selected_module_if_eligible(&name, candidates)
                .map(EnumeratedChild::Module);
        }
        self.unresolved_overlay_prefix(component)
            .map(EnumeratedChild::OverlayPrefix)
    }

    fn top_level_names(&self) -> BTreeSet<String> {
        let context = &self.search.resolver.context;
        let mut names = BTreeSet::new();
        for path in search_paths(context.db, context.resolver_environment, context.mode) {
            self.collect_child_names(
                &ModuleResolutionCandidate::root(path),
                EntryLocation::SearchRoot,
                &mut names,
            );
        }
        names
    }

    fn package_child_names(&self, prefix: &PrefixSearch<'db>) -> BTreeSet<String> {
        let context = &self.search.resolver.context;
        let candidates = match &prefix.candidates {
            SearchCandidates::Typing(typing) => {
                typing.full_search_candidates(context, &prefix.name)
            }
            SearchCandidates::Runtime(candidates) => candidates.as_slice(),
        };
        let mut names = BTreeSet::new();
        for candidate in self.search.overlay_candidates().iter().chain(candidates) {
            self.collect_child_names(candidate, EntryLocation::Package, &mut names);
        }
        names
    }

    fn selected_module_if_eligible(
        &self,
        name: &ModuleName,
        candidates: ResolvedNames<'db>,
    ) -> Option<Module<'db>> {
        let first = candidates.first()?;
        // An implicit namespace has no single defining location. Any eligible portion can
        // supply the module. Concrete modules must use the selected location.
        let eligible = if matches!(first.module, ResolvedModule::NamespacePackage) {
            candidates
                .iter()
                .any(|candidate| self.candidate_is_eligible(candidate))
        } else {
            self.candidate_is_eligible(first)
        };
        if !eligible {
            return None;
        }
        let context = &self.search.resolver.context;
        candidates
            .into_iter()
            .next()
            .map(|candidate| candidate.into_module(context.db, context.resolver_environment, name))
    }

    fn unresolved_overlay_prefix(&self, component: &str) -> Option<ModuleName> {
        if self.search.overlay_candidates().is_empty() {
            return None;
        }
        let search = self.search.enter_package(component)?;
        let eligible = search.overlay_candidates().iter().any(|candidate| {
            !matches!(candidate.module, ResolvedModule::Module(_))
                && self.candidate_is_eligible(candidate)
        });
        if !eligible {
            return None;
        }
        // A local stub can patch `acme.nested.tools` even when installed stubs omit
        // `acme.nested`. Keep that prefix for traversal without inventing a module.
        self.search.child_name(component)
    }

    fn collect_child_names(
        &self,
        candidate: &ModuleResolutionCandidate<'db>,
        location: EntryLocation,
        names: &mut BTreeSet<String>,
    ) {
        if matches!(candidate.module, ResolvedModule::Module(_))
            || !self.candidate_is_eligible(candidate)
        {
            return;
        }
        let context = &self.search.resolver.context;
        if let ModuleDirectory::System(Some(listing)) = candidate.directory(context) {
            for (name, file_type) in listing.iter() {
                context.db.unwind_if_revision_cancelled();
                add_child_name(names, name, file_type, location);
            }
        } else if let Some(path) = candidate.path.to_vendored_path() {
            for entry in context.db.vendored().read_directory(&path) {
                let Some(name) = entry.path().file_name() else {
                    continue;
                };
                let file_type = if entry.file_type().is_directory() {
                    FileType::Directory
                } else {
                    FileType::File
                };
                add_child_name(names, name, file_type, location);
            }
        }
    }

    /// Enumeration follows top-level symlinks and the path to a known package, but excludes
    /// symlinks below either starting point. Resolution still keeps these candidates so they
    /// can shadow other locations.
    fn candidate_is_eligible(&self, candidate: &ModuleResolutionCandidate) -> bool {
        let db = self.search.resolver.context.db;
        let Some(root) = candidate.path.search_path().as_system_path() else {
            return true;
        };
        let path = match candidate.module {
            ResolvedModule::Module(file) => {
                file.path(db).as_system_path().map(SystemPath::to_path_buf)
            }
            _ => candidate.path.to_system_path(),
        };
        let Some(path) = path else { return false };
        // Exempt the known package's ancestry only for locations beneath its directory.
        // Other namespace portions and stub overlays keep their full ancestry checks.
        let known_package = self
            .known_package_directory
            .filter(|package| path.starts_with(package));
        let root = known_package.unwrap_or(root);
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let mut parent = root.to_path_buf();
        for (depth, component) in relative.components().enumerate() {
            if (known_package.is_some() || depth > 0)
                && directory_listing(db, &parent)
                    .ok()
                    .and_then(|listing| listing.file_type(component.as_str()))
                    .is_none_or(FileType::is_symlink)
            {
                return false;
            }
            parent.push(component.as_str());
        }
        true
    }
}

/// Resolved immediate children, plus prefixes needed only for stub-overlay enumeration.
///
/// An installed stub package can hide `acme.nested` while a local override still resolves
/// `acme.nested.tools`. Such prefixes are traversal positions, not resolved modules: recursive
/// enumeration must explore them, but import-statement completion must not offer them.
#[derive(Default)]
pub(crate) struct EnumeratedChildren<'db> {
    /// Modules that resolve independently and are eligible for enumeration.
    pub(crate) modules: Vec<Module<'db>>,
    /// Unresolved names with eligible stub-overlay locations to search for descendants.
    pub(crate) overlay_prefixes: Vec<ModuleName>,
}

enum EnumeratedChild<'db> {
    Module(Module<'db>),
    OverlayPrefix(ModuleName),
}

#[derive(Clone, Copy)]
enum EntryLocation {
    SearchRoot,
    Package,
}

fn add_child_name(
    names: &mut BTreeSet<String>,
    entry: &str,
    file_type: FileType,
    location: EntryLocation,
) {
    if matches!(location, EntryLocation::Package)
        && (file_type.is_symlink() || matches!(entry, "__init__.py" | "__init__.pyi"))
    {
        return;
    }
    let name = if !file_type.is_directory()
        && let Some(stem) = entry
            .strip_suffix(".py")
            .or_else(|| entry.strip_suffix(".pyi"))
    {
        stem
    } else if !file_type.is_file() {
        entry
    } else {
        return;
    };
    let name = match location {
        EntryLocation::SearchRoot => name.strip_suffix("-stubs").unwrap_or(name),
        EntryLocation::Package => name,
    };
    if ModuleName::new(name).is_some() && !name.contains('.') {
        names.insert(name.to_owned());
    }
}

/// A reusable cursor for resolving immediate children of a module-name prefix.
struct ModuleSearch<'resolver, 'db> {
    resolver: &'resolver NameResolver<'db>,
    cursor: SearchCursor<'db>,
}

impl<'resolver, 'db> ModuleSearch<'resolver, 'db> {
    /// Starts before the first module-name component, without probing search paths.
    fn new(resolver: &'resolver NameResolver<'db>) -> Self {
        Self {
            resolver,
            cursor: SearchCursor::Root,
        }
    }

    /// Builds a reusable search for a prefix, or for top-level names when absent.
    fn for_prefix(
        resolver: &'resolver NameResolver<'db>,
        prefix: Option<&ModuleName>,
    ) -> Option<Self> {
        let mut search = Self::new(resolver);
        if let Some(prefix) = prefix {
            for component in prefix.components() {
                search = search.enter_package(component)?;
            }
        }
        Some(search)
    }

    /// Preserves a module's location when importing-file fallback found it outside the
    /// configured paths. Other modules use the complete configured search, including overlays.
    /// File modules without possible descendant locations need no search.
    fn for_module(resolver: &'resolver NameResolver<'db>, module: Module<'db>) -> Option<Self> {
        let context = &resolver.context;
        if let Some(path) = module.search_path(context.db)
            && !search_paths(context.db, context.resolver_environment, context.mode)
                .any(|configured| configured == path)
        {
            return Some(Self {
                resolver,
                cursor: SearchCursor::Prefix(PrefixSearch {
                    name: module.name(context.db).clone(),
                    candidates: SearchCandidates::in_search_path(
                        context,
                        module.name(context.db),
                        path,
                    ),
                }),
            });
        }
        if module.kind(context.db) == ModuleKind::Module
            && !Self::may_have_children(context, module.name(context.db))
        {
            return None;
        }
        Self::for_prefix(resolver, Some(module.name(context.db)))
    }

    /// Enters one component without selecting a final module or consuming the parent cursor.
    ///
    /// Retains partial stub-package namespaces that may provide descendants even when a concrete
    /// package or module would shadow them if this component were the final import target.
    fn enter_package(&self, component: &str) -> Option<Self> {
        let name = self.child_name(component)?;
        let context = &self.resolver.context;

        let candidates = match &self.cursor {
            SearchCursor::Root => SearchCandidates::from_roots(context, component),
            SearchCursor::Prefix(prefix) => prefix.candidates.enter_package(context, component),
        };
        if candidates.is_empty(context, &name) {
            return None;
        }

        Some(Self {
            resolver: self.resolver,
            cursor: SearchCursor::Prefix(PrefixSearch { name, candidates }),
        })
    }

    /// Selects one immediate child using endpoint precedence, leaving its parent reusable.
    ///
    /// Typing resolution first tries stub overlays, then the full search including stub packages.
    /// Runtime resolution searches for runtime files only.
    fn resolve_child(&self, component: &str) -> Option<Module<'db>> {
        let name = self.child_name(component)?;
        let context = &self.resolver.context;
        self.resolve_child_candidates(&name)
            .into_iter()
            .next()
            .map(|candidate| candidate.into_module(context.db, context.resolver_environment, &name))
    }

    /// Overlay locations that can supply descendants even when this prefix does not resolve.
    fn overlay_candidates(&self) -> &[ModuleResolutionCandidate<'db>] {
        match &self.cursor {
            SearchCursor::Prefix(PrefixSearch {
                candidates: SearchCandidates::Typing(typing),
                ..
            }) => &typing.overlay_candidates,
            _ => &[],
        }
    }

    /// Selects candidates for one immediate child without converting them to a final module.
    fn resolve_child_candidates(&self, name: &ModuleName) -> ResolvedNames<'db> {
        let context = &self.resolver.context;
        if let SearchCursor::Prefix(prefix) = &self.cursor {
            return prefix
                .candidates
                .resolve_child(context, &prefix.name, name.last_component());
        }

        // A top-level name needs no separate overlay pass: there is no parent to be shadowed.
        let stubs = if context.mode.is_typing() {
            stub_package_index(context.db, context.resolver_environment).all()
        } else {
            StubPackagePaths::default()
        };

        let roots = discover_roots(
            context,
            name.first_component(),
            context.mode.is_non_shadowable(
                context
                    .resolver_environment
                    .python_version(context.db)
                    .minor,
                name.as_str(),
            ),
            search_paths(context.db, context.resolver_environment, context.mode),
            stubs,
        );

        normalize_candidates(context.db, roots, false)
    }

    fn child_name(&self, component: &str) -> Option<ModuleName> {
        let child = ModuleName::new(component)?;
        match &self.cursor {
            SearchCursor::Root => Some(child),
            SearchCursor::Prefix(prefix) => {
                let mut name = prefix.name.clone();
                name.extend(&child);
                Some(name)
            }
        }
    }

    /// Checks for possible descendant directories without resolving the prefix's ancestors.
    /// Finding a directory is only a reason to search; it may still be shadowed.
    fn may_have_children(context: &ResolverContext, name: &ModuleName) -> bool {
        // Partial stub namespaces can supply children even when the prefix resolves to a file.
        // Their directories use a different top-level name (`acme-stubs` for `acme`), so leave
        // environments containing stub packages to the full search.
        if context.mode.is_typing()
            && !stub_package_index(context.db, context.resolver_environment)
                .all()
                .is_empty()
        {
            return true;
        }

        if name.first_component() == name.as_str() {
            // Top-level files cannot share a name-based summary. Reuse the root listings
            // already read by enumeration instead of interning a missing directory per name.
            // Unlike nested files, these checks can be invalidated by unrelated root entries.
            return search_paths(context.db, context.resolver_environment, context.mode).any(
                |root| {
                    if let Some(path) = root.as_system_path()
                        && let Ok(listing) = directory_listing(context.db, path)
                    {
                        return listing.entry_is_directory(context.db, path, name.as_str());
                    }
                    // Preserve direct checks when listing fails, and vendored version rules.
                    root_contains_top_level_directory(context, root, name.as_str())
                },
            );
        }

        // `acme.tools` and `acme.reports` share the roots containing `acme/`. Intern only
        // `acme` so their separate child-list queries reuse the same directory checks.
        let top_level = ModuleNameIngredient::new(
            context.db,
            name.top_level(),
            context.mode,
            context.resolver_environment,
        );
        roots_containing_top_level_directory(context.db, top_level)
            .iter()
            .any(|root| {
                let mut path = root.to_module_path();
                for component in name.components() {
                    path.push(component);
                }
                // Reuse the containing directory's listing instead of creating an input
                // and probing a missing path for every leaf. Overlays may supply this directory.
                path.is_directory(context)
            })
    }
}

/// Roots containing the directory for a single-component module name, in search-path order.
///
/// Nested file modules share this summary through their top-level name, mode, and environment.
/// These are only possible descendant locations; normal resolution still decides precedence.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn roots_containing_top_level_directory<'db>(
    db: &'db dyn Db,
    top_level: ModuleNameIngredient<'db>,
) -> Box<[SearchPath]> {
    let context = ResolverContext::new(db, top_level.resolver_environment(db), top_level.mode(db));
    let component = top_level.name(db).as_str();
    search_paths(db, context.resolver_environment, context.mode)
        .filter(|root| root_contains_top_level_directory(&context, root, component))
        .cloned()
        .collect()
}

fn root_contains_top_level_directory(
    context: &ResolverContext,
    root: &SearchPath,
    component: &str,
) -> bool {
    let mut path = root.to_module_path();
    path.push(component);
    // Track the directory's status, not its containing root's listing: unrelated root
    // entries must not invalidate this result. Vendored paths retain Python-version checks.
    if let Some(path) = path.to_system_path() {
        system_path_to_directory(context.db, path).is_ok()
    } else {
        path.is_directory(context)
    }
}

/// The position in a module name, independent of the resolution mode.
enum SearchCursor<'db> {
    Root,
    Prefix(PrefixSearch<'db>),
}

/// Candidates retained for searching beneath a prefix such as `acme.tools`.
///
/// The prefix need not resolve independently: a stub overlay can supply a descendant even when
/// an installed package shadows its parent.
struct PrefixSearch<'db> {
    name: ModuleName,
    candidates: SearchCandidates<'db>,
}

/// The mode-specific searches available at a prefix.
enum SearchCandidates<'db> {
    Typing(TypingCandidates<'db>),
    Runtime(ResolvedNames<'db>),
}

impl<'db> SearchCandidates<'db> {
    /// Discovers the first component as a parent, retaining candidates for its descendants.
    ///
    /// Non-shadowable names apply to the complete import, not its parent prefixes: resolving
    /// `types.child` does not force `types` to come from stdlib.
    fn from_roots(context: &ResolverContext<'db>, component: &str) -> Self {
        if context.mode.is_typing() {
            return Self::Typing(TypingCandidates::from_roots(context, component));
        }

        // Runtime mode selects the real standard library instead of typeshed.
        let paths = search_paths(context.db, context.resolver_environment, context.mode);
        let roots = discover_roots(
            context,
            component,
            false,
            paths,
            // Stub packages do not participate in runtime resolution.
            StubPackagePaths::default(),
        );

        Self::Runtime(normalize_candidates(context.db, roots, true))
    }

    /// Searches a prefix within one importing-file fallback path, without a separate overlay pass.
    fn in_search_path(
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
        path: &SearchPath,
    ) -> Self {
        let stubs = StubPackageIndex::from_search_paths(context.db, std::iter::once(path));
        let mut candidates = normalize_candidates(
            context.db,
            discover_roots(
                context,
                prefix.first_component(),
                false,
                std::iter::once(path),
                stubs.all(),
            ),
            true,
        );
        for component in prefix.components().skip(1) {
            candidates = advance_candidates(
                context,
                candidates,
                component,
                ComponentFileFilter::ByMode,
                true,
            );
        }
        if context.mode.is_typing() {
            Self::Typing(TypingCandidates {
                overlay_candidates: Vec::new(),
                extra_path_roots: None,
                full_search_candidates: OnceCell::from(candidates),
            })
        } else {
            Self::Runtime(candidates)
        }
    }

    fn enter_package(&self, context: &ResolverContext<'db>, component: &str) -> Self {
        match self {
            Self::Typing(typing) => Self::Typing(typing.enter_package(context, component)),
            Self::Runtime(candidates) => Self::Runtime(advance_candidates(
                context,
                candidates.clone(),
                component,
                ComponentFileFilter::ByMode,
                true,
            )),
        }
    }

    fn resolve_child(
        &self,
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
        component: &str,
    ) -> ResolvedNames<'db> {
        match self {
            Self::Typing(typing) => typing.resolve_child(context, prefix, component),
            Self::Runtime(candidates) => advance_candidates(
                context,
                candidates.clone(),
                component,
                ComponentFileFilter::ByMode,
                false,
            ),
        }
    }

    fn is_empty(&self, context: &ResolverContext<'db>, prefix: &ModuleName) -> bool {
        match self {
            Self::Typing(typing) => {
                // An overlay may supply descendants without probing the rest of the search paths.
                typing.overlay_candidates.is_empty()
                    && typing.full_search_candidates(context, prefix).is_empty()
            }
            Self::Runtime(candidates) => candidates.is_empty(),
        }
    }
}

/// Typing resolution first searches extra-path stub overlays, then the complete configured paths.
struct TypingCandidates<'db> {
    /// Candidates for the current prefix, searched separately so other roots cannot shadow them.
    overlay_candidates: ResolvedNames<'db>,
    /// Candidates for the first component from extra paths, before normalization or descent.
    ///
    /// The full search combines these with the other roots before applying precedence. Keep the
    /// originals to avoid probing extra paths again; share them across cursors to avoid copying
    /// the same starting point at each depth. `None` means no extra-path candidates were found.
    extra_path_roots: Option<Rc<ResolvedNames<'db>>>,
    /// Candidates for the current prefix from all paths, including PEP 561 stub packages.
    ///
    /// Compute these lazily so overlay-only resolution need not probe other paths, and reuse
    /// them across sibling lookups.
    full_search_candidates: OnceCell<ResolvedNames<'db>>,
}

impl<'db> TypingCandidates<'db> {
    fn from_roots(context: &ResolverContext<'db>, component: &str) -> Self {
        let (stubs, _) =
            stub_package_index(context.db, context.resolver_environment).split_overlay();
        let roots = discover_roots(
            context,
            component,
            false,
            search_paths(context.db, context.resolver_environment, context.mode)
                .take_while(|path| path.is_extra()),
            stubs,
        );
        Self {
            overlay_candidates: normalize_candidates(context.db, roots.clone(), true),
            extra_path_roots: (!roots.is_empty()).then(|| Rc::new(roots)),
            full_search_candidates: OnceCell::new(),
        }
    }

    fn enter_package(&self, context: &ResolverContext<'db>, component: &str) -> Self {
        let overlay_candidates = advance_candidates(
            context,
            self.overlay_candidates.clone(),
            component,
            ComponentFileFilter::ByMode,
            true,
        );
        // Advance an already computed full search, but do not force it while following an overlay.
        let full_search_candidates = self.full_search_candidates.get().map(|candidates| {
            advance_candidates(
                context,
                candidates.clone(),
                component,
                ComponentFileFilter::ByMode,
                true,
            )
        });
        Self {
            overlay_candidates,
            extra_path_roots: self.extra_path_roots.as_ref().map(Rc::clone),
            full_search_candidates: full_search_candidates
                .map(OnceCell::from)
                .unwrap_or_default(),
        }
    }

    fn resolve_child(
        &self,
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
        component: &str,
    ) -> ResolvedNames<'db> {
        // When resolving `acme.tools`, the overlay search may have entered `acme` through
        // `acme/__init__.py`. The lookup for the requested child `tools` must now find a stub:
        // `tools.pyi` or `tools/__init__.pyi`, not a runtime `.py` file.
        let overlay = advance_candidates(
            context,
            self.overlay_candidates.clone(),
            component,
            ComponentFileFilter::StubOnly,
            false,
        );
        if !overlay.is_empty() {
            return overlay;
        }

        advance_candidates(
            context,
            self.full_search_candidates(context, prefix).to_vec(),
            component,
            ComponentFileFilter::ByMode,
            false,
        )
    }

    /// Builds the complete search once for this prefix, retaining ordinary typing precedence.
    fn full_search_candidates(
        &self,
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
    ) -> &[ModuleResolutionCandidate<'db>] {
        self.full_search_candidates.get_or_init(|| {
            let (_, stubs) =
                stub_package_index(context.db, context.resolver_environment).split_overlay();
            let mut candidates = self
                .extra_path_roots
                .as_deref()
                .cloned()
                .unwrap_or_default();
            candidates.extend(discover_roots(
                context,
                prefix.first_component(),
                false,
                search_paths(context.db, context.resolver_environment, context.mode)
                    .skip_while(|path| path.is_extra()),
                stubs,
            ));
            // Merge at the first component: a package there can shadow other roots before we
            // reach the current prefix. Appending candidates at the current depth would be wrong.
            candidates = normalize_candidates(context.db, candidates, true);
            for component in prefix.components().skip(1) {
                candidates = advance_candidates(
                    context,
                    candidates,
                    component,
                    ComponentFileFilter::ByMode,
                    true,
                );
            }
            candidates
        })
    }
}

/// Finds candidates for a top-level name across the supplied search paths and stub packages.
fn discover_roots<'a, 'db>(
    context: &ResolverContext<'db>,
    root_component: &str,
    is_non_shadowable: bool,
    search_paths: impl Iterator<Item = &'a SearchPath>,
    stub_paths: StubPackagePaths<'_>,
) -> ResolvedNames<'db> {
    let mut cur_candidates = Vec::new();
    let stub_name =
        (!stub_paths.is_empty() && !is_non_shadowable).then(|| format!("{root_component}-stubs"));
    let mut pending_stub_paths = Vec::new();

    if let Some(stub_name) = &stub_name {
        cur_candidates.extend(stub_paths.before_stdlib.iter().filter_map(|search_path| {
            resolve_stub_package_in_search_path(context, search_path, stub_name)
        }));
        // Defer file probes after stdlib until we know that stdlib does not win.
        pending_stub_paths.extend(stub_paths.after_stdlib.iter().filter(|search_path| {
            candidate_may_exist(
                context,
                &ModuleResolutionCandidate::stub(search_path),
                stub_name,
            )
        }));
    }

    for search_path in search_paths {
        // When a builtin module is imported, standard module resolution is bypassed:
        // the module name always resolves to the stdlib module,
        // even if there's a module of the same name in the first-party root
        // (which would normally result in the stdlib module being overridden).
        // TODO: offer a diagnostic if there is a first-party module of the same name
        if is_non_shadowable && !search_path.is_standard_library() {
            continue;
        }

        let is_stdlib = search_path.is_standard_library();
        // A terminal candidate can stop the search unless a matching post-stdlib stub package
        // could still override it. A terminal stdlib candidate always stops the search.
        let can_stop = is_stdlib || pending_stub_paths.is_empty();
        let mut candidate = ModuleResolutionCandidate::root(search_path);
        let resolved = resolve_component(
            context,
            &mut candidate,
            root_component,
            ComponentFileFilter::ByMode,
        )
        .is_ok();
        let terminal = candidate.missing_submodule_is_terminal();
        if resolved {
            cur_candidates.push(candidate);
        }
        // A terminal candidate shadows all later search paths. Earlier candidates remain in
        // play because they already shadow this candidate.
        if terminal && can_stop {
            break;
        }

        // Reaching this point for stdlib means that it did not provide a terminal candidate.
        // The deferred post-stdlib stub packages are therefore eligible, so resolve them now.
        if is_stdlib && let Some(stub_name) = &stub_name {
            cur_candidates.extend(pending_stub_paths.drain(..).filter_map(|search_path| {
                resolve_stub_package_in_search_path(context, search_path, stub_name)
            }));
        }
    }

    cur_candidates
}

/// Advances normalized prefix candidates, preserving terminal shadowing even on a failed probe.
///
/// With `for_descendants`, retain partial stub-package namespaces for the next component.
/// At the final component, concrete packages and modules shadow those namespaces.
fn advance_candidates<'db>(
    context: &ResolverContext<'db>,
    mut candidates: ResolvedNames<'db>,
    component: &str,
    filter: ComponentFileFilter,
    for_descendants: bool,
) -> ResolvedNames<'db> {
    let mut remaining_are_shadowed = false;
    candidates.retain_mut(|candidate| {
        if remaining_are_shadowed {
            return false;
        }
        let resolved = resolve_component(context, candidate, component, filter).is_ok();
        remaining_are_shadowed = candidate.missing_submodule_is_terminal();
        resolved
    });
    normalize_candidates(context.db, candidates, for_descendants)
}

fn resolve_stub_package_in_search_path<'db>(
    context: &ResolverContext<'db>,
    search_path: &SearchPath,
    stub_name: &str,
) -> Option<ModuleResolutionCandidate<'db>> {
    let mut candidate = ModuleResolutionCandidate::stub(search_path);
    resolve_component(
        context,
        &mut candidate,
        stub_name,
        ComponentFileFilter::ByMode,
    )
    .ok()?;

    // `mypackage-stubs.py(i)` is not a valid result.
    if matches!(candidate.module, ResolvedModule::Module(_)) {
        tracing::debug!(
            "Search path `{search_path}` contains a module named `{stub_name}` but a standalone \
             module isn't a valid stub."
        );
        None
    } else {
        Some(candidate)
    }
}

fn normalize_candidates<'db>(
    db: &dyn Db,
    mut candidates: ResolvedNames<'db>,
    has_remaining_components: bool,
) -> ResolvedNames<'db> {
    let best_concrete_precedence = candidates
        .iter()
        .filter(|candidate| !candidate.is_any_namespace_package())
        .map(|candidate| candidate.precedence)
        .min();

    candidates.sort_by_key(|candidate| candidate.precedence);

    // Note that we intentionally do *not* filter out ordinary search-path candidates when a stub
    // package is found. Even when a non-namespace, non-partial stub package exists, we keep the
    // other candidates as fallbacks because sub-packages within the stubs may override py.typed to
    // partial. The stub-package candidate is ordered first so it takes priority. Other candidates
    // are only used when the stub package fails to find a submodule in a partial sub-package.
    candidates.retain(|candidate| {
        if !candidate.is_any_namespace_package() {
            return true;
        }

        // A higher-precedence partial namespace remains available while resolving its descendants.
        // At the final component, a concrete package or module shadows it.
        let preserved_for_descendants = best_concrete_precedence.is_none_or(|precedence| {
            has_remaining_components
                && candidate.py_typed == PyTyped::Partial
                && candidate.precedence < precedence
        });

        if preserved_for_descendants {
            return true;
        }

        // TODO: It might be useful to warn when a concrete package or module shadows a legacy
        // namespace package. If we only find legacy and non-legacy namespace packages, this logic
        // retains both.

        tracing::trace!(
            "Discarding namespace package `{}` because a non-namespace entry of the same name \
             was found",
            candidate.to_str(db),
        );
        false
    });

    candidates
}

/// Resolves one component relative to the candidate's current package.
fn resolve_component<'db>(
    context: &ResolverContext<'db>,
    candidate: &mut ModuleResolutionCandidate<'db>,
    module_name: &str,
    file_filter: ComponentFileFilter,
) -> Result<(), ()> {
    if matches!(candidate.module, ResolvedModule::Module(_)) {
        tracing::trace!(
            "Non-package module {} cannot have a child",
            candidate.to_str(context.db)
        );
        return Err(());
    }

    if !candidate_may_exist(context, candidate, module_name) {
        return Err(());
    }

    let directory = candidate.directory(context);
    let may_be_package = directory.may_contain_directory(context, &candidate.path, module_name);
    // A copied candidate initially shares its parent's entries. Advancing changes which
    // directory it represents, so those entries must not be reused for the next component.
    candidate.directory = OnceCell::new();
    let package_path = &mut candidate.path;
    package_path.push(module_name);

    // Check for a regular package first (highest priority), but only if its directory may exist.
    // A `tools.py` entry alone does not require probing `tools/__init__.py(i)`.
    if may_be_package {
        let package_directory = ModuleDirectory::new(context, package_path);
        candidate.directory = OnceCell::from(package_directory);
        package_path.push("__init__");
        let init =
            resolve_file_module_with_filter(package_path, context, package_directory, file_filter);
        package_path.pop();
        if let Some(init) = init {
            candidate.py_typed = package_path
                .py_typed(context)
                .inherit_parent(candidate.py_typed);
            if is_legacy_namespace_package(package_path, context, init) {
                candidate.module = ResolvedModule::LegacyNamespacePackage(init);
            } else {
                candidate.module = ResolvedModule::RegularPackage(init);
            }
            return Ok(());
        }
    }

    // Check for a file module next
    if let Some(file_module) =
        resolve_file_module_with_filter(package_path, context, directory, file_filter)
    {
        candidate.module = ResolvedModule::Module(file_module);
        return Ok(());
    }

    // Last resort, check if a folder with the given name exists. If so,
    // then this is a namespace package. We need to skip this check for
    // typeshed because `resolve_file_module_with_filter` can also return `None` if the
    // `__init__.py` exists but isn't available for the current Python version.
    // Let's assume that the `xml` module is only available on Python 3.11+ and
    // we're resolving for Python 3.10:
    //
    // * Looking up `xml/__init__.pyi` returns `None` even though
    //   the file exists but the module isn't available for the current Python
    //   version.
    // * The check here would now return `true` because the `xml` directory
    //   exists, resulting in a false positive for a namespace package.
    //
    // Since typeshed doesn't use any namespace packages today (May 2025),
    // simply skip this check which also helps performance. If typeshed
    // ever uses namespace packages, ensure that this check also takes the
    // `VERSIONS` file into consideration.
    // A namespace package is not backed by a file, so it cannot satisfy a stub-only lookup.
    if file_filter != ComponentFileFilter::StubOnly
        && !package_path.search_path().is_standard_library()
        && package_path.is_directory(context)
    {
        candidate.py_typed = package_path
            .py_typed(context)
            .inherit_parent(candidate.py_typed);
        candidate.module = ResolvedModule::NamespacePackage;
        return Ok(());
    }

    Err(())
}

/// Uses the parent directory's entries to reject candidates that cannot exist without performing
/// individual file-system probes for every supported module layout.
fn candidate_may_exist<'db>(
    context: &ResolverContext<'db>,
    candidate: &ModuleResolutionCandidate<'db>,
    module_name: &str,
) -> bool {
    // Other suffixes are harmless false positives; the normal probes still determine whether the
    // module exists.
    match candidate.directory(context) {
        ModuleDirectory::System(listing) => {
            listing.is_some_and(|listing| listing.contains_name_with_prefix(module_name))
        }
        ModuleDirectory::Vendored => true,
    }
}

type ResolvedNames<'db> = Vec<ModuleResolutionCandidate<'db>>;

fn resolve_file_module_with_filter(
    module: &ModulePath,
    resolver_state: &ResolverContext,
    directory: ModuleDirectory,
    filter: ComponentFileFilter,
) -> Option<File> {
    let name = module.file_stem()?;
    // Reject absent entries before `to_file` interns a path. It still checks the file's status,
    // including the target of a symlink.
    let stub_file = if resolver_state.mode.is_typing() && directory.may_contain_file(name, "pyi") {
        module.with_pyi_extension().to_file(resolver_state)
    } else {
        None
    };
    if filter == ComponentFileFilter::StubOnly {
        return stub_file;
    }

    stub_file.or_else(|| {
        if !directory.may_contain_file(name, "py") {
            return None;
        }
        module
            .with_py_extension()
            .and_then(|path| path.to_file(resolver_state))
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
    context: &ResolverContext,
    init: File,
) -> bool {
    // Just an optimization, the stdlib and typeshed are never legacy namespace packages
    if package_path.search_path().is_standard_library() {
        return false;
    }

    // This is all syntax-only analysis so it *could* be fooled but it's really unlikely.
    //
    // The benefit of being syntax-only is speed and avoiding circular dependencies
    // between module resolution and semantic analysis.
    //
    // The downside is if you write slightly different syntax we will fail to detect the idiom,
    // but hey, this is better than nothing!
    let parsed = ruff_db::parsed::parsed_module(
        context.db,
        PythonFile::new(
            context.db,
            init,
            context.resolver_environment.python_version(context.db),
        ),
    );
    let mut visitor = LegacyNamespacePackageVisitor::default();
    visitor.visit_body(parsed.load(context.db).suite());

    visitor.is_legacy_namespace_package
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
    pub(super) resolver_environment: ResolverEnvironment<'db>,
    pub(super) mode: ModuleResolveMode,
}

impl<'db> ResolverContext<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
        mode: ModuleResolveMode,
    ) -> Self {
        Self {
            db,
            resolver_environment,
            mode,
        }
    }

    pub(super) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
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
/// or
/// ```python
/// __import__('pkg_resources').declare_namespace(__name__)
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

        match stmt {
            // __path__ = pkgutil.extend_path(__path__, __name__)
            // __path__ = __import__("pkgutil").extend_path(__path__, __name__)
            ast::Stmt::Assign(ast::StmtAssign { value, targets, .. }) => {
                self.check_pkgutil_extend_path(targets, value);
            }
            // __import__('pkg_resources').declare_namespace(__name__)
            ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                self.check_pkg_resources_declare_namespace(value);
            }
            _ => {}
        }
    }
}

impl LegacyNamespacePackageVisitor {
    /// Check for `__path__ = pkgutil.extend_path(__path__, __name__)` or
    /// `__path__ = __import__("pkgutil").extend_path(__path__, __name__)`
    fn check_pkgutil_extend_path(&mut self, targets: &[ast::Expr], value: &ast::Expr) {
        let [ast::Expr::Name(maybe_path)] = targets else {
            return;
        };

        if &*maybe_path.id != "__path__" {
            return;
        }

        let ast::Expr::Call(ast::ExprCall {
            func: extend_func,
            arguments: extend_arguments,
            ..
        }) = value
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

    /// Check for `__import__('pkg_resources').declare_namespace(__name__)`
    fn check_pkg_resources_declare_namespace(&mut self, value: &ast::Expr) {
        let ast::Expr::Call(ast::ExprCall {
            func,
            arguments: declare_arguments,
            ..
        }) = value
        else {
            return;
        };

        let ast::Expr::Attribute(ast::ExprAttribute {
            value: maybe_pkg_resources,
            attr: maybe_declare_namespace,
            ..
        }) = &**func
        else {
            return;
        };

        if maybe_declare_namespace != "declare_namespace" {
            return;
        }

        // Match `__import__("pkg_resources")`
        let ast::Expr::Call(ast::ExprCall {
            func: maybe_import,
            arguments: import_arguments,
            ..
        }) = &**maybe_pkg_resources
        else {
            return;
        };

        let ast::Expr::Name(maybe_import) = &**maybe_import else {
            return;
        };

        if maybe_import.id() != "__import__" {
            return;
        }

        let Some(ast::Expr::StringLiteral(name)) = import_arguments.find_argument_value("name", 0)
        else {
            return;
        };

        if name.value.to_str() != "pkg_resources" {
            return;
        }

        // Check that the argument is `__name__`
        let Some(ast::Expr::Name(name_arg)) = declare_arguments.find_argument_value("name", 0)
        else {
            return;
        };

        self.is_legacy_namespace_package = name_arg.id() == "__name__";
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "These are tests, so it's fine to do I/O by-passing System."
    )]
    use std::assert_matches;

    use ruff_db::Db;
    use ruff_db::files::{File, FilePath, Files, system_path_to_file};
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem as _};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::PythonVersion;

    use crate::db::tests::TestDb;
    use crate::module::ModuleKind;
    use crate::module_name::ModuleName;
    use crate::strategy::FallibleStrategy;
    #[cfg(target_family = "unix")]
    use crate::testing::symlink_enumeration_db;
    use crate::testing::{
        FileSpec, MockedTypeshed, TestCase, TestCaseBuilder, enumeration_db, unresolved_overlay_db,
    };

    use super::*;

    fn resolve_module_confident<'db>(
        db: &'db TestDb,
        module_name: &ModuleName,
    ) -> Option<Module<'db>> {
        super::resolve_module_confident(db, db.resolver_environment(), module_name)
    }

    fn resolve_real_module_confident<'db>(
        db: &'db TestDb,
        module_name: &ModuleName,
    ) -> Option<Module<'db>> {
        super::resolve_real_module_confident(db, db.resolver_environment(), module_name)
    }

    fn path_to_module<'db>(db: &'db TestDb, path: &FilePath) -> Option<Module<'db>> {
        super::path_to_module(db, db.resolver_environment(), path)
    }

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
            path_to_module(&db, &FilePath::from(expected_foo_path))
        );
    }

    #[test]
    fn site_packages_stub_overrides_first_party_package_when_stdlib_is_missing() {
        let TestCase {
            db, site_packages, ..
        } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", "")])
            .with_site_packages_files(&[("foo-stubs/__init__.pyi", "")])
            .build();

        let foo = resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        assert_eq!(
            foo.file(&db).unwrap().path(&db),
            &site_packages.join("foo-stubs/__init__.pyi")
        );
    }

    #[test]
    fn first_party_stub_package_precedes_stdlib() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("foo.pyi", "")],
            versions: "foo: 3.8-",
        };

        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_src_files(&[("foo-stubs/__init__.pyi", "")])
            .build();

        let foo = resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        assert_eq!(
            foo.file(&db).unwrap().path(&db),
            &src.join("foo-stubs/__init__.pyi")
        );
    }

    #[test]
    fn desperate_resolution_finds_stub_package() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[
                ("nested/main.py", ""),
                ("nested/foo/__init__.py", ""),
                ("nested/foo-stubs/__init__.pyi", ""),
            ])
            .build();
        let importing_file = system_path_to_file(&db, src.join("nested/main.py")).unwrap();

        let foo = resolve_module(
            &db,
            ImportingFile::File(importing_file, db.resolver_environment()),
            &ModuleName::new_static("foo").unwrap(),
        )
        .unwrap();
        assert_eq!(
            foo.file(&db).unwrap().path(&db),
            &src.join("nested/foo-stubs/__init__.pyi")
        );
    }

    #[test]
    fn missing_modules_do_not_create_file_inputs() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("other.py", ""), ("package/__init__.py", "")])
            .build();

        for name in ["missing", "package.missing"] {
            assert!(
                resolve_module_confident(&db, &ModuleName::new_static(name).unwrap()).is_none()
            );
        }

        for relative_path in [
            "missing-stubs/__init__.pyi",
            "missing-stubs/__init__.py",
            "missing/__init__.pyi",
            "missing/__init__.py",
            "missing.pyi",
            "missing.py",
            "package/missing/__init__.pyi",
            "package/missing/__init__.py",
            "package/missing.pyi",
            "package/missing.py",
        ] {
            assert_eq!(
                db.files().try_system(&db, &src.join(relative_path)),
                None,
                "unexpected point probe for {relative_path}"
            );
        }
    }

    #[test]
    fn stdlib_precedes_stub_package_in_site_packages() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("foo.pyi", "")],
            versions: "foo: 3.8-",
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_site_packages_files(&[("foo-stubs/__init__.pyi", "")])
            .build();

        let foo = resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).unwrap();
        assert_eq!(foo.file(&db).unwrap().path(&db), &stdlib.join("foo.pyi"));
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
            path_to_module(&db, &FilePath::from(expected_foo_path))
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
            path_to_module(&db, &FilePath::from(expected_foo_path))
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
            path_to_module(&db, &FilePath::from(expected_functools_path))
        );
    }

    fn create_module_names(raw_names: &[&str]) -> Vec<ModuleName> {
        raw_names
            .iter()
            .map(|raw| ModuleName::new(raw).unwrap())
            .collect()
    }

    #[test]
    fn resolve_module_uses_resolver_environment_python_version() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("_sha256.pyi", ""), ("py312_only.pyi", "")],
            versions: "_sha256: 3.11-\npy312_only: 3.12-",
        };

        let TestCase {
            db, src, stdlib, ..
        } = TestCaseBuilder::new()
            .with_src_files(&[
                ("main.py", ""),
                ("_sha256.py", ""),
                ("namespace/module.py", ""),
            ])
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY311)
            .build();
        let importing_file = system_path_to_file(&db, src.join("main.py")).unwrap();
        let py311 = ResolverEnvironment::new(&db, PythonVersion::PY311, db.search_paths());
        let py312 = ResolverEnvironment::new(&db, PythonVersion::PY312, db.search_paths());
        let sha256 = ModuleName::new_static("_sha256").unwrap();
        let py311_module =
            resolve_module(&db, ImportingFile::File(importing_file, py311), &sha256).unwrap();
        let py312_module =
            resolve_module(&db, ImportingFile::File(importing_file, py312), &sha256).unwrap();
        assert_eq!(
            py311_module.file(&db).unwrap().path(&db),
            &stdlib.join("_sha256.pyi")
        );
        assert_eq!(
            py312_module.file(&db).unwrap().path(&db),
            &src.join("_sha256.py")
        );
        assert_eq!(py311_module.python_version(&db), PythonVersion::PY311);
        assert_eq!(py312_module.python_version(&db), PythonVersion::PY312);

        let namespace = ModuleName::new_static("namespace").unwrap();
        let py311_namespace =
            resolve_module(&db, ImportingFile::File(importing_file, py311), &namespace).unwrap();
        let py312_namespace =
            resolve_module(&db, ImportingFile::File(importing_file, py312), &namespace).unwrap();
        assert_matches!(py311_namespace, Module::Namespace(_));
        assert_matches!(py312_namespace, Module::Namespace(_));
        assert_eq!(py311_namespace.python_version(&db), PythonVersion::PY311);
        assert_eq!(py312_namespace.python_version(&db), PythonVersion::PY312);
        assert_ne!(py311_namespace, py312_namespace);

        let py312_only = ModuleName::new_static("py312_only").unwrap();
        assert!(
            resolve_module(&db, ImportingFile::File(importing_file, py311), &py312_only).is_none()
        );
        assert_eq!(
            resolve_module(&db, ImportingFile::File(importing_file, py312), &py312_only)
                .and_then(|module| module.file(&db))
                .unwrap()
                .path(&db),
            &stdlib.join("py312_only.pyi")
        );
    }

    #[test]
    fn resolve_module_uses_resolver_environment_search_paths() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("main.py", ""), ("shared.py", "from_src = True")])
            .with_vendored_typeshed()
            .build();
        db.write_file("/alternate/shared.py", "from_alternate = True")
            .unwrap();

        let alternate_paths = SearchPathSettings {
            src_roots: vec![SystemPathBuf::from("/alternate")],
            ..SearchPathSettings::empty()
        }
        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
        .unwrap();
        alternate_paths.try_register_static_roots(&db);

        let primary = db.resolver_environment();
        let alternate = ResolverEnvironment::new(&db, PythonVersion::default(), &alternate_paths);
        let importing_file = system_path_to_file(&db, src.join("main.py")).unwrap();
        let name = ModuleName::new_static("shared").unwrap();

        let primary_module =
            resolve_module(&db, ImportingFile::File(importing_file, primary), &name).unwrap();
        let alternate_module =
            resolve_module(&db, ImportingFile::File(importing_file, alternate), &name).unwrap();

        assert_eq!(
            primary_module.file(&db).unwrap().path(&db),
            &src.join("shared.py")
        );
        assert_eq!(
            alternate_module.file(&db).unwrap().path(&db),
            &SystemPathBuf::from("/alternate/shared.py")
        );
        assert_ne!(primary_module, alternate_module);
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            functools: 3.8-             # Top-level single-file module
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("functools.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let existing_modules = create_module_names(&["asyncio", "functools"]);
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
        ";

        const STDLIB: &[FileSpec] = &[
            ("collections/__init__.pyi", ""),
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        let nonexisting_modules = create_module_names(&["collections", "asyncio.tasks"]);

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
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("collections/__init__.pyi", ""),
            ("functools.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY39)
            .build();

        let existing_modules =
            create_module_names(&["asyncio", "functools", "collections", "asyncio.tasks"]);

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
            path_to_module(&db, &FilePath::from(src.join("functools.py")))
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
            path_to_module(&db, &FilePath::from(foo_path)).as_ref()
        );

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, path_to_module(&db, &FilePath::from(src.join("foo"))));
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
            path_to_module(&db, &FilePath::from(foo_init_path))
        );
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::from(src.join("foo.py")))
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

        assert_eq!(Some(foo), path_to_module(&db, &FilePath::from(foo_stub)));
        assert_eq!(
            Some(foo_real),
            path_to_module(&db, &FilePath::from(src.join("foo.py")))
        );
        assert_ne!(foo_real, foo);
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
            path_to_module(&db, &FilePath::from(baz_path))
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
            path_to_module(&db, &FilePath::from(foo_src_path))
        );

        assert_eq!(
            None,
            path_to_module(&db, &FilePath::from(site_packages.join("foo.py")))
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
            .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
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

        assert_eq!(Some(foo_module), path_to_module(&db, &FilePath::from(foo)));
        assert_eq!(Some(bar_module), path_to_module(&db, &FilePath::from(bar)));

        Ok(())
    }

    #[test]
    fn deleting_file_from_different_directory_doesnt_change_module_resolution() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "x = 1"), ("other/bar.py", "x = 2")])
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

        let bar_path = src.join("other/bar.py");
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
            ModuleNameIngredient::new(
                &db,
                functools_module_name,
                ModuleResolveMode::Typing,
                db.resolver_environment(),
            ),
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
            &FilePath::from(site_packages.join("spam/spam.py"))
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
            ModuleResolveModeIngredient::new(
                &db,
                db.resolver_environment(),
                ModuleResolveMode::Typing,
            ),
            &events,
        );
    }

    #[test]
    fn nested_site_packages_change_does_not_invalidate_dynamic_resolution_paths() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src"), ("package/__init__.py", "")];

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        dynamic_resolution_paths(
            &db,
            ModuleResolveModeIngredient::new(
                &db,
                db.resolver_environment(),
                ModuleResolveMode::Typing,
            ),
        );
        db.clear_salsa_events();

        db.write_file(site_packages.join("package/nested.py"), "")
            .unwrap();
        dynamic_resolution_paths(
            &db,
            ModuleResolveModeIngredient::new(
                &db,
                db.resolver_environment(),
                ModuleResolveMode::Typing,
            ),
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(
            &db,
            dynamic_resolution_paths,
            ModuleResolveModeIngredient::new(
                &db,
                db.resolver_environment(),
                ModuleResolveMode::Typing,
            ),
            &events,
        );
    }

    #[test]
    fn modifying_pth_file_invalidates_dynamic_resolution_paths() {
        const SITE_PACKAGES: &[FileSpec] = &[("_editable.pth", "/x/src")];

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .build();
        db.write_files([("/x/src/foo.py", ""), ("/y/src/bar.py", "")])
            .unwrap();

        assert!(resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).is_some());
        assert_eq!(
            editable_search_paths(&db, db.resolver_environment()).collect::<Vec<_>>(),
            [SystemPath::new("/x/src")]
        );

        let pth_path = site_packages.join("_editable.pth");
        db.memory_file_system()
            .write_file(&pth_path, "/y/src")
            .unwrap();
        File::sync_path_only(&mut db, &pth_path);

        assert_eq!(
            editable_search_paths(&db, db.resolver_environment()).collect::<Vec<_>>(),
            [SystemPath::new("/y/src")]
        );
        assert!(resolve_module_confident(&db, &ModuleName::new_static("foo").unwrap()).is_none());
        assert!(resolve_module_confident(&db, &ModuleName::new_static("bar").unwrap()).is_some());
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
            &FilePath::from(src_path.join("foo.py"))
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
            search_paths(&db, db.resolver_environment(), ModuleResolveMode::Typing).collect();

        assert!(search_paths.contains(
            &&SearchPath::first_party(db.system(), SystemPathBuf::from("/src")).unwrap()
        ));
        assert!(
            !search_paths.contains(
                &&SearchPath::editable(db.system(), SystemPathBuf::from("/src")).unwrap()
            )
        );
        assert_eq!(
            editable_search_paths(&db, db.resolver_environment()).collect::<Vec<_>>(),
            [SystemPath::new("/src")]
        );
    }

    #[test]
    fn first_party_roots_exclude_dynamic_search_paths() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("_foo.pth", "/editable")])
            .build();
        db.memory_file_system()
            .create_directory_all("/editable")
            .expect("valid editable directory");

        let all_paths: Vec<_> =
            search_paths(&db, db.resolver_environment(), ModuleResolveMode::Typing).collect();
        assert!(
            all_paths.contains(
                &&SearchPath::editable(db.system(), SystemPathBuf::from("/editable"))
                    .expect("valid editable search path")
            )
        );

        assert_eq!(
            db.search_paths().first_party_roots().collect::<Vec<_>>(),
            [&*src]
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
            .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
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

        // The lexical directory listing must accept the symlink named `a` while rejecting `A`.
        std::os::unix::fs::symlink(a_package_target.as_std_path(), a_src.as_std_path())
            .context("Failed to symlink `src/a` to `a-package`")?;

        db.set_search_paths(
            SearchPathSettings::new(vec![src])
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
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
        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
        .expect("Valid search path settings");
        db.set_search_paths(search_paths);

        let foo_module_file = File::new(&db, FilePath::from(installed_foo_module));
        let module = file_to_module(
            &db,
            ResolverFile::new(&db, foo_module_file, db.resolver_environment()),
        )
        .unwrap();
        assert_eq!(module.search_path(&db).unwrap(), &site_packages);
    }

    #[test]
    fn split_and_nested_namespaces() {
        let db = enumeration_db(
            &[
                "/src/acme/left.py",
                "/site-packages/acme/right.py",
                "/src/acme/nested/deep.py",
                "/src/acme/regular/__init__.py",
                "/src/acme/regular/namespace/child.py",
            ],
            &[],
        );
        assert_children(&db, None, &["acme"], &[]);
        assert_children(
            &db,
            Some("acme"),
            &["acme.left", "acme.nested", "acme.regular", "acme.right"],
            &[],
        );
        assert_children(&db, Some("acme.nested"), &["acme.nested.deep"], &[]);
        assert_children(&db, Some("acme.regular"), &["acme.regular.namespace"], &[]);
        assert_children(
            &db,
            Some("acme.regular.namespace"),
            &["acme.regular.namespace.child"],
            &[],
        );
    }

    #[test]
    fn namespace_children_uses_resolution_precedence() {
        let db = enumeration_db(
            &[
                "/src/acme/duplicate.py",
                "/site-packages/acme/duplicate.py",
                "/src/acme/stubbed.py",
                "/src/acme/stubbed.pyi",
                "/src/acme/package/__init__.py",
                "/src/acme/package.pyi",
                "/src/acme/package/local.py",
                "/site-packages/acme/package/hidden.py",
                "/src/acme/module.py",
                "/site-packages/acme/module/hidden.py",
            ],
            &[],
        );
        assert_children(
            &db,
            Some("acme"),
            &[
                "acme.duplicate",
                "acme.module",
                "acme.package",
                "acme.stubbed",
            ],
            &[],
        );
        assert_children(&db, Some("acme.package"), &["acme.package.local"], &[]);
        assert_children(&db, Some("acme.module"), &[], &[]);
        assert_enumerated_file(&db, "acme.duplicate", "/src/acme/duplicate.py");
        assert_enumerated_file(&db, "acme.stubbed", "/src/acme/stubbed.pyi");
        assert_enumerated_file(&db, "acme.package", "/src/acme/package/__init__.py");
    }

    #[test]
    fn concrete_parents_shadow_namespace_portions() {
        for parent in ["/site-packages/acme/__init__.py", "/site-packages/acme.py"] {
            let db = enumeration_db(&["/src/acme/hidden.py", parent], &[]);
            assert_children(&db, None, &["acme"], &[]);
            assert_children(&db, Some("acme"), &[], &[]);
            assert_enumerated_file(&db, "acme", parent);
        }
    }

    #[test]
    fn legacy_namespace_portions() {
        for declaration in [
            r#"
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
"#,
            r#"
import pkgutil
__path__ = pkgutil.extend_path(__path__, __name__)
"#,
            r#"
__import__("pkg_resources").declare_namespace(__name__)
"#,
        ] {
            let mut db =
                enumeration_db(&["/src/acme/left.py", "/site-packages/acme/right.py"], &[]);
            for init in ["/src/acme/__init__.py", "/site-packages/acme/__init__.py"] {
                db.write_file(init, declaration)
                    .expect("write legacy declaration");
            }
            assert_children(&db, Some("acme"), &["acme.left", "acme.right"], &[]);
            assert_enumerated_file(&db, "acme", "/src/acme/__init__.py");
        }
    }

    #[test]
    fn partial_stub_namespaces() {
        let mut db = enumeration_db(
            &[
                "/src/acme/__init__.py",
                "/src/acme/api/__init__.py",
                "/src/acme/api/runtime.py",
                "/site-packages/acme-stubs/api/stubbed.pyi",
            ],
            &[],
        );
        db.write_file(
            "/site-packages/acme-stubs/py.typed",
            r#"
partial
"#,
        )
        .expect("preserve partial stub namespace alongside regular runtime packages");
        assert_children(&db, Some("acme"), &["acme.api"], &[]);
        assert_children(
            &db,
            Some("acme.api"),
            &["acme.api.runtime", "acme.api.stubbed"],
            &[],
        );
        assert_enumerated_file(&db, "acme.api", "/src/acme/api/__init__.py");
        assert_enumerated_file(
            &db,
            "acme.api.stubbed",
            "/site-packages/acme-stubs/api/stubbed.pyi",
        );
    }

    #[test]
    fn partial_child_of_complete_stub_package() {
        let mut db = enumeration_db(
            &[
                "/site-packages/acme-stubs/__init__.pyi",
                "/site-packages/acme-stubs/api/__init__.pyi",
                "/site-packages/acme-stubs/api/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/hidden.py",
                "/src/acme/api/__init__.py",
                "/src/acme/api/runtime.py",
            ],
            &[],
        );
        db.write_file(
            "/site-packages/acme-stubs/api/py.typed",
            r#"
partial
"#,
        )
        .expect("mark child as partial");
        assert_children(&db, Some("acme"), &["acme.api"], &[]);
        assert_children(
            &db,
            Some("acme.api"),
            &["acme.api.runtime", "acme.api.stubbed"],
            &[],
        );
        assert_enumerated_file(
            &db,
            "acme.api",
            "/site-packages/acme-stubs/api/__init__.pyi",
        );
    }

    #[test]
    fn partial_stub_descendants_of_file_module() {
        let mut db = enumeration_db(
            &["/src/acme.py", "/site-packages/acme-stubs/child.pyi"],
            &[],
        );
        db.write_file(
            "/site-packages/acme-stubs/py.typed",
            r#"
partial
"#,
        )
        .expect("mark the stub namespace as partial");
        assert_children(&db, Some("acme"), &["acme.child"], &[]);
        assert_enumerated_file(&db, "acme", "/src/acme.py");
    }

    #[test]
    fn overlay_and_runtime_siblings() {
        let db = enumeration_db(
            &[
                "/extra/acme/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/stubbed.py",
                "/src/acme/runtime.py",
            ],
            &["/extra"],
        );
        assert_children(&db, Some("acme"), &["acme.runtime", "acme.stubbed"], &[]);
        assert_enumerated_file(&db, "acme.stubbed", "/extra/acme/stubbed.pyi");
        assert_enumerated_file(&db, "acme.runtime", "/src/acme/runtime.py");
    }

    #[test]
    fn overlay_descendants_of_runtime_module() {
        let db = enumeration_db(
            &[
                "/extra/acme/api/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/api.py",
            ],
            &["/extra"],
        );
        assert_children(&db, Some("acme"), &["acme.api"], &[]);
        assert_children(&db, Some("acme.api"), &["acme.api.stubbed"], &[]);
        assert_enumerated_file(&db, "acme.api", "/src/acme/api.py");
        assert_enumerated_file(&db, "acme.api.stubbed", "/extra/acme/api/stubbed.pyi");
    }

    #[test]
    fn top_level_overlay_descendants_appear_and_disappear() {
        let mut db = enumeration_db(&["/src/leaf.py", "/extra/unrelated.py"], &["/extra"]);
        assert_children(&db, Some("leaf"), &[], &[]);

        db.write_file(
            "/extra/leaf/stubbed.pyi",
            r#"
"#,
        )
        .expect("add an overlay descendant to a top-level file module");
        assert_children(&db, Some("leaf"), &["leaf.stubbed"], &[]);
        assert_enumerated_file(&db, "leaf.stubbed", "/extra/leaf/stubbed.pyi");

        db.memory_file_system()
            .remove_file("/extra/leaf/stubbed.pyi")
            .expect("remove the overlay descendant");
        db.memory_file_system()
            .remove_directory("/extra/leaf")
            .expect("remove the empty overlay directory");
        Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
        assert_children(&db, Some("leaf"), &[], &[]);
    }

    #[test]
    fn overlay_descendants_appear_and_disappear() {
        // An overlay can introduce `acme/` or add `api/` beneath an existing `acme/`.
        for existing_top_level in [false, true] {
            let mut db = enumeration_db(
                &[
                    "/src/acme/__init__.py",
                    "/src/acme/api.py",
                    "/src/acme/reports.py",
                    "/extra/unrelated.py",
                ],
                &["/extra"],
            );
            if existing_top_level {
                db.write_file(
                    "/extra/acme/other.pyi",
                    r#"
"#,
                )
                .expect("create an existing overlay portion");
            }
            assert_children(&db, Some("acme.api"), &[], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);

            db.write_file(
                "/extra/acme/api/stubbed.pyi",
                r#"
"#,
            )
            .expect("add an overlay descendant to a runtime file module");
            assert_children(&db, Some("acme.api"), &["acme.api.stubbed"], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);
            assert_enumerated_file(&db, "acme.api.stubbed", "/extra/acme/api/stubbed.pyi");

            db.memory_file_system()
                .remove_file("/extra/acme/api/stubbed.pyi")
                .expect("remove the overlay descendant");
            db.memory_file_system()
                .remove_directory("/extra/acme/api")
                .expect("remove the empty overlay subpackage");
            if !existing_top_level {
                db.memory_file_system()
                    .remove_directory("/extra/acme")
                    .expect("remove the empty overlay portion");
            }
            Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
            assert_children(&db, Some("acme.api"), &[], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);
        }
    }

    #[test]
    fn unresolved_overlay_prefixes_are_not_modules() {
        let db = unresolved_overlay_db();
        assert_children(&db, None, &["acme"], &[]);
        assert_children(&db, Some("acme"), &[], &["acme.nested"]);
        assert_children(&db, Some("acme.nested"), &[], &["acme.nested.deep"]);
        assert_children(
            &db,
            Some("acme.nested.deep"),
            &["acme.nested.deep.tools"],
            &[],
        );
        assert_enumerated_file(
            &db,
            "acme.nested.deep.tools",
            "/extra/acme/nested/deep/tools.pyi",
        );
    }

    #[test]
    fn stdlib_precedence_over_installed_stub_package() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(MockedTypeshed {
                stdlib_files: &[(
                    "fractions.pyi",
                    r#"
"#,
                )],
                versions: r#"
fractions: 3.8-
"#,
            })
            .with_site_packages_files(&[(
                "fractions-stubs/__init__.pyi",
                r#"
"#,
            )])
            .build();
        assert_children(&db, None, &["fractions"], &[]);
        assert_enumerated_file(&db, "fractions", "/typeshed/stdlib/fractions.pyi");
    }

    #[test]
    fn concrete_package_shadows_legacy_namespace() {
        let mut db = enumeration_db(
            &[
                "/src/acme/__init__.py",
                "/src/acme/hidden.py",
                "/site-packages/acme/__init__.py",
                "/site-packages/acme/visible.py",
            ],
            &[],
        );
        db.write_file(
            "/src/acme/__init__.py",
            r#"
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
"#,
        )
        .expect("declare a legacy namespace");
        assert_children(&db, Some("acme"), &["acme.visible"], &[]);
        assert_enumerated_file(&db, "acme", "/site-packages/acme/__init__.py");
    }

    #[test]
    fn children_of_package_found_by_importing_file_fallback() {
        let db = enumeration_db(
            &[
                "/src/nested/main.py",
                "/src/nested/acme/__init__.py",
                "/src/nested/acme/child.py",
            ],
            &[],
        );
        let name = ModuleName::new_static("acme").expect("valid package name");
        assert_children(&db, Some("acme"), &[], &[]);
        let file = system_path_to_file(&db, "/src/nested/main.py").expect("importing file exists");
        let package = crate::resolve_module(
            &db,
            ImportingFile::File(file, db.resolver_environment()),
            &name,
        )
        .expect("importing-file fallback finds the package");
        let resolver = NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let children = SubmoduleEnumeration::for_module(&resolver, package)
            .expect("enumerate the explicitly resolved package")
            .collect();
        assert_eq!(
            children
                .modules
                .iter()
                .map(|module| module.name(&db).as_str())
                .collect::<Vec<_>>(),
            ["acme.child"]
        );
        assert!(children.overlay_prefixes.is_empty());
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink_eligibility_does_not_change_resolution() {
        let (_temp, db, _root) = symlink_enumeration_db();
        assert_children(&db, None, &["acme", "alias", "top_alias"], &[]);
        assert_children(&db, Some("acme"), &["acme.ns", "acme.own"], &[]);
        assert_children(&db, Some("acme.ns"), &["acme.ns.visible"], &[]);
        assert_children(&db, Some("alias"), &["alias.own"], &[]);
        for name in ["acme.hidden", "acme.ns.masked", "acme.blocked"] {
            assert!(
                crate::resolve_module_confident(
                    &db,
                    db.resolver_environment(),
                    &ModuleName::new(name).expect("valid name")
                )
                .is_some()
            );
        }
        for (name, expected) in [
            (
                "acme.blocked",
                vec!["acme.blocked.child", "acme.blocked.nested"],
            ),
            ("acme.blocked.nested", vec!["acme.blocked.nested.child"]),
        ] {
            let name = ModuleName::new(name).expect("valid package name");
            let package = crate::resolve_module_confident(&db, db.resolver_environment(), &name)
                .expect("explicit resolution follows symlinks");
            let resolver =
                NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
            let children = SubmoduleEnumeration::for_module(&resolver, package)
                .expect("enumerate the explicitly resolved symlink package")
                .collect();
            assert_eq!(
                children
                    .modules
                    .iter()
                    .map(|module| module.name(&db).as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(children.overlay_prefixes.is_empty());
        }
    }

    fn enumerate<'db>(db: &'db TestDb, package: Option<&str>) -> EnumeratedChildren<'db> {
        let package = package.map(|name| ModuleName::new(name).expect("valid package name"));
        let resolver = NameResolver::new(db, db.resolver_environment(), ModuleResolveMode::Typing);
        let enumeration = if let Some(name) = package.as_ref()
            && let Some(module) = resolve_module_confident(db, name)
        {
            SubmoduleEnumeration::for_module(&resolver, module)
        } else {
            SubmoduleEnumeration::for_prefix(&resolver, package.as_ref())
        };
        enumeration
            .map(|enumeration| enumeration.collect())
            .unwrap_or_default()
    }

    fn assert_children(db: &TestDb, package: Option<&str>, expected: &[&str], prefixes: &[&str]) {
        let children = enumerate(db, package);
        let names: Vec<_> = children
            .modules
            .iter()
            .map(|module| {
                let name = module.name(db);
                assert_eq!(
                    Some(*module),
                    crate::resolve_module_confident(db, db.resolver_environment(), name),
                    "enumeration must agree with resolution for {name}"
                );
                name.as_str()
            })
            .collect();
        assert_eq!(names, expected);
        let names: Vec<_> = children
            .overlay_prefixes
            .iter()
            .map(|name| {
                assert!(
                    crate::resolve_module_confident(db, db.resolver_environment(), name).is_none(),
                    "traversal prefix must not invent a resolved module"
                );
                name.as_str()
            })
            .collect();
        assert_eq!(names, prefixes);
    }

    fn assert_enumerated_file(db: &TestDb, name: &str, expected: &str) {
        let children = enumerate(db, name.rsplit_once('.').map(|(parent, _)| parent));
        let module = children
            .modules
            .iter()
            .find(|module| module.name(db).as_str() == name)
            .expect("module should be enumerated");
        let file = module.file(db).expect("module should have a defining file");
        assert_eq!(
            file.path(db).as_system_path(),
            Some(SystemPath::new(expected))
        );
    }
}
