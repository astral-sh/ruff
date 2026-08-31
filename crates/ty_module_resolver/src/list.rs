use crate::ResolverEnvironment;
use crate::db::Db;
use crate::module::Module;
use crate::module_name::ModuleName;
use crate::resolve::{DiscoveredChildren, ModuleNameIngredient, ModuleResolveMode, NameResolver};

/// List all available modules, including all sub-modules, sorted in lexicographic order.
pub fn all_modules<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> Vec<Module<'db>> {
    let mut modules = Vec::new();
    let mut stack = vec![list_modules_impl(db, resolver_environment)];
    while let Some(children) = stack.pop() {
        for &module in &children.modules {
            modules.push(module);
            stack.push(module.children(db));
        }
        // Overlay prefixes can lead to resolved descendants without resolving independently.
        // Only the descendants belong in the resulting module list.
        for prefix in &children.overlay_prefixes {
            let name = ModuleNameIngredient::new(
                db,
                prefix,
                ModuleResolveMode::Typing,
                resolver_environment,
            );
            stack.push(overlay_children(db, name));
        }
    }
    modules.sort_by_key(|module| module.name(db));
    modules
}

/// Cached child discovery, keeping unresolved overlay prefixes separate from public module lists.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct ModuleChildren<'db> {
    /// Resolved immediate children, suitable for import-statement completion.
    pub(crate) modules: Box<[Module<'db>]>,
    /// Prefixes used only by recursive discovery; see [`DiscoveredChildren`].
    overlay_prefixes: Box<[ModuleName]>,
}

impl<'db> From<DiscoveredChildren<'db>> for ModuleChildren<'db> {
    fn from(children: DiscoveredChildren<'db>) -> Self {
        Self {
            modules: children.modules.into_boxed_slice(),
            overlay_prefixes: children.overlay_prefixes.into_boxed_slice(),
        }
    }
}

/// List all available top-level modules.
pub fn list_modules<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> &'db [Module<'db>] {
    &list_modules_impl(db, resolver_environment).modules
}

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn list_modules_impl<'db>(
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
) -> ModuleChildren<'db> {
    NameResolver::new(db, resolver_environment, ModuleResolveMode::Typing)
        .children(None)
        .into()
}

/// Cache each unresolved prefix by its name and environment, just as resolved packages cache
/// their children by `Module`. No candidate state survives the child-list computation.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn overlay_children<'db>(db: &'db dyn Db, name: ModuleNameIngredient<'db>) -> ModuleChildren<'db> {
    NameResolver::new(db, name.resolver_environment(db), name.mode(db))
        .children(Some(name.name(db)))
        .into()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "These are tests, so it's fine to do I/O by-passing System."
    )]

    use camino::{Utf8Component, Utf8Path};
    use ruff_db::Db as _;
    use ruff_db::files::{File, FilePath, FileRootKind};
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::PythonVersion;

    use crate::db::{Db, tests::TestDb};
    use crate::module::Module;
    use crate::module_name::ModuleName;
    use crate::resolve::{
        ModuleResolveMode, ModuleResolveModeIngredient, dynamic_resolution_paths,
    };
    use crate::settings::SearchPathSettings;
    use crate::strategy::FallibleStrategy;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    fn list_modules(db: &TestDb) -> &[Module<'_>] {
        super::list_modules(db, db.resolver_environment())
    }

    struct ModuleDebugSnapshot<'db> {
        db: &'db dyn Db,
        module: Module<'db>,
    }

    impl std::fmt::Debug for ModuleDebugSnapshot<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self.module {
                Module::Namespace(pkg) => {
                    write!(f, "Module::Namespace({name:?})", name = pkg.name(self.db))
                }
                Module::File(module) => {
                    // For snapshots, just normalize all paths to using
                    // Unix slashes for simplicity.
                    let path_components = match module.file(self.db).path(self.db) {
                        FilePath::System(path) => path.components(),
                        FilePath::Vendored(path) => path.components(),
                        FilePath::SystemVirtual(path) => Utf8Path::new(path.as_str()).components(),
                    };
                    let nice_path = path_components
                        // Avoid including a root component, since that
                        // results in a platform dependent separator.
                        // Convert to an empty string so that we get a
                        // path beginning with `/` regardless of platform.
                        .map(|component| {
                            if let Utf8Component::RootDir = component {
                                Utf8Component::Normal("")
                            } else {
                                component
                            }
                        })
                        .map(|component| component.as_str())
                        .collect::<Vec<&str>>()
                        .join("/");
                    write!(
                        f,
                        "Module::File({name:?}, {search_path:?}, {path:?}, {kind:?}, {known:?})",
                        name = module.name(self.db).as_str(),
                        search_path = module.search_path(self.db).debug_kind(),
                        path = nice_path,
                        kind = module.kind(self.db),
                        known = module.known(self.db),
                    )
                }
            }
        }
    }

    fn sorted_list(db: &TestDb) -> Vec<Module<'_>> {
        let mut modules = list_modules(db).to_vec();
        modules.sort_by(|m1, m2| m1.name(db).cmp(m2.name(db)));
        modules
    }

    fn list_snapshot(db: &TestDb) -> Vec<ModuleDebugSnapshot<'_>> {
        list_snapshot_filter(db, |_| true)
    }

    fn list_snapshot_filter<'db>(
        db: &'db TestDb,
        predicate: impl Fn(&Module<'db>) -> bool,
    ) -> Vec<ModuleDebugSnapshot<'db>> {
        sorted_list(db)
            .into_iter()
            .filter(predicate)
            .map(|module| ModuleDebugSnapshot { db, module })
            .collect()
    }

    #[test]
    fn first_party_module() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn stubs_over_module_source() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", ""), ("foo.pyi", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.pyi", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn stubs_over_package_source() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", ""), ("foo.pyi", "")])
            .build();

        // NOTE: This matches the behavior of the "resolve this module"
        // implementation, even though it seems inconsistent with the
        // `stubs_over_module_source` test.
        //
        // TODO: Check what other type checkers do. It seems like this (and
        // "resolve this module") should prefer the stub file, although the
        // typing spec isn't perfectly clear on this point:
        // https://typing.python.org/en/latest/spec/distributing.html#stub-files
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    /// Tests that if we have a `foo.py` and a `foo/__init__.py`, then the
    /// latter takes precedence.
    ///
    /// This is somewhat difficult to test using the in-memory file system,
    /// since it always returns directory entries in lexicographic order. This
    /// in turn implies that `foo` will always appear before `foo.py`. But to
    /// truly test this, we would like to also be correct in the case where
    /// `foo.py` appears before `foo` (which can certainly happen in the real
    /// world).
    #[test]
    fn package_over_module1() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", ""), ("foo/__init__.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    /// Similar to `package_over_module1`, but flips the order of files.
    ///
    /// (At time of writing, 2025-08-07, this doesn't actually make a
    /// difference since the in-memory file system sorts directory entries.)
    #[test]
    fn package_over_module2() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", ""), ("foo.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn builtins_vendored() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_vendored_typeshed()
            .with_src_files(&[("builtins.py", "FOOOO = 42")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "builtins"),
            @r#"
        [
            Module::File("builtins", "std-vendored", "stdlib/builtins.pyi", Module, Some(Builtins)),
        ]
        "#,
        );
    }

    #[test]
    fn ty_extensions_vendored() {
        let TestCase { db, .. } = TestCaseBuilder::new().with_vendored_typeshed().build();

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |module| module.name(&db).as_str() == "ty_extensions"),
            @r#"
        [
            Module::File("ty_extensions", "std-vendored", "stdlib/ty_extensions/__init__.pyi", Package, Some(TyExtensions)),
        ]
        "#,
        );
    }

    #[test]
    fn builtins_custom() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("builtins.pyi", "def min(a, b): ...")],
            versions: "builtins: 3.8-",
        };

        const SRC: &[FileSpec] = &[("builtins.py", "FOOOO = 42")];

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("builtins", "std-custom", "/typeshed/stdlib/builtins.pyi", Module, Some(Builtins)),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            functools: 3.8-             # Top-level single-file module
            random: 3.8-                # 'Regular' file module on py38+
            xml: 3.8-3.8                # Namespace package on py38 only
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("functools.pyi", ""),
            ("random.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        // NOTE: This currently doesn't return `xml` since
        // the implementation assumes that typeshed doesn't
        // have namespace packages. But our test setup (copied
        // from the "resolve this module" tests) does.
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("asyncio", "std-custom", "/typeshed/stdlib/asyncio/__init__.pyi", Package, None),
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
            Module::File("random", "std-custom", "/typeshed/stdlib/random.pyi", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_nonexisting_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            importlib: 3.9-             # Namespace package on py39+
            random: 3.9-                # 'Regular' file module on py39+
            xml: 3.8-3.8                # Namespace package on 3.8 only
            foo: 3.9-
        ";

        const STDLIB: &[FileSpec] = &[
            ("collections/__init__.pyi", ""),
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("importlib/abc.pyi", ""),
            ("random.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        // NOTE: This currently doesn't return any of the namespace
        // packages defined above in our mock typeshed (that is,
        // `importlib` and `xml`) because our implementation assumes
        // namespace packages cannot occur in typeshed.
        //
        // Relatedly, `collections` and `random` should not appear
        // because they are limited to 3.9+.
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("asyncio", "std-custom", "/typeshed/stdlib/asyncio/__init__.pyi", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py39_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            functools: 3.8-             # Top-level single-file module
            importlib: 3.9-             # Namespace package on py39+
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("collections/__init__.pyi", ""),
            ("functools.pyi", ""),
            ("importlib/abc.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY39)
            .build();

        // NOTE: This currently doesn't return any of the namespace
        // packages defined above in our mock typeshed (that is,
        // `importlib`) because our implementation assumes namespace
        // packages cannot occur in typeshed.
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("asyncio", "std-custom", "/typeshed/stdlib/asyncio/__init__.pyi", Package, None),
            Module::File("collections", "std-custom", "/typeshed/stdlib/collections/__init__.pyi", Package, Some(Collections)),
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py39_nonexisting_modules() {
        const VERSIONS: &str = "\
            importlib: 3.9-   # 'Regular' package on py39+
            xml: 3.8-3.8      # 'Regular' package on 3.8 only
        ";

        // Since our implementation assumes typeshed doesn't contain
        // any namespace packages (as an optimization), this test case
        // is modified from the corresponding test in the "resolve a
        // file" implementation so that both namespace packages are
        // just regular packages. ---AG
        const STDLIB: &[FileSpec] = &[
            ("importlib/__init__.pyi", ""),
            ("importlib/abc.pyi", ""),
            ("xml/__init__.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY39)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("importlib", "std-custom", "/typeshed/stdlib/importlib/__init__.pyi", Package, Some(ImportLib)),
        ]
        "#,
        );
    }

    #[test]
    fn first_party_precedence_over_stdlib() {
        const SRC: &[FileSpec] = &[("functools.py", "def update_wrapper(): ...")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "first-party", "/src/functools.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn stdlib_precedence_over_installed_stub_package() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[(
                "foo.pyi", r#"
"#,
            )],
            versions: r#"
foo: 3.8-
"#,
        };
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_site_packages_files(&[(
                "foo-stubs/__init__.pyi",
                r#"
"#,
            )])
            .build();

        assert_listed_file(&db, "foo", &stdlib.join("foo.pyi"));
    }

    #[test]
    fn concrete_package_shadows_legacy_namespace() {
        let TestCase {
            db, site_packages, ..
        } = TestCaseBuilder::new()
            .with_src_files(&[(
                "acme/__init__.py",
                r#"
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
"#,
            )])
            .with_site_packages_files(&[(
                "acme/__init__.py",
                r#"
"#,
            )])
            .build();

        assert_listed_file(&db, "acme", &site_packages.join("acme/__init__.py"));
    }

    #[test]
    fn stdlib_uses_vendored_typeshed_when_no_custom_typeshed_supplied() {
        let TestCase { db, .. } = TestCaseBuilder::new().with_vendored_typeshed().build();

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str().contains("pydoc_data")),
            @r#"
        [
            Module::File("pydoc_data", "std-vendored", "stdlib/pydoc_data/__init__.pyi", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn resolve_package() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", "print('Hello, world!'")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn package_priority_over_module() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", "print('Hello, world!')"),
            ("foo.py", "print('Hello, world!')"),
        ];

        let TestCase { db, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn typing_stub_over_module() {
        const SRC: &[FileSpec] = &[("foo.py", "print('Hello, world!')"), ("foo.pyi", "x: int")];

        let TestCase { db, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.pyi", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn sub_packages() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", ""),
            ("foo/bar/__init__.py", ""),
            ("foo/bar/baz.py", "print('Hello, world!)'"),
        ];

        let TestCase { db, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn module_search_path_priority() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("foo.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        use anyhow::Context;

        let mut db = TestDb::new().with_python_version(PythonVersion::PY38);

        let temp_dir = tempfile::TempDir::with_prefix("PREFIX-SENTINEL")?;
        let root = temp_dir
            .path()
            .canonicalize()
            .context("Failed to canonicalize temp dir")?;
        let root = SystemPath::from_std_path(&root).unwrap();
        db.use_system(ruff_db::system::OsSystem::new(root));

        let src = root.join("src");
        let site_packages = root.join("site-packages");
        let custom_typeshed = root.join("typeshed");

        let foo = src.join("foo.py");
        let bar = src.join("bar.py");

        std::fs::create_dir_all(src.as_std_path())?;
        std::fs::create_dir_all(site_packages.as_std_path())?;
        std::fs::create_dir_all(custom_typeshed.join("stdlib").as_std_path())?;
        std::fs::File::create(custom_typeshed.join("stdlib/VERSIONS").as_std_path())?;

        std::fs::write(foo.as_std_path(), "")?;
        std::os::unix::fs::symlink(foo.as_std_path(), bar.as_std_path())?;

        let settings = SearchPathSettings {
            src_roots: vec![src.clone()],
            custom_typeshed: Some(custom_typeshed),
            site_packages_paths: vec![site_packages],
            ..SearchPathSettings::empty()
        };

        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .expect("Valid search path settings"),
        );

        db.files().try_add_root(&db, &src, FileRootKind::Project);

        // From the original test in the "resolve this module"
        // implementation, this test seems to symlink a Python module
        // and assert that they are treated as two distinct modules.
        // That's what we capture here when listing modules as well.
        insta::with_settings!({
            // Temporary directory often have random chars in them, so
            // get rid of that part for a stable snapshot.
            filters => [(r#""\S*PREFIX-SENTINEL.*?/"#, r#""/"#)],
        }, {
            insta::assert_debug_snapshot!(
                list_snapshot(&db),
                @r#"
            [
                Module::File("bar", "first-party", "/src/bar.py", Module, None),
                Module::File("foo", "first-party", "/src/foo.py", Module, None),
            ]
            "#,
            );
        });

        Ok(())
    }

    // NOTE: I've omitted the
    // `deleting_an_unrelated_file_doesnt_change_module_resolution`
    // test here since it likely seems inapplicable to "listing"
    // modules. ---AG

    #[test]
    fn adding_file_on_which_module_resolution_depends_invalidates_previously_failing_query_that_now_succeeds()
    -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new().build();
        let foo_path = src.join("foo.py");

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @"[]",
        );

        // Now write the foo file
        db.write_file(&foo_path, "x = 1")?;

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.py", Module, None),
        ]
        "#,
        );

        Ok(())
    }

    #[test]
    fn deeply_nested_file_does_not_change_top_level_listing() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[
                (
                    "package/__init__.py",
                    r#"
"#,
                ),
                (
                    "package/sub/__init__.py",
                    r#"
"#,
                ),
            ])
            .build();

        let before = list_modules(&db)
            .iter()
            .map(|module| module.name(&db).to_string())
            .collect::<Vec<_>>();
        db.write_file(
            src.join("package/sub/nested.py"),
            r#"
"#,
        )
        .expect("create nested module");
        let after = list_modules(&db)
            .iter()
            .map(|module| module.name(&db).to_string())
            .collect::<Vec<_>>();
        assert_eq!(before, after);
    }

    #[test]
    fn sibling_file_does_not_change_package_submodules() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[(
                "package/__init__.py",
                r#"
"#,
            )])
            .build();

        {
            let package = list_modules(&db)
                .iter()
                .find(|module| module.name(&db).as_str() == "package")
                .copied()
                .expect("package to exist");
            assert!(package.all_submodules(&db).is_empty());
        }

        db.write_file(
            src.join("sibling.py"),
            r#"
"#,
        )
        .expect("create sibling module");
        let package = list_modules(&db)
            .iter()
            .find(|module| module.name(&db).as_str() == "package")
            .copied()
            .expect("package to exist");
        assert!(package.all_submodules(&db).is_empty());
    }

    #[test]
    fn removing_file_on_which_module_resolution_depends_invalidates_previously_successful_query_that_now_fails()
    -> anyhow::Result<()> {
        const SRC: &[FileSpec] = &[("foo.py", "x = 1"), ("foo/__init__.py", "x = 2")];

        let TestCase { mut db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();
        let foo_path = src.join("foo/__init__.py");

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo/__init__.py", Package, None),
        ]
        "#,
        );

        // Delete `foo/__init__.py` and the `foo` folder. `foo` should
        // now resolve to `foo.py`
        db.memory_file_system().remove_file(&foo_path)?;
        db.memory_file_system()
            .remove_directory(foo_path.parent().unwrap())?;
        // NOTE: This is present in the test for the "resolve this
        // module" implementation as well. It seems like it kind of
        // defeats the point to me. Shouldn't this be the thing we're
        // testing? ---AG
        File::sync_path(&mut db, &foo_path);
        File::sync_path(&mut db, foo_path.parent().unwrap());

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.py", Module, None),
        ]
        "#,
        );

        Ok(())
    }

    // Slightly changed from
    // `adding_file_to_search_path_with_lower_priority_does_not_invalidate_query`
    // to just check that adding a file doesn't change the results. (i.e., This is
    // no longer a test of caching.)
    #[test]
    fn adding_file_to_search_path_with_lower_priority_does_not_change_results() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );

        // Adding a file to site-packages does not invalidate the query,
        // since site-packages takes lower priority in the module resolution
        db.clear_salsa_events();
        let site_packages_functools_path = site_packages.join("functools.py");
        db.write_file(&site_packages_functools_path, "f: int")
            .unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );
    }

    #[test]
    fn adding_file_to_search_path_with_higher_priority_invalidates_the_query() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );

        // Adding a first-party file should do some kind of cache
        // invalidation here, since first-party files take higher
        // priority in module resolution:
        let src_functools_path = src.join("functools.py");
        db.write_file(&src_functools_path, "FOO: int").unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "first-party", "/src/functools.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn deleting_file_from_higher_priority_search_path_invalidates_the_query() {
        const SRC: &[FileSpec] = &[("functools.py", "FOO: int")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();
        let src_functools_path = src.join("functools.py");

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "first-party", "/src/functools.py", Module, None),
        ]
        "#,
        );

        // If we now delete the first-party file,
        // it should resolve to the stdlib:
        db.memory_file_system()
            .remove_file(&src_functools_path)
            .unwrap();
        // NOTE: This is present in the test for the "resolve this
        // module" implementation as well. It seems like it kind of
        // defeats the point to me. Shouldn't this be the thing we're
        // testing? In any case, removing it results in the cache not
        // being invalidated. ---AG
        File::sync_path(&mut db, &src_functools_path);

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, Some(Functools)),
        ]
        "#,
        );
    }

    #[test]
    fn editable_install_absolute_path() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo/__init__.py", ""), ("/x/src/foo/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x")
            .build();

        db.write_files(x_directory).unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "editable", "/x/src/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn editable_install_pth_file_with_whitespace() {
        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "        /x/src"),
            ("_bar.pth", "/y/src        "),
        ];
        let external_files = [("/x/src/foo.py", ""), ("/y/src/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/y/src")
            .build();

        db.write_files(external_files).unwrap();

        // Lines with leading whitespace in `.pth` files do not parse,
        // so this excludes `foo`. Lines with trailing whitespace in
        // `.pth` files do parse, so this includes `bar`.
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("bar", "editable", "/y/src/bar.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn editable_install_relative_path() {
        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "../../x/../x/y/src"),
            ("../x/y/src/foo.pyi", ""),
        ];

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x")
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "editable", "/x/y/src/foo.pyi", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn editable_install_multiple_pth_files_with_multiple_paths() {
        const COMPLEX_PTH_FILE: &str = "\
/

# a comment
/baz

import not_an_editable_install; do_something_else_crazy_dynamic()

# another comment
spam

not_a_directory
";

        const SITE_PACKAGES: &[FileSpec] = &[
            ("_foo.pth", "../../x/../x/y/src"),
            ("_lots_of_others.pth", COMPLEX_PTH_FILE),
            ("../x/y/src/foo.pyi", ""),
            ("spam/spam.py", ""),
        ];

        let root_files = [("/a.py", ""), ("/baz/b.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x/y/src")
            .with_library_root("/")
            .with_library_root("/baz")
            .build();

        db.write_files(root_files).unwrap();

        // NOTE: The `src`, `typeshed` and `x` namespace packages here
        // are a bit odd, but this seems to be a result of `/` in the
        // pth file. It's also consistent with "resolve this module,"
        // which will indeed happily resolve `src`, `typeshed` or `x`
        // as top-level modules. ---AG
        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("a", "editable", "/a.py", Module, None),
            Module::File("b", "editable", "/baz/b.py", Module, None),
            Module::Namespace(ModuleName("baz")),
            Module::File("foo", "editable", "/x/y/src/foo.pyi", Module, None),
            Module::File("spam", "editable", "/site-packages/spam/spam.py", Module, None),
            Module::Namespace(ModuleName("src")),
            Module::Namespace(ModuleName("typeshed")),
            Module::Namespace(ModuleName("x")),
        ]
        "#,
        );
    }

    #[test]
    fn module_resolution_paths_cached_between_different_module_resolutions() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src"), ("_bar.pth", "/y/src")];
        let external_directories = [("/x/src/foo.py", ""), ("/y/src/bar.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x")
            .with_library_root("/y")
            .build();

        db.write_files(external_directories).unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("bar", "editable", "/y/src/bar.py", Module, None),
            Module::File("foo", "editable", "/x/src/foo.py", Module, None),
        ]
        "#,
        );

        db.clear_salsa_events();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("bar", "editable", "/y/src/bar.py", Module, None),
            Module::File("foo", "editable", "/x/src/foo.py", Module, None),
        ]
        "#,
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(
            &db,
            dynamic_resolution_paths,
            ModuleResolveModeIngredient::new(
                &db,
                db.resolver_environment(),
                ModuleResolveMode::Typing,
            ),
            &events,
        );
    }

    #[test]
    fn deleting_pth_file_on_which_module_resolution_depends_invalidates_cache() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo.py", "")];

        let TestCase {
            mut db,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x")
            .build();

        db.write_files(x_directory).unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "editable", "/x/src/foo.py", Module, None),
        ]
        "#,
        );

        db.memory_file_system()
            .remove_file(site_packages.join("_foo.pth"))
            .unwrap();
        // NOTE: This is present in the test for the "resolve this
        // module" implementation as well. It seems like it kind of
        // defeats the point to me. Shouldn't this be the thing we're
        // testing? ---AG
        File::sync_path(&mut db, &site_packages.join("_foo.pth"));

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @"[]",
        );
    }

    #[test]
    fn deleting_editable_install_on_which_module_resolution_depends_invalidates_cache() {
        const SITE_PACKAGES: &[FileSpec] = &[("_foo.pth", "/x/src")];
        let x_directory = [("/x/src/foo.py", "")];

        let TestCase { mut db, .. } = TestCaseBuilder::new()
            .with_site_packages_files(SITE_PACKAGES)
            .with_library_root("/x")
            .build();
        let src_path = SystemPathBuf::from("/x/src");

        db.write_files(x_directory).unwrap();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "editable", "/x/src/foo.py", Module, None),
        ]
        "#,
        );

        db.memory_file_system()
            .remove_file(src_path.join("foo.py"))
            .unwrap();
        db.memory_file_system().remove_directory(&src_path).unwrap();
        // NOTE: This is present in the test for the "resolve this
        // module" implementation as well. It seems like it kind of
        // defeats the point to me. Shouldn't this be the thing we're
        // testing? ---AG
        File::sync_path(&mut db, &src_path.join("foo.py"));
        File::sync_path(&mut db, &src_path);

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @"[]",
        );
    }

    #[test]
    fn editable_installs_into_first_party_search_path() {
        let mut db = TestDb::new();

        let src = SystemPath::new("/src");
        let venv_site_packages = SystemPathBuf::from("/venv-site-packages");
        let site_packages_pth = venv_site_packages.join("foo.pth");
        let editable_install_location = src.join("x/y/a.py");

        db.write_files([
            (&site_packages_pth, "/src/x/y/"),
            (&editable_install_location, ""),
        ])
        .unwrap();

        db.files()
            .try_add_root(&db, SystemPath::new("/src"), FileRootKind::Project);

        let settings = SearchPathSettings {
            site_packages_paths: vec![venv_site_packages],
            ..SearchPathSettings::new(vec![src.to_path_buf()])
        };

        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .expect("Valid search path settings"),
        );

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "a"),
            @r#"
        [
            Module::File("a", "editable", "/src/x/y/a.py", Module, None),
        ]
        "#,
        );

        let editable_root = db
            .files()
            .root(&db, &editable_install_location)
            .expect("file root for editable install");

        assert_eq!(editable_root.path(&db), src);
    }

    #[test]
    fn multiple_site_packages_with_editables() {
        let mut db = TestDb::new();

        let venv_site_packages = SystemPathBuf::from("/venv-site-packages");
        let site_packages_pth = venv_site_packages.join("foo.pth");
        let system_site_packages = SystemPathBuf::from("/system-site-packages");
        let editable_install_location = SystemPathBuf::from("/x/y/a.py");
        let system_site_packages_location = system_site_packages.join("a.py");

        db.memory_file_system()
            .create_directory_all("/src")
            .unwrap();
        db.write_files([
            (&site_packages_pth, "/x/y"),
            (&editable_install_location, ""),
            (&system_site_packages_location, ""),
        ])
        .unwrap();

        db.files()
            .try_add_root(&db, SystemPath::new("/src"), FileRootKind::Project);

        let settings = SearchPathSettings {
            site_packages_paths: vec![venv_site_packages, system_site_packages],
            ..SearchPathSettings::new(vec![SystemPathBuf::from("/src")])
        };

        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .expect("Valid search path settings"),
        );

        // The editable installs discovered from the `.pth` file in the
        // first `site-packages` directory take precedence over the
        // second `site-packages` directory...
        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "a"),
            @r#"
        [
            Module::File("a", "editable", "/x/y/a.py", Module, None),
        ]
        "#,
        );

        db.memory_file_system()
            .remove_file(&site_packages_pth)
            .unwrap();
        // NOTE: This is present in the test for the "resolve this
        // module" implementation as well. It seems like it kind of
        // defeats the point to me. Shouldn't this be the thing we're
        // testing? ---AG
        File::sync_path(&mut db, &site_packages_pth);

        // ...But now that the `.pth` file in the first `site-packages`
        // directory has been deleted, the editable install no longer
        // exists, so the module now resolves to the file in the second
        // `site-packages` directory
        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "a"),
            @r#"
        [
            Module::File("a", "site-packages", "/system-site-packages/a.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    #[cfg(unix)]
    fn case_sensitive_resolution_with_symlinked_directory() -> anyhow::Result<()> {
        use anyhow::Context as _;

        let temp_dir = tempfile::TempDir::with_prefix("PREFIX-SENTINEL")?;
        let root = SystemPathBuf::from_path_buf(
            temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalized path")?,
        )
        .expect("UTF8 path for temp dir");

        let mut db = TestDb::new();

        let src = root.join("src");
        let a_package_target = root.join("a-package");
        let a_src = src.join("a");

        db.use_system(ruff_db::system::OsSystem::new(&root));

        db.write_file(
            a_package_target.join("__init__.py"),
            "class Foo: x: int = 4",
        )
        .context("Failed to write `a-package/__init__.py`")?;

        db.write_file(src.join("main.py"), "print('Hy')")
            .context("Failed to write `main.py`")?;

        // The symlink triggers the slow-path in the `OsSystem`'s
        // `exists_path_case_sensitive` code because canonicalizing the path
        // for `a/__init__.py` results in `a-package/__init__.py`
        std::os::unix::fs::symlink(a_package_target.as_std_path(), a_src.as_std_path())
            .context("Failed to symlink `src/a` to `a-package`")?;

        db.files().try_add_root(&db, &root, FileRootKind::Project);

        let settings = SearchPathSettings::new(vec![src]);
        let search_paths = settings
            .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
            .expect("valid search path settings");
        db.set_search_paths(search_paths);

        insta::with_settings!({
            // Temporary directory often have random chars in them, so
            // get rid of that part for a stable snapshot.
            filters => [(r#""\S*PREFIX-SENTINEL.*?/"#, r#""/"#)],
        }, {
            insta::assert_debug_snapshot!(
                list_snapshot_filter(&db, |m| matches!(m.name(&db).as_str(), "A" | "a")),
                @r#"
            [
                Module::File("a", "first-party", "/src/a/__init__.py", Package, None),
            ]
            "#,
            );
        });

        Ok(())
    }

    #[test]
    fn file_to_module_where_one_search_path_is_subdirectory_of_other() {
        let project_directory = SystemPathBuf::from("/project");
        let site_packages = project_directory.join(".venv/lib/python3.13/site-packages");
        let installed_foo_module = site_packages.join("foo/__init__.py");

        let mut db = TestDb::new();
        db.write_file(&installed_foo_module, "").unwrap();

        db.files()
            .try_add_root(&db, &project_directory, FileRootKind::Project);

        let settings = SearchPathSettings {
            site_packages_paths: vec![site_packages],
            ..SearchPathSettings::new(vec![project_directory])
        };
        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .unwrap(),
        );

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "foo"),
            @r#"
        [
            Module::File("foo", "site-packages", "/project/.venv/lib/python3.13/site-packages/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn namespace_package() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/bar.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::Namespace(ModuleName("foo")),
        ]
        "#,
        );
    }

    /// Regardless of search path priority, if we have a "regular" package of
    /// the same name as a namespace package, the regular package always takes
    /// priority.
    #[test]
    fn namespace_package_precedence() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/bar.py", "")])
            .with_site_packages_files(&[("foo.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "site-packages", "/site-packages/foo.py", Module, None),
        ]
        "#,
        );

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("foo/bar.py", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo.py", Module, None),
        ]
        "#,
        );
    }

    #[test]
    fn stub_package() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo-stubs/__init__.pyi", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo-stubs/__init__.pyi", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn stub_file_module_not_allowed() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo-stubs.pyi", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @"[]",
        );
    }

    #[test]
    fn stub_package_precedence() {
        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", ""), ("foo-stubs/__init__.pyi", "")])
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @r#"
        [
            Module::File("foo", "first-party", "/src/foo-stubs/__init__.pyi", Package, None),
        ]
        "#,
        );
    }

    #[test]
    fn stub_package_not_allowed_in_typeshed() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "foo: 3.8-",
            stdlib_files: &[("foo-stubs/__init__.pyi", "")],
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(TYPESHED)
            .with_python_version(PythonVersion::PY38)
            .build();

        insta::assert_debug_snapshot!(
            list_snapshot(&db),
            @"[]",
        );
    }

    #[test]
    fn enumerate_split_and_nested_namespaces() {
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
        assert_eq!(
            enumerated_names(&db),
            [
                "acme",
                "acme.left",
                "acme.nested",
                "acme.nested.deep",
                "acme.regular",
                "acme.regular.namespace",
                "acme.regular.namespace.child",
                "acme.right",
            ]
        );
    }

    #[test]
    fn enumerate_namespace_children_uses_resolution_precedence() {
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
        assert_eq!(
            enumerated_names(&db),
            [
                "acme",
                "acme.duplicate",
                "acme.module",
                "acme.package",
                "acme.package.local",
                "acme.stubbed",
            ]
        );
        assert_enumerated_file(&db, "acme.duplicate", "/src/acme/duplicate.py");
        assert_enumerated_file(&db, "acme.stubbed", "/src/acme/stubbed.pyi");
        assert_enumerated_file(&db, "acme.package", "/src/acme/package/__init__.py");
    }

    #[test]
    fn enumerate_deep_namespaces() {
        let components = (0..32)
            .map(|depth| format!("level_{depth}"))
            .collect::<Vec<_>>();
        let path = format!("/src/{}/leaf.py", components.join("/"));
        let db = enumeration_db(&[&path], &[]);
        let names = enumerated_names(&db);
        assert_eq!(names.len(), 33);
        assert!(names.contains(&format!("{}.leaf", components.join("."))));
    }

    #[test]
    fn enumerate_concrete_parents_shadow_namespace_portions() {
        for parent in ["/site-packages/acme/__init__.py", "/site-packages/acme.py"] {
            let db = enumeration_db(&["/src/acme/hidden.py", parent], &[]);
            assert_eq!(enumerated_names(&db), ["acme"]);
            assert_enumerated_file(&db, "acme", parent);
        }
    }

    #[test]
    fn enumerate_legacy_namespace_portions() {
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
            assert_eq!(enumerated_names(&db), ["acme", "acme.left", "acme.right"]);
            assert_enumerated_file(&db, "acme", "/src/acme/__init__.py");
        }
    }

    #[test]
    fn enumerate_partial_stub_namespaces() {
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
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.api", "acme.api.runtime", "acme.api.stubbed"]
        );
        assert_enumerated_file(&db, "acme.api", "/src/acme/api/__init__.py");
        assert_enumerated_file(
            &db,
            "acme.api.stubbed",
            "/site-packages/acme-stubs/api/stubbed.pyi",
        );
    }

    #[test]
    fn enumerate_partial_child_of_complete_stub_package() {
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
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.api", "acme.api.runtime", "acme.api.stubbed"]
        );
        assert_enumerated_file(
            &db,
            "acme.api",
            "/site-packages/acme-stubs/api/__init__.pyi",
        );
    }

    #[test]
    fn enumerate_overlay_and_runtime_siblings() {
        let db = enumeration_db(
            &[
                "/extra/acme/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/stubbed.py",
                "/src/acme/runtime.py",
            ],
            &["/extra"],
        );
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.runtime", "acme.stubbed"]
        );
        assert_enumerated_file(&db, "acme.stubbed", "/extra/acme/stubbed.pyi");
        assert_enumerated_file(&db, "acme.runtime", "/src/acme/runtime.py");
    }

    #[test]
    fn enumerate_overlay_descendants_of_runtime_module() {
        let db = enumeration_db(
            &[
                "/extra/acme/api/stubbed.pyi",
                "/src/acme/__init__.py",
                "/src/acme/api.py",
            ],
            &["/extra"],
        );
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.api", "acme.api.stubbed"]
        );
        assert_enumerated_file(&db, "acme.api", "/src/acme/api.py");
        assert_enumerated_file(&db, "acme.api.stubbed", "/extra/acme/api/stubbed.pyi");
    }

    #[test]
    fn enumerate_overlay_through_unresolved_prefix() {
        let db = unresolved_overlay_db();
        let environment = db.resolver_environment();
        let package = crate::resolve_module_confident(
            &db,
            environment,
            &ModuleName::new_static("acme").expect("valid package name"),
        )
        .expect("installed stub package resolves");
        // Installed stubs hide the runtime parents, but a local stub can still patch the leaf.
        assert!(package.all_submodules(&db).is_empty());
        for name in ["acme.nested", "acme.nested.deep"] {
            let name = ModuleName::new(name).expect("valid prefix");
            assert!(crate::resolve_module_confident(&db, environment, &name).is_none());
            assert!(crate::resolve_real_module_confident(&db, environment, &name).is_some());
        }
        assert_eq!(enumerated_names(&db), ["acme", "acme.nested.deep.tools"]);
        assert_enumerated_file(
            &db,
            "acme.nested.deep.tools",
            "/extra/acme/nested/deep/tools.pyi",
        );
    }

    #[test]
    fn enumeration_tracks_unresolved_overlay_prefix_changes() {
        let mut db = unresolved_overlay_db();
        assert_eq!(enumerated_names(&db), ["acme", "acme.nested.deep.tools"]);
        remove_enumerated_file(&mut db, "/extra/acme/nested/deep/tools.pyi");
        assert_eq!(enumerated_names(&db), ["acme"]);
        db.write_file(
            "/extra/acme/nested/deep/tools.pyi",
            r#"
"#,
        )
        .expect("restore the local stub");
        assert_eq!(enumerated_names(&db), ["acme", "acme.nested.deep.tools"]);

        // A newly supplied initializer turns a traversal-only prefix into a resolved module.
        db.write_file(
            "/extra/acme/nested/__init__.pyi",
            r#"
"#,
        )
        .expect("provide the overlay parent");
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.nested", "acme.nested.deep.tools"]
        );
        remove_enumerated_file(&mut db, "/extra/acme/nested/__init__.pyi");
        assert_eq!(enumerated_names(&db), ["acme", "acme.nested.deep.tools"]);
    }

    #[test]
    fn enumeration_tracks_files_and_initializers() {
        let mut db = enumeration_db(&["/src/acme/left.py", "/site-packages/acme/right.py"], &[]);
        assert_eq!(enumerated_names(&db), ["acme", "acme.left", "acme.right"]);
        db.write_file(
            "/src/acme/nested/new.py",
            r#"
"#,
        )
        .expect("create nested namespace and module");
        assert_eq!(
            enumerated_names(&db),
            [
                "acme",
                "acme.left",
                "acme.nested",
                "acme.nested.new",
                "acme.right"
            ]
        );
        remove_enumerated_file(&mut db, "/src/acme/nested/new.py");
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.left", "acme.nested", "acme.right"]
        );

        db.write_file(
            "/src/acme/__init__.py",
            r#"
"#,
        )
        .expect("turn namespace into regular package");
        assert_eq!(enumerated_names(&db), ["acme", "acme.left", "acme.nested"]);
        db.write_file(
            "/src/acme/__init__.py",
            r#"
__path__ = __import__("pkgutil").extend_path(__path__, __name__)
"#,
        )
        .expect("turn initializer into a legacy namespace declaration");
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.left", "acme.nested", "acme.right"]
        );
        remove_enumerated_file(&mut db, "/src/acme/__init__.py");
        assert_eq!(
            enumerated_names(&db),
            ["acme", "acme.left", "acme.nested", "acme.right"]
        );
    }

    #[test]
    fn enumeration_tracks_typing_marker_changes() {
        let mut db = enumeration_db(
            &[
                "/site-packages/acme-stubs/__init__.pyi",
                "/src/acme/__init__.py",
                "/src/acme/runtime.py",
            ],
            &[],
        );
        assert_eq!(enumerated_names(&db), ["acme"]);
        let marker = "/site-packages/acme-stubs/py.typed";
        db.write_file(
            marker,
            r#"
partial
"#,
        )
        .expect("create partial marker");
        assert_eq!(enumerated_names(&db), ["acme", "acme.runtime"]);
        db.write_file(
            marker, r#"
"#,
        )
        .expect("make marker complete");
        assert_eq!(enumerated_names(&db), ["acme"]);
        db.write_file(
            marker,
            r#"
partial
"#,
        )
        .expect("make marker partial again");
        assert_eq!(enumerated_names(&db), ["acme", "acme.runtime"]);
        remove_enumerated_file(&mut db, marker);
        assert_eq!(enumerated_names(&db), ["acme"]);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn enumerate_preserves_symlink_boundary_and_shadowing() {
        let (_temp, db, root) = os_enumeration_db();
        for path in [
            "src/acme/own.py",
            "site-packages/acme/hidden.py",
            "site-packages/acme/ns/masked.py",
            "site-packages/acme/ns/visible.py",
            "site-packages/acme/blocked/visible.py",
            "other_ns/masked.py",
            "other_ns/source_only.py",
            "regular/__init__.py",
            "regular/child.py",
            "regular/nested/__init__.py",
            "regular/nested/child.py",
            "target.py",
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("fixture parent").as_std_path())
                .expect("create fixture directory");
            std::fs::write(
                path.as_std_path(),
                r#"
"#,
            )
            .expect("write symlink fixture");
        }
        for (source, link) in [
            ("target.py", "src/top_alias.py"),
            ("target.py", "src/acme/hidden.py"),
            ("other_ns", "src/acme/ns"),
            ("regular", "src/acme/blocked"),
            ("src/acme", "src/alias"),
            ("target.py", "regular/linked.py"),
            ("other_ns", "regular/linked_dir"),
        ] {
            std::os::unix::fs::symlink(
                root.join(source).as_std_path(),
                root.join(link).as_std_path(),
            )
            .expect("create fixture symlink");
        }
        assert_eq!(
            enumerated_names(&db),
            [
                "acme",
                "acme.ns",
                "acme.ns.visible",
                "acme.own",
                "alias",
                "alias.own",
                "top_alias",
            ]
        );
        for name in ["acme.hidden", "acme.ns.masked", "acme.blocked"] {
            assert!(
                crate::resolve_module_confident(
                    &db,
                    db.resolver_environment(),
                    &ModuleName::new(name).expect("valid module name")
                )
                .is_some(),
                "ordinary resolution still follows symlinks for {name}"
            );
        }

        // An explicit import can resolve a package through a nested symlink. Completion
        // lists its ordinary children, while still excluding new symlinks below that package.
        for (name, expected) in [
            (
                "acme.blocked",
                vec!["acme.blocked.child", "acme.blocked.nested"],
            ),
            ("acme.blocked.nested", vec!["acme.blocked.nested.child"]),
        ] {
            let package = crate::resolve_module_confident(
                &db,
                db.resolver_environment(),
                &ModuleName::new(name).expect("valid package name"),
            )
            .expect("resolve explicitly named symlink package");
            let children: Vec<_> = package
                .all_submodules(&db)
                .iter()
                .map(|module| module.name(&db).as_str())
                .collect();
            assert_eq!(children, expected);
        }
    }

    /// This is a regression test for mishandling of file root matching.
    ///
    /// In particular, in some cases, `/` is added as a search root. This
    /// should in turn match everything. But the way we were setting up the
    /// wildcard for matching was incorrect for this one specific case. That in
    /// turn meant that the module resolver couldn't find an appropriate file
    /// root which in turn caused a panic.
    ///
    /// See: <https://github.com/astral-sh/ty/issues/1277>
    #[test]
    fn root_directory_for_search_path_is_okay() {
        let project_directory = SystemPathBuf::from("/project");
        let installed_foo_module = project_directory.join("foo/__init__.py");

        let mut db = TestDb::new();
        db.write_file(&installed_foo_module, "").unwrap();

        db.files()
            .try_add_root(&db, SystemPath::new("/"), FileRootKind::Project);

        let settings = SearchPathSettings::new(vec![project_directory]);
        let search_paths = settings
            .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
            .expect("Valid search path settings");
        db.set_search_paths(search_paths);

        insta::assert_debug_snapshot!(
            list_snapshot_filter(&db, |m| m.name(&db).as_str() == "foo"),
            @r#"
        [
            Module::File("foo", "first-party", "/project/foo/__init__.py", Package, None),
        ]
        "#,
        );
    }

    fn unresolved_overlay_db() -> TestDb {
        enumeration_db(
            &[
                "/site-packages/acme-stubs/__init__.pyi",
                "/site-packages/acme-stubs/py.typed",
                "/site-packages/acme/__init__.py",
                "/site-packages/acme/nested/__init__.py",
                "/site-packages/acme/nested/deep/__init__.py",
                "/site-packages/acme/nested/deep/tools.py",
                "/extra/acme/nested/deep/tools.pyi",
                "/extra/acme/nested/source_only.py",
            ],
            &["/extra"],
        )
    }

    fn enumeration_db(paths: &[&str], extra_paths: &[&str]) -> TestDb {
        let TestCase { mut db, .. } = TestCaseBuilder::new().build();
        db.write_files(paths.iter().map(|path| {
            (
                *path, r#"
"#,
            )
        }))
        .expect("create enumeration fixtures");
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
                .expect("configure enumeration search paths"),
        );
        db
    }

    fn enumerated_names(db: &TestDb) -> Vec<String> {
        super::all_modules(db, db.resolver_environment())
            .into_iter()
            .map(|module| {
                let name = module.name(db);
                assert_eq!(
                    Some(module),
                    crate::resolve_module_confident(db, db.resolver_environment(), name),
                    "enumeration must agree with resolution for {name}"
                );
                name.to_string()
            })
            .collect()
    }

    fn assert_enumerated_file(db: &TestDb, name: &str, expected: &str) {
        let modules = super::all_modules(db, db.resolver_environment());
        let module = modules
            .iter()
            .find(|module| module.name(db).as_str() == name)
            .expect("module should be enumerated");
        let file = module.file(db).expect("module should have a defining file");
        assert_eq!(
            file.path(db).as_system_path(),
            Some(SystemPath::new(expected))
        );
    }

    fn remove_enumerated_file(db: &mut TestDb, path: &str) {
        db.memory_file_system()
            .remove_file(path)
            .expect("remove fixture file");
        File::sync_path(db, SystemPath::new(path));
    }

    #[cfg(target_family = "unix")]
    fn os_enumeration_db() -> (tempfile::TempDir, TestDb, SystemPathBuf) {
        let temp = tempfile::TempDir::new().expect("create enumeration workspace");
        let canonical = temp
            .path()
            .canonicalize()
            .expect("canonical workspace path");
        let root = SystemPathBuf::from_path_buf(canonical).expect("UTF-8 workspace path");
        for path in ["src", "site-packages", "typeshed/stdlib"] {
            std::fs::create_dir_all(root.join(path).as_std_path()).expect("create search root");
        }
        std::fs::write(
            root.join("typeshed/stdlib/VERSIONS").as_std_path(),
            r#"
"#,
        )
        .expect("write empty typeshed versions");
        let mut db = TestDb::new();
        db.use_system(ruff_db::system::OsSystem::new(&root));
        let settings = SearchPathSettings {
            src_roots: vec![root.join("src")],
            site_packages_paths: vec![root.join("site-packages")],
            custom_typeshed: Some(root.join("typeshed")),
            ..SearchPathSettings::empty()
        };
        db.set_search_paths(
            settings
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .expect("configure real filesystem search paths"),
        );
        (temp, db, root)
    }

    fn assert_listed_file(db: &TestDb, name: &str, expected: &SystemPath) {
        let module = list_modules(db)
            .iter()
            .find(|module| module.name(db).as_str() == name)
            .expect("top-level module should be listed");
        let file = module.file(db).expect("module should have a defining file");
        assert_eq!(file.path(db).as_system_path(), Some(expected));
    }
}
