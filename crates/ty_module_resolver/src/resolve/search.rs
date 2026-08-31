//! Reusable prefix search for module resolution.

use std::cell::OnceCell;
use std::rc::Rc;

use crate::module_name::ModuleName;

use super::{
    ComponentFileFilter, ModuleResolutionCandidate, NameResolver, ResolvedNames, StubPackagePaths,
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
    use ruff_db::Db as _;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    use crate::settings::SearchPathSettings;
    use crate::strategy::FallibleStrategy;
    use crate::testing::TestCaseBuilder;

    use super::{ModuleSearch, NameResolver};

    #[test]
    fn sibling_searches_reuse_split_namespace() {
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
    fn selected_initializer_preserves_legacy_namespace_search() {
        let mut db = search_db(
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

    fn search_db(paths: &[&str], extra_paths: &[&str]) -> TestDb {
        let mut db = TestCaseBuilder::new().build().db;
        db.write_files(paths.iter().map(|path| {
            (
                *path, r#"
"#,
            )
        }))
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
