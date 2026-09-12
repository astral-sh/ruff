//! Reusable prefix search and module enumeration.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use camino::{Utf8Path, Utf8PathBuf};
use ruff_db::files::{directory_listing, system_path_to_directory};
use ruff_db::system::{FileType, SystemPath};

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, SearchPath};

use super::{
    ComponentFileFilter, ModuleNameIngredient, ModuleResolutionCandidate, NameResolver,
    ResolvedModule, ResolvedNames, ResolverContext, StubPackageIndex, StubPackagePaths,
    normalize_candidates, search_paths, stub_package_index,
};

/// A search position retaining the candidates that can supply children of a module-name prefix.
///
/// Entering a package preserves its descendant search; resolving a child selects candidates
/// for its final module. For example, resolving a legacy namespace selects one initializer,
/// while its children can come from several directories. A selected `Module` cannot replace
/// this search state.
///
/// Both operations borrow the parent, so looking up one child leaves its siblings searchable.
/// Candidate state lasts only for this search; Salsa caches the selected modules separately.
pub(super) struct ModuleSearch<'resolver, 'db> {
    resolver: &'resolver NameResolver<'db>,
    cursor: SearchCursor<'db>,
}

impl<'resolver, 'db> ModuleSearch<'resolver, 'db> {
    /// Starts before the first module-name component, without probing search paths.
    pub(super) fn new(resolver: &'resolver NameResolver<'db>) -> Self {
        Self {
            resolver,
            cursor: SearchCursor::Root,
        }
    }

    /// Builds a reusable search for a prefix, or for top-level names when absent.
    pub(super) fn for_prefix(
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

    /// Builds a prefix search within one importing-file fallback path.
    pub(super) fn in_search_path(
        resolver: &'resolver NameResolver<'db>,
        prefix: &ModuleName,
        path: &SearchPath,
    ) -> Self {
        Self {
            resolver,
            cursor: SearchCursor::Prefix {
                name: prefix.clone(),
                candidates: SearchCandidates::in_search_path(resolver, prefix, path),
            },
        }
    }

    /// Enters one component without selecting a final module or consuming the parent cursor.
    ///
    /// Retains partial stub-package namespaces that may provide descendants even when a concrete
    /// package or module would shadow them if this component were the final import target.
    pub(super) fn enter_package(&self, component: &str) -> Option<Self> {
        let name = self.child_name(component)?;

        let candidates = match &self.cursor {
            SearchCursor::Root => SearchCandidates::from_roots(self.resolver, component),
            SearchCursor::Prefix { candidates, .. } => {
                candidates.enter_package(self.resolver, component)
            }
        };
        if candidates.is_empty(self.resolver, &name) {
            return None;
        }

        Some(Self {
            resolver: self.resolver,
            cursor: SearchCursor::Prefix { name, candidates },
        })
    }

    /// Selects candidates for one immediate child using endpoint precedence.
    /// Leaves the parent reusable for another child.
    ///
    /// Typing resolution first tries stub overlays, then the full search including stub packages.
    /// Runtime resolution searches for runtime files only.
    pub(super) fn resolve_child(&self, component: &str) -> Option<ResolvedNames<'db>> {
        let name = self.child_name(component)?;
        let candidates = self.resolve_child_candidates(&name);
        (!candidates.is_empty()).then_some(candidates)
    }

    /// Collects resolved immediate children and unresolved overlay prefixes to explore.
    pub(super) fn enumerate_modules(&self) -> ModuleEnumeration<'db> {
        let db = self.resolver.context.db;
        let mut children = ModuleEnumeration::default();
        for component in self.child_names() {
            db.unwind_if_revision_cancelled();
            match self.enumerate_child(&component) {
                Some(EnumerationEntry::Module(module)) => children.modules.push(module),
                Some(EnumerationEntry::OverlayPrefix(prefix)) => {
                    children.overlay_prefixes.push(prefix);
                }
                None => {}
            }
        }
        children
    }

    fn child_names(&self) -> BTreeSet<String> {
        match &self.cursor {
            SearchCursor::Root => self.top_level_names(),
            SearchCursor::Prefix { name, candidates } => self.package_child_names(name, candidates),
        }
    }

    fn enumerate_child(&self, component: &str) -> Option<EnumerationEntry<'db>> {
        let name = self.child_name(component)?;
        let candidates = self.resolve_child_candidates(&name);
        if !candidates.is_empty() {
            // A resolved but excluded module still shadows other locations. Its exclusion
            // must not be treated as a failed resolution that permits an overlay prefix.
            return self
                .selected_module_if_eligible(&name, candidates)
                .map(EnumerationEntry::Module);
        }
        self.unresolved_overlay_prefix(component)
            .map(EnumerationEntry::OverlayPrefix)
    }

    fn top_level_names(&self) -> BTreeSet<String> {
        let context = &self.resolver.context;
        let mut names = BTreeSet::new();
        for path in search_paths(context.db, context.resolver_environment, context.mode) {
            self.collect_child_names(
                &ModuleResolutionCandidate::root(context, path),
                EntryLocation::SearchRoot,
                &mut names,
            );
        }
        names
    }

    fn package_child_names(
        &self,
        name: &ModuleName,
        candidates: &SearchCandidates<'db>,
    ) -> BTreeSet<String> {
        let candidates = match candidates {
            SearchCandidates::Typing(typing) => typing.full_search_candidates(self.resolver, name),
            SearchCandidates::Runtime(candidates) => candidates.as_slice(),
        };
        let mut names = BTreeSet::new();
        for candidate in self.overlay_candidates().iter().chain(candidates) {
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
        let context = &self.resolver.context;
        candidates
            .into_iter()
            .next()
            .map(|candidate| candidate.into_module(context.db, context.resolver_environment, name))
    }

    fn unresolved_overlay_prefix(&self, component: &str) -> Option<ModuleName> {
        if self.overlay_candidates().is_empty() {
            return None;
        }
        let search = self.enter_package(component)?;
        let eligible = search.overlay_candidates().iter().any(|candidate| {
            !matches!(candidate.module, ResolvedModule::Module(_))
                && self.candidate_is_eligible(candidate)
        });
        if !eligible {
            return None;
        }
        // A local stub can patch `acme.nested.tools` even when installed stubs omit
        // `acme.nested`. Keep that prefix for traversal without inventing a module.
        self.child_name(component)
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
        let context = &self.resolver.context;
        if let Some(listing) = candidate.directory.system_listing() {
            for (name, file_type) in listing.iter() {
                context.db.unwind_if_revision_cancelled();
                add_child_name(names, name, file_type, location);
            }
        } else if let Some(path) = candidate.directory.to_vendored_path() {
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
        let db = self.resolver.context.db;
        let Some(root) = candidate.directory.search_path().as_system_path() else {
            return true;
        };
        let path = match candidate.module {
            ResolvedModule::Module(file) => {
                file.path(db).as_system_path().map(SystemPath::to_path_buf)
            }
            _ => candidate.directory.to_system_path(),
        };
        let Some(path) = path else { return false };
        // Exempt the known package's ancestry only for locations beneath its directory.
        // Other namespace portions and stub overlays keep their full ancestry checks.
        let known_package = self
            .resolver
            .known_package
            .filter(|module| module.kind(db) == ModuleKind::Package)
            .and_then(|module| module.file(db))
            .and_then(|file| file.path(db).as_system_path())
            .and_then(SystemPath::parent)
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

    /// Overlay locations that can supply descendants even when this prefix does not resolve.
    fn overlay_candidates(&self) -> &[ModuleResolutionCandidate<'db>] {
        match &self.cursor {
            SearchCursor::Prefix {
                candidates: SearchCandidates::Typing(typing),
                ..
            } => &typing.overlay_candidates,
            _ => &[],
        }
    }

    /// Selects candidates for one immediate child without converting them to a final module.
    fn resolve_child_candidates(&self, name: &ModuleName) -> ResolvedNames<'db> {
        let context = &self.resolver.context;
        if let SearchCursor::Prefix {
            name: prefix,
            candidates,
        } = &self.cursor
        {
            return candidates.resolve_child(self.resolver, prefix, name.last_component());
        }

        // A top-level name needs no separate overlay pass: there is no parent to be shadowed.
        let stubs = if context.mode.is_typing() {
            stub_package_index(context.db, context.resolver_environment).all()
        } else {
            StubPackagePaths::default()
        };

        let roots = self.resolver.discover_roots(
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
            SearchCursor::Prefix { name, .. } => {
                let mut name = name.clone();
                name.extend(&child);
                Some(name)
            }
        }
    }

    /// Checks for possible descendant directories without resolving the prefix's ancestors.
    /// Finding a directory is only a reason to search; it may still be shadowed.
    pub(super) fn may_have_children(context: &ResolverContext, name: &ModuleName) -> bool {
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
        let relative_path: Utf8PathBuf = name.components().collect();
        roots_containing_top_level_directory(context.db, top_level)
            .iter()
            .any(|root| {
                // Reuse the containing directory's listing instead of creating an input
                // and probing a missing path for every leaf. Overlays may supply this directory.
                ModuleDirectory::exists_at(context, root, &relative_path)
            })
    }
}

/// Resolved immediate children, plus prefixes needed only for stub-overlay enumeration.
///
/// An installed stub package can hide `acme.nested` while a local override still resolves
/// `acme.nested.tools`. Such prefixes are traversal positions, not resolved modules: recursive
/// enumeration must explore them, but import-statement completion must not offer them.
#[derive(Default)]
pub(crate) struct ModuleEnumeration<'db> {
    /// Modules that resolve independently and are eligible for enumeration.
    pub(crate) modules: Vec<Module<'db>>,
    /// Unresolved names with eligible stub-overlay locations to search for descendants.
    pub(crate) overlay_prefixes: Vec<ModuleName>,
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
    // Track the directory's status, not its containing root's listing: unrelated root
    // entries must not invalidate this result. Vendored paths retain Python-version checks.
    if let Some(path) = root.as_system_path() {
        system_path_to_directory(context.db, path.join(component)).is_ok()
    } else {
        ModuleDirectory::exists_at(context, root, Utf8Path::new(component))
    }
}

enum EnumerationEntry<'db> {
    Module(Module<'db>),
    OverlayPrefix(ModuleName),
}

#[derive(Clone, Copy)]
enum EntryLocation {
    SearchRoot,
    Package,
}

/// The root search or the candidates retained beneath a module-name prefix.
enum SearchCursor<'db> {
    Root,
    Prefix {
        name: ModuleName,
        candidates: SearchCandidates<'db>,
    },
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
    fn from_roots(resolver: &NameResolver<'db>, component: &str) -> Self {
        let context = &resolver.context;
        if context.mode.is_typing() {
            return Self::Typing(TypingCandidates::from_roots(resolver, component));
        }

        // Runtime mode selects the real standard library instead of typeshed.
        let paths = search_paths(context.db, context.resolver_environment, context.mode);
        let roots = resolver.discover_roots(
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
        resolver: &NameResolver<'db>,
        prefix: &ModuleName,
        path: &SearchPath,
    ) -> Self {
        let context = &resolver.context;
        let stubs = StubPackageIndex::from_search_paths(context.db, std::iter::once(path));
        let mut candidates = normalize_candidates(
            context.db,
            resolver.discover_roots(
                prefix.first_component(),
                false,
                std::iter::once(path),
                if context.mode.is_typing() {
                    stubs.all()
                } else {
                    StubPackagePaths::default()
                },
            ),
            true,
        );
        for component in prefix.components().skip(1) {
            candidates = resolver.advance_candidates(
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

    fn enter_package(&self, resolver: &NameResolver<'db>, component: &str) -> Self {
        match self {
            Self::Typing(typing) => Self::Typing(typing.enter_package(resolver, component)),
            Self::Runtime(candidates) => Self::Runtime(resolver.advance_candidates(
                candidates.clone(),
                component,
                ComponentFileFilter::ByMode,
                true,
            )),
        }
    }

    fn resolve_child(
        &self,
        resolver: &NameResolver<'db>,
        prefix: &ModuleName,
        component: &str,
    ) -> ResolvedNames<'db> {
        match self {
            Self::Typing(typing) => typing.resolve_child(resolver, prefix, component),
            Self::Runtime(candidates) => resolver.advance_candidates(
                candidates.clone(),
                component,
                ComponentFileFilter::ByMode,
                false,
            ),
        }
    }

    fn is_empty(&self, resolver: &NameResolver<'db>, prefix: &ModuleName) -> bool {
        match self {
            Self::Typing(typing) => {
                // An overlay may supply descendants without probing the rest of the search paths.
                typing.overlay_candidates.is_empty()
                    && typing.full_search_candidates(resolver, prefix).is_empty()
            }
            Self::Runtime(candidates) => candidates.is_empty(),
        }
    }
}

/// Typing resolution retains overlay candidates and a lazy full search for the same prefix.
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
/// Typing resolution first tries extra-path stub overlays when resolving a submodule.
///   Here, `acme.patched` resolves to `extra/acme/patched.pyi`. While following a dotted name,
///   the overlay search may traverse namespace packages or packages defined by `__init__.py`
///   for the preceding components (i.e., `acme`). It succeeds only if the requested module
///   (`acme.patched`) is defined by a `.pyi` file; finding only `patched.py` would not suffice.
///   If no overlay supplies the submodule, a full search considers stubs and runtime modules
///   under typing precedence: `acme.stubbed` resolves to `acme-stubs/stubbed.pyi`, while
///   `acme.source_only` resolves to `acme/source_only.py`[^1]. Top-level names (e.g., `acme`)
///   go directly through the full search.
///
/// [^1]: The `partial` marker allows the full search to use runtime modules missing from the
/// stub package. Without this marker, the stub package is treated as complete, so
/// `acme.source_only` would not resolve.
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
    fn from_roots(resolver: &NameResolver<'db>, component: &str) -> Self {
        let context = &resolver.context;
        let (stubs, _) =
            stub_package_index(context.db, context.resolver_environment).split_overlay();
        let roots = resolver.discover_roots(
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

    fn enter_package(&self, resolver: &NameResolver<'db>, component: &str) -> Self {
        let overlay_candidates = resolver.advance_candidates(
            self.overlay_candidates.clone(),
            component,
            ComponentFileFilter::ByMode,
            true,
        );
        // Advance an already computed full search, but do not force it while following an overlay.
        let full_search_candidates = self.full_search_candidates.get().map(|candidates| {
            resolver.advance_candidates(
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
        resolver: &NameResolver<'db>,
        prefix: &ModuleName,
        component: &str,
    ) -> ResolvedNames<'db> {
        // When resolving `acme.tools`, the overlay search may have entered `acme` through
        // `acme/__init__.py`. The lookup for the requested child `tools` must now find a stub:
        // `tools.pyi` or `tools/__init__.pyi`, not a runtime `.py` file.
        let overlay = resolver.advance_candidates(
            self.overlay_candidates.clone(),
            component,
            ComponentFileFilter::StubOnly,
            false,
        );
        if !overlay.is_empty() {
            return overlay;
        }

        resolver.advance_candidates(
            self.full_search_candidates(resolver, prefix).to_vec(),
            component,
            ComponentFileFilter::ByMode,
            false,
        )
    }

    /// Builds the complete search once for this prefix, retaining ordinary typing precedence.
    fn full_search_candidates(
        &self,
        resolver: &NameResolver<'db>,
        prefix: &ModuleName,
    ) -> &[ModuleResolutionCandidate<'db>] {
        let context = &resolver.context;
        self.full_search_candidates.get_or_init(|| {
            let (_, stubs) =
                stub_package_index(context.db, context.resolver_environment).split_overlay();
            let mut candidates = self
                .extra_path_roots
                .as_deref()
                .cloned()
                .unwrap_or_default();
            candidates.extend(
                resolver.discover_roots(
                    prefix.first_component(),
                    false,
                    search_paths(context.db, context.resolver_environment, context.mode)
                        .skip_while(|path| path.is_extra()),
                    stubs,
                ),
            );
            // Merge at the first component: a package there can shadow other roots before we
            // reach the current prefix. Appending candidates at the current depth would be wrong.
            candidates = normalize_candidates(context.db, candidates, true);
            for component in prefix.components().skip(1) {
                candidates = resolver.advance_candidates(
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

#[cfg(test)]
mod tests {
    use ruff_db::files::{Files, system_path_to_file};
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem, SystemPath};

    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    #[cfg(target_family = "unix")]
    use crate::testing::symlink_enumeration_db;
    use crate::testing::{
        MockedTypeshed, TestCase, TestCaseBuilder, enumeration_db, unresolved_overlay_db,
    };
    use crate::{ImportingFile, ModuleName};

    use super::{ModuleEnumeration, ModuleSearch, NameResolver};

    #[test]
    fn sibling_searches_reuse_split_namespace() {
        let db = enumeration_db(
            &["/src/acme/reports.py", "/site-packages/acme/tools.py"],
            &[],
        );
        for mode in [ModuleResolveMode::Typing, ModuleResolveMode::Runtime] {
            let resolver = NameResolver::new(&db, db.resolver_environment(), mode);
            let root = ModuleSearch::new(&resolver);
            let acme = root.enter_package("acme").expect("namespace exists");
            assert_resolves_to(&db, &acme, "reports", "/src/acme/reports.py");
            assert_resolves_to(&db, &acme, "tools", "/site-packages/acme/tools.py");
            assert!(acme.resolve_child("missing").is_none());
            assert_resolves_to(&db, &acme, "reports", "/src/acme/reports.py");
        }
    }

    #[test]
    fn selected_initializer_preserves_legacy_namespace_search() {
        let mut db = enumeration_db(
            &["/src/acme/reports.py", "/site-packages/acme/tools.py"],
            &[],
        );
        for initializer in ["/src/acme/__init__.py", "/site-packages/acme/__init__.py"] {
            db.write_file(
                initializer,
                r#"
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
"#,
            )
            .expect("write legacy namespace initializer");
        }
        let resolver = NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let root = ModuleSearch::new(&resolver);
        assert_resolves_to(&db, &root, "acme", "/src/acme/__init__.py");
        let acme = root.enter_package("acme").expect("legacy namespace exists");
        assert_resolves_to(&db, &acme, "reports", "/src/acme/reports.py");
        assert_resolves_to(&db, &acme, "tools", "/site-packages/acme/tools.py");
    }

    #[test]
    fn overlay_and_runtime_siblings_in_either_order() {
        let db = enumeration_db(
            &[
                "/extra/acme/patched.pyi",
                "/src/acme/__init__.py",
                "/src/acme/patched.py",
                "/src/acme/runtime.py",
            ],
            &["/extra"],
        );
        for children in [["patched", "runtime"], ["runtime", "patched"]] {
            let resolver =
                NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
            let acme = ModuleSearch::new(&resolver)
                .enter_package("acme")
                .expect("package has an overlay and runtime candidates");
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
        let children = resolver
            .with_known_package(package)
            .enumerate_modules_in_search_path(
                &name,
                package.search_path(&db).expect("fallback search path"),
            );
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
            let children = resolver
                .with_known_package(package)
                .enumerate_modules(Some(&name));
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

    #[test]
    fn runtime_enumeration_ignores_stub_packages_and_files() {
        let db = enumeration_db(
            &[
                "/site-packages/acme/__init__.py",
                "/site-packages/acme/child.py",
                "/site-packages/acme/child.pyi",
                "/site-packages/acme/stub_only.pyi",
                "/site-packages/acme-stubs/__init__.pyi",
                "/site-packages/acme-stubs/child.pyi",
            ],
            &[],
        );
        let resolver =
            NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Runtime);
        let name = ModuleName::new_static("acme").expect("valid name");
        let package = crate::resolve_real_module_confident(&db, db.resolver_environment(), &name)
            .expect("runtime package");
        let path = package.search_path(&db).expect("package search path");
        for children in [
            resolver.enumerate_modules(Some(&name)),
            resolver.enumerate_modules_in_search_path(&name, path),
        ] {
            assert!(children.overlay_prefixes.is_empty());
            assert_eq!(children.modules.len(), 1);
            let child = children.modules[0];
            assert_eq!(child.name(&db).as_str(), "acme.child");
            assert_eq!(
                child
                    .file(&db)
                    .expect("runtime file")
                    .path(&db)
                    .as_system_path(),
                Some(SystemPath::new("/site-packages/acme/child.py"))
            );
        }
    }

    fn enumerate<'db>(db: &'db TestDb, package: Option<&str>) -> ModuleEnumeration<'db> {
        let package = package.map(|name| ModuleName::new(name).expect("valid package name"));
        let resolver = NameResolver::new(db, db.resolver_environment(), ModuleResolveMode::Typing);
        let resolver = if let Some(name) = package.as_ref()
            && let Some(module) =
                crate::resolve_module_confident(db, db.resolver_environment(), name)
        {
            resolver.with_known_package(module)
        } else {
            resolver
        };
        resolver.enumerate_modules(package.as_ref())
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

    fn assert_resolves_to(db: &TestDb, search: &ModuleSearch, component: &str, expected: &str) {
        let name = search.child_name(component).expect("valid child name");
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
}
