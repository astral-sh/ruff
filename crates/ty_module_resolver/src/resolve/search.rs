//! Module-name resolution using typing or runtime precedence.

use ruff_db::files::File;

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, SearchPath};
use crate::{ResolverEnvironment, ResolverFile};

use super::{
    ComponentFileFilter, ModuleResolutionCandidate, ModuleResolveMode, ResolvedNames,
    ResolverContext, StubPackageIndex, StubPackagePaths, absolute_desperate_search_paths,
    normalize_candidates, resolve_component, resolve_stub_package_in_search_path, search_paths,
    stub_package_index,
};

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
pub(super) fn resolve_name<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    name: &ModuleName,
    mode: ModuleResolveMode,
) -> Option<ResolvedNames<'db>> {
    let resolver = NameResolver::new(db, resolver_environment, name, mode);

    match mode {
        ModuleResolveMode::Typing => {
            resolver.resolve_typing(stub_package_index(db, resolver_environment))
        }
        ModuleResolveMode::Runtime | ModuleResolveMode::RuntimeSomeShadowingAllowed => {
            resolver.resolve_runtime(search_paths(db, resolver_environment, mode))
        }
    }
}

/// Like `resolve_name` but for cases where it failed to resolve the module
/// and we are now Getting Desperate and willing to try the ancestor directories of
/// the `importing_file` as potential temporary search paths that are private
/// to this import.
pub(super) fn desperately_resolve_name<'db>(
    db: &'db dyn Db,
    importing_file: File,
    resolver_environment: ResolverEnvironment<'db>,
    name: &ModuleName,
    mode: ModuleResolveMode,
) -> Option<ResolvedNames<'db>> {
    let importing_file = ResolverFile::new(db, importing_file, resolver_environment);
    let search_paths = absolute_desperate_search_paths(db, importing_file).unwrap_or_default();
    let resolver = NameResolver::new(db, resolver_environment, name, mode);
    let stub_packages = mode
        .is_typing()
        .then(|| StubPackageIndex::from_search_paths(db, search_paths.iter()));
    let mut candidates = resolver.discover_roots(
        name.first_component(),
        resolver.is_non_shadowable,
        search_paths.iter(),
        stub_packages
            .as_ref()
            .map_or_else(StubPackagePaths::default, StubPackageIndex::all),
    );
    let mut components = name.components().skip(1).peekable();

    candidates = normalize_candidates(db, candidates, components.peek().is_some());
    while let Some(component) = components.next() {
        candidates = resolver.advance_candidates(
            candidates,
            component,
            ComponentFileFilter::ByMode,
            components.peek().is_some(),
        );
    }

    (!candidates.is_empty()).then_some(candidates)
}

struct NameResolver<'db, 'name> {
    context: ResolverContext<'db>,
    name: &'name ModuleName,
    is_non_shadowable: bool,
}

impl<'db, 'name> NameResolver<'db, 'name> {
    fn new(
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
        name: &'name ModuleName,
        mode: ModuleResolveMode,
    ) -> Self {
        let python_version = resolver_environment.python_version(db);
        Self {
            context: ResolverContext::new(db, resolver_environment, mode),
            name,
            is_non_shadowable: mode.is_non_shadowable(python_version.minor, name.as_str()),
        }
    }

    /// Resolves the name as seen by a type checker.
    ///
    /// This includes PEP 561 stub packages and user-provided stub overlays, with runtime source as
    /// a fallback when no stub provides the requested module. A stub overlay may use runtime
    /// packages as parents, but its final module must come from a stub file.
    fn resolve_typing(&self, stub_packages: &StubPackageIndex) -> Option<ResolvedNames<'db>> {
        if self.name.components().nth(1).is_none() {
            let candidates = self.discover_roots(
                self.name.first_component(),
                self.is_non_shadowable,
                search_paths(
                    self.context.db,
                    self.context.resolver_environment,
                    ModuleResolveMode::Typing,
                ),
                stub_packages.all(),
            );
            return self.resolve_remaining(candidates, ComponentFileFilter::ByMode);
        }

        // Only submodules need separate overlay resolution: their extra-path namespace parent can
        // be shadowed before the resolver reaches the requested stub. Reuse those roots for the
        // normal fallback so that each extra path is probed only once.
        let (overlay_stub_packages, remaining_stub_packages) = stub_packages.split_overlay();
        let mut candidates = self.discover_roots(
            self.name.first_component(),
            self.is_non_shadowable,
            search_paths(
                self.context.db,
                self.context.resolver_environment,
                ModuleResolveMode::Typing,
            )
            .take_while(|search_path| search_path.is_extra()),
            overlay_stub_packages,
        );
        if let Some(resolved) =
            self.resolve_remaining(candidates.clone(), ComponentFileFilter::StubOnly)
        {
            return Some(resolved);
        }

        let remaining_candidates = self.discover_roots(
            self.name.first_component(),
            self.is_non_shadowable,
            search_paths(
                self.context.db,
                self.context.resolver_environment,
                ModuleResolveMode::Typing,
            )
            .skip_while(|search_path| search_path.is_extra()),
            remaining_stub_packages,
        );
        candidates.extend(remaining_candidates);

        self.resolve_remaining(candidates, ComponentFileFilter::ByMode)
    }

    /// Resolves the name to the implementation that is available at runtime.
    ///
    /// The runtime resolver ignores stub packages and `.pyi` files. Its search paths also use the
    /// real standard library instead of typeshed.
    fn resolve_runtime<'a>(
        &self,
        search_paths: impl Iterator<Item = &'a SearchPath>,
    ) -> Option<ResolvedNames<'db>> {
        let candidates = self.discover_roots(
            self.name.first_component(),
            self.is_non_shadowable,
            search_paths,
            StubPackagePaths::default(),
        );
        self.resolve_remaining(candidates, ComponentFileFilter::ByMode)
    }

    fn resolve_remaining(
        &self,
        mut cur_candidates: ResolvedNames<'db>,
        final_filter: ComponentFileFilter,
    ) -> Option<ResolvedNames<'db>> {
        if cur_candidates.is_empty() {
            return None;
        }

        let mut components = self.name.components().skip(1).peekable();

        // Keep a partial stub package's namespace while resolving the next part of the module
        // name. Once the complete name is resolved, a concrete package or module shadows that
        // namespace.
        cur_candidates =
            normalize_candidates(self.context.db, cur_candidates, components.peek().is_some());

        while let Some(component) = components.next() {
            let has_remaining_components = components.peek().is_some();
            let file_filter = if has_remaining_components {
                ComponentFileFilter::ByMode
            } else {
                final_filter
            };

            cur_candidates = self.advance_candidates(
                cur_candidates,
                component,
                file_filter,
                has_remaining_components,
            );

            if cur_candidates.is_empty() {
                return None;
            }
        }

        Some(cur_candidates)
    }

    /// Finds candidates for a top-level name across the supplied search paths and stub packages.
    fn discover_roots<'a>(
        &self,
        root_component: &str,
        is_non_shadowable: bool,
        search_paths: impl Iterator<Item = &'a SearchPath>,
        stub_paths: StubPackagePaths<'_>,
    ) -> ResolvedNames<'db> {
        let context = &self.context;
        let mut cur_candidates = Vec::new();
        let stub_name = (!stub_paths.is_empty() && !is_non_shadowable)
            .then(|| format!("{root_component}-stubs"));
        let mut pending_stub_paths = Vec::new();

        if let Some(stub_name) = &stub_name {
            cur_candidates.extend(stub_paths.before_stdlib.iter().filter_map(|search_path| {
                resolve_stub_package_in_search_path(context, search_path, stub_name)
            }));
            // Defer file probes after stdlib until we know that stdlib does not win.
            pending_stub_paths.extend(stub_paths.after_stdlib.iter().filter(|search_path| {
                ModuleDirectory::new(context, search_path.to_module_path())
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
    fn advance_candidates(
        &self,
        mut candidates: ResolvedNames<'db>,
        component: &str,
        filter: ComponentFileFilter,
        for_descendants: bool,
    ) -> ResolvedNames<'db> {
        let context = &self.context;
        let mut remaining_are_shadowed = false;
        candidates.retain_mut(|candidate| {
            if remaining_are_shadowed {
                return false;
            }

            let resolved = resolve_component(context, candidate, component, filter).is_ok();

            // A terminal candidate shadows every lower-priority candidate, even if resolving
            // this component fails. Higher-priority candidates remain in play.
            remaining_are_shadowed = candidate.missing_submodule_is_terminal();

            resolved
        });
        normalize_candidates(context.db, candidates, for_descendants)
    }
}
