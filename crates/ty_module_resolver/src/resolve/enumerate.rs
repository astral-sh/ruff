use std::collections::BTreeSet;

use ruff_db::files::directory_listing;
use ruff_db::system::{FileType, SystemPath};
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::ModuleDirectory;

use super::search::ModuleSearch;
use super::{
    ModuleNameIngredient, ModuleResolutionCandidate, ModuleResolveModeIngredient, NameResolver,
    ResolvedModule, ResolverContext, search_paths, stub_package_index,
};

impl<'db> NameResolver<'db> {
    /// Enumerates immediate child modules of a complete prefix, or top-level modules for `None`.
    ///
    /// Directory entries supply possible names; resolution selects each name before module enumeration
    /// applies its eligibility rules. Unresolved stub override prefixes are returned separately so
    /// recursive enumeration can visit them without offering them as importable modules.
    /// The resolved module, when available, supplies its fallback search path.
    /// Its package ancestry may contain symlinks; other namespace portions and stub overrides
    /// keep their normal checks.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Module enumeration is consumed by the next change's cached listings"
        )
    )]
    pub(super) fn enumerate_modules(
        &self,
        prefix: Option<&ModuleName>,
        module: Option<Module<'db>>,
    ) -> ModuleEnumeration<'db> {
        let context = &self.context;
        let db = context.db;
        let search_path = module
            .and_then(|module| module.search_path(db))
            .filter(|path| {
                !search_paths(db, context.resolver_environment, context.mode)
                    .any(|configured| configured == *path)
            });
        if search_path.is_none()
            && let Some(module) = module
            && prefix == Some(module.name(db))
            && module.kind(db) == ModuleKind::Module
            && !may_have_children(context, module.name(db))
        {
            return ModuleEnumeration::default();
        }
        let Some(search) = prefix.map_or_else(
            || Some(ModuleSearch::new(self)),
            |prefix| ModuleSearch::for_prefix(self, prefix, search_path),
        ) else {
            return ModuleEnumeration::default();
        };
        let eligible =
            |candidate: &ModuleResolutionCandidate| candidate_is_eligible(self, module, candidate);
        let mut names = BTreeSet::new();
        let mut collect = |directory: &ModuleDirectory| {
            for_each_entry(db, directory, |name, kind| {
                add_child_name(&mut names, name, kind, prefix.is_none());
            });
        };
        if prefix.is_none() {
            for path in search_paths(db, context.resolver_environment, context.mode) {
                collect(&ModuleDirectory::new(context, path.to_module_path()));
            }
        } else {
            for candidate in search.candidates().filter(|candidate| {
                !matches!(candidate.module, ResolvedModule::Module(_)) && eligible(candidate)
            }) {
                collect(&candidate.directory);
            }
        }
        let mut children = ModuleEnumeration::default();
        for component_name in names {
            db.unwind_if_revision_cancelled();
            let Some(name) = search.full_module_name(&component_name) else {
                continue;
            };
            if let Some(candidates) = search.resolve_child(&component_name) {
                let mut candidates = candidates.into_iter();
                // An implicit namespace has no single defining location. Any eligible portion can
                // supply the module. Concrete modules must use the selected location.
                if let Some(candidate) = candidates.next()
                    && (eligible(&candidate)
                        || matches!(candidate.module, ResolvedModule::NamespacePackage)
                            && candidates.any(|candidate| eligible(&candidate)))
                {
                    children.modules.push(candidate.into_module(
                        db,
                        context.resolver_environment,
                        &name,
                    ));
                }
                // An excluded module still shadows other candidates. Only failed resolution
                // permits an unresolved stub override prefix.
                continue;
            }

            // The full search found no module, so any remaining prefix candidates belong
            // to the stub override search: `acme.nested` may lead to `acme/nested/tools.pyi`
            // even when installed stubs omit `acme.nested`.
            if let Some(child_search) = search.enter_package(&component_name)
                && child_search.candidates().any(|candidate| {
                    !matches!(candidate.module, ResolvedModule::Module(_)) && eligible(candidate)
                })
            {
                children.stub_override_prefixes.push(name);
            }
        }
        children
    }
}

/// Resolved immediate child modules and unresolved module name prefixes for stub overrides.
///
/// Installed stubs can omit `acme.nested` while a stub override supplies `acme.nested.tools`.
/// Recursive enumeration searches that prefix, but import-statement completion omits it.
#[derive(Default)]
pub(super) struct ModuleEnumeration<'db> {
    /// Modules that resolve independently and are eligible for enumeration.
    pub(super) modules: Vec<Module<'db>>,
    /// Unresolved module name prefixes with eligible stub override candidates.
    pub(super) stub_override_prefixes: Vec<ModuleName>,
}

/// Checks for possible descendant directories without resolving the prefix's ancestors.
/// Finding a directory is only a reason to search; it may still be shadowed.
fn may_have_children(context: &ResolverContext, name: &ModuleName) -> bool {
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

/// Enumeration follows top-level symlinks and the path to a resolved package, but excludes
/// symlinks below either starting point. Resolution still keeps these candidates so they
/// can shadow other locations.
fn candidate_is_eligible(
    resolver: &NameResolver,
    module: Option<Module>,
    candidate: &ModuleResolutionCandidate,
) -> bool {
    let db = resolver.context.db;
    let Some(root) = candidate.directory.path().search_path().as_system_path() else {
        return true;
    };
    let path = match candidate.module {
        ResolvedModule::Module(file) => file.path(db).as_system_path().map(SystemPath::to_path_buf),
        _ => candidate.directory.path().to_system_path(),
    };
    let Some(path) = path else { return false };
    // Exempt the resolved package's ancestry only for locations beneath its directory.
    // Other namespace portions and stub overrides keep their full ancestry checks.
    let package_directory = module
        .filter(|module| module.kind(db) == ModuleKind::Package)
        .and_then(|module| module.file(db))
        .and_then(|file| file.path(db).as_system_path())
        .and_then(SystemPath::parent)
        .filter(|package| path.starts_with(package));
    let root = package_directory.unwrap_or(root);
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut parent = root.to_path_buf();
    for (depth, component) in relative.components().enumerate() {
        if (package_directory.is_some() || depth > 0)
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

fn add_child_name(
    names: &mut BTreeSet<String>,
    entry: &str,
    file_type: FileType,
    at_search_root: bool,
) {
    if !at_search_root
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
    let name = if at_search_root {
        name.strip_suffix("-stubs").unwrap_or(name)
    } else {
        name
    };
    if is_identifier(name) {
        names.insert(name.to_owned());
    }
}

/// Directory names beneath this prefix across the configured search paths.
///
/// Sibling modules share this result. Adding a file to an existing directory
/// leaves the summary unchanged. Resolution still decides whether a listed directory can
/// supply descendants; this query does not apply shadowing or eligibility rules.
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
        let directory = ModuleDirectory::new(&context, path.clone());
        for_each_entry(db, &directory, |name, kind| {
            if matches!(kind, FileType::Directory | FileType::Symlink) && is_identifier(name) {
                let mut child = path.clone();
                child.push(name);
                // Follow directory symlinks and record target-status dependencies.
                // Keep typeshed's Python-version availability checks.
                if child.is_directory(&context) {
                    names.insert(name.to_owned());
                }
            }
        });
    }
    names.into_iter().collect()
}

fn for_each_entry(db: &dyn Db, directory: &ModuleDirectory, mut visit: impl FnMut(&str, FileType)) {
    if let Some(listing) = directory.system_listing() {
        for (name, kind) in listing.iter() {
            db.unwind_if_revision_cancelled();
            visit(name, kind);
        }
    } else if let Some(path) = directory.path().to_vendored_path() {
        for entry in db.vendored().read_directory(&path) {
            if let Some(name) = entry.path().file_name() {
                let kind = if entry.file_type().is_directory() {
                    FileType::Directory
                } else {
                    FileType::File
                };
                visit(name, kind);
            }
        }
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
        MockedTypeshed, TestCase, TestCaseBuilder, enumeration_db, unresolved_stub_override_db,
        write_empty_file,
    };
    use crate::{ImportingFile, ModuleName};

    use super::NameResolver;

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
    fn namespace_children_use_resolution_precedence() {
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
    }

    #[test]
    fn concrete_parents_shadow_namespace_portions() {
        for parent in ["/site-packages/acme/__init__.py", "/site-packages/acme.py"] {
            let db = enumeration_db(&["/src/acme/hidden.py", parent], &[]);
            assert_children(&db, None, &["acme"], &[]);
            assert_children(&db, Some("acme"), &[], &[]);
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
        .expect("preserve partial stub namespace alongside ordinary packages");
        assert_children(&db, Some("acme"), &["acme.api"], &[]);
        assert_children(
            &db,
            Some("acme.api"),
            &["acme.api.runtime", "acme.api.stubbed"],
            &[],
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
    }

    #[test]
    fn partial_stub_descendants_of_source_module() {
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
    }

    #[test]
    fn stub_override_and_source_siblings() {
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
    }

    #[test]
    fn top_level_stub_override_descendants_appear_and_disappear() {
        let mut db = enumeration_db(
            &[
                "/src/leaf.py",
                "/extra/unrelated.py",
                "/site-packages/example-1.0.dist-info/METADATA",
            ],
            &["/extra"],
        );
        assert_children(&db, Some("leaf"), &[], &[]);

        write_empty_file(&mut db, "/extra/leaf/stubbed.pyi");
        assert_children(&db, Some("leaf"), &["leaf.stubbed"], &[]);

        db.memory_file_system()
            .remove_file("/extra/leaf/stubbed.pyi")
            .expect("remove the stub override descendant");
        db.memory_file_system()
            .remove_directory("/extra/leaf")
            .expect("remove the empty stub override directory");
        Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
        assert_children(&db, Some("leaf"), &[], &[]);
    }

    #[test]
    fn stub_override_descendants_appear_and_disappear() {
        // A stub override can introduce `acme/` or add `api/` beneath an existing `acme/`.
        for existing_top_level in [false, true] {
            let mut db = enumeration_db(
                &[
                    "/src/acme/__init__.py",
                    "/src/acme/api.py",
                    "/src/acme/reports.py",
                    "/src/acme/assets.v1/data.txt",
                    "/extra/unrelated.py",
                ],
                &["/extra"],
            );
            if existing_top_level {
                write_empty_file(&mut db, "/extra/acme/other.pyi");
            }
            assert_children(&db, Some("acme.api"), &[], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);

            write_empty_file(&mut db, "/extra/acme/api/stubbed.pyi");
            assert_children(&db, Some("acme.api"), &["acme.api.stubbed"], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);

            write_empty_file(&mut db, "/extra/acme/reports/monthly.pyi");
            assert_children(&db, Some("acme.api"), &["acme.api.stubbed"], &[]);
            assert_children(&db, Some("acme.reports"), &["acme.reports.monthly"], &[]);

            db.memory_file_system()
                .remove_file("/extra/acme/api/stubbed.pyi")
                .expect("remove the stub override descendant");
            db.memory_file_system()
                .remove_directory("/extra/acme/api")
                .expect("remove the empty stub override subpackage");
            Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
            assert_children(&db, Some("acme.api"), &[], &[]);
            assert_children(&db, Some("acme.reports"), &["acme.reports.monthly"], &[]);

            db.memory_file_system()
                .remove_file("/extra/acme/reports/monthly.pyi")
                .expect("remove the sibling's stub override");
            db.memory_file_system()
                .remove_directory("/extra/acme/reports")
                .expect("remove the empty sibling override directory");
            if !existing_top_level {
                db.memory_file_system()
                    .remove_directory("/extra/acme")
                    .expect("remove the empty stub override directory");
            }
            Files::sync_all_recursive(&mut db, [SystemPath::new("/extra")]);
            assert_children(&db, Some("acme.api"), &[], &[]);
            assert_children(&db, Some("acme.reports"), &[], &[]);
        }
    }

    #[test]
    fn unresolved_stub_override_prefixes_are_not_modules() {
        let db = unresolved_stub_override_db();
        assert_children(&db, None, &["acme"], &[]);
        assert_children(&db, Some("acme"), &[], &["acme.nested"]);
        assert_children(&db, Some("acme.nested"), &[], &["acme.nested.deep"]);
        assert_children(
            &db,
            Some("acme.nested.deep"),
            &["acme.nested.deep.tools"],
            &[],
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
        let children = resolver.enumerate_modules(Some(&name), Some(package));
        assert_eq!(
            children
                .modules
                .iter()
                .map(|module| module.name(&db).as_str())
                .collect::<Vec<_>>(),
            ["acme.child"]
        );
        assert!(children.stub_override_prefixes.is_empty());
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
            assert_children(&db, Some(name), &expected, &[]);
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
        for module in [None, Some(package)] {
            let children = resolver.enumerate_modules(Some(&name), module);
            assert!(children.stub_override_prefixes.is_empty());
            assert_eq!(children.modules.len(), 1);
            let child = children.modules[0];
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
    }

    fn assert_children(
        db: &TestDb,
        prefix: Option<&str>,
        expected: &[&str],
        stub_override_prefixes: &[&str],
    ) {
        let prefix = prefix.map(|name| ModuleName::new(name).expect("valid module name prefix"));
        let resolver = NameResolver::new(db, db.resolver_environment(), ModuleResolveMode::Typing);
        let module = prefix
            .as_ref()
            .and_then(|name| crate::resolve_module_confident(db, db.resolver_environment(), name));
        let children = resolver.enumerate_modules(prefix.as_ref(), module);
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
            .stub_override_prefixes
            .iter()
            .map(|name| {
                assert!(
                    crate::resolve_module_confident(db, db.resolver_environment(), name).is_none(),
                    "traversal prefix must not invent a resolved module"
                );
                name.as_str()
            })
            .collect();
        assert_eq!(names, stub_override_prefixes);
    }
}
