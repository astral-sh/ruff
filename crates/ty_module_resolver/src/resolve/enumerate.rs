//! Discovers module names using ordinary resolution's precedence rules.
//!
//! Enumeration restricts directory symlinks so recursive traversal cannot follow cycles.
//! Search roots and their immediate children may be directory symlinks, allowing linked
//! environments and top-level package aliases. Below those locations, directory symlinks
//! are excluded. File symlinks are allowed at any depth because they do not introduce
//! another directory to traverse. Ordinary resolution can still resolve excluded locations.
//!
//! These restrictions apply after choosing the resolution winner. If a directory symlink
//! supplies the winning package, omitting it must not expose a shadowed file or package.
//! An implicit namespace can be listed when any of its portions is eligible.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;
use compact_str::CompactString;
use ruff_db::system::FileType;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::ModuleDirectory;

use super::search::{ModuleSearchCursor, RootSearchPaths};
use super::{
    ModuleNameIngredient, ModuleResolutionCandidate, ModuleResolveModeIngredient, ResolvedModule,
    ResolvedNames, ResolverContext, search_paths, stub_package_index,
};

/// Lists the immediate modules at the search cursor's position.
pub(crate) fn list_modules<'db>(
    context: &ResolverContext<'db>,
    search: &ModuleSearchCursor<'_, 'db>,
) -> ModuleListing<'db> {
    let db = context.db;
    let prefix = search.prefix();
    let is_listable_location =
        |candidate: &ModuleResolutionCandidate| candidate.is_listable_location(db);
    let mut names = BTreeMap::<_, ChildNameSummary>::new();
    let listable_directories: Vec<_> = search.listing_directories(is_listable_location).collect();
    for directory in &listable_directories {
        for entry in directory.entries(db) {
            if let Some(name) = entry.file_name()
                && let Some(name) = child_module_name(name, entry.file_type(), prefix.is_none())
            {
                // A file module can have descendants from a matching directory in
                // another location. Symlinks may also turn out to be directories.
                let summary = names.entry(CompactString::new(name)).or_default();
                summary.record_entry(entry.file_type());
            }
        }
    }

    // Check whether the resolved location is allowed by the listing policy.
    // Top-level locations are allowed. Below a prefix, reuse the checks for
    // directories already visited and their non-symlink children; otherwise,
    // check the candidate's full path.
    let is_listable_location = |candidate: &ModuleResolutionCandidate| {
        prefix.is_none()
            || listable_directories
                .iter()
                .any(|directory| directory.is_same_or_child_directory(&candidate.directory))
            || is_listable_location(candidate)
    };
    let mut listing = ModuleListing::default();

    for (component_name, summary) in names {
        let Some(name) = search.full_module_name(&component_name) else {
            continue;
        };

        if let Some(candidates) = search.resolve_child(&component_name) {
            if let Some(candidate) = select_candidate_for_listing(candidates, is_listable_location)
            {
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
        if let Some(child_search) = search.enter_package(&component_name)
            && child_search
                .listing_directories(is_listable_location)
                .next()
                .is_some()
        {
            listing.stub_override_prefixes.push(name);
        }
    }
    listing
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

/// Lists top-level modules across the configured search paths.
pub(crate) fn list_root_modules<'db>(context: &ResolverContext<'db>) -> ModuleListing<'db> {
    list_modules(
        context,
        &ModuleSearchCursor::with_configured_search_paths(context),
    )
}

/// Lists immediate submodules of a resolved module across the configured search paths.
pub(crate) fn list_submodules<'db>(
    context: &ResolverContext<'db>,
    module: Module<'db>,
) -> ModuleListing<'db> {
    let name = module.name(context.db);
    if module.kind(context.db) == ModuleKind::Module && !may_have_children(context, name) {
        return ModuleListing::default();
    }

    list_submodules_by_name(context, name)
}

/// Lists immediate submodules without requiring the parent name to resolve.
///
/// This allows enumeration to reach local stub overrides beneath unresolved prefixes.
pub(crate) fn list_submodules_by_name<'db>(
    context: &ResolverContext<'db>,
    name: &ModuleName,
) -> ModuleListing<'db> {
    let Some(search) = ModuleSearchCursor::for_prefix(context, name, &RootSearchPaths::Configured)
    else {
        return ModuleListing::default();
    };

    list_modules(context, &search)
}

/// Summarizes entries that supply the same child module name across the directories being listed.
///
/// For example, `foo.py`, `foo.pyi`, and `foo/` contribute to one summary. Enumeration
/// uses this information to decide whether to search for the module's children.
#[derive(Default)]
struct ChildNameSummary {
    may_have_children: bool,
}

impl ChildNameSummary {
    /// Updates the summary with an entry supplying this child module name.
    fn record_entry(&mut self, kind: FileType) {
        self.may_have_children |= kind != FileType::File;
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

    let parent = match name.parent() {
        Some(parent) => DirectoryParent::Prefix(ModuleNameIngredient::new(
            context.db,
            parent,
            context.mode,
            context.resolver_environment,
        )),
        None => DirectoryParent::Root(ModuleResolveModeIngredient::new(
            context.db,
            context.resolver_environment,
            context.mode,
        )),
    };
    child_directory_names(context.db, parent)
        .binary_search_by(|child| child.as_str().cmp(name.last_component()))
        .is_ok()
}

/// Selects the resolved module for listing if its location is allowed by the listing policy.
/// An implicit namespace can be listed if any of its portions is allowed.
///
/// This method assumes that `candidates` is ordered by resolution precedence
/// (which ordinarily happens through `resolve::normalize_candidates`).
fn select_candidate_for_listing<'db>(
    candidates: ResolvedNames<'db>,
    is_listable_location: impl Fn(&ModuleResolutionCandidate<'db>) -> bool,
) -> Option<ModuleResolutionCandidate<'db>> {
    let mut candidates = candidates.into_iter();

    // Select the first candidate from the ordered list (i.e., the same winner
    // as ordinary resolution) before applying the listing policy so that
    // excluding a concrete module cannot expose a lower-priority alternative.
    let selected = candidates.next()?;

    let can_list = match selected.module {
        // An implicit namespace has no defining file, so any eligible portion suffices.
        ResolvedModule::NamespacePackage => {
            is_listable_location(&selected)
                || candidates.any(|candidate| is_listable_location(&candidate))
        }
        // Concrete modules, including legacy namespaces, must use the selected location.
        ResolvedModule::Package(_) | ResolvedModule::Module(_) => is_listable_location(&selected),
    };
    can_list.then_some(selected)
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
fn child_directory_names<'db>(db: &'db dyn Db, parent: DirectoryParent<'db>) -> Box<[String]> {
    let (resolver_environment, mode, parent) = match parent {
        DirectoryParent::Root(mode) => (mode.resolver_environment(db), mode.mode(db), None),
        DirectoryParent::Prefix(parent) => (
            parent.resolver_environment(db),
            parent.mode(db),
            Some(parent.name(db)),
        ),
    };
    let context = ResolverContext::new(db, resolver_environment, mode);
    let relative_path: Utf8PathBuf = parent
        .map(|parent| parent.components().collect())
        .unwrap_or_default();
    let mut names = BTreeSet::new();

    for search_path in search_paths(db, context.resolver_environment, context.mode) {
        let directory =
            ModuleDirectory::from_parts(&context, search_path.clone(), relative_path.clone());
        for entry in directory.entries(db) {
            if matches!(entry.file_type(), FileType::Directory | FileType::Symlink)
                && let Some(name) = entry.file_name()
                && is_identifier(name)
                && directory.child_directory_path(&context, name).is_some()
            {
                names.insert(name.to_owned());
            }
        }
    }

    names.into_iter().collect()
}

/// Reuses the root or prefix ingredient without interning another combined query key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Supertype)]
enum DirectoryParent<'db> {
    Root(ModuleResolveModeIngredient<'db>),
    Prefix(ModuleNameIngredient<'db>),
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem, SystemPath};

    use crate::ModuleName;
    use crate::db::tests::TestDb;
    use crate::resolve::{ModuleResolveMode, ResolverContext};
    use crate::testing::{
        ModuleDebugSnapshot, TestCaseBuilder, unresolved_stub_override_db, write_empty_file,
    };
    #[cfg(target_family = "unix")]
    use crate::testing::{os_enumeration_db, symlink_enumeration_db};

    use super::{ModuleListing, list_root_modules, list_submodules, list_submodules_by_name};

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
    fn identifies_modules_that_may_have_descendants() {
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[
                // A file without a corresponding directory cannot supply children.
                ("leaf.py", ""),
                // A package remains eligible even when it currently has no children.
                ("pkg/__init__.py", ""),
                // A file can have children supplied by a partial stub namespace.
                ("acme.py", ""),
            ])
            .with_site_packages_files(&[("acme-stubs/child.pyi", "")])
            .build()
            .db;
        db.write_file("/site-packages/acme-stubs/py.typed", "partial")
            .expect("mark the stub namespace as partial");
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let listing = list_root_modules(&context);
        let possible_parents: Vec<_> = listing
            .modules_with_possible_children
            .iter()
            .map(|module| module.name(&db).as_str())
            .collect();
        // `leaf` cannot have children; `acme` can have children despite resolving to a file.
        assert_eq!(possible_parents, ["acme", "pkg"]);
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
    fn merges_legacy_namespace_portions_until_shadowed() {
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

        write_empty_file(&mut db, "/site-packages/acme/__init__.py");
        ListingCase::for_name("acme")
            .expect_module("acme.right")
            .assert(&db);
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
        ListingCase::for_name("leaf").assert(&db);

        write_empty_file(&mut db, target.join("child.pyi"));
        Files::sync_all(&mut db);
        ListingCase::for_name("leaf")
            .expect_module("leaf.child")
            .assert(&db);

        std::fs::remove_dir_all(&target).expect("remove stub override target");
        Files::sync_all(&mut db);
        ListingCase::for_name("leaf").assert(&db);
    }

    #[test]
    fn updates_top_level_stub_override_descendants_after_directory_changes() -> anyhow::Result<()> {
        // The extra search root exists before the override directory is created.
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[("api.py", "")])
            .with_extra_path("/extra", &[("unrelated.py", "")])
            .build()
            .db;
        ListingCase::for_name("api").assert(&db);

        db.write_file("/extra/api/stubbed.pyi", "")?;
        ListingCase::for_name("api")
            .expect_module("api.stubbed")
            .assert(&db);

        db.memory_file_system()
            .remove_file("/extra/api/stubbed.pyi")?;
        db.memory_file_system().remove_directory("/extra/api")?;
        Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
        ListingCase::for_name("api").assert(&db);

        Ok(())
    }

    #[test]
    fn updates_nested_stub_override_descendants_when_parent_is_created() -> anyhow::Result<()> {
        // `/extra` exists, but `/extra/acme` does not.
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[("acme/__init__.py", ""), ("acme/api.py", "")])
            .with_extra_path("/extra", &[("unrelated.py", "")])
            .build()
            .db;
        ListingCase::for_name("acme.api").assert(&db);

        db.write_file("/extra/acme/api/stubbed.pyi", "")?;
        ListingCase::for_name("acme.api")
            .expect_module("acme.api.stubbed")
            .assert(&db);

        db.memory_file_system()
            .remove_file("/extra/acme/api/stubbed.pyi")?;
        db.memory_file_system()
            .remove_directory("/extra/acme/api")?;
        db.memory_file_system().remove_directory("/extra/acme")?;
        Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
        ListingCase::for_name("acme.api").assert(&db);

        Ok(())
    }

    #[test]
    fn updates_nested_stub_override_descendants_with_existing_parent() -> anyhow::Result<()> {
        // The sibling override keeps `/extra/acme` present throughout the test.
        let mut db = TestCaseBuilder::new()
            .with_src_files(&[
                ("acme/__init__.py", ""),
                ("acme/api.py", ""),
                ("acme/reports.py", ""),
            ])
            .with_extra_path("/extra", &[("acme/reports/monthly.pyi", "")])
            .build()
            .db;
        ListingCase::for_name("acme.api").assert(&db);
        ListingCase::for_name("acme.reports")
            .expect_module("acme.reports.monthly")
            .assert(&db);

        db.write_file("/extra/acme/api/stubbed.pyi", "")?;
        ListingCase::for_name("acme.api")
            .expect_module("acme.api.stubbed")
            .assert(&db);

        db.memory_file_system()
            .remove_file("/extra/acme/api/stubbed.pyi")?;
        db.memory_file_system()
            .remove_directory("/extra/acme/api")?;
        Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
        ListingCase::for_name("acme.api").assert(&db);
        ListingCase::for_name("acme.reports")
            .expect_module("acme.reports.monthly")
            .assert(&db);

        Ok(())
    }

    #[test]
    fn separates_unresolved_stub_override_prefixes_from_modules() {
        let db = unresolved_stub_override_db();
        ListingCase::root().expect_module("acme").assert(&db);
        ListingCase::for_name("acme")
            .expect_stub_override_prefix("acme.nested")
            .assert(&db);
        ListingCase::for_name("acme.nested")
            .expect_stub_override_prefix("acme.nested.deep")
            .assert(&db);
        ListingCase::for_name("acme.nested.deep")
            .expect_module("acme.nested.deep.tools")
            .assert(&db);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn enumerates_file_aliases_and_top_level_directory_aliases() {
        let (_temp, db, _root) = symlink_enumeration_db();
        ListingCase::root()
            .expect_modules(&["acme", "alias", "regular_alias", "top_alias"])
            .assert(&db);
        ListingCase::for_name("acme")
            .expect_modules(&["acme.hidden", "acme.ns", "acme.own", "acme.stubbed"])
            .assert(&db);
        ListingCase::for_name("acme.ns")
            .expect_module("acme.ns.visible")
            .assert(&db);
        ListingCase::for_name("alias")
            .expect_modules(&["alias.hidden", "alias.own", "alias.stubbed"])
            .assert(&db);
        ListingCase::for_name("regular_alias")
            .expect_modules(&[
                "regular_alias.child",
                "regular_alias.linked",
                "regular_alias.nested",
            ])
            .assert(&db);
        ListingCase::for_name("regular_alias.nested")
            .expect_module("regular_alias.nested.child")
            .assert(&db);
        for name in ["acme.blocked", "acme.blocked.nested", "regular_alias.loop"] {
            crate::resolve_module_confident(
                &db,
                db.resolver_environment(),
                &ModuleName::new(name).expect("valid module name"),
            )
            .expect("package resolves");
            ListingCase::for_name(name).assert(&db);
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
        ListingCase::for_name("acme").assert(&db);
        write_empty_file(&mut db, root.join("src/acme/__init__.py"));
        ListingCase::for_name("acme").assert(&db);
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

    /// An enumeration target and the modules and unresolved prefixes expected beneath it.
    struct ListingCase<'a> {
        parent_module_name: Option<&'a str>,
        expected_module_names: Vec<&'a str>,
        expected_stub_override_prefixes: Vec<&'a str>,
    }

    impl<'a> ListingCase<'a> {
        fn root() -> Self {
            Self {
                parent_module_name: None,
                expected_module_names: Vec::new(),
                expected_stub_override_prefixes: Vec::new(),
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

        fn expect_stub_override_prefix(mut self, name: &'a str) -> Self {
            self.expected_stub_override_prefixes.push(name);
            self
        }

        /// Formats listed modules after checking that they agree with ordinary resolution.
        #[track_caller]
        fn snapshot<'db>(&self, db: &'db TestDb) -> Vec<ModuleDebugSnapshot<'db>> {
            let listing = self.list_modules(db);
            assert_eq!(listing.stub_override_prefixes, Vec::<ModuleName>::new());
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

            let stub_override_prefixes: Vec<_> = listing
                .stub_override_prefixes
                .iter()
                .map(ModuleName::as_str)
                .collect();
            assert_eq!(stub_override_prefixes, self.expected_stub_override_prefixes);
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
            for name in &listing.stub_override_prefixes {
                assert!(
                    crate::resolve_module_confident(db, db.resolver_environment(), name).is_none(),
                    "traversal prefix must not invent a resolved module"
                );
            }
            listing
        }
    }
}
