//! This module exposes a [`ModuleSearchCursor`] abstraction, which encapsulates
//! logic for efficient module resolution and (namespace-aware) module enumeration.
//!
//! It provides the following interfaces:
//!
//! - [`ModuleSearchCursor::resolve_name`] which resolves a module relative to the current
//!   position of the cursor (i.e., at some point along the individual components of a dotted
//!   module name).
//! - [`ModuleSearchCursor::list_modules`] which list modules immediately available (i.e., the
//!   direct sub-modules) at the current position of the cursor.
//!
//! The latter operation is also accessible via the free (convenience) functions
//! [`list_root_modules`] and [`list_submodules`].

use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use compact_str::CompactString;
use itertools::Either;
use ruff_db::system::FileType;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::module::Module;
use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, ModulePath, SearchPath};

use super::{
    CandidatePrecedence, ComponentFileFilter, ModuleNameIngredient, ModuleResolutionCandidate,
    PyTyped, ResolvedModule, ResolvedNames, ResolverContext, StubPackageIndex, StubPackagePaths,
    normalize_candidates, resolve_component, resolve_stub_package_in_search_path, search_paths,
    stub_package_index,
};

/// Lists top-level modules across the configured search paths.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Module listing is consumed by the next change's cached queries"
    )
)]
pub(crate) fn list_root_modules<'db>(context: &ResolverContext<'db>) -> ModuleListing<'db> {
    ModuleSearchCursor::with_configured_search_paths(context).list_modules()
}

/// Lists immediate submodules of a resolved module across the configured search paths.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Module listing is consumed by the next change's cached queries"
    )
)]
pub(crate) fn list_submodules<'db>(
    context: &ResolverContext<'db>,
    module: Module<'db>,
) -> ModuleListing<'db> {
    list_submodules_by_name(context, module.name(context.db))
}

/// Lists immediate submodules of the given name without requiring that name to be resolvable.
/// This allows module enumeration to reach local stub overrides beneath unresolved names.
fn list_submodules_by_name<'db>(
    context: &ResolverContext<'db>,
    name: &ModuleName,
) -> ModuleListing<'db> {
    let Some(search) =
        ModuleSearchCursor::for_module_name(context, name, &RootSearchPaths::Configured)
    else {
        return ModuleListing::default();
    };

    search.list_modules()
}

/// Encapsulates reusable search state for module resolution and enumeration.
pub(crate) struct ModuleSearchCursor<'a, 'db> {
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

    /// Starts a search beneath the given (absolute) module name using
    /// the given root search paths.
    pub(super) fn for_module_name(
        context: &'a ResolverContext<'db>,
        name: &ModuleName,
        paths: &RootSearchPaths<'db>,
    ) -> Option<Self> {
        match paths {
            RootSearchPaths::Configured => {
                // The cache describes absolute names under the configured search paths,
                // so a search starting at their root can restore the cached candidates.

                let name_key = ModuleNameIngredient::new(
                    context.db,
                    name,
                    context.mode,
                    context.resolver_environment,
                );
                let snapshot = module_search_snapshot(context.db, name_key)?;
                Some(Self::from_snapshot(context, name, snapshot))
            }
            RootSearchPaths::Supplied(paths) => {
                // Supplied paths are not part of the cache key. Walk them directly
                // rather than restoring candidates from the configured search paths.
                let mut search = Self::with_supplied_search_paths(context, paths);
                for component in name.components() {
                    search = search.advance(component)?;
                }
                Some(search)
            }
        }
    }

    /// Saves candidates for the current module name, or returns `None` at the search roots.
    fn to_snapshot(&self) -> Option<ModuleSearchSnapshot> {
        let db = self.context.db;
        match &self.position {
            Position::Prefix(PrefixResolver::Typing(resolver)) => {
                Some(ModuleSearchSnapshot::Typing {
                    stub_override_candidates: resolver
                        .stub_override_candidates
                        .iter()
                        .map(|candidate| CachedCandidate::new(db, candidate))
                        .collect(),
                    full_search_candidates: resolver
                        .full_search_candidates(self.context)
                        .iter()
                        .map(|candidate| CachedCandidate::new(db, candidate))
                        .collect(),
                })
            }
            Position::Prefix(PrefixResolver::Runtime(resolver)) => {
                Some(ModuleSearchSnapshot::Runtime(
                    resolver
                        .candidates
                        .iter()
                        .map(|candidate| CachedCandidate::new(db, candidate))
                        .collect(),
                ))
            }
            Position::Root(_) => None,
        }
    }

    /// Restores a search using its original resolver context and module name.
    fn from_snapshot(
        context: &'a ResolverContext<'db>,
        name: &ModuleName,
        snapshot: &ModuleSearchSnapshot,
    ) -> Self {
        let restore = |candidates: &[CachedCandidate]| {
            candidates
                .iter()
                .map(|candidate| candidate.restore(context))
                .collect()
        };

        let resolver = match snapshot {
            ModuleSearchSnapshot::Typing {
                stub_override_candidates,
                full_search_candidates,
            } => PrefixResolver::Typing(TypingModeResolver {
                prefix: name.clone(),
                root_candidates_from_extra_paths: None,
                stub_override_candidates: restore(stub_override_candidates),
                full_search_candidates: OnceCell::from(restore(full_search_candidates)),
            }),
            ModuleSearchSnapshot::Runtime(candidates) => {
                PrefixResolver::Runtime(RuntimeModeResolver {
                    prefix: name.clone(),
                    candidates: restore(candidates),
                })
            }
        };
        Self {
            context,
            position: Position::Prefix(resolver),
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

    /// Lists the immediate modules at the cursor's current position using
    /// the same precedence rules as ordinary module resolution.
    ///
    /// Search roots may be symlinks. Below each root, name discovery skips directories whose
    /// relative path crosses a directory symlink, so recursive enumeration cannot follow cycles.
    /// Symlink entries can still supply names, which are resolved normally. File symlinks and
    /// symlinked package initializers are allowed because they do not introduce directory traversal.
    ///
    /// For example, with `pkg/loop -> .`, listing `pkg` includes `pkg.loop`, but listing
    /// `pkg.loop` does not discover children through that directory. A namespace package can
    /// still supply children through other portions whose paths do not cross directory symlinks.
    pub(crate) fn list_modules(&self) -> ModuleListing<'db> {
        let context = self.context;
        let db = context.db;
        let prefix = self.prefix();
        let mut names = BTreeSet::new();

        for directory in self.directories_to_enumerate() {
            for entry in directory.entries(db) {
                if let Some(name) = entry.file_name()
                    && let Some(name) = child_module_name(name, entry.file_type(), prefix.is_none())
                {
                    names.insert(CompactString::new(name));
                }
            }
        }

        let mut modules = Vec::new();
        let mut unresolved_names = Vec::new();
        let mut modules_with_possible_children = Vec::new();

        for component_name in names {
            let Some(name) = self.full_module_name(&component_name) else {
                continue;
            };

            if let Some(candidates) = self.resolve_child(&component_name) {
                if let Some(candidate) = candidates.into_iter().next() {
                    let module =
                        candidate.into_module(db, context.resolver_environment, Cow::Owned(name));
                    modules.push(module);
                    modules_with_possible_children.push(module);
                }

                // A resolved module takes precedence over unresolved stub override names.
                continue;
            }

            // The full search found no module, so any remaining prefix candidates belong
            // to the stub override search: `acme.nested` may lead to `acme/nested/tools.pyi`
            // even when installed stubs omit `acme.nested`.
            if let Some(child_search) = self.advance(&component_name)
                && child_search.directories_to_enumerate().next().is_some()
            {
                unresolved_names.push(name);
            }
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

    /// Returns the directories to scan for child module names at the cursor's current position.
    ///
    /// For a search positioned at the root, this returns all search paths. Beneath a prefix,
    /// this excludes file modules and directories reached through directory symlinks below
    /// their search roots.
    fn directories_to_enumerate(&self) -> impl Iterator<Item = Cow<'_, ModuleDirectory<'db>>> {
        match &self.position {
            Position::Root(paths) => Either::Left(paths.iter(self.context).map(|path| {
                Cow::Owned(ModuleDirectory::new(
                    self.context,
                    path.to_module_path(),
                    None,
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

    fn with_paths(context: &'a ResolverContext<'db>, search_paths: RootSearchPaths<'db>) -> Self {
        Self {
            context,
            position: Position::Root(search_paths),
        }
    }
}

/// Resolved modules and unresolved module names for stub overrides.
///
/// We store both resolved modules and unresolved names because
/// installed stubs can omit intermediate packages. For instance, they can omit
/// a name like `acme.nested` while a stub override supplies `acme.nested.tools`.
/// Hence recursive enumeration must search `acme.nested` even while import
/// statement completion omits it.
#[derive(Default)]
#[expect(
    dead_code,
    reason = "Module listing is consumed by the next change's cached queries"
)]
pub(crate) struct ModuleListing<'db> {
    /// Modules resolved from discovered names, including symlink aliases.
    pub(crate) modules: Box<[Module<'db>]>,
    /// Unresolved module names that are nonetheless eligible for enumeration
    /// because they have eligible stub override candidates.
    pub(crate) unresolved_names: Box<[ModuleName]>,
    /// Listed modules that may have enumerable descendants, including files with stub overrides.
    pub(crate) modules_with_possible_children: Box<[Module<'db>]>,
}

/// Returns `Some(name)` for a directory entry that supplies a candidate child
/// module name, or `None` for an entry excluded from enumeration.
///
/// Accepts directories, `.py`/`.pyi` files, and symlinks to either. After stripping file extensions
/// and top-level `-stubs` suffixes, the name must be a Python identifier other than a
/// keyword. Below search roots, `__init__.py` and `__init__.pyi` are excluded because
/// they define the parent package.
///
/// `Some(name)` does not guarantee that the name resolves to an importable module.
fn child_module_name(entry: &str, file_type: FileType, at_search_root: bool) -> Option<&str> {
    if !at_search_root && matches!(entry, "__init__.py" | "__init__.pyi") {
        return None;
    }

    let python_stem = || {
        entry
            .strip_suffix(".py")
            .or_else(|| entry.strip_suffix(".pyi"))
    };
    let name = match file_type {
        FileType::Directory => entry,
        FileType::Symlink => python_stem().unwrap_or(entry),
        FileType::File => python_stem()?,
    };
    let name = if at_search_root {
        name.strip_suffix("-stubs").unwrap_or(name)
    } else {
        name
    };
    is_identifier(name).then_some(name)
}

/// Caches the package locations and resolution metadata used when searching beneath this name.
///
/// For example, enumerating `acme.tools` and `acme.reports` both requires finding the portions
/// of `acme` across search paths and applying package and stub precedence. Both searches reuse
/// the cached candidates for `acme` before advancing through their own final component.
/// Callers then enumerate children by reading the directory contents at the candidate locations.
/// The snapshot includes enumeration eligibility so unrelated ancestor entries do not invalidate
/// child listings merely because enumeration checks for directory symlinks.
#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn module_search_snapshot<'db>(
    db: &'db dyn Db,
    name: ModuleNameIngredient<'db>,
) -> Option<ModuleSearchSnapshot> {
    let context = ResolverContext::new(db, name.resolver_environment(db), name.mode(db));
    let module_name = name.name(db);

    let search = match module_name.parent() {
        Some(parent_name) => ModuleSearchCursor::for_module_name(
            &context,
            &parent_name,
            &RootSearchPaths::Configured,
        )?,
        None => ModuleSearchCursor::with_configured_search_paths(&context),
    };
    let search = search.advance(module_name.last_component())?;

    search.to_snapshot()
}

/// Owned search state at a module name, without borrowed directory listings.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
enum ModuleSearchSnapshot {
    Typing {
        stub_override_candidates: Box<[CachedCandidate]>,
        full_search_candidates: Box<[CachedCandidate]>,
    },
    Runtime(Box<[CachedCandidate]>),
}

/// An owned candidate description without a borrowed directory listing.
///
/// Salsa cannot retain the database-lifetime reference in `ModuleDirectory` across revisions.
/// Restoring the directory reads its current listing; changes to unrelated entries can leave
/// this cached description unchanged.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct CachedCandidate {
    path: ModulePath,
    enumeration_allowed: bool,
    module: ResolvedModule,
    py_typed: PyTyped,
    precedence: CandidatePrecedence,
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
        let name = ModuleName::new(component_name)?;

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
        let name = ModuleName::new(component_name)?;
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

pub(super) enum RootSearchPaths<'db> {
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
            ModuleDirectory::new(context, search_path.to_module_path(), None)
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
    use std::borrow::Cow;

    use insta::assert_debug_snapshot;

    use ruff_db::system::{DbWithWritableSystem, SystemPath};

    use crate::ModuleName;
    use crate::db::tests::TestDb;
    use crate::resolve::{ModuleResolveMode, ResolverContext};
    #[cfg(target_family = "unix")]
    use crate::testing::os_enumeration_db;
    use crate::testing::{ModuleDebugSnapshot, TestCaseBuilder};

    use super::{
        ModuleListing, ModuleSearchCursor, list_root_modules, list_submodules,
        list_submodules_by_name,
    };

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
    fn preserves_file_precedence_when_listing_roots() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                // Within one root, the stub wins over the source file.
                ("leaf.py", ""),
                ("leaf.pyi", ""),
                ("shared.py", ""),
            ])
            .with_site_packages_files(&[("shared.pyi", "")])
            .with_extra_path(
                "/extra",
                &[
                    // The extra-path stub override wins over both other locations.
                    ("shared.pyi", ""),
                ],
            )
            .build()
            .db;
        assert_debug_snapshot!(ListingCase::root().snapshot(&db), @r#"
        [
            Module::File("leaf", "first-party", "/src/leaf.pyi", Module, None),
            Module::File("shared", "extra", "/extra/shared.pyi", Module, None),
        ]
        "#);
    }

    #[test]
    fn excludes_local_files_with_protected_standard_library_names() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("leaf.py", ""), ("sys.py", "")])
            .build()
            .db;
        // A local file cannot supply a protected name, even when typeshed omits it.
        ListingCase::root().expect_module("leaf").assert(&db);
    }

    #[test]
    fn enumerates_namespace_portions_across_search_paths() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/left.py", "")])
            .with_site_packages_files(&[("acme/right.py", "")])
            .build()
            .db;
        ListingCase::root().expect_module("acme").assert(&db);
        ListingCase::for_name("acme")
            .expect_modules(&["acme.left", "acme.right"])
            .assert(&db);
    }

    #[test]
    fn enumerates_nested_namespaces() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/nested/deep.py", "")])
            .build()
            .db;
        ListingCase::for_name("acme")
            .expect_module("acme.nested")
            .assert(&db);
        ListingCase::for_name("acme.nested")
            .expect_module("acme.nested.deep")
            .assert(&db);
    }

    #[test]
    fn enumerates_namespaces_inside_regular_packages() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/regular/__init__.py", ""),
                ("acme/regular/namespace/child.py", ""),
            ])
            .build()
            .db;
        ListingCase::for_name("acme")
            .expect_module("acme.regular")
            .assert(&db);
        ListingCase::for_name("acme.regular")
            .expect_module("acme.regular.namespace")
            .assert(&db);
        ListingCase::for_name("acme.regular.namespace")
            .expect_module("acme.regular.namespace.child")
            .assert(&db);
    }

    #[test]
    fn updates_listing_when_a_package_becomes_a_legacy_namespace() {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[("acme/__init__.py", ""), ("acme/local.py", "")])
            .with_site_packages_files(&[("acme/other.py", "")])
            .build()
            .db;
        ListingCase::for_name("acme")
            .expect_module("acme.local")
            .assert(&db);

        db.write_file(
            "/src/acme/__init__.py",
            "__path__ = __import__('pkgutil').extend_path(__path__, __name__)",
        )
        .expect("extend the package across search roots");
        ListingCase::for_name("acme")
            .expect_modules(&["acme.local", "acme.other"])
            .assert(&db);

        db.write_file("/src/acme/__init__.py", "")
            .expect("restore the regular package");
        ListingCase::for_name("acme")
            .expect_module("acme.local")
            .assert(&db);
    }

    #[test]
    fn uses_resolution_precedence_for_namespace_children() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                // A regular package wins over a module with the same name.
                ("acme/package/__init__.py", ""),
                ("acme/package.pyi", ""),
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
        assert_debug_snapshot!(ListingCase::root().snapshot(&db), @r#"
        [
            Module::Namespace(ModuleName("acme")),
        ]
        "#);
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
    fn excludes_namespace_portions_shadowed_by_regular_packages() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/hidden.py", "")])
            .with_site_packages_files(&[("acme/__init__.py", "")])
            .build()
            .db;
        ListingCase::root().expect_module("acme").assert(&db);
        ListingCase::for_name("acme").assert(&db);
    }

    #[test]
    fn excludes_namespace_portions_shadowed_by_file_modules() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[("acme/hidden.py", "")])
            .with_site_packages_files(&[("acme.py", "")])
            .build()
            .db;
        ListingCase::root().expect_module("acme").assert(&db);
        ListingCase::for_name("acme").assert(&db);
    }

    #[test]
    fn merges_legacy_namespace_portions_until_shadowed() -> anyhow::Result<()> {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[("acme/left.py", "")])
            .with_site_packages_files(&[("acme/right.py", "")])
            .build()
            .db;
        db.write_files([
            (
                "/src/acme/__init__.py",
                "__path__ = __import__(\"pkgutil\").extend_path(__path__, __name__)",
            ),
            (
                "/site-packages/acme/__init__.py",
                "__path__ = __import__(\"pkgutil\").extend_path(__path__, __name__)",
            ),
        ])
        .expect("write legacy declarations");
        ListingCase::for_name("acme")
            .expect_modules(&["acme.left", "acme.right"])
            .assert(&db);

        db.write_file("/site-packages/acme/__init__.py", "")?;
        ListingCase::for_name("acme")
            .expect_module("acme.right")
            .assert(&db);

        Ok(())
    }

    #[test]
    fn combines_partial_stub_namespaces_with_source_packages() {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/__init__.py", ""),
                ("acme/api/__init__.py", ""),
                ("acme/api/runtime.py", ""),
            ])
            .with_site_packages_files(&[("acme-stubs/api/stubbed.pyi", "")])
            .build()
            .db;
        db.write_file("/site-packages/acme-stubs/py.typed", "partial\n")
            .expect("preserve partial stub namespace alongside ordinary packages");
        assert_debug_snapshot!(ListingCase::for_name("acme").snapshot(&db), @r#"
        [
            Module::File("acme.api", "first-party", "/src/acme/api/__init__.py", Package, None),
        ]
        "#);
        assert_debug_snapshot!(ListingCase::for_name("acme.api").snapshot(&db), @r#"
        [
            Module::File("acme.api.runtime", "first-party", "/src/acme/api/runtime.py", Module, None),
            Module::File("acme.api.stubbed", "site-packages", "/site-packages/acme-stubs/api/stubbed.pyi", Module, None),
        ]
        "#);
    }

    #[test]
    fn includes_source_children_of_partial_stub_subpackages() {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[
                // Complete parent stubs hide `acme.hidden`.
                ("acme/__init__.py", ""),
                ("acme/hidden.py", ""),
                // Partial child stubs allow `runtime` alongside `stubbed` within `acme.api`.
                ("acme/api/__init__.py", ""),
                ("acme/api/runtime.py", ""),
            ])
            .with_site_packages_files(&[
                ("acme-stubs/__init__.pyi", ""),
                ("acme-stubs/api/__init__.pyi", ""),
                ("acme-stubs/api/stubbed.pyi", ""),
            ])
            .build()
            .db;
        db.write_file("/site-packages/acme-stubs/api/py.typed", "partial\n")
            .expect("mark child as partial");
        ListingCase::for_name("acme")
            .expect_module("acme.api")
            .assert(&db);
        ListingCase::for_name("acme.api")
            .expect_modules(&["acme.api.runtime", "acme.api.stubbed"])
            .assert(&db);
    }

    #[test]
    fn enumerates_partial_stub_descendants_of_source_modules() {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[("acme.py", "")])
            .with_site_packages_files(&[("acme-stubs/child.pyi", "")])
            .build()
            .db;
        db.write_file("/site-packages/acme-stubs/py.typed", "partial\n")
            .expect("mark the stub namespace as partial");
        ListingCase::for_name("acme")
            .expect_module("acme.child")
            .assert(&db);
    }

    #[test]
    fn combines_stub_overrides_with_source_siblings() {
        let db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/__init__.py", ""),
                ("acme/stubbed.py", ""),
                ("acme/runtime.py", ""),
            ])
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
    fn separates_unresolved_names_from_modules() {
        let db = TestCaseBuilder::new()
            .with_site_packages_files(&[
                // Complete installed stubs omit `acme.nested` and block fallback to source packages.
                ("acme-stubs/__init__.pyi", ""),
                ("acme-stubs/py.typed", ""),
                ("acme/__init__.py", ""),
                ("acme/nested/__init__.py", ""),
                ("acme/nested/deep/__init__.py", ""),
                ("acme/nested/deep/tools.py", ""),
            ])
            .with_extra_path(
                "/extra",
                &[
                    // A local stub override remains discoverable through those unresolved parents.
                    ("acme/nested/deep/tools.pyi", ""),
                    // A source file on an extra path must not count as a stub override.
                    ("acme/nested/source_only.py", ""),
                ],
            )
            .build()
            .db;
        ListingCase::root().expect_module("acme").assert(&db);
        ListingCase::for_name("acme")
            .expect_unresolved_name("acme.nested")
            .assert(&db);
        ListingCase::for_name("acme.nested")
            .expect_unresolved_name("acme.nested.deep")
            .assert(&db);
        ListingCase::for_name("acme.nested.deep")
            .expect_module("acme.nested.deep.tools")
            .assert(&db);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn enumerates_aliases_without_traversing_directory_symlinks() -> anyhow::Result<()> {
        let (_temp, mut db, root) = os_enumeration_db(&[])?;
        for path in [
            "src/acme/own.py",
            "src/acme/stubbed.py",
            "site-packages/acme/hidden.py",
            "site-packages/acme/ns/masked.py",
            "site-packages/acme/ns/visible.py",
            "site-packages/acme/blocked/visible.py",
            "other_ns/masked.py",
            "other_ns/source_only.py",
            "src/regular/__init__.py",
            "src/regular/child.py",
            "src/regular/nested/child.py",
            "target.py",
            "target.pyi",
        ] {
            db.write_file(root.join(path), "")?;
        }
        for (source, link) in [
            ("target.py", "src/top_alias.py"),
            // These aliases shadow ordinary entries from lower-priority candidates.
            ("target.py", "src/acme/hidden.py"),
            ("target.pyi", "src/acme/stubbed.pyi"),
            ("other_ns", "src/acme/ns"),
            ("src/regular", "src/acme/blocked"),
            ("src/acme", "src/alias"),
            ("src/regular", "src/regular_alias"),
            ("target.py", "src/regular/linked.py"),
            ("other_ns", "src/regular/linked_dir"),
            ("src/regular", "src/regular/loop"),
            ("src/regular/__init__.py", "src/regular/nested/__init__.py"),
        ] {
            std::os::unix::fs::symlink(
                root.join(source).as_std_path(),
                root.join(link).as_std_path(),
            )
            .expect("create fixture symlink");
        }

        ListingCase::root()
            .expect_modules(&["acme", "alias", "regular", "regular_alias", "top_alias"])
            .assert(&db);
        ListingCase::for_name("acme")
            .expect_modules(&[
                "acme.blocked",
                "acme.hidden",
                "acme.ns",
                "acme.own",
                "acme.stubbed",
            ])
            .assert(&db);
        // The ordinary namespace portion supplies names, but resolution can select an alias.
        ListingCase::for_name("acme.ns")
            .expect_modules(&["acme.ns.masked", "acme.ns.visible"])
            .assert(&db);
        ListingCase::for_name("regular")
            .expect_modules(&[
                "regular.child",
                "regular.linked",
                "regular.linked_dir",
                "regular.loop",
                "regular.nested",
            ])
            .assert(&db);
        ListingCase::for_name("regular.nested")
            .expect_module("regular.nested.child")
            .assert(&db);
        for name in [
            "alias",
            "acme.blocked",
            "acme.blocked.nested",
            "regular_alias",
            "regular_alias.nested",
            "regular.loop",
        ] {
            crate::resolve_module_confident(
                &db,
                db.resolver_environment(),
                &ModuleName::new(name).expect("valid module name"),
            )
            .expect("package resolves");
            ListingCase::for_name(name).assert(&db);
        }

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn selects_directory_aliases_over_shadowed_files() -> anyhow::Result<()> {
        let (_temp, mut db, root) = os_enumeration_db(&[])?;
        db.write_file(root.join("src/acme/blocked.py"), "")?;
        db.write_file(root.join("target/__init__.py"), "")?;
        std::os::unix::fs::symlink(root.join("target"), root.join("src/acme/blocked"))
            .expect("create a package symlink that shadows the file");

        // ListingCase also checks that the selected module agrees with ordinary resolution.
        ListingCase::for_name("acme")
            .expect_module("acme.blocked")
            .assert(&db);
        db.write_file(root.join("src/acme/__init__.py"), "")?;
        ListingCase::for_name("acme")
            .expect_module("acme.blocked")
            .assert(&db);

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn enumerates_modules_beneath_a_symlinked_search_root() -> anyhow::Result<()> {
        let (_temp, mut db, root) = os_enumeration_db(&[])?;
        std::fs::rename(root.join("src"), root.join("source"))?;
        std::os::unix::fs::symlink(root.join("source"), root.join("src"))?;
        db.write_file(root.join("source/pkg/__init__.py"), "")?;
        db.write_file(root.join("source/pkg/child.py"), "")?;

        ListingCase::root().expect_module("pkg").assert(&db);
        ListingCase::for_name("pkg")
            .expect_module("pkg.child")
            .assert(&db);

        Ok(())
    }

    #[test]
    fn excludes_stub_packages_and_files_in_runtime_mode() {
        let db = TestCaseBuilder::new()
            .with_site_packages_files(&[
                ("acme/__init__.py", ""),
                ("acme/child.py", ""),
                ("acme/child.pyi", ""),
                ("acme/stub_only.pyi", ""),
                ("acme-stubs/__init__.pyi", ""),
                ("acme-stubs/child.pyi", ""),
            ])
            .build()
            .db;
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Runtime);
        let name = ModuleName::new_static("acme").expect("valid name");
        let module = crate::resolve_real_module_confident(&db, db.resolver_environment(), &name)
            .expect("runtime package");
        let listing = list_submodules(&context, module);
        assert!(listing.unresolved_names.is_empty());
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
        let module = candidate.into_module(db, db.resolver_environment(), Cow::Owned(name));
        let file = module.file(db).expect("child has a defining file");
        assert_eq!(
            file.path(db).as_system_path(),
            Some(SystemPath::new(expected))
        );
    }

    /// An enumeration target and the modules and unresolved names expected beneath it.
    struct ListingCase<'a> {
        parent_module_name: Option<&'a str>,
        expected_module_names: Vec<&'a str>,
        expected_unresolved_names: Vec<&'a str>,
    }

    impl<'a> ListingCase<'a> {
        fn root() -> Self {
            Self {
                parent_module_name: None,
                expected_module_names: Vec::new(),
                expected_unresolved_names: Vec::new(),
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

        fn expect_unresolved_name(mut self, name: &'a str) -> Self {
            self.expected_unresolved_names.push(name);
            self
        }

        /// Formats listed modules after checking that they agree with ordinary resolution.
        #[track_caller]
        fn snapshot<'db>(&self, db: &'db TestDb) -> Vec<ModuleDebugSnapshot<'db>> {
            let listing = self.list_modules(db);
            assert_eq!(listing.unresolved_names, Box::<[ModuleName]>::default());
            listing
                .modules
                .into_iter()
                .map(|module| ModuleDebugSnapshot::new(db, module))
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

            let unresolved_names: Vec<_> = listing
                .unresolved_names
                .iter()
                .map(ModuleName::as_str)
                .collect();
            assert_eq!(unresolved_names, self.expected_unresolved_names);
        }

        /// Lists modules and checks that enumeration agrees with ordinary resolution.
        #[track_caller]
        fn list_modules<'db>(&self, db: &'db TestDb) -> ModuleListing<'db> {
            let context =
                ResolverContext::new(db, db.resolver_environment(), ModuleResolveMode::Typing);
            let listing = match self.parent_module_name {
                None => list_root_modules(&context),
                Some(name) => {
                    let name = ModuleName::new(name).expect("valid module name");
                    match crate::resolve_module_confident(db, db.resolver_environment(), &name) {
                        Some(module) => list_submodules(&context, module),
                        None => list_submodules_by_name(&context, &name),
                    }
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
                    "unresolved name must not invent a resolved module"
                );
            }
            listing
        }
    }
}
