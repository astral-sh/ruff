//! This module exposes a [`ModuleSearch`] abstraction, which encapsulates reusable
//! search state for namespace-aware module enumeration, and is equally usable for
//! ordinary, single module resolution.
//!
//! [`ModuleSearch`] provides an interface that describes traversal of the components
//! of a module name
//!
//! - [`ModuleSearch::enter_package`] returns a search object that can be used to resolve
//!   the descendants of a module prefix. For example `ModuleSearch::enter_package("acme")`
//!   initializes a search that can be used to resolve any submodules of `acme` (e.g., `acme.tools`,
//!   `acme.reports`, etc.).
//! - [`ModuleSearch::resolve_child`] selects the module candidates for a particular terminal
//!   component of a module name (e.g. `ModuleSearch::resolve_child("tools")`, to resolve
//!   `acme.tools` given a prior call to `ModuleSearch::enter_package("acme")`), while leaving the
//!   search object reusable for resolving a different child with the same module name prefix.

use std::cell::OnceCell;
use std::rc::Rc;

use crate::module_name::ModuleName;

use super::{
    ComponentFileFilter, NameResolver, ResolvedNames, StubPackagePaths, normalize_candidates,
    search_paths, stub_package_index,
};

pub(super) struct ModuleSearch<'resolver, 'db> {
    resolver: &'resolver NameResolver<'db>,
    cursor: SearchCursor<'db>,
}

impl<'resolver, 'db> ModuleSearch<'resolver, 'db> {
    /// Initializes a new search procedure in one of the two module resolution
    /// modes (typing or runtime).
    pub(super) fn new(resolver: &'resolver NameResolver<'db>) -> Self {
        let cursor = if resolver.context.mode.is_typing() {
            SearchCursor::Typing(TypingSearchCursor::Root)
        } else {
            SearchCursor::Runtime(RuntimeSearchCursor::Root)
        };
        Self { resolver, cursor }
    }

    /// Returns a new search which has been advanced by one component of a
    /// module name. The previous search object can be reused for searching
    /// sibling module name components.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called first with "acme", and then again with "tools" on the
    /// resulting object.
    pub(super) fn enter_package(&self, component_name: &str) -> Option<Self> {
        Some(Self {
            resolver: self.resolver,
            cursor: self.cursor.enter_package(self.resolver, component_name)?,
        })
    }

    /// Resolves the given terminal component of a module name.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called with "power" after previous calls to [`ModuleSearch::enter_package`]
    /// with "acme" and "tools".
    pub(super) fn resolve_child(&self, component_name: &str) -> Option<ResolvedNames<'db>> {
        self.cursor.resolve_child(self.resolver, component_name)
    }

    #[cfg(test)]
    fn full_module_name(&self, component_name: &str) -> Option<ModuleName> {
        let prefix = match &self.cursor {
            SearchCursor::Typing(cursor) => cursor.prefix(),
            SearchCursor::Runtime(cursor) => cursor.prefix(),
        };
        full_module_name(prefix, component_name)
    }
}

/// Represents search progress through the components of a module name.
enum SearchCursor<'db> {
    Typing(TypingSearchCursor<'db>),
    Runtime(RuntimeSearchCursor<'db>),
}

impl<'db> SearchCursor<'db> {
    /// Advances a search to the given component name (i.e., one component
    /// further than the current module name prefix for this search).
    fn enter_package(&self, resolver: &NameResolver<'db>, component_name: &str) -> Option<Self> {
        match self {
            Self::Typing(cursor) => cursor
                .enter_package(resolver, component_name)
                .map(Self::Typing),
            Self::Runtime(cursor) => cursor
                .enter_package(resolver, component_name)
                .map(Self::Runtime),
        }
    }

    /// Resolve the module identified by the given component name (which is appended to the current
    /// module name prefix for this search) using ordinary import resolution rules.
    fn resolve_child(
        &self,
        resolver: &NameResolver<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        match self {
            Self::Typing(cursor) => cursor.resolve_child(resolver, component_name),
            Self::Runtime(cursor) => cursor.resolve_child(resolver, component_name),
        }
    }
}

/// Implements the typing-mode rules for resolving a module.
///
/// Typing-mode resolution of submodules proceeds in two phases:
///
/// 1. We search any configured extra paths for a stub (i.e., `.pyi`) override
///    of the module name.
/// 2. If the first search fails, then we search all configured search paths
///    (including extra paths) for a module candidate (i.e., an ordinary package,
///    a namespace package, or a plain module), giving priority to stub-only
///    packages over ordinary packages/modules.
///
/// The second phase has a few additional nuances:
///
/// - A stub-only package from a search path after the standard library does not
///   override a standard library package/module (unlike in the first phase).
/// - A stub-only package can be marked as partial, in which case any modules missing
///   from that package may instead be supplied by an ordinary package.
/// - An ordinary package may provide a stub file, a source file, or both;
///   within a single directory the stub takes precedence over the source.
enum TypingSearchCursor<'db> {
    /// Describes a new search, where the cursor is positioned before any
    /// module name components.
    Root,
    /// Describes a search that has progressed past the encapsulated module name prefix.
    Prefix(TypingSearchPrefix<'db>),
}

impl<'db> TypingSearchCursor<'db> {
    fn enter_package(&self, resolver: &NameResolver<'db>, component_name: &str) -> Option<Self> {
        let prefix = full_module_name(self.prefix(), component_name)?;
        let prefix = match self {
            Self::Root => {
                let context = &resolver.context;
                let (extra_stub_package_paths, _) =
                    stub_package_index(context.db, context.resolver_environment)
                        .split_by_extra_paths();
                let root_candidates = resolver.discover_roots(
                    component_name,
                    false,
                    search_paths(context.db, context.resolver_environment, context.mode)
                        .take_while(|path| path.is_extra()),
                    extra_stub_package_paths,
                );
                let stub_override_candidates =
                    normalize_candidates(context.db, root_candidates.clone(), true);
                TypingSearchPrefix {
                    prefix,
                    root_candidates_from_extra_paths: (!root_candidates.is_empty())
                        .then(|| Rc::new(root_candidates)),
                    stub_override_candidates,
                    full_search_candidates: OnceCell::new(),
                }
            }
            Self::Prefix(parent) => {
                let stub_override_candidates = resolver.advance_candidates(
                    parent.stub_override_candidates.clone(),
                    component_name,
                    ComponentFileFilter::ByMode,
                    true,
                );

                // Advance the second-phase candidates if already computed.
                // Otherwise, defer that search while the first phase may still find a stub override.
                let full_search_candidates =
                    parent.full_search_candidates.get().map(|candidates| {
                        resolver.advance_candidates(
                            candidates.clone(),
                            component_name,
                            ComponentFileFilter::ByMode,
                            true,
                        )
                    });
                TypingSearchPrefix {
                    prefix,
                    root_candidates_from_extra_paths: parent
                        .root_candidates_from_extra_paths
                        .as_ref()
                        .map(Rc::clone),
                    stub_override_candidates,
                    full_search_candidates: full_search_candidates
                        .map(OnceCell::from)
                        .unwrap_or_default(),
                }
            }
        };

        // Stop if neither search phase has candidates for this module name prefix.
        if prefix.stub_override_candidates.is_empty()
            && prefix.full_search_candidates(resolver).is_empty()
        {
            return None;
        }

        Some(Self::Prefix(prefix))
    }

    fn resolve_child(
        &self,
        resolver: &NameResolver<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        let name = full_module_name(self.prefix(), component_name)?;
        let candidates = match self {
            Self::Root => {
                let context = &resolver.context;
                // Typing mode requires us to consider stub-only packages (such as the package named
                // `acme-stubs`, which only contains `.pyi` files) when resolving a module name
                // (e.g., the name `acme`), so we must select all stub package paths here.
                let stub_paths = stub_package_index(context.db, context.resolver_environment).all();
                resolve_root(resolver, &name, stub_paths)
            }
            Self::Prefix(prefix) => {
                // First phase: attempt to resolve the child through a stub override in extra paths.
                let stub_override = resolver.advance_candidates(
                    prefix.stub_override_candidates.clone(),
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

                // Second phase: resolve the child using candidates from all configured search paths.
                resolver.advance_candidates(
                    prefix.full_search_candidates(resolver).clone(),
                    name.last_component(),
                    ComponentFileFilter::ByMode,
                    false,
                )
            }
        };
        (!candidates.is_empty()).then_some(candidates)
    }

    fn prefix(&self) -> Option<&ModuleName> {
        match self {
            Self::Root => None,
            Self::Prefix(prefix) => Some(&prefix.prefix),
        }
    }
}

/// Search state for a module name prefix in typing mode.
struct TypingSearchPrefix<'db> {
    prefix: ModuleName,

    /// Candidates for the root module name component that were discovered in
    /// extra paths during the first phase of the module search.
    ///
    /// This is stored before advancing the search to later module name components
    /// so that the second phase of the search can combine them with candidates
    /// from other paths without searching extra paths again.
    root_candidates_from_extra_paths: Option<Rc<ResolvedNames<'db>>>,

    /// Candidates for the current module name prefix that should be considered
    /// for the first-phase stub override search from extra paths.
    stub_override_candidates: ResolvedNames<'db>,

    /// Candidates for the current module name prefix that should be considered
    /// for the second-phase search across all configured paths.
    ///
    /// Upon entering a package, this cell is either initialized from the same
    /// cell for the parent module name prefix (if the second-phase search was
    /// already invoked for that prefix), or it is left uninitialized until it
    /// is first needed. After initialization, it is retained for future
    /// searches from the same module name prefix.
    full_search_candidates: OnceCell<ResolvedNames<'db>>,
}

impl<'db> TypingSearchPrefix<'db> {
    /// Returns module candidates for the second phase of the search (which
    /// looks across all configured paths), initializing them on first access.
    fn full_search_candidates(&self, resolver: &NameResolver<'db>) -> &ResolvedNames<'db> {
        self.full_search_candidates.get_or_init(|| {
            let context = &resolver.context;
            let (_, remaining_stub_package_paths) =
                stub_package_index(context.db, context.resolver_environment).split_by_extra_paths();

            // Combine candidates across all search paths before traversing the
            // module name prefix, so that an ordinary package in one search path
            // can shadow a namespace portion in another.
            let mut candidates = self
                .root_candidates_from_extra_paths
                .as_deref()
                .cloned()
                .unwrap_or_default();
            candidates.extend(
                resolver.discover_roots(
                    self.prefix.first_component(),
                    false,
                    search_paths(context.db, context.resolver_environment, context.mode)
                        .skip_while(|path| path.is_extra()),
                    remaining_stub_package_paths,
                ),
            );
            candidates = normalize_candidates(context.db, candidates, true);

            for component_name in self.prefix.components().skip(1) {
                candidates = resolver.advance_candidates(
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
/// Searches the configured search paths for source modules and packages,
/// ignoring stub files and stub-only packages.
enum RuntimeSearchCursor<'db> {
    /// Describes a new search, where the cursor is positioned before any
    /// module name components.
    Root,
    /// Describes a search that has progressed past the encapsulated module name prefix.
    Prefix {
        prefix: ModuleName,
        candidates: ResolvedNames<'db>,
    },
}

impl<'db> RuntimeSearchCursor<'db> {
    fn enter_package(&self, resolver: &NameResolver<'db>, component_name: &str) -> Option<Self> {
        let prefix = full_module_name(self.prefix(), component_name)?;
        let candidates = match self {
            Self::Root => {
                let context = &resolver.context;
                let candidates = resolver.discover_roots(
                    component_name,
                    false,
                    search_paths(context.db, context.resolver_environment, context.mode),
                    StubPackagePaths::default(),
                );
                normalize_candidates(context.db, candidates, true)
            }
            Self::Prefix { candidates, .. } => resolver.advance_candidates(
                candidates.clone(),
                component_name,
                ComponentFileFilter::ByMode,
                true,
            ),
        };
        (!candidates.is_empty()).then_some(Self::Prefix { prefix, candidates })
    }

    fn resolve_child(
        &self,
        resolver: &NameResolver<'db>,
        component_name: &str,
    ) -> Option<ResolvedNames<'db>> {
        let name = full_module_name(self.prefix(), component_name)?;
        let candidates = match self {
            Self::Root => resolve_root(
                resolver,
                &name,
                // Runtime resolution ignores stub packages, so we can pass the default value for stub
                // package paths (i.e., no stub package paths at all).
                StubPackagePaths::default(),
            ),
            Self::Prefix { candidates, .. } => resolver.advance_candidates(
                candidates.clone(),
                name.last_component(),
                ComponentFileFilter::ByMode,
                false,
            ),
        };
        (!candidates.is_empty()).then_some(candidates)
    }

    fn prefix(&self) -> Option<&ModuleName> {
        match self {
            Self::Root => None,
            Self::Prefix { prefix, .. } => Some(prefix),
        }
    }
}

fn resolve_root<'db>(
    resolver: &NameResolver<'db>,
    name: &ModuleName,
    stub_paths: StubPackagePaths<'_>,
) -> ResolvedNames<'db> {
    let context = &resolver.context;
    let is_non_shadowable = context.mode.is_non_shadowable(
        context
            .resolver_environment
            .python_version(context.db)
            .minor,
        name.as_str(),
    );
    let candidates = resolver.discover_roots(
        name.first_component(),
        is_non_shadowable,
        search_paths(context.db, context.resolver_environment, context.mode),
        stub_paths,
    );
    normalize_candidates(context.db, candidates, false)
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
    use ruff_db::Db as _;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    use crate::settings::SearchPathSettings;
    use crate::strategy::FallibleStrategy;
    use crate::testing::TestCaseBuilder;

    use super::{ModuleSearch, NameResolver};

    #[test]
    fn module_search_can_be_reused_across_sibling_module_resolutions() {
        let db = search_db(
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
    fn sibling_modules_can_be_resolved_correctly_in_any_order() {
        let db = search_db(
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
    fn module_resolution_does_not_affect_nested_package_searches() {
        let db = search_db(
            &[
                "/extra/acme/tools/patched.pyi",
                "/src/acme/__init__.py",
                "/src/acme/runtime.py",
                "/src/acme/tools/__init__.py",
                "/src/acme/tools/patched.py",
                "/src/acme/tools/runtime.py",
            ],
            &["/extra"],
        );
        let resolver = NameResolver::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let acme = ModuleSearch::new(&resolver)
            .enter_package("acme")
            .expect("parent package exists");

        // Enter the nested package before and after a sibling lookup requires
        // the parent's second-phase search across all configured paths.
        let tools_before = acme.enter_package("tools").expect("nested package exists");
        assert_resolves_to(&db, &acme, "runtime", "/src/acme/runtime.py");
        let tools_after = acme.enter_package("tools").expect("nested package exists");

        for tools in [&tools_before, &tools_after] {
            assert_resolves_to(&db, tools, "patched", "/extra/acme/tools/patched.pyi");
            assert_resolves_to(&db, tools, "runtime", "/src/acme/tools/runtime.py");
        }
    }

    fn search_db(paths: &[&str], extra_paths: &[&str]) -> TestDb {
        let mut db = TestCaseBuilder::new().build().db;
        db.write_files(paths.iter().map(|path| (*path, "")))
            .expect("write search fixtures");
        let settings = SearchPathSettings {
            src_roots: vec![SystemPathBuf::from("/src")],
            site_packages_paths: vec![SystemPathBuf::from("/site-packages")],
            custom_typeshed: Some(SystemPathBuf::from("/typeshed")),
            extra_paths: extra_paths
                .iter()
                .copied()
                .map(SystemPathBuf::from)
                .collect(),
            ..SearchPathSettings::empty()
        };
        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .expect("configure search fixtures"),
        );
        db
    }

    fn assert_resolves_to(db: &TestDb, search: &ModuleSearch, component: &str, expected: &str) {
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
}
