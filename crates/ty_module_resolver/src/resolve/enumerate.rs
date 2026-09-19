use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use compact_str::{CompactString, format_compact};
use ruff_db::files::directory_listing;
use ruff_db::system::FileType;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::{ModuleDirectory, SearchPath};

use super::search::ModuleSearchCursor;
use super::{
    ComponentFileFilter, ModuleNameIngredient, ModuleResolutionCandidate,
    ModuleResolveModeIngredient, ResolvedModule, ResolvedNames, ResolverContext,
    resolve_file_module_with_filter, search_paths, stub_package_index,
};

/// Lists top-level modules or immediate children of a resolved or unresolved module name.
pub(crate) fn list_modules<'db>(
    context: &ResolverContext<'db>,
    target: &ListingTarget<'db>,
) -> ModuleListing<'db> {
    let db = context.db;
    let (name, module) = match target {
        ListingTarget::Root => (None, None),
        ListingTarget::ResolvedName(module) => (Some(module.name(db)), Some(*module)),
        ListingTarget::UnresolvedName(name) => (Some(name), None),
    };

    if let Some(module) = module
        && module.kind(db) == ModuleKind::Module
        && !may_have_children(context, module.name(db))
    {
        return ModuleListing::default();
    }

    let Some(search) = name.map_or_else(
        || Some(ModuleSearchCursor::with_configured_search_paths(context)),
        |prefix| ModuleSearchCursor::with_configured_search_paths(context).for_prefix(prefix),
    ) else {
        return ModuleListing::default();
    };

    search.list_modules()
}

/// The location whose immediate children should be enumerated.
pub(crate) enum ListingTarget<'db> {
    /// Enumerate top-level module names across the configured search paths.
    Root,
    /// Enumerate children of this resolved module.
    ResolvedName(Module<'db>),
    /// Search an unresolved name for descendants supplied by local stub overrides.
    UnresolvedName(ModuleName),
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

impl<'db> ModuleSearchCursor<'_, 'db> {
    /// Given a module, lists immediate submodules of that module; otherwise
    /// lists top-level modules at the start of the search.
    pub(crate) fn list_modules(&self) -> ModuleListing<'db> {
        let context = self.context;
        let db = context.db;
        let prefix = self.prefix();
        let is_listable =
            |candidate: &ModuleResolutionCandidate| is_listable_location(db, candidate);
        let mut names = BTreeMap::<_, ChildNameSummary>::new();
        let mut listable_directories = Vec::new();
        let mut has_symlinks = false;
        let mut collect = |directory: &ModuleDirectory, search_path: Option<&'db SearchPath>| {
            directory.for_each_entry(db, |entry, kind| {
                // Even excluded directory symlinks can shadow a `.py` or `.pyi` file.
                has_symlinks |= kind.is_symlink();

                if let Some(name) = child_module_name(entry, kind, prefix.is_none()) {
                    // A file module can have descendants from a matching directory in
                    // another location. Symlinks may also turn out to be directories.
                    let summary =
                        names
                            .entry(CompactString::new(name))
                            .or_insert_with(|| ChildNameSummary {
                                search_path,
                                ..ChildNameSummary::default()
                            });
                    summary.record_entry(entry, kind, search_path);
                }
            });
        };

        if prefix.is_none() {
            context.prepare_root_directories(self.root_search_paths());
            for path in self.root_search_paths() {
                collect(&context.root_directory(path), Some(path));
            }
        } else {
            for candidate in self
                .candidates()
                .filter(|candidate| is_listable_package(candidate, is_listable))
            {
                listable_directories.push(&candidate.directory);
                collect(&candidate.directory, None);
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
        let single_candidate = self.single_candidate();

        for (component_name, summary) in names {
            let Some(name) = self.full_module_name(&component_name) else {
                continue;
            };

            let root_directory = summary
                .search_path
                .filter(|path| {
                    path.is_standard_library()
                        || !context.mode.is_non_shadowable(
                            context.resolver_environment.python_version(db).minor,
                            name.as_str(),
                        )
                })
                .map(|path| context.root_directory(path));
            if let Some(directory) = root_directory
                .as_ref()
                .or_else(|| single_candidate.map(|parent| &parent.directory))
                && !summary.may_have_children
                && !has_symlinks
            {
                // With one directory and no package at this name, only file precedence applies.
                let file = if summary.has_stub {
                    resolve_file_module_with_filter(
                        directory,
                        context,
                        &component_name,
                        ComponentFileFilter::ByMode,
                    )
                } else {
                    directory.resolve_file(context, &format_compact!("{component_name}.py"))
                };
                if let Some(file) = file {
                    listing.modules.push(Module::file_module(
                        db,
                        file,
                        context.resolver_environment,
                        Cow::Owned(name),
                        ModuleKind::Module,
                        directory.path().search_path().clone(),
                    ));
                }
                continue;
            }

            if let Some(candidates) = self.resolve_child(&component_name) {
                if let Some(candidate) = select_candidate_for_listing(candidates, is_listable) {
                    let module =
                        candidate.into_module(db, context.resolver_environment, Cow::Owned(name));
                    listing.modules.push(module);
                    if summary.may_have_children {
                        listing.modules_with_possible_children.push(module);
                    }
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

/// Summarizes entries that supply the same child module name across the directories being listed.
///
/// For example, `foo.py`, `foo.pyi`, and `foo/` contribute to one summary. Enumeration
/// uses this information to simplify resolution of the name and decide whether
/// to search for its children.
#[derive(Default)]
struct ChildNameSummary<'db> {
    may_have_children: bool,
    has_stub: bool,
    /// The sole search root containing this name, if discovered at the root.
    search_path: Option<&'db SearchPath>,
}

impl<'db> ChildNameSummary<'db> {
    /// Updates the summary with an entry supplying this child module name.
    fn record_entry(&mut self, entry: &str, kind: FileType, search_path: Option<&'db SearchPath>) {
        if self.search_path != search_path {
            self.search_path = None;
        }
        self.may_have_children |= kind != FileType::File;
        self.has_stub |= entry.strip_suffix(".pyi").is_some();
    }
}

/// Uses conservative checks to rule out descendants of a file module before reconstructing
/// its module search and resolving child names. Returning `true` means the full search
/// is still needed; it does not guarantee that a child resolves.
fn may_have_children(context: &ResolverContext, name: &ModuleName) -> bool {
    // With `acme.py` and a partial `acme-stubs/child.pyi`, `acme` resolves to a file
    // but `acme.child` still resolves to the stub. The directory-name check below looks
    // for `acme`, not `acme-stubs`, so environments containing stub packages need the full search.
    if context.mode.is_typing()
        && !stub_package_index(context.db, context.resolver_environment)
            .all()
            .is_empty()
    {
        return true;
    }

    let mode =
        ModuleResolveModeIngredient::new(context.db, context.resolver_environment, context.mode);
    let parent = name.parent().map(|parent| {
        ModuleNameIngredient::new(
            context.db,
            parent,
            context.mode,
            context.resolver_environment,
        )
    });
    child_directory_names(context.db, mode, parent)
        .binary_search_by(|child| child.as_str().cmp(name.last_component()))
        .is_ok()
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
fn select_candidate_for_listing<'db>(
    candidates: ResolvedNames<'db>,
    is_listable: impl Fn(&ModuleResolutionCandidate<'db>) -> bool,
) -> Option<ModuleResolutionCandidate<'db>> {
    let mut candidates = candidates.into_iter();
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

/// Directory names beneath this prefix across the configured search paths.
///
/// Sibling modules share this result. Adding a regular file to an existing directory
/// leaves the summary unchanged because the summary contains only directory names.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn child_directory_names<'db>(
    db: &'db dyn Db,
    mode: ModuleResolveModeIngredient<'db>,
    parent: Option<ModuleNameIngredient<'db>>,
) -> Box<[String]> {
    let context = ResolverContext::new(db, mode.resolver_environment(db), mode.mode(db));
    let mut names = BTreeSet::new();

    for search_path in search_paths(db, context.resolver_environment, context.mode) {
        let mut path = search_path.to_module_path();
        if let Some(parent) = parent {
            for component_name in parent.name(db).components() {
                path.push(component_name);
            }
        }

        let directory = ModuleDirectory::new(&context, path);
        directory.for_each_entry(db, |name, kind| {
            if matches!(kind, FileType::Directory | FileType::Symlink)
                && is_identifier(name)
                && directory.child_directory_path(&context, name).is_some()
            {
                names.insert(name.to_owned());
            }
        });
    }

    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem, SystemPath};

    use crate::ModuleName;
    use crate::db::tests::TestDb;
    use crate::resolve::{ModuleResolveMode, ResolverContext};
    use crate::testing::{enumeration_db, unresolved_stub_override_db, write_empty_file};
    #[cfg(target_family = "unix")]
    use crate::testing::{os_enumeration_db, symlink_enumeration_db};

    use super::{ListingTarget, list_modules};

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
        // The helper also checks that each listed module equals the resolved module.
        db.assert_listing(None, &["leaf", "shared"]);
    }

    #[test]
    fn excludes_local_files_with_protected_standard_library_names() {
        let db = enumeration_db(&["/src/leaf.py", "/src/sys.py"], &[]);
        // A local file cannot supply a protected name, even when typeshed omits it.
        db.assert_listing(None, &["leaf"]);
    }

    #[test]
    fn identifies_modules_that_may_have_descendants() {
        let mut db = enumeration_db(
            &[
                // A file without a corresponding directory cannot supply children.
                "/src/leaf.py",
                // A package remains eligible even when it currently has no children.
                "/src/pkg/__init__.py",
                // A file can have children supplied by a partial stub namespace.
                "/src/acme.py",
                "/site-packages/acme-stubs/child.pyi",
            ],
            &[],
        );
        db.write_file("/site-packages/acme-stubs/py.typed", "partial")
            .expect("mark the stub namespace as partial");
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let listing = list_modules(&context, &ListingTarget::Root);
        let possible_parents: Vec<_> = listing
            .modules_with_possible_children
            .iter()
            .map(|module| module.name(&db).as_str())
            .collect();
        // `leaf` cannot have children; `acme` can have children despite resolving to a file.
        assert_eq!(possible_parents, ["acme", "pkg"]);
    }

    #[test]
    fn enumerates_split_and_nested_namespaces() {
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
        db.assert_listing(None, &["acme"]);
        db.assert_listing(
            Some("acme"),
            &["acme.left", "acme.nested", "acme.regular", "acme.right"],
        );
        db.assert_listing(Some("acme.nested"), &["acme.nested.deep"]);
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
        db.assert_listing(Some("acme"), &["acme.module", "acme.package"]);
        db.assert_listing(Some("acme.package"), &["acme.package.local"]);
        db.assert_listing(Some("acme.module"), &[]);
    }

    #[test]
    fn excludes_namespace_portions_shadowed_by_concrete_parents() {
        for parent in ["/site-packages/acme/__init__.py", "/site-packages/acme.py"] {
            let db = enumeration_db(&["/src/acme/hidden.py", parent], &[]);
            db.assert_listing(None, &["acme"]);
            db.assert_listing(Some("acme"), &[]);
        }
    }

    #[test]
    fn merges_legacy_namespace_portions_until_shadowed() {
        let mut db = enumeration_db(&["/src/acme/left.py", "/site-packages/acme/right.py"], &[]);
        for init in ["/src/acme/__init__.py", "/site-packages/acme/__init__.py"] {
            db.write_file(
                init,
                "__path__ = __import__(\"pkgutil\").extend_path(__path__, __name__)",
            )
            .expect("write legacy declaration");
        }
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
        db.assert_listing(Some("acme"), &["acme.api"]);
        db.assert_listing(Some("acme.api"), &["acme.api.runtime", "acme.api.stubbed"]);
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
        db.assert_listing(Some("acme"), &["acme.runtime", "acme.stubbed"]);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn updates_stub_override_descendants_when_symlink_targets_change() {
        let (_temp, mut db, root) = os_enumeration_db(&["extra"]);
        write_empty_file(&mut db, root.join("src/leaf.py"));
        let extra = root.join("extra");

        // The extra path's entries stay unchanged as the symlink target appears and
        // disappears. The directory summary must track the target's status too.
        let target = root.join("stub_override");
        std::os::unix::fs::symlink(&target, extra.join("leaf"))
            .expect("create dangling stub override symlink");
        db.assert_listing(Some("leaf"), &[]);

        write_empty_file(&mut db, target.join("child.pyi"));
        Files::sync_all(&mut db);
        db.assert_listing(Some("leaf"), &["leaf.child"]);

        std::fs::remove_dir_all(&target).expect("remove stub override target");
        Files::sync_all(&mut db);
        db.assert_listing(Some("leaf"), &[]);
    }

    #[test]
    fn updates_stub_override_descendants_after_directory_changes() {
        // Exercise top-level modules and nested modules whose override parent may already exist.
        for (parent, existing_parent) in [("", true), ("acme", false), ("acme", true)] {
            let src = SystemPath::new("/src").join(parent);
            let extra = SystemPath::new("/extra").join(parent);
            let mut db = enumeration_db(
                &[
                    "/extra/unrelated.py",
                    "/site-packages/example-1.0.dist-info/METADATA",
                ],
                &["/extra"],
            );
            if !parent.is_empty() {
                write_empty_file(&mut db, src.join("__init__.py"));
            }
            // `assets.v1` must not count as a possible module directory.
            for file in ["api.py", "reports.py", "assets.v1/data.txt"] {
                write_empty_file(&mut db, src.join(file));
            }
            if existing_parent {
                // This unrelated stub keeps the override parent present throughout the test.
                write_empty_file(&mut db, extra.join("other.pyi"));
            }
            let api = qualified_name(parent, "api");
            let reports = qualified_name(parent, "reports");
            let stubbed = qualified_name(&api, "stubbed");
            let monthly = qualified_name(&reports, "monthly");
            db.assert_listing(Some(&api), &[]);
            db.assert_listing(Some(&reports), &[]);

            write_empty_file(&mut db, extra.join("api/stubbed.pyi"));
            db.assert_listing(Some(&api), &[&stubbed]);
            db.assert_listing(Some(&reports), &[]);

            write_empty_file(&mut db, extra.join("reports/monthly.pyi"));
            db.assert_listing(Some(&api), &[&stubbed]);
            db.assert_listing(Some(&reports), &[&monthly]);

            remove_file_and_parent(&mut db, &extra.join("api/stubbed.pyi"));
            db.assert_listing(Some(&api), &[]);
            db.assert_listing(Some(&reports), &[&monthly]);

            remove_file_and_parent(&mut db, &extra.join("reports/monthly.pyi"));
            if !existing_parent {
                db.memory_file_system()
                    .remove_directory(&extra)
                    .expect("remove the empty stub override directory");
                Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
            }
            db.assert_listing(Some(&api), &[]);
            db.assert_listing(Some(&reports), &[]);
        }
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
        let listing = list_modules(&context, &ListingTarget::ResolvedName(module));
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

    fn qualified_name(parent: &str, child: &str) -> String {
        if parent.is_empty() {
            child.to_owned()
        } else {
            format!("{parent}.{child}")
        }
    }

    fn remove_file_and_parent(db: &mut TestDb, path: &SystemPath) {
        db.memory_file_system()
            .remove_file(path)
            .expect("remove stub override");
        db.memory_file_system()
            .remove_directory(path.parent().expect("fixture parent"))
            .expect("remove empty stub override directory");
        Files::sync_all_recursive(db, [SystemPath::new("/extra")]);
    }

    impl TestDb {
        fn assert_listing(&self, name: Option<&str>, expected: &[&str]) {
            self.assert_listing_with_stub_overrides(name, expected, &[]);
        }

        fn assert_listing_with_stub_overrides(
            &self,
            name: Option<&str>,
            expected: &[&str],
            stub_override_prefixes: &[&str],
        ) {
            let target = match name {
                None => ListingTarget::Root,
                Some(name) => {
                    let name = ModuleName::new(name).expect("valid module name");
                    match crate::resolve_module_confident(self, self.resolver_environment(), &name)
                    {
                        Some(module) => ListingTarget::ResolvedName(module),
                        None => ListingTarget::UnresolvedName(name),
                    }
                }
            };
            let context =
                ResolverContext::new(self, self.resolver_environment(), ModuleResolveMode::Typing);
            let listing = list_modules(&context, &target);
            let names: Vec<_> = listing
                .modules
                .iter()
                .map(|module| {
                    let name = module.name(self);
                    assert_eq!(
                        Some(*module),
                        crate::resolve_module_confident(self, self.resolver_environment(), name),
                        "enumeration must agree with resolution for {name}"
                    );
                    name.as_str()
                })
                .collect();
            assert_eq!(names, expected);
            let names: Vec<_> = listing
                .stub_override_prefixes
                .iter()
                .map(|name| {
                    assert!(
                        crate::resolve_module_confident(self, self.resolver_environment(), name)
                            .is_none(),
                        "traversal prefix must not invent a resolved module"
                    );
                    name.as_str()
                })
                .collect();
            assert_eq!(names, stub_override_prefixes);
        }
    }
}
