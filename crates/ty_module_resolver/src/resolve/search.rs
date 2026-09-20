//! This module exposes a [`ModuleSearchCursor`] abstraction, which encapsulates reusable
//! search state for namespace-aware module enumeration, and is equally usable for
//! ordinary, single module resolution.
//!
//! [`ModuleSearchCursor`] provides an interface that describes traversal of the components
//! of a module name
//!
//! - [`ModuleSearchCursor::enter_package`] returns a search object that can be used to resolve
//!   the descendants of a module prefix. For example `ModuleSearchCursor::enter_package("acme")`
//!   initializes a search that can be used to resolve any submodules of `acme` (e.g., `acme.tools`,
//!   `acme.reports`, etc.).
//! - [`ModuleSearchCursor::resolve_child`] selects the module candidates for a particular terminal
//!   component of a module name (e.g. `ModuleSearchCursor::resolve_child("tools")`, to resolve
//!   `acme.tools` given a prior call to `ModuleSearchCursor::enter_package("acme")`), while leaving the
//!   search object reusable for resolving a different child with the same module name prefix.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::rc::Rc;

use itertools::Either;

use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, SearchPath};

use super::{
    ComponentFileFilter, ModuleResolutionCandidate, ResolvedNames, ResolverContext,
    StubPackageIndex, StubPackagePaths, normalize_candidates, resolve_component,
    resolve_stub_package_in_search_path, search_paths, stub_package_index,
};

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

    fn with_paths(context: &'a ResolverContext<'db>, search_paths: RootSearchPaths<'db>) -> Self {
        Self {
            context,
            position: Position::Root(search_paths),
        }
    }

    /// Resolves a module name relative to the current position of this search's cursor.
    pub(super) fn resolve_name(mut self, name: &ModuleName) -> Option<ResolvedNames<'db>> {
        let mut components = name.components();
        let last = components.next_back()?;
        for component in components {
            self = self.enter_package(component)?;
        }
        self.resolve_child(last)
    }

    /// Returns a new search which has been advanced by one component of a
    /// module name. The previous search object can be reused for searching
    /// sibling module name components.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called first with "acme", and then again with "tools" on the
    /// resulting object.
    fn enter_package(&self, component_name: &str) -> Option<Self> {
        let resolver = match &self.position {
            Position::Root(paths) => PrefixResolver::new(self.context, paths, component_name)?,
            Position::Prefix(resolver) => resolver.enter_package(self.context, component_name)?,
        };
        Some(Self {
            context: self.context,
            position: Position::Prefix(resolver),
        })
    }

    /// Resolves the given terminal component of a module name.
    ///
    /// For instance, when resolving the module `acme.tools.power`, this method
    /// should be called with "power" after previous calls to [`ModuleSearchCursor::enter_package`]
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

    #[cfg(test)]
    fn full_module_name(&self, component_name: &str) -> Option<ModuleName> {
        let prefix = match &self.position {
            Position::Root(_) => None,
            Position::Prefix(resolver) => Some(resolver.prefix()),
        };
        full_module_name(prefix, component_name)
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
    fn enter_package(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
        match self {
            Self::Typing(resolver) => resolver
                .enter_package(context, component_name)
                .map(Self::Typing),
            Self::Runtime(resolver) => resolver
                .enter_package(context, component_name)
                .map(Self::Runtime),
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

    #[cfg(test)]
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
    /// When entering a package, we initialize this cell immediately if the parent
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

    fn enter_package(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
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

    fn enter_package(&self, context: &ResolverContext<'db>, component_name: &str) -> Option<Self> {
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
            ModuleDirectory::new(context, search_path.to_module_path()).may_contain_name(stub_name)
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
    use ruff_db::Db as _;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    use crate::settings::SearchPathSettings;
    use crate::strategy::FallibleStrategy;
    use crate::testing::TestCaseBuilder;

    use super::{ModuleSearchCursor, ResolverContext};

    #[test]
    fn module_search_can_be_reused_across_sibling_module_resolutions() {
        let db = search_db(
            &["/src/acme/reports.py", "/site-packages/acme/tools.py"],
            &[],
        );
        for mode in [ModuleResolveMode::Typing, ModuleResolveMode::Runtime] {
            let context = ResolverContext::new(&db, db.resolver_environment(), mode);
            let root = ModuleSearchCursor::with_configured_search_paths(&context);
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
            let context =
                ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
            let acme = ModuleSearchCursor::with_configured_search_paths(&context)
                .enter_package("acme")
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
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let acme = ModuleSearchCursor::with_configured_search_paths(&context)
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
}
