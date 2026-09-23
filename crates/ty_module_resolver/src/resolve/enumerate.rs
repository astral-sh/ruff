use std::borrow::Cow;
use std::collections::BTreeSet;

use compact_str::CompactString;
use ruff_db::files::directory_listing;
use ruff_db::system::FileType;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::module::Module;
use crate::module_name::ModuleName;
use crate::path::ModuleDirectory;

use super::search::ModuleSearchCursor;
use super::{ModuleResolutionCandidate, ResolvedModule, ResolvedNames, ResolverContext};

/// Lists top-level modules across the configured search paths.
pub(crate) fn list_root_modules<'db>(context: &ResolverContext<'db>) -> ModuleListing<'db> {
    ModuleSearchCursor::with_configured_search_paths(context).list_modules()
}

/// Resolved modules and unresolved module name prefixes for stub overrides.
///
/// We store both resolved modules and unresolved name prefixes because
/// installed stubs can omit intermediate packages. For instance, they can omit
/// a name like `acme.nested` while a stub override supplies `acme.nested.tools`.
/// Hence recursive enumeration must search `acme.nested` even while import
/// statement completion omits it.
#[derive(Default)]
pub(crate) struct ModuleListing<'db> {
    /// Modules that resolve independently and are also eligible for enumeration.
    pub(crate) modules: Vec<Module<'db>>,
    /// Unresolved module name prefixes that are nonetheless eligible for enumeration
    /// because they have eligible stub override candidates.
    pub(crate) stub_override_prefixes: Vec<ModuleName>,
    /// Listed modules that may have descendants, including files with stub overrides.
    pub(crate) modules_with_possible_children: Vec<Module<'db>>,
}

/// Lists immediate submodules of a resolved module across the configured search paths.
pub(crate) fn list_submodules<'db>(
    context: &ResolverContext<'db>,
    module: Module<'db>,
) -> ModuleListing<'db> {
    let name = module.name(context.db);
    list_submodules_by_name(context, name)
}

/// Lists immediate submodules without requiring the parent name to resolve.
///
/// This allows enumeration to reach local stub overrides beneath unresolved prefixes.
pub(crate) fn list_submodules_by_name<'db>(
    context: &ResolverContext<'db>,
    name: &ModuleName,
) -> ModuleListing<'db> {
    let Some(search) = ModuleSearchCursor::with_configured_search_paths(context).for_prefix(name)
    else {
        return ModuleListing::default();
    };

    search.list_modules()
}

impl<'db> ModuleSearchCursor<'_, 'db> {
    /// Given a module, lists immediate submodules of that module; otherwise
    /// lists top-level modules at the start of the search.
    pub(crate) fn list_modules(&self) -> ModuleListing<'db> {
        let context = self.context;
        let db = context.db;
        let prefix = self.prefix();
        let is_listable =
            |candidate: &ModuleResolutionCandidate| is_listable_location(db, candidate);
        let mut names = BTreeSet::new();
        let mut listable_directories = Vec::new();
        let mut collect = |directory: &ModuleDirectory| {
            directory.for_each_entry(db, |entry, kind| {
                if let Some(name) = child_module_name(entry, kind, prefix.is_none()) {
                    names.insert(CompactString::new(name));
                }
            });
        };

        if prefix.is_none() {
            context.prepare_root_directories(self.root_search_paths());
            for path in self.root_search_paths() {
                collect(&context.root_directory(path));
            }
        } else {
            for candidate in self
                .candidates()
                .filter(|candidate| is_listable_package(candidate, is_listable))
            {
                listable_directories.push(&candidate.directory);
                collect(&candidate.directory);
            }
        }

        // Check whether the resolved location is allowed by the listing policy.
        // Top-level locations are allowed. Below a prefix, reuse the checks for
        // directories already visited and their non-symlink children; otherwise,
        // check the candidate's full path.
        let is_listable = |candidate: &ModuleResolutionCandidate| {
            let path = candidate.directory.path();
            prefix.is_none()
                || listable_directories
                    .iter()
                    .any(|directory| directory.path() == path || directory.is_child_directory(path))
                || is_listable(candidate)
        };
        let mut listing = ModuleListing::default();

        for component_name in names {
            let Some(name) = self.full_module_name(&component_name) else {
                continue;
            };

            if let Some(candidates) = self.resolve_child(&component_name) {
                if let Some(candidate) = select_candidate_for_listing(candidates, is_listable) {
                    let module =
                        candidate.into_module(db, context.resolver_environment, Cow::Owned(name));
                    listing.modules.push(module);
                    listing.modules_with_possible_children.push(module);
                }

                // Resolution succeeded, so move on to the next name. The resolved module
                // takes precedence over stub override prefixes even when excluded from listing.
                continue;
            }

            // The full search found no module, so any remaining prefix candidates belong
            // to the stub override search: `acme.nested` may lead to `acme/nested/tools.pyi`
            // even when installed stubs omit `acme.nested`.
            if let Some(child_search) = self.enter_package(&component_name)
                && child_search
                    .candidates()
                    .any(|candidate| is_listable_package(candidate, is_listable))
            {
                listing.stub_override_prefixes.push(name);
            }
        }
        listing
    }
}

/// Whether enumeration may discover child names from this candidate's directory.
fn is_listable_package(
    candidate: &ModuleResolutionCandidate,
    is_listable: impl FnOnce(&ModuleResolutionCandidate) -> bool,
) -> bool {
    !matches!(candidate.module, ResolvedModule::Module(_)) && is_listable(candidate)
}

/// Selects the resolved module for listing if its location is allowed by the listing policy.
/// An implicit namespace can be listed if any of its portions is allowed.
///
/// This method assumes that `candidates` is ordered by resolution precedence
/// (which ordinarily happens through `resolve::normalize_candidates`).
fn select_candidate_for_listing<'db>(
    candidates: ResolvedNames<'db>,
    is_listable: impl Fn(&ModuleResolutionCandidate<'db>) -> bool,
) -> Option<ModuleResolutionCandidate<'db>> {
    let mut candidates = candidates.into_iter();

    // Select the first candidate from the ordered list (i.e., the same winner
    // as ordinary resolution) before applying the listing policy so that
    // excluding a concrete module cannot expose a lower-priority alternative.
    let selected = candidates.next()?;

    let can_list = match selected.module {
        // An implicit namespace has no defining file, so any eligible portion suffices.
        ResolvedModule::NamespacePackage => {
            is_listable(&selected) || candidates.any(|candidate| is_listable(&candidate))
        }
        // Concrete modules, including legacy namespaces, must use the selected location.
        ResolvedModule::Package(_) | ResolvedModule::Module(_) => is_listable(&selected),
    };
    can_list.then_some(selected)
}

/// Allows directory symlinks only at search roots and their immediate children.
///
/// This supports symlinked search roots and top-level package aliases while preventing
/// recursive enumeration from following directory cycles indefinitely.
fn is_listable_location(db: &dyn Db, candidate: &ModuleResolutionCandidate) -> bool {
    let path = candidate.directory.path();
    let Some(search_root) = path.search_path().as_system_path() else {
        return true;
    };
    let Some(directory) = path.to_system_path() else {
        return false;
    };
    let Ok(relative) = directory.strip_prefix(search_root) else {
        return false;
    };
    let mut parent = search_root.to_path_buf();

    for (depth, component) in relative.components().enumerate() {
        if depth > 0
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

/// Returns `Some(name)` for a directory entry that supplies a candidate child
/// module name, or `None` for an entry excluded from enumeration.
///
/// Accepts directories and `.py`/`.pyi` files or symlinks. At search roots, symlinks
/// can also name packages without either extension. After stripping file extensions
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
        FileType::Symlink if at_search_root => python_stem().unwrap_or(entry),
        FileType::File | FileType::Symlink => python_stem()?,
    };
    let name = if at_search_root {
        name.strip_suffix("-stubs").unwrap_or(name)
    } else {
        name
    };
    is_identifier(name).then_some(name)
}

#[cfg(test)]
mod tests {
    use ruff_db::system::{DbWithWritableSystem, SystemPath};

    use crate::ModuleName;
    use crate::db::tests::TestDb;
    use crate::resolve::{ModuleResolveMode, ResolverContext};
    use crate::testing::{enumeration_db, unresolved_stub_override_db, write_empty_file};
    #[cfg(target_family = "unix")]
    use crate::testing::{os_enumeration_db, symlink_enumeration_db};

    use super::{ModuleListing, list_root_modules, list_submodules, list_submodules_by_name};

    #[test]
    fn preserves_file_precedence_when_listing_roots() {
        let db = enumeration_db(
            &[
                // Within one root, the stub wins over the source file.
                "/src/leaf.py",
                "/src/leaf.pyi",
                // The extra-path stub override wins over both other locations.
                "/extra/shared.pyi",
                "/src/shared.py",
                "/site-packages/shared.pyi",
            ],
            &["/extra"],
        );
        db.assert_listing_with_origins(
            None,
            &[
                ("leaf", Some("/src/leaf.pyi")),
                ("shared", Some("/extra/shared.pyi")),
            ],
        );
    }

    #[test]
    fn excludes_local_files_with_protected_standard_library_names() {
        let db = enumeration_db(&["/src/leaf.py", "/src/sys.py"], &[]);
        // A local file cannot supply a protected name, even when typeshed omits it.
        db.assert_listing(None, &["leaf"]);
    }

    #[test]
    fn enumerates_namespace_portions_across_search_paths() {
        let db = enumeration_db(&["/src/acme/left.py", "/site-packages/acme/right.py"], &[]);
        db.assert_listing(None, &["acme"]);
        db.assert_listing(Some("acme"), &["acme.left", "acme.right"]);
    }

    #[test]
    fn enumerates_nested_namespaces() {
        let db = enumeration_db(&["/src/acme/nested/deep.py"], &[]);
        db.assert_listing(Some("acme"), &["acme.nested"]);
        db.assert_listing(Some("acme.nested"), &["acme.nested.deep"]);
    }

    #[test]
    fn enumerates_namespaces_inside_regular_packages() {
        let db = enumeration_db(
            &[
                "/src/acme/regular/__init__.py",
                "/src/acme/regular/namespace/child.py",
            ],
            &[],
        );
        db.assert_listing(Some("acme"), &["acme.regular"]);
        db.assert_listing(Some("acme.regular"), &["acme.regular.namespace"]);
        db.assert_listing(
            Some("acme.regular.namespace"),
            &["acme.regular.namespace.child"],
        );
    }

    #[test]
    fn updates_listing_when_a_package_becomes_a_legacy_namespace() {
        let mut db = enumeration_db(
            &[
                "/src/acme/__init__.py",
                "/src/acme/local.py",
                "/site-packages/acme/other.py",
            ],
            &[],
        );
        db.assert_listing(Some("acme"), &["acme.local"]);

        db.write_file(
            "/src/acme/__init__.py",
            "__path__ = __import__('pkgutil').extend_path(__path__, __name__)",
        )
        .expect("extend the package across search roots");
        db.assert_listing(Some("acme"), &["acme.local", "acme.other"]);

        db.write_file("/src/acme/__init__.py", "")
            .expect("restore the regular package");
        db.assert_listing(Some("acme"), &["acme.local"]);
    }

    #[test]
    fn uses_resolution_precedence_for_namespace_children() {
        let db = enumeration_db(
            &[
                // A regular package wins over a module with the same name.
                "/src/acme/package/__init__.py",
                "/src/acme/package.pyi",
                // The regular package excludes children from the other namespace portion.
                "/src/acme/package/local.py",
                "/site-packages/acme/package/hidden.py",
                // A file module blocks descendants from a competing namespace directory.
                "/src/acme/module.py",
                "/site-packages/acme/module/hidden.py",
            ],
            &[],
        );
        db.assert_listing_with_origins(None, &[("acme", None)]);
        db.assert_listing_with_origins(
            Some("acme"),
            &[
                ("acme.module", Some("/src/acme/module.py")),
                ("acme.package", Some("/src/acme/package/__init__.py")),
            ],
        );
        db.assert_listing_with_origins(
            Some("acme.package"),
            &[("acme.package.local", Some("/src/acme/package/local.py"))],
        );
        db.assert_listing(Some("acme.module"), &[]);
    }

    #[test]
    fn excludes_namespace_portions_shadowed_by_regular_packages() {
        let db = enumeration_db(
            &["/src/acme/hidden.py", "/site-packages/acme/__init__.py"],
            &[],
        );
        db.assert_listing(None, &["acme"]);
        db.assert_listing(Some("acme"), &[]);
    }

    #[test]
    fn excludes_namespace_portions_shadowed_by_file_modules() {
        let db = enumeration_db(&["/src/acme/hidden.py", "/site-packages/acme.py"], &[]);
        db.assert_listing(None, &["acme"]);
        db.assert_listing(Some("acme"), &[]);
    }

    #[test]
    fn merges_legacy_namespace_portions_until_shadowed() {
        let mut db = enumeration_db(&["/src/acme/left.py", "/site-packages/acme/right.py"], &[]);
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
        db.assert_listing(Some("acme"), &["acme.left", "acme.right"]);

        write_empty_file(&mut db, "/site-packages/acme/__init__.py");
        db.assert_listing(Some("acme"), &["acme.right"]);
    }

    #[test]
    fn combines_partial_stub_namespaces_with_source_packages() {
        let mut db = enumeration_db(
            &[
                "/src/acme/__init__.py",
                "/src/acme/api/__init__.py",
                "/src/acme/api/runtime.py",
                "/site-packages/acme-stubs/api/stubbed.pyi",
            ],
            &[],
        );
        db.write_file("/site-packages/acme-stubs/py.typed", "partial\n")
            .expect("preserve partial stub namespace alongside ordinary packages");
        db.assert_listing_with_origins(
            Some("acme"),
            &[("acme.api", Some("/src/acme/api/__init__.py"))],
        );
        db.assert_listing_with_origins(
            Some("acme.api"),
            &[
                ("acme.api.runtime", Some("/src/acme/api/runtime.py")),
                (
                    "acme.api.stubbed",
                    Some("/site-packages/acme-stubs/api/stubbed.pyi"),
                ),
            ],
        );
    }

    #[test]
    fn includes_source_children_of_partial_stub_subpackages() {
        let mut db = enumeration_db(
            &[
                "/site-packages/acme-stubs/__init__.pyi",
                "/site-packages/acme-stubs/api/__init__.pyi",
                "/site-packages/acme-stubs/api/stubbed.pyi",
                // Complete parent stubs hide `acme.hidden`.
                "/src/acme/__init__.py",
                "/src/acme/hidden.py",
                // Partial child stubs allow `runtime` alongside `stubbed` within `acme.api`.
                "/src/acme/api/__init__.py",
                "/src/acme/api/runtime.py",
            ],
            &[],
        );
        db.write_file("/site-packages/acme-stubs/api/py.typed", "partial\n")
            .expect("mark child as partial");
        db.assert_listing(Some("acme"), &["acme.api"]);
        db.assert_listing(Some("acme.api"), &["acme.api.runtime", "acme.api.stubbed"]);
    }

    #[test]
    fn enumerates_partial_stub_descendants_of_source_modules() {
        let mut db = enumeration_db(
            &["/src/acme.py", "/site-packages/acme-stubs/child.pyi"],
            &[],
        );
        db.write_file("/site-packages/acme-stubs/py.typed", "partial\n")
            .expect("mark the stub namespace as partial");
        db.assert_listing(Some("acme"), &["acme.child"]);
    }

    #[test]
    fn combines_stub_overrides_with_source_siblings() {
        let db = enumeration_db(
            &[
                "/extra/acme/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/stubbed.py",
                "/src/acme/runtime.py",
            ],
            &["/extra"],
        );
        db.assert_listing_with_origins(
            Some("acme"),
            &[
                ("acme.runtime", Some("/src/acme/runtime.py")),
                ("acme.stubbed", Some("/extra/acme/stubbed.pyi")),
            ],
        );
    }

    #[test]
    fn separates_unresolved_stub_override_prefixes_from_modules() {
        let db = unresolved_stub_override_db();
        db.assert_listing(None, &["acme"]);
        db.assert_listing_with_stub_overrides(Some("acme"), &[], &["acme.nested"]);
        db.assert_listing_with_stub_overrides(Some("acme.nested"), &[], &["acme.nested.deep"]);
        db.assert_listing(Some("acme.nested.deep"), &["acme.nested.deep.tools"]);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn enumerates_file_aliases_and_top_level_directory_aliases() {
        let (_temp, db, _root) = symlink_enumeration_db();
        db.assert_listing(None, &["acme", "alias", "regular_alias", "top_alias"]);
        db.assert_listing(
            Some("acme"),
            &["acme.hidden", "acme.ns", "acme.own", "acme.stubbed"],
        );
        db.assert_listing(Some("acme.ns"), &["acme.ns.visible"]);
        db.assert_listing(
            Some("alias"),
            &["alias.hidden", "alias.own", "alias.stubbed"],
        );
        db.assert_listing(
            Some("regular_alias"),
            &[
                "regular_alias.child",
                "regular_alias.linked",
                "regular_alias.nested",
            ],
        );
        db.assert_listing(
            Some("regular_alias.nested"),
            &["regular_alias.nested.child"],
        );
        for name in ["acme.blocked", "acme.blocked.nested", "regular_alias.loop"] {
            crate::resolve_module_confident(
                &db,
                db.resolver_environment(),
                &ModuleName::new(name).expect("valid module name"),
            )
            .expect("package resolves");
            db.assert_listing(Some(name), &[]);
        }
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn excludes_files_shadowed_by_nested_directory_symlinks() {
        let (_temp, mut db, root) = os_enumeration_db(&[]);
        write_empty_file(&mut db, root.join("src/acme/blocked.py"));
        write_empty_file(&mut db, root.join("target/__init__.py"));
        std::os::unix::fs::symlink(root.join("target"), root.join("src/acme/blocked"))
            .expect("create a package symlink that shadows the file");

        // The selected package is excluded; its shadowed file must not appear instead.
        db.assert_listing(Some("acme"), &[]);
        write_empty_file(&mut db, root.join("src/acme/__init__.py"));
        db.assert_listing(Some("acme"), &[]);
    }

    #[test]
    fn excludes_stub_packages_and_files_in_runtime_mode() {
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
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Runtime);
        let name = ModuleName::new_static("acme").expect("valid name");
        let module = crate::resolve_real_module_confident(&db, db.resolver_environment(), &name)
            .expect("runtime package");
        let listing = list_submodules(&context, module);
        assert!(listing.stub_override_prefixes.is_empty());
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

    impl TestDb {
        #[track_caller]
        fn assert_listing(&self, name: Option<&str>, expected: &[&str]) {
            self.assert_listing_with_stub_overrides(name, expected, &[]);
        }

        /// Checks the listed modules' names and defining file paths.
        ///
        /// `parent_module_name` is the fully qualified name whose immediate children are listed;
        /// `None` lists top-level modules.
        ///
        /// Each entry in `expected` is a `(fully_qualified_module_name, file_path)` pair.
        /// A `None` file path denotes an implicit namespace package.
        #[track_caller]
        fn assert_listing_with_origins(
            &self,
            parent_module_name: Option<&str>,
            expected: &[(&str, Option<&str>)],
        ) {
            let listing = self.list_modules(parent_module_name);
            let modules: Vec<_> = listing
                .modules
                .iter()
                .map(|module| {
                    (
                        module.name(self).as_str(),
                        module
                            .file(self)
                            .and_then(|file| file.path(self).as_system_path()),
                    )
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|(name, path)| (*name, path.map(SystemPath::new)))
                .collect();
            assert_eq!(modules, expected);
            assert_eq!(listing.stub_override_prefixes, Vec::<ModuleName>::new());
        }

        #[track_caller]
        fn assert_listing_with_stub_overrides(
            &self,
            parent_module_name: Option<&str>,
            expected_module_names: &[&str],
            expected_stub_override_prefixes: &[&str],
        ) {
            let listing = self.list_modules(parent_module_name);
            let module_names: Vec<_> = listing
                .modules
                .iter()
                .map(|module| module.name(self).as_str())
                .collect();
            assert_eq!(module_names, expected_module_names);

            let stub_override_prefixes: Vec<_> = listing
                .stub_override_prefixes
                .iter()
                .map(ModuleName::as_str)
                .collect();
            assert_eq!(stub_override_prefixes, expected_stub_override_prefixes);
        }

        /// Lists modules and checks that enumeration agrees with ordinary resolution.
        #[track_caller]
        fn list_modules(&self, name: Option<&str>) -> ModuleListing<'_> {
            let context =
                ResolverContext::new(self, self.resolver_environment(), ModuleResolveMode::Typing);
            let listing = match name {
                None => list_root_modules(&context),
                Some(name) => {
                    let name = ModuleName::new(name).expect("valid module name");
                    match crate::resolve_module_confident(self, self.resolver_environment(), &name)
                    {
                        Some(module) => list_submodules(&context, module),
                        None => list_submodules_by_name(&context, &name),
                    }
                }
            };
            for module in &listing.modules {
                let name = module.name(self);
                assert_eq!(
                    Some(*module),
                    crate::resolve_module_confident(self, self.resolver_environment(), name),
                    "enumeration must agree with resolution for {name}"
                );
            }
            for name in &listing.stub_override_prefixes {
                assert!(
                    crate::resolve_module_confident(self, self.resolver_environment(), name)
                        .is_none(),
                    "traversal prefix must not invent a resolved module"
                );
            }
            listing
        }
    }
}
