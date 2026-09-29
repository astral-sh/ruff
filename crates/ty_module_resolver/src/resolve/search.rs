//! This module exposes a [`ModuleSearchCursor`] abstraction, which encapsulates
//! logic for efficient module resolution and (namespace-aware) module enumeration.
//!
//! It provides the following interfaces:
//!
//! - [`ModuleSearchCursor::resolve_name`] which resolves a module relative to the current
//!   position of the cursor (i.e., at some point along the individual components of a dotted
//!   module name like `acme.tools.power`).
//! - [`ModuleSearchCursor::list_modules`] which list modules immediately available (i.e., the
//!   direct sub-modules) at the current position of the cursor.
//!
//! The latter operation is also accessible via the free functions
//! [`list_root_modules`] and [`list_submodules`]. [`list_all_modules`] recursively
//! lists all available modules.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use compact_str::CompactString;
use itertools::Either;
use ruff_db::system::FileType;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::ResolverEnvironment;
use crate::db::Db;
use crate::module::Module;
use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, ModuleDirectoryEntry, ModulePath, SearchPath};

use super::{
    CandidatePrecedence, ComponentFileFilter, ModuleNameIngredient, ModuleResolutionCandidate,
    ModuleResolveMode, PyTyped, ResolvedModule, ResolvedNames, ResolverContext, StubPackageIndex,
    StubPackagePaths, normalize_candidates, resolve_component, resolve_stub_package_in_search_path,
    search_paths, stub_package_index,
};

/// Recursively lists all available modules and submodules.
///
/// Returns a collection sorted in lexicographic order.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn list_all_modules<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> Box<[Module<'db>]> {
    let mut modules = Vec::new();
    let mut stack = vec![list_root_modules(db, resolver_environment)];
    while let Some(listing) = stack.pop() {
        modules.extend_from_slice(&listing.modules);
        for module in &listing.modules_with_possible_children {
            let children = list_submodules(db, *module);
            if !children.is_empty() {
                stack.push(children);
            }
        }
        // Reach local stub overrides through unresolved names;
        // see the example on `ModuleListing::stub_override_listings`.
        stack.extend(listing.stub_override_listings(db, resolver_environment));
    }
    modules.sort_by_cached_key(|module| module.name(db));
    modules.into_boxed_slice()
}

/// Lists top-level modules across the configured search paths.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn list_root_modules<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> ModuleListing<'db> {
    let context = ResolverContext::new(db, resolver_environment, ModuleResolveMode::Typing);
    ModuleSearchCursor::with_configured_search_paths(&context).list_modules()
}

/// Lists immediate submodules of a resolved module.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn list_submodules<'db>(db: &'db dyn Db, module: Module<'db>) -> ModuleListing<'db> {
    let resolver_environment = module.resolver_environment(db);
    let context = ResolverContext::new(db, resolver_environment, ModuleResolveMode::Typing);

    // Desperate resolution can use a search path absent from the configuration.
    // Preserve that path when listing the module's submodules.
    let paths = match module.search_path(db) {
        Some(path)
            if !search_paths(db, resolver_environment, ModuleResolveMode::Typing)
                .any(|configured| configured == path) =>
        {
            RootSearchPaths::Supplied(std::slice::from_ref(path))
        }
        _ => RootSearchPaths::Configured,
    };

    ModuleSearchCursor::at_module_name_prefix(&context, module.name(db), &paths)
        .map(|search| search.list_modules())
        .unwrap_or_default()
}

/// Lists immediate submodules of the given name without requiring that name to be resolvable.
/// This allows module enumeration to reach local stub overrides beneath unresolved names.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn list_submodules_by_name<'db>(
    db: &'db dyn Db,
    name: ModuleNameIngredient<'db>,
) -> ModuleListing<'db> {
    let context = ResolverContext::new(db, name.resolver_environment(db), name.mode(db));
    let Some(search) = ModuleSearchCursor::at_module_name_prefix(
        &context,
        name.name(db),
        &RootSearchPaths::Configured,
    ) else {
        return ModuleListing::default();
    };

    search.list_modules()
}

/// Manages state and logic for advancing through the components of a dotted
/// module name (i.e., `acme`, `tools`, and `power` in the name `acme.tools.power`)
/// during module resolution or enumeration.
pub(super) struct ModuleSearchCursor<'a, 'db> {
    context: &'a ResolverContext<'db>,
    position: Position<'db>,
}

impl<'a, 'db> ModuleSearchCursor<'a, 'db> {
    /// Starts a search using configured search paths.
    pub(super) fn with_configured_search_paths(context: &'a ResolverContext<'db>) -> Self {
        Self::with_paths(context, RootSearchPaths::Configured)
    }

    /// Starts a search using only the supplied search paths.
    pub(super) fn with_supplied_search_paths(
        context: &'a ResolverContext<'db>,
        search_paths: &'db [SearchPath],
    ) -> Self {
        Self::with_paths(context, RootSearchPaths::Supplied(search_paths))
    }

    /// Positions a search that uses the given search roots at the given (absolute)
    /// module name prefix, such that the search can be resumed from that point.
    fn at_module_name_prefix(
        context: &'a ResolverContext<'db>,
        module_name_prefix: &ModuleName,
        paths: &RootSearchPaths<'db>,
    ) -> Option<Self> {
        match paths {
            RootSearchPaths::Configured => {
                // For a search under a configured root path, we can reload search
                // state from Salsa.
                let key = ModuleNameIngredient::new(
                    context.db,
                    module_name_prefix,
                    context.mode,
                    context.resolver_environment,
                );
                Self::restore(context, key)
            }
            RootSearchPaths::Supplied(paths) => {
                // Searches under supplied paths are not cached, so we have to
                // start from the root and advance the cursor to the right position.
                let mut cursor = Self::with_supplied_search_paths(context, paths);
                for component in module_name_prefix.components() {
                    cursor = cursor.advance(component)?;
                }
                Some(cursor)
            }
        }
    }

    /// Resolves a module name relative to the current position of this search's cursor.
    pub(super) fn resolve_name(mut self, name: &ModuleName) -> Option<ResolvedNames<'db>> {
        let mut components = name.components();
        let last = components.next_back()?;
        for component in components {
            self = self.advance(component)?;
        }
        self.resolve_child(last)
    }

    /// Lists the modules immediately available at the cursor's current position.
    ///
    /// In order to prevent cycles that might occur during recursive enumeration
    /// (i.e., when calling this method in a loop) this method enforces the
    /// following directory symlink policy:
    ///
    /// - We allow symlinks for search roots, so listing a symlinked search root
    ///   always returns the modules available immediately beneath it.
    /// - Below a search root, we do list the contents of a directory whose
    ///   relative path from the search root contains a symlink.
    ///
    /// For example, consider the following layout in which `loop` is a symlink
    /// back to `/src/pkg`:
    ///
    /// ```text
    /// src
    /// └── pkg
    ///     ├── __init__.py
    ///     ├── child.py
    ///     └── loop -> .
    /// ```
    ///
    /// Listing `pkg` will return `pkg.child` and `pkg.loop`, but listing
    /// `pkg.loop` will not return any children because its relative path from
    /// `/src` (i.e., `pkg/loop`) crosses a directory symlink. Recursive
    /// enumeration would therefore stop instead of discovering
    /// `pkg.loop.child`, `pkg.loop.loop`, and so on.
    ///
    /// This policy is applied independently to each portion of a namespace
    /// package (so a directory excluded in one portion might still be included
    /// through another).
    ///
    /// File symlinks are always allowed because they cannot create cycles.
    ///
    /// The symlink policy does not affect ordinary module resolution
    /// (which will always traverse a finite set of directories regardless of symlinks).
    fn list_modules(&self) -> ModuleListing<'db> {
        let context = self.context;
        let db = context.db;
        let mut has_directory_by_name = BTreeMap::<_, bool>::new();

        for directory in self.directories_allowed_for_enumeration() {
            for entry in directory.entries(db) {
                if let Some(name) = self.enumerable_module_name(&entry) {
                    *has_directory_by_name
                        .entry(CompactString::new(name))
                        .or_default() |= entry.file_type() == FileType::Directory;
                }
            }
        }

        let mut modules = Vec::new();
        let mut unresolved_names = Vec::new();
        let mut modules_with_possible_children = Vec::new();
        for (component_name, has_directory) in has_directory_by_name {
            let Some(name) = self.full_module_name(&component_name) else {
                continue;
            };

            if let Some(candidates) = self.resolve_child(&component_name) {
                if let Some(candidate) = candidates.into_iter().next() {
                    let module = candidate.into_module(db, context.resolver_environment, &name);
                    modules.push(module);
                    if has_directory {
                        modules_with_possible_children.push(module);
                    }
                }

                // A resolved module takes precedence over unresolved stub override names.
                continue;
            }

            // The full search found no module, so any remaining candidates belong
            // to the stub override search (see [`ModuleListing::unresolved_names`]).
            unresolved_names.push(name);
        }

        ModuleListing {
            modules: modules.into_boxed_slice(),
            unresolved_names: unresolved_names.into_boxed_slice(),
            modules_with_possible_children: modules_with_possible_children.into_boxed_slice(),
        }
    }

    /// Returns a new search which has been advanced by one component of a
    /// module name. The previous search object can be reused for searching
    /// sibling module name components.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called first with "acme", and then again with "tools" on the
    /// resulting object.
    fn advance(&self, component_name: &str) -> Option<Self> {
        let resolver = match &self.position {
            Position::Root(paths) => PrefixResolver::new(self.context, paths, component_name)?,
            Position::Prefix(resolver) => resolver.advance(self.context, component_name)?,
        };
        Some(Self {
            context: self.context,
            position: Position::Prefix(resolver),
        })
    }

    /// Resolves the given terminal component of a module name.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called with "power" after previous calls to [`ModuleSearchCursor::advance`]
    /// with "acme" and "tools".
    fn resolve_child(&self, component_name: &str) -> Option<ResolvedNames<'db>> {
        match &self.position {
            Position::Root(paths) => {
                let name = ModuleName::new(component_name)?;
                let candidates = paths.resolve_root(self.context, &name, false);
                (!candidates.is_empty()).then_some(candidates)
            }
            Position::Prefix(resolver) => resolver.resolve_child(self.context, component_name),
        }
    }

    fn with_paths(context: &'a ResolverContext<'db>, search_paths: RootSearchPaths<'db>) -> Self {
        Self {
            context,
            position: Position::Root(search_paths),
        }
    }

    /// Returns a representation of this cursor that can be cached with Salsa.
    fn snapshot(&self) -> Option<ModuleSearchSnapshot> {
        match &self.position {
            Position::Root(_) => None,
            Position::Prefix(resolver) => Some(resolver.snapshot(self.context)),
        }
    }

    /// Restores a cursor from the snapshot for the given key.
    fn restore(context: &'a ResolverContext<'db>, key: ModuleNameIngredient<'db>) -> Option<Self> {
        Some(Self {
            context,
            position: Position::Prefix(PrefixResolver::restore(context, key)?),
        })
    }

    /// Returns the directories to scan for child module names at the cursor's current position.
    fn directories_allowed_for_enumeration(
        &self,
    ) -> impl Iterator<Item = Cow<'_, ModuleDirectory<'db>>> {
        match &self.position {
            Position::Root(paths) => Either::Left(paths.iter(self.context).map(|path| {
                Cow::Owned(ModuleDirectory::new(
                    self.context,
                    path.to_module_path(),
                    Some(true),
                ))
            })),
            Position::Prefix(resolver) => Either::Right(
                resolver
                    .candidates(self.context)
                    .filter(move |candidate| {
                        !matches!(candidate.module, ResolvedModule::Module(_))
                            && candidate.directory.enumeration_allowed(self.context.db)
                    })
                    .map(|candidate| Cow::Borrowed(&candidate.directory)),
            ),
        }
    }

    /// Returns the candidate module name supplied by the given directory entry
    /// (if the entry does indeed supply a valid module name).
    ///
    /// `Some(name)` does not guarantee that the name resolves to an importable module.
    fn enumerable_module_name<'entry>(
        &self,
        entry: &'entry ModuleDirectoryEntry<'db>,
    ) -> Option<&'entry str> {
        let name = entry.file_name()?;
        let at_search_root = self.prefix().is_none();

        // Below search roots, initializers define the parent package.
        if !at_search_root && matches!(name, "__init__.py" | "__init__.pyi") {
            return None;
        }

        let python_stem = || {
            name.strip_suffix(".py")
                .or_else(|| name.strip_suffix(".pyi"))
        };
        let name = match entry.file_type() {
            FileType::Directory => name,
            // A symlink may name either a Python file or a directory.
            FileType::Symlink => python_stem().unwrap_or(name),
            // Reject files without a Python source or stub extension.
            FileType::File => python_stem()?,
        };
        let name = if at_search_root {
            // Strip the suffix that identifies a top-level stub package.
            name.strip_suffix("-stubs").unwrap_or(name)
        } else {
            name
        };

        // Reject invalid Python identifiers and keywords.
        is_identifier(name).then_some(name)
    }

    /// Appends a component name to this search's module name prefix.
    fn full_module_name(&self, component_name: &str) -> Option<ModuleName> {
        full_module_name(self.prefix(), component_name)
    }

    /// Returns the module name prefix, or `None` before the first component.
    fn prefix(&self) -> Option<&ModuleName> {
        match &self.position {
            Position::Root(_) => None,
            Position::Prefix(resolver) => Some(resolver.prefix()),
        }
    }
}

/// Represents the result of enumerating the immediate submodules of a module
/// name (or the modules immediately available at search roots).
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct ModuleListing<'db> {
    /// The list of fully resolved modules at this stage of enumeration.
    pub(crate) modules: Box<[Module<'db>]>,
    /// The subset of resolved modules that may have enumerable descendants.
    modules_with_possible_children: Box<[Module<'db>]>,
    /// Unresolved module names that are nonetheless eligible for enumeration
    /// because they may have eligible stub override candidates.
    ///
    /// For instance, a stub override (from a configured extra path) can supply
    /// the module `acme.nested.tools` even if the source does not supply the
    /// module `acme.nested`. Hence `acme.nested` must be preserved here as
    /// an unresolved name so that we can still discover `acme.nested.tools`
    /// underneath it at a later point in the recursive enumeration process.
    unresolved_names: Box<[ModuleName]>,
}

impl<'db> ModuleListing<'db> {
    /// Returns non-empty module listings for unresolved stub override names.
    ///
    /// For example, suppose `/extra` is configured as an extra path and
    /// `acme-stubs` is a complete stub package (i.e. not marked as partial):
    ///
    /// ```text
    /// /extra/acme/nested/tools.pyi
    ///
    /// /site-packages/acme-stubs/__init__.pyi
    ///
    /// /site-packages/acme/__init__.py
    /// /site-packages/acme/nested/__init__.py
    /// /site-packages/acme/nested/tools.py
    /// ```
    ///
    /// Typing-mode resolution selects the installed stubs for `acme`. Those
    /// stubs do not supply `acme.nested`, and because the stub package is
    /// "complete" it prevents falling back to the source package. Nonetheless,
    /// the extra-path stub still supplies `acme.nested.tools`.
    ///
    /// Module enumeration must therefore visit the name `acme.nested` to reach
    /// `tools`, even though the name itself does not resolve to a module. This
    /// method returns listings beneath that name without including the name
    /// among the resolved modules.
    fn stub_override_listings(
        &self,
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
    ) -> impl Iterator<Item = &'db ModuleListing<'db>> {
        self.unresolved_names.iter().filter_map(move |name| {
            let name_key = ModuleNameIngredient::new(
                db,
                name,
                ModuleResolveMode::Typing,
                resolver_environment,
            );
            let listing = list_submodules_by_name(db, name_key);
            (!listing.is_empty()).then_some(listing)
        })
    }

    /// Whether there are no modules or stub override names to visit during recursive enumeration.
    fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.unresolved_names.is_empty()
    }
}

/// Caches data used when searching beneath the given module name.
///
/// For example, enumerating either `acme.tools` or `acme.reports` requires
/// finding the portions of `acme` across search paths and applying package and
/// stub precedence to whittle down the set of possible module candidates that
/// should actually be produced by the enumeration operation. As such, both
/// enumerations can reuse a snapshot saved for `acme` before advancing through
/// their respective final components.
#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn module_search_snapshot<'db>(
    db: &'db dyn Db,
    name: ModuleNameIngredient<'db>,
) -> Option<ModuleSearchSnapshot> {
    let context = ResolverContext::new(db, name.resolver_environment(db), name.mode(db));
    let module_name = name.name(db);

    // Retrieve a cursor positioned just before the leaf of the given module name.
    let cursor = match module_name.parent() {
        Some(parent_name) => ModuleSearchCursor::at_module_name_prefix(
            &context,
            &parent_name,
            &RootSearchPaths::Configured,
        )?,
        None => ModuleSearchCursor::with_configured_search_paths(&context),
    };

    // Advance the cursor through the leaf.
    let cursor = cursor.advance(module_name.last_component())?;

    cursor.snapshot()
}

/// Cached state that we use to rehydrate a [`ModuleSearchCursor`].
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
enum ModuleSearchSnapshot {
    Typing {
        stub_override_candidates: Box<[CachedCandidate]>,
        full_search_candidates: Box<[CachedCandidate]>,
    },
    Runtime(Box<[CachedCandidate]>),
}

/// A module resolution candidate saved as part of a [`ModuleSearchSnapshot`].
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct CachedCandidate {
    path: ModulePath,
    module: ResolvedModule,
    py_typed: PyTyped,
    precedence: CandidatePrecedence,

    /// Stores the result of the symlink policy for enumeration. This prevents
    /// unnecessary cache invalidation of a `ModuleListing`, since the symlink
    /// policy makes that result depend on the directory listing of an ancestor.
    ///
    /// For example, enforcing the symlink policy for `/src/acme` requires
    /// reading `/src`. By storing this value, we make it so that we don't need
    /// to invalidate the cached value of `acme`'s submodules if an unrelated
    /// file is added to `/src`.
    enumeration_allowed: bool,
}

impl CachedCandidate {
    fn new(db: &dyn Db, candidate: &ModuleResolutionCandidate<'_>) -> Self {
        Self {
            path: candidate.directory.path().clone(),
            enumeration_allowed: candidate.directory.enumeration_allowed(db),
            module: candidate.module,
            py_typed: candidate.py_typed,
            precedence: candidate.precedence,
        }
    }

    fn restore<'db>(&self, context: &ResolverContext<'db>) -> ModuleResolutionCandidate<'db> {
        ModuleResolutionCandidate {
            directory: ModuleDirectory::new(
                context,
                self.path.clone(),
                Some(self.enumeration_allowed),
            ),
            module: self.module,
            py_typed: self.py_typed,
            precedence: self.precedence,
        }
    }
}

/// Represents search progress through the components of a module name.
enum Position<'db> {
    /// Starting position of a new search, before any module name components have been consumed.
    Root(RootSearchPaths<'db>),
    /// Position of an ongoing search that has already progressed through some components of a
    /// module name (the prefix).
    Prefix(PrefixResolver<'db>),
}

/// Encapsulates state and logic for advancing a search that has already
/// progressed through a particular prefix of the module name.
///
/// Searches proceed in either typing mode or runtime mode, and can use either
/// configured search paths or explicitly supplied search paths.
enum PrefixResolver<'db> {
    Typing(TypingModeResolver<'db>),
    Runtime(RuntimeModeResolver<'db>),
}

impl<'db> PrefixResolver<'db> {
    /// Starts a prefix search from the root component of a module name.
    fn new(
        context: &ResolverContext<'db>,
        paths: &RootSearchPaths<'db>,
        component_name: &str,
    ) -> Option<Self> {
        let prefix = ModuleName::new(component_name)?;
        if context.mode.is_typing() {
            TypingModeResolver::new(context, paths, prefix).map(Self::Typing)
        } else {
            RuntimeModeResolver::new(context, paths, prefix).map(Self::Runtime)
        }
    }

    /// Saves the candidates for this module name prefix.
    fn snapshot(&self, context: &ResolverContext<'db>) -> ModuleSearchSnapshot {
        match self {
            Self::Typing(resolver) => resolver.snapshot(context),
            Self::Runtime(resolver) => resolver.snapshot(context.db),
        }
    }

    /// Restores a prefix resolver from the snapshot for the given key.
    fn restore(context: &ResolverContext<'db>, key: ModuleNameIngredient<'db>) -> Option<Self> {
        let snapshot = module_search_snapshot(context.db, key)?;
        let prefix = key.name(context.db);

        Some(match snapshot {
            ModuleSearchSnapshot::Typing {
                stub_override_candidates,
                full_search_candidates,
            } => Self::Typing(TypingModeResolver::restore(
                context,
                prefix,
                stub_override_candidates,
                full_search_candidates,
            )),
            ModuleSearchSnapshot::Runtime(candidates) => {
                Self::Runtime(RuntimeModeResolver::restore(context, prefix, candidates))
            }
        })
    }

    /// Returns a resolver advanced by one prefix component, retaining the candidates
    /// needed to resolve its descendants. This resolver remains reusable for siblings.
    fn advance(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
        match self {
            Self::Typing(resolver) => resolver.advance(context, component_name).map(Self::Typing),
            Self::Runtime(resolver) => resolver.advance(context, component_name).map(Self::Runtime),
        }
    }

    // Resolves a terminal component of a module name.
    fn resolve_child(
        &self,
        context: &ResolverContext<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        match self {
            Self::Typing(resolver) => resolver.resolve_child(context, component_name),
            Self::Runtime(resolver) => resolver.resolve_child(context, component_name),
        }
    }

    fn candidates(
        &self,
        context: &ResolverContext<'db>,
    ) -> impl Iterator<Item = &ModuleResolutionCandidate<'db>> {
        match self {
            Self::Typing(resolver) => Either::Left(resolver.candidates(context)),
            Self::Runtime(resolver) => Either::Right(resolver.candidates.iter()),
        }
    }

    fn prefix(&self) -> &ModuleName {
        match self {
            Self::Typing(resolver) => &resolver.prefix,
            Self::Runtime(resolver) => &resolver.prefix,
        }
    }
}

/// Implements the typing-mode rules for resolving a module.
///
/// With configured search paths, resolution proceeds in two phases:
///
/// 1. We search any configured extra paths for a stub (i.e., `.pyi`) override
///    of the module name.
/// 2. If the first search fails, then we search all configured search paths
///    (including extra paths) for a module candidate (i.e., an ordinary package,
///    a namespace package, or a plain module), giving priority to stub-only
///    packages over ordinary packages/modules. We generally refer to this phase
///    as the "full" search.
///
/// The second phase has a few additional nuances:
///
/// - A stub-only package from a search path after the standard library does not
///   override a standard library package/module (unlike in the first phase).
/// - A stub-only package can be marked as partial, in which case any modules missing
///   from that package may instead be supplied by an ordinary package.
/// - An ordinary package may provide a stub file, a source file, or both;
///   within a single directory the stub takes precedence over the source.
///
/// Typing mode with supplied search paths still considers stubs, but skips
/// the separate extra-path stub override search.
struct TypingModeResolver<'db> {
    prefix: ModuleName,

    /// Candidates for the root module name component that were discovered in
    /// extra paths during the first phase of the module search.
    ///
    /// This is only used with configured search paths. It is stored before
    /// advancing the search to later module name components so that the second
    /// phase of the search can combine them with candidates from other paths
    /// without searching extra paths again.
    root_candidates_from_extra_paths: Option<Rc<[ModuleResolutionCandidate<'db>]>>,

    /// Candidates for the current module name prefix that should be considered
    /// for the first-phase stub override search from extra paths.
    ///
    /// This is only used with configured search paths.
    stub_override_candidates: ResolvedNames<'db>,

    /// Candidates for the current module name prefix that should be considered
    /// for the full search.
    ///
    /// When advancing the search, we initialize this cell immediately if the parent
    /// prefix's cell is already initialized. We do so by advancing the parent's
    /// candidates by one component. Otherwise, we defer initialization until the
    /// stub override search cannot supply a result.
    ///
    /// Searches using supplied paths always initialize this cell when creating the
    /// prefix. The cell retains its candidates for subsequent searches from the
    /// same prefix.
    full_search_candidates: OnceCell<ResolvedNames<'db>>,
}

impl<'db> TypingModeResolver<'db> {
    /// Starts a prefix search from the root component of a module name.
    fn new(
        context: &ResolverContext<'db>,
        paths: &RootSearchPaths<'db>,
        prefix: ModuleName,
    ) -> Option<Self> {
        let resolver = match paths {
            RootSearchPaths::Configured => {
                let (extra_stub_package_paths, _) =
                    stub_package_index(context.db, context.resolver_environment)
                        .split_by_extra_paths();
                let root_candidates = discover_roots(
                    context,
                    prefix.as_str(),
                    false,
                    search_paths(context.db, context.resolver_environment, context.mode)
                        .take_while(|path| path.is_extra()),
                    extra_stub_package_paths,
                );
                let stub_override_candidates =
                    normalize_candidates(context, root_candidates.clone(), true);
                Self {
                    prefix,
                    root_candidates_from_extra_paths: (!root_candidates.is_empty())
                        .then(|| Rc::from(root_candidates)),
                    stub_override_candidates,
                    full_search_candidates: OnceCell::new(),
                }
            }
            RootSearchPaths::Supplied(_) => Self {
                full_search_candidates: OnceCell::from(paths.resolve_root(context, &prefix, true)),
                prefix,
                root_candidates_from_extra_paths: None,
                stub_override_candidates: Vec::new(),
            },
        };

        // Stop if neither search phase has candidates for this module name prefix.
        if resolver.stub_override_candidates.is_empty()
            && resolver.full_search_candidates(context).is_empty()
        {
            return None;
        }

        Some(resolver)
    }

    /// Saves the candidates for both phases of the search.
    fn snapshot(&self, context: &ResolverContext<'db>) -> ModuleSearchSnapshot {
        let db = context.db;
        ModuleSearchSnapshot::Typing {
            stub_override_candidates: self
                .stub_override_candidates
                .iter()
                .map(|candidate| CachedCandidate::new(db, candidate))
                .collect(),
            full_search_candidates: self
                .full_search_candidates(context)
                .iter()
                .map(|candidate| CachedCandidate::new(db, candidate))
                .collect(),
        }
    }

    /// Restores both search phases from their cached candidates.
    fn restore(
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
        stub_override_candidates: &[CachedCandidate],
        full_search_candidates: &[CachedCandidate],
    ) -> Self {
        let restore = |candidates: &[CachedCandidate]| {
            candidates
                .iter()
                .map(|candidate| candidate.restore(context))
                .collect()
        };

        Self {
            prefix: prefix.clone(),
            root_candidates_from_extra_paths: None,
            stub_override_candidates: restore(stub_override_candidates),
            full_search_candidates: OnceCell::from(restore(full_search_candidates)),
        }
    }

    fn advance(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
        let prefix = full_module_name(Some(&self.prefix), component_name)?;
        let stub_override_candidates = advance_candidates(
            context,
            self.stub_override_candidates.clone(),
            component_name,
            ComponentFileFilter::ByMode,
            true,
        );

        // Advance the second-phase candidates if already computed.
        // Otherwise, defer that search while the first phase may still find a stub override.
        let full_search_candidates = self.full_search_candidates.get().map(|candidates| {
            advance_candidates(
                context,
                candidates.clone(),
                component_name,
                ComponentFileFilter::ByMode,
                true,
            )
        });
        let resolver = Self {
            prefix,
            root_candidates_from_extra_paths: self
                .root_candidates_from_extra_paths
                .as_ref()
                .map(Rc::clone),
            stub_override_candidates,
            full_search_candidates: full_search_candidates
                .map(OnceCell::from)
                .unwrap_or_default(),
        };

        // Stop if neither search phase has candidates for this module name prefix.
        if resolver.stub_override_candidates.is_empty()
            && resolver.full_search_candidates(context).is_empty()
        {
            return None;
        }

        Some(resolver)
    }

    fn resolve_child(
        &self,
        context: &ResolverContext<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        let name = full_module_name(Some(&self.prefix), component_name)?;

        // First phase: attempt to resolve the child through a stub override in extra paths.
        let stub_override = advance_candidates(
            context,
            self.stub_override_candidates.clone(),
            name.last_component(),
            // When resolving `acme.tools`, the stub override search may have entered `acme`
            // through an ordinary package's `acme/__init__.py`. The requested child must now
            // come from `tools.pyi` or `tools/__init__.pyi`, not a source (`.py`) file.
            ComponentFileFilter::StubOnly,
            false,
        );
        if !stub_override.is_empty() {
            return Some(stub_override);
        }

        // Second phase: resolve the child using the full search candidates.
        let candidates = advance_candidates(
            context,
            self.full_search_candidates(context).clone(),
            name.last_component(),
            ComponentFileFilter::ByMode,
            false,
        );
        (!candidates.is_empty()).then_some(candidates)
    }

    fn candidates(
        &self,
        context: &ResolverContext<'db>,
    ) -> impl Iterator<Item = &ModuleResolutionCandidate<'db>> {
        self.stub_override_candidates
            .iter()
            .chain(self.full_search_candidates(context))
    }

    /// Returns module candidates for the full search, initializing the candidates
    /// on first access if needed.
    fn full_search_candidates(&self, context: &ResolverContext<'db>) -> &ResolvedNames<'db> {
        self.full_search_candidates.get_or_init(|| {
            let (_, remaining_stub_package_paths) =
                stub_package_index(context.db, context.resolver_environment).split_by_extra_paths();

            // Combine candidates across all search paths before traversing the
            // module name prefix, so that an ordinary package in one search path
            // can shadow a namespace portion in another.
            let mut candidates = self
                .root_candidates_from_extra_paths
                .as_deref()
                .unwrap_or_default()
                .to_vec();
            candidates.extend(discover_roots(
                context,
                self.prefix.first_component(),
                false,
                search_paths(context.db, context.resolver_environment, context.mode)
                    .skip_while(|path| path.is_extra()),
                remaining_stub_package_paths,
            ));
            candidates = normalize_candidates(context, candidates, true);

            for component_name in self.prefix.components().skip(1) {
                candidates = advance_candidates(
                    context,
                    candidates,
                    component_name,
                    ComponentFileFilter::ByMode,
                    true,
                );
            }

            candidates
        })
    }
}

/// Implements the runtime-mode rules for resolving a module.
///
/// Searches the configured or supplied search paths for source modules and packages,
/// ignoring stub files and stub-only packages.
struct RuntimeModeResolver<'db> {
    prefix: ModuleName,
    candidates: ResolvedNames<'db>,
}

impl<'db> RuntimeModeResolver<'db> {
    /// Starts a prefix search from the root component of a module name.
    fn new(
        context: &ResolverContext<'db>,
        paths: &RootSearchPaths<'db>,
        prefix: ModuleName,
    ) -> Option<Self> {
        let candidates = paths.resolve_root(context, &prefix, true);
        (!candidates.is_empty()).then_some(Self { prefix, candidates })
    }

    /// Saves the runtime search candidates.
    fn snapshot(&self, db: &dyn Db) -> ModuleSearchSnapshot {
        ModuleSearchSnapshot::Runtime(
            self.candidates
                .iter()
                .map(|candidate| CachedCandidate::new(db, candidate))
                .collect(),
        )
    }

    /// Restores the runtime search from its cached candidates.
    fn restore(
        context: &ResolverContext<'db>,
        prefix: &ModuleName,
        candidates: &[CachedCandidate],
    ) -> Self {
        Self {
            prefix: prefix.clone(),
            candidates: candidates
                .iter()
                .map(|candidate| candidate.restore(context))
                .collect(),
        }
    }

    fn advance(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
        let prefix = full_module_name(Some(&self.prefix), component_name)?;
        let candidates = advance_candidates(
            context,
            self.candidates.clone(),
            component_name,
            ComponentFileFilter::ByMode,
            true,
        );
        (!candidates.is_empty()).then_some(Self { prefix, candidates })
    }

    fn resolve_child(
        &self,
        context: &ResolverContext<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        let name = full_module_name(Some(&self.prefix), component_name)?;
        let candidates = advance_candidates(
            context,
            self.candidates.clone(),
            name.last_component(),
            ComponentFileFilter::ByMode,
            false,
        );
        (!candidates.is_empty()).then_some(candidates)
    }
}

enum RootSearchPaths<'db> {
    Configured,
    Supplied(&'db [SearchPath]),
}

impl<'db> RootSearchPaths<'db> {
    fn resolve_root(
        &self,
        context: &ResolverContext<'db>,
        name: &ModuleName,
        for_module_name_prefix: bool,
    ) -> ResolvedNames<'db> {
        let is_non_shadowable = !for_module_name_prefix
            && context.mode.is_non_shadowable(
                context
                    .resolver_environment
                    .python_version(context.db)
                    .minor,
                name.as_str(),
            );

        // Typing mode requires us to consider stub-only packages (such as the package named
        // `acme-stubs`, which only contains `.pyi` files) when resolving a module name
        // (e.g., the name `acme`), so we must select all stub package paths here.
        let stub_packages = context.mode.is_typing().then(|| match self {
            Self::Configured => {
                Cow::Borrowed(stub_package_index(context.db, context.resolver_environment))
            }
            Self::Supplied(paths) => Cow::Owned(StubPackageIndex::from_search_paths(
                context.db,
                paths.iter(),
            )),
        });

        // Runtime resolution ignores stub packages, so we can pass the default value for stub
        // package paths (i.e., no stub package paths at all).
        let stub_paths = stub_packages
            .as_ref()
            .map_or_else(StubPackagePaths::default, |index| index.all());

        let candidates = discover_roots(
            context,
            name.first_component(),
            is_non_shadowable,
            self.iter(context),
            stub_paths,
        );

        normalize_candidates(context, candidates, for_module_name_prefix)
    }

    fn iter(&self, context: &ResolverContext<'db>) -> impl Iterator<Item = &'db SearchPath> {
        match self {
            Self::Configured => Either::Left(search_paths(
                context.db,
                context.resolver_environment,
                context.mode,
            )),
            Self::Supplied(paths) => Either::Right(paths.iter()),
        }
    }
}

/// Finds candidates for the root component of a module name (i.e. `foo`
/// from `foo.bar.baz`) across the supplied search paths and stub packages.
///
/// Callers should pass `true` for `is_non_shadowable` only when the given
/// `root_component` is the complete module name being resolved and that name
/// is non-shadowable according to `ModuleResolveMode::is_non_shadowable`.
/// This prevents a standard library module name like `types` from being
/// shadowed by a local source.
///
/// Conversely, if `root_component` is only the prefix of the module name
/// being resolved (e.g., when using this function to discover roots for the
/// module name `types.child`), `is_non_shadowable` should be `false`.
///
/// Before the resulting candidates can be advanced for subsequent components
/// of a module name (see [`advance_candidates`]), they should
/// be combined with candidates discovered from other search paths and normalized
/// with [`normalize_candidates`] (with `for_module_name_prefix` set to `true`).
fn discover_roots<'db, 'a>(
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
            ModuleDirectory::new(context, search_path.to_module_path(), Some(true))
                .may_contain_name(stub_name)
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
        let mut candidate = ModuleResolutionCandidate::root(context, search_path);
        let resolved = resolve_component(
            context,
            &mut candidate,
            root_component,
            ComponentFileFilter::ByMode,
        )
        .is_ok();
        let terminal = candidate.missing_submodule_is_terminal(context);
        if resolved {
            cur_candidates.push(candidate);
        }
        // A terminal candidate shadows all later search paths. Keep earlier candidates;
        // normalization may still discard namespace portions shadowed by this candidate.
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

/// Finds candidates for the next component of a module name, starting from
/// candidates for its parent prefix. `filter` determines which file types
/// may supply the component.
///
/// Input candidates must already be ordered by priority, with shadowed
/// namespace portions removed.
///
/// Callers should pass `true` for `component_name_is_prefix` when more
/// module name components will be resolved. This preserves partial namespace
/// portions from stub-only packages so they can supply later components, even
/// when an ordinary package or module exists for the same prefix.
///
/// Otherwise, callers should pass `false` to resolve the complete module
/// name. An ordinary package or plain module then shadows namespace portions.
fn advance_candidates<'db>(
    context: &ResolverContext<'db>,
    mut candidates: ResolvedNames<'db>,
    component_name: &str,
    filter: ComponentFileFilter,
    component_name_is_prefix: bool,
) -> ResolvedNames<'db> {
    let mut remaining_are_shadowed = false;
    let mut remaining = candidates.len();
    candidates.retain_mut(|candidate| {
        remaining -= 1;
        if remaining_are_shadowed {
            return false;
        }

        let resolved = resolve_component(context, candidate, component_name, filter).is_ok();

        // A terminal candidate shadows every lower-priority candidate, even if resolving
        // this component fails. Higher-priority candidates remain in play.
        remaining_are_shadowed = remaining > 0 && candidate.missing_submodule_is_terminal(context);

        resolved
    });
    normalize_candidates(context, candidates, component_name_is_prefix)
}

fn full_module_name(prefix: Option<&ModuleName>, component_name: &str) -> Option<ModuleName> {
    let child = ModuleName::new(component_name)?;
    match prefix {
        None => Some(child),
        Some(prefix) => {
            let mut name = prefix.clone();
            name.extend(&child);
            Some(name)
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    #[cfg(target_family = "unix")]
    use ruff_db::Db as _;
    use ruff_db::system::SystemPath;
    #[cfg(target_family = "unix")]
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, OsSystem};

    use crate::ModuleName;
    use crate::db::tests::TestDb;
    use crate::resolve::{ModuleResolveMode, ResolverContext};
    #[cfg(target_family = "unix")]
    use crate::settings::SearchPathSettings;
    #[cfg(target_family = "unix")]
    use crate::strategy::FallibleStrategy;
    use crate::testing::{ModuleDebugSnapshot, TestCaseBuilder};

    use super::{ModuleListing, ModuleSearchCursor, list_root_modules, list_submodules};

    #[test]
    fn module_search_can_be_reused_across_sibling_module_resolutions() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/reports.py", "")])
            .with_site_packages_files(&[("acme/tools.py", "")])
            .build()
            .db;
        for mode in [ModuleResolveMode::Typing, ModuleResolveMode::Runtime] {
            let context = ResolverContext::new(&db, db.resolver_environment(), mode);
            let root = ModuleSearchCursor::with_configured_search_paths(&context);
            let acme = root.advance("acme").expect("namespace exists");

            assert_resolves_to(&db, &acme, "reports", "/src/acme/reports.py");
            assert_resolves_to(&db, &acme, "tools", "/site-packages/acme/tools.py");

            assert!(acme.resolve_child("missing").is_none());

            assert_resolves_to(&db, &acme, "reports", "/src/acme/reports.py");
        }
    }

    #[test]
    fn sibling_modules_can_be_resolved_correctly_in_any_order() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/__init__.py", ""),
                ("acme/patched.py", ""),
                ("acme/runtime.py", ""),
            ])
            .with_extra_path("/extra", &[("acme/patched.pyi", "")])
            .build()
            .db;
        for children in [["patched", "runtime"], ["runtime", "patched"]] {
            let context =
                ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
            let acme = ModuleSearchCursor::with_configured_search_paths(&context)
                .advance("acme")
                .expect("package has a stub override and full search candidates");
            for child in children {
                let expected = match child {
                    "patched" => "/extra/acme/patched.pyi",
                    _ => "/src/acme/runtime.py",
                };
                assert_resolves_to(&db, &acme, child, expected);
            }
        }
    }

    #[test]
    fn module_resolution_does_not_affect_nested_package_searches() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/__init__.py", ""),
                ("acme/runtime.py", ""),
                ("acme/tools/__init__.py", ""),
                ("acme/tools/patched.py", ""),
                ("acme/tools/runtime.py", ""),
            ])
            .with_extra_path("/extra", &[("acme/tools/patched.pyi", "")])
            .build()
            .db;
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let acme = ModuleSearchCursor::with_configured_search_paths(&context)
            .advance("acme")
            .expect("parent package exists");

        // Advance to the nested package before and after a sibling lookup requires
        // the parent's second-phase search across all configured paths.
        let tools_before = acme.advance("tools").expect("nested package exists");
        assert_resolves_to(&db, &acme, "runtime", "/src/acme/runtime.py");
        let tools_after = acme.advance("tools").expect("nested package exists");

        for tools in [&tools_before, &tools_after] {
            assert_resolves_to(&db, tools, "patched", "/extra/acme/tools/patched.pyi");
            assert_resolves_to(&db, tools, "runtime", "/src/acme/tools/runtime.py");
        }
    }

    #[test]
    fn module_enumeration_excludes_local_files_with_protected_standard_library_names() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("leaf.py", ""), ("sys.py", "")])
            .build()
            .db;
        // A local file cannot supply a protected name, even when typeshed omits it.
        ListingCase::root().expect_module("leaf").assert(&db);
    }

    #[test]
    fn module_enumeration_includes_namespaces_inside_regular_packages() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("regular/__init__.py", ""),
                ("regular/namespace/child.py", ""),
            ])
            .build()
            .db;
        ListingCase::for_name("regular")
            .expect_module("regular.namespace")
            .assert(&db);
        ListingCase::for_name("regular.namespace")
            .expect_module("regular.namespace.child")
            .assert(&db);
    }

    #[test]
    fn module_enumeration_uses_resolution_precedence_for_namespace_children() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/package/__init__.py", ""),
                // The regular package excludes children from the other namespace portion.
                ("acme/package/local.py", ""),
                // A file module blocks descendants from a competing namespace directory.
                ("acme/module.py", ""),
            ])
            .with_site_packages_files(&[
                ("acme/package/hidden.py", ""),
                ("acme/module/hidden.py", ""),
            ])
            .build()
            .db;
        assert_debug_snapshot!(ListingCase::for_name("acme").snapshot(&db), @r#"
        [
            Module::File("acme.module", "first-party", "/src/acme/module.py", Module, None),
            Module::File("acme.package", "first-party", "/src/acme/package/__init__.py", Package, None),
        ]
        "#);
        assert_debug_snapshot!(ListingCase::for_name("acme.package").snapshot(&db), @r#"
        [
            Module::File("acme.package.local", "first-party", "/src/acme/package/local.py", Module, None),
        ]
        "#);
        ListingCase::for_name("acme.module").assert(&db);
    }

    #[test]
    fn module_enumeration_combines_partial_stub_namespaces_with_source_packages() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/__init__.py", ""), ("acme/runtime.py", "")])
            .with_site_packages_files(&[
                ("acme-stubs/stubbed.pyi", ""),
                ("acme-stubs/py.typed", "partial\n"),
            ])
            .build()
            .db;
        assert_debug_snapshot!(ListingCase::for_name("acme").snapshot(&db), @r#"
        [
            Module::File("acme.runtime", "first-party", "/src/acme/runtime.py", Module, None),
            Module::File("acme.stubbed", "site-packages", "/site-packages/acme-stubs/stubbed.pyi", Module, None),
        ]
        "#);
    }

    #[test]
    fn module_enumeration_includes_partial_stub_descendants_of_source_modules() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme.py", "")])
            .with_site_packages_files(&[
                ("acme-stubs/child.pyi", ""),
                ("acme-stubs/py.typed", "partial\n"),
            ])
            .build()
            .db;
        ListingCase::for_name("acme")
            .expect_module("acme.child")
            .assert(&db);
    }

    #[test]
    fn module_enumeration_combines_stub_overrides_with_source_siblings() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/__init__.py", ""), ("acme/runtime.py", "")])
            .with_extra_path("/extra", &[("acme/stubbed.pyi", "")])
            .build()
            .db;
        assert_debug_snapshot!(ListingCase::for_name("acme").snapshot(&db), @r#"
        [
            Module::File("acme.runtime", "first-party", "/src/acme/runtime.py", Module, None),
            Module::File("acme.stubbed", "extra", "/extra/acme/stubbed.pyi", Module, None),
        ]
        "#);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn module_enumeration_of_aliases_does_not_traverse_directory_symlinks() -> anyhow::Result<()> {
        let temp = tempfile::TempDir::new()?;
        let root = temp.path().canonicalize()?;
        let root = SystemPath::from_std_path(&root).expect("UTF-8 workspace path");
        let mut db = TestDb::new();
        db.use_system(OsSystem::new(root));
        for path in [
            "site-packages/acme/shared.py",
            "site-packages/acme/visible.py",
            "other_ns/shared.py",
            "other_ns/hidden.py",
            "src/regular/__init__.py",
            "src/regular/nested/child.py",
        ] {
            db.write_file(root.join(path), "")?;
        }
        for (source, link) in [
            ("other_ns", "src/acme"),
            ("src/regular", "src/alias"),
            ("src/regular/__init__.py", "src/regular/nested/__init__.py"),
        ] {
            std::os::unix::fs::symlink(root.join(source), root.join(link))?;
        }

        let settings = SearchPathSettings {
            src_roots: vec![root.join("src")],
            site_packages_paths: vec![root.join("site-packages")],
            ..SearchPathSettings::empty()
        };
        db.set_search_paths(settings.to_search_paths(
            db.system(),
            db.vendored(),
            &FallibleStrategy,
        )?);

        // The ordinary namespace portion supplies names, but resolution can select an alias.
        ListingCase::for_name("acme")
            .expect_modules(&["acme.shared", "acme.visible"])
            .assert(&db);
        // An initializer symlink does not prevent traversal of its containing directory.
        ListingCase::for_name("regular.nested")
            .expect_module("regular.nested.child")
            .assert(&db);
        ListingCase::for_name("alias").assert(&db);
        ListingCase::for_name("alias.nested").assert(&db);

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn module_enumeration_supports_symlinked_search_roots() -> anyhow::Result<()> {
        let temp = tempfile::TempDir::new()?;
        let root = temp.path().canonicalize()?;
        let root = SystemPath::from_std_path(&root).expect("UTF-8 workspace path");
        let mut db = TestDb::new();
        db.use_system(OsSystem::new(root));
        db.write_file(root.join("source/pkg/__init__.py"), "")?;
        db.write_file(root.join("source/pkg/child.py"), "")?;
        db.write_file(root.join("typeshed/stdlib/VERSIONS"), "")?;
        std::os::unix::fs::symlink(root.join("source"), root.join("src"))?;

        let settings = SearchPathSettings {
            src_roots: vec![root.join("src")],
            custom_typeshed: Some(root.join("typeshed")),
            ..SearchPathSettings::empty()
        };
        db.set_search_paths(settings.to_search_paths(
            db.system(),
            db.vendored(),
            &FallibleStrategy,
        )?);

        ListingCase::root().expect_module("pkg").assert(&db);
        ListingCase::for_name("pkg")
            .expect_module("pkg.child")
            .assert(&db);

        Ok(())
    }

    #[test]
    fn module_enumeration_excludes_stub_files_in_runtime_mode() {
        let db = TestCaseBuilder::new()
            .with_site_packages_files(&[
                ("acme/__init__.py", ""),
                ("acme/child.py", ""),
                ("acme/stub_only.pyi", ""),
            ])
            .build()
            .db;
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Runtime);
        let name = ModuleName::new_static("acme").expect("valid name");
        let listing = ModuleSearchCursor::at_module_name_prefix(
            &context,
            &name,
            &super::RootSearchPaths::Configured,
        )
        .expect("runtime package")
        .list_modules();
        assert_eq!(listing.modules.len(), 1);
        let child = listing.modules[0];
        assert_eq!(child.name(&db).as_str(), "acme.child");
        assert_eq!(
            child
                .file(&db)
                .expect("source file")
                .path(&db)
                .as_system_path(),
            Some(SystemPath::new("/site-packages/acme/child.py"))
        );
    }

    fn assert_resolves_to(
        db: &TestDb,
        search: &ModuleSearchCursor,
        component: &str,
        expected: &str,
    ) {
        let name = search
            .full_module_name(component)
            .expect("valid child name");
        let candidate = search
            .resolve_child(component)
            .and_then(|candidates| candidates.into_iter().next())
            .expect("child resolves");
        let module = candidate.into_module(db, db.resolver_environment(), &name);
        let file = module.file(db).expect("child has a defining file");
        assert_eq!(
            file.path(db).as_system_path(),
            Some(SystemPath::new(expected))
        );
    }

    /// An enumeration target and the modules expected beneath it.
    struct ListingCase<'a> {
        parent_module_name: Option<&'a str>,
        expected_module_names: Vec<&'a str>,
    }

    impl<'a> ListingCase<'a> {
        fn root() -> Self {
            Self {
                parent_module_name: None,
                expected_module_names: Vec::new(),
            }
        }

        fn for_name(name: &'a str) -> Self {
            Self {
                parent_module_name: Some(name),
                ..Self::root()
            }
        }

        fn expect_module(mut self, name: &'a str) -> Self {
            self.expected_module_names.push(name);
            self
        }

        fn expect_modules(mut self, names: &[&'a str]) -> Self {
            self.expected_module_names.extend_from_slice(names);
            self
        }

        /// Formats listed modules after checking that they agree with ordinary resolution.
        #[track_caller]
        fn snapshot<'db>(&self, db: &'db TestDb) -> Vec<ModuleDebugSnapshot<'db>> {
            let listing = self.list_modules(db);
            listing
                .modules
                .iter()
                .copied()
                .map(|module| ModuleDebugSnapshot { db, module })
                .collect()
        }

        #[track_caller]
        fn assert(&self, db: &TestDb) {
            let listing = self.list_modules(db);
            let module_names: Vec<_> = listing
                .modules
                .iter()
                .map(|module| module.name(db).as_str())
                .collect();
            assert_eq!(module_names, self.expected_module_names);
        }

        /// Lists modules and checks that enumeration agrees with ordinary resolution.
        #[track_caller]
        fn list_modules<'db>(&self, db: &'db TestDb) -> &'db ModuleListing<'db> {
            let listing = match self.parent_module_name {
                None => list_root_modules(db, db.resolver_environment()),
                Some(name) => {
                    let name = ModuleName::new(name).expect("valid module name");
                    let module =
                        crate::resolve_module_confident(db, db.resolver_environment(), &name)
                            .expect("parent module resolves");
                    list_submodules(db, module)
                }
            };
            for module in &listing.modules {
                let name = module.name(db);
                assert_eq!(
                    Some(*module),
                    crate::resolve_module_confident(db, db.resolver_environment(), name),
                    "enumeration must agree with resolution for {name}"
                );
            }
            for name in &listing.unresolved_names {
                assert!(
                    crate::resolve_module_confident(db, db.resolver_environment(), name).is_none(),
                    "unresolved name {name} must not resolve"
                );
            }
            listing
        }
    }
}
