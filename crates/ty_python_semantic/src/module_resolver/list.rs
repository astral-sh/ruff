use std::collections::btree_map::{BTreeMap, Entry};

use ruff_python_ast::PythonVersion;

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::program::Program;

use super::module::{Module, ModuleKind};
use super::path::{SearchPath, SystemOrVendoredPathRef};
use super::resolver::{
    ModuleResolveMode, ResolverContext, is_non_shadowable, resolve_file_module, search_paths,
};

/// List all available modules.
#[salsa::tracked]
pub(crate) fn list_modules<'db>(db: &'db dyn Db) -> Vec<Module<'db>> {
    let mut lister = Lister::new(db);
    for search_path in search_paths(db) {
        match search_path.as_path() {
            SystemOrVendoredPathRef::System(system_search_path) => {
                let Ok(it) = db.system().read_directory(system_search_path) else {
                    continue;
                };
                for result in it {
                    let Ok(entry) = result else { continue };
                    lister.add_path(&search_path, entry.path(), entry.file_type().is_directory());
                }
            }
            SystemOrVendoredPathRef::Vendored(vendored_search_path) => {
                for entry in db.vendored().read_directory(vendored_search_path) {
                    lister.add_path(&search_path, entry.path(), entry.file_type().is_directory());
                }
            }
        }
    }
    lister.into_modules()
}

struct Lister<'db> {
    db: &'db dyn Db,
    program: Program,
    modules: BTreeMap<&'db ModuleName, Module<'db>>,
}

impl<'db> Lister<'db> {
    fn new(db: &'db dyn Db) -> Lister<'db> {
        let program = Program::get(db);
        Lister {
            db,
            program,
            modules: BTreeMap::new(),
        }
    }

    /// Returns the modules collected, sorted by module name.
    fn into_modules(self) -> Vec<Module<'db>> {
        self.modules.into_values().collect()
    }

    fn add_path<'p>(
        &mut self,
        search_path: &SearchPath,
        path: impl Into<SystemOrVendoredPathRef<'p>>,
        is_dir: bool,
    ) {
        let path = path.into();
        let mut has_py_extension = false;
        // We must have no extension, a Python source file extension (`.py`)
        // or a Python stub file extension (`.pyi`).
        if let Some(ext) = path.extension() {
            has_py_extension = self.is_python_extension(ext);
            if !has_py_extension {
                return;
            }
        }

        let Some(name) = path.file_name() else { return };
        let mut module_path = search_path.to_module_path();
        module_path.push(name);
        let Some(module_name) = module_path.to_module_name() else {
            return;
        };

        // Some modules cannot shadow a subset of special
        // modules from the standard library.
        if !search_path.is_standard_library() && self.is_non_shadowable(&module_name) {
            return;
        }

        if is_dir {
            if module_path.is_regular_package(&self.context()) {
                module_path.push("__init__");
                if let Some(file) = resolve_file_module(&module_path, &self.context()) {
                    self.add_module(Module::file_module(
                        self.db,
                        module_name,
                        ModuleKind::Package,
                        search_path.clone(),
                        file,
                    ));
                    return;
                }
                module_path.pop();
            }

            // BREADCRUMBS: Test namespace package priority. Test py
            // versus pyi priority.

            // Otherwise, we kind of have to assume that we have a
            // namespace package, which can be any directory that
            // *doesn't* contain an `__init__.{py,pyi}`.
            //
            // Note though that if we find a "regular" module in a
            // lower priority search path, that will be allowed to
            // overwrite this namespace package.
            //
            // We only do this when in a standard library search
            // path, which matches how the "resolve this module"
            // implementation works. In particular, typeshed doesn't
            // use any namespace packages at time of writing
            // (2025-08-08), so if we're in a standard library search
            // path, we "know" this can't actually be a package.
            //
            // NOTE: Note that the
            // `module_path.is_regular_package()` check above takes
            // `VERSIONS` into consideration. Which means it can return
            // `false` even when, say, `package/__init__.py` exists. In
            // that case, outside of a standard library search path,
            // we'd incorrectly report it here as a namespace package.
            // HOWEVER, `VERSIONS` is only applicable for typeshed, so
            // this ends up working okay. But if typeshed ever uses
            // namespace packages, then this will need to be accounted
            // for.
            if !search_path.is_standard_library() {
                self.add_module(Module::namespace_package(self.db, module_name));
            }
            return;
        }

        // At this point, we're looking for a file module.
        // For a file module, we require a `.py` or `.pyi`
        // extension.
        if !has_py_extension {
            return;
        }

        let Some(file) = module_path.to_file(&self.context()) else {
            return;
        };
        self.add_module(Module::file_module(
            self.db,
            module_name,
            ModuleKind::Module,
            search_path.clone(),
            file,
        ));
    }

    /// Adds the given module to the collection.
    ///
    /// If the module had already been added and shouldn't override any
    /// existing entry, then this is a no-op. That is, this assumes that the
    /// caller looks for modules in search path priority order.
    fn add_module(&mut self, module: Module<'db>) {
        fn is_stub(db: &dyn Db, module: &Module<'_>) -> bool {
            let Some(file) = module.file(db) else {
                return false;
            };
            let Some(ext) = file.path(db).extension() else {
                return false;
            };
            ext == "pyi"
        }

        let mut entry = match self.modules.entry(module.name(self.db)) {
            Entry::Vacant(entry) => {
                entry.insert(module);
                return;
            }
            Entry::Occupied(entry) => entry,
        };

        let existing = entry.get();
        match (existing.search_path(self.db), module.search_path(self.db)) {
            // When we had a namespace package and now try to
            // insert a non-namespace package, the latter always
            // takes precedent, even if it's in a lower priority
            // search path.
            (None, Some(_)) => {
                entry.insert(module);
                return;
            }
            (Some(search_path_existing), Some(search_path_new)) => {
                // All of the cases below require that we're
                // in the same directory.
                if search_path_existing != search_path_new {
                    return;
                }
                // When we have a `foo/__init__.py` and a `foo.py` in
                // the same directory, the former takes precedent.
                // (This case can only occur when both have a search
                // path.)
                if existing.kind(self.db) == ModuleKind::Module
                    && module.kind(self.db) == ModuleKind::Package
                {
                    entry.insert(module);
                    return;
                }
                // Or if we have two file modules and the new one
                // is a stub, then the stub takes priority.
                if existing.kind(self.db) == ModuleKind::Module
                    && module.kind(self.db) == ModuleKind::Module
                    && is_stub(self.db, &module)
                {
                    entry.insert(module);
                    return;
                }
            }
            _ => {}
        }
    }

    /// Returns true if the given module name cannot be shadowable.
    fn is_non_shadowable(&self, name: &ModuleName) -> bool {
        is_non_shadowable(self.python_version().minor, name.as_str())
    }

    /// Returns the Python version we want to perform module resolution
    /// with.
    fn python_version(&self) -> PythonVersion {
        self.program.python_version(self.db)
    }

    /// Returns true if and only if the given file extension corresponds
    /// to a Python source or stub file. This also takes file system
    /// case sensitivity into account.
    fn is_python_extension(&self, ext: &str) -> bool {
        if self.db.system().case_sensitivity().is_case_sensitive() {
            ext == "py" || ext == "pyi"
        } else {
            // Using ASCII rules here is OK because the only
            // Unicode case folding rules that apply to ASCII
            // are for letters `s` and `k`.
            ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyi")
        }
    }

    /// Constructs a resolver context for use with some APIs that require it.
    fn context(&self) -> ResolverContext<'db> {
        ResolverContext {
            db: self.db,
            python_version: self.python_version(),
            // We don't currently support listing modules
            // in a "no stubs allowed" mode.
            mode: ModuleResolveMode::StubsAllowed,
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::PythonVersion;

    use crate::db::Db;
    use crate::module_resolver::module::Module;
    use crate::module_resolver::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::list_modules;

    struct ModuleDebugSnapshot<'db> {
        db: &'db dyn Db,
        module: Module<'db>,
    }

    impl<'db> std::fmt::Debug for ModuleDebugSnapshot<'db> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self.module {
                Module::Namespace(pkg) => {
                    write!(f, "Module::Namespace({name:?})", name = pkg.name(self.db))
                }
                Module::File(module) => write!(
                    f,
                    "Module::File({name:?}, {search_path:?}, {path:?}, {kind:?}, {known:?})",
                    name = module.name(self.db).as_str(),
                    search_path = module.search_path(self.db).debug_kind(),
                    path = module.file(self.db).path(self.db).as_str(),
                    kind = module.kind(self.db),
                    known = module.known(self.db),
                ),
            }
        }
    }

    fn sorted_list(db: &dyn Db) -> Vec<Module<'_>> {
        let mut modules = list_modules(db);
        modules.sort_by(|m1, m2| {
            let key1 = (
                m1.name(db),
                m1.search_path(db).map(|sp| sp.as_path()),
                m1.kind(db),
                m1.known(db),
            );
            let key2 = (
                m2.name(db),
                m2.search_path(db).map(|sp| sp.as_path()),
                m2.kind(db),
                m2.known(db),
            );
            key1.cmp(&key2)
        });
        modules
    }

    fn list_snapshot(db: &dyn Db) -> Vec<ModuleDebugSnapshot<'_>> {
        list_snapshot_filter(db, |_| true)
    }

    fn list_snapshot_filter<'db>(
        db: &'db dyn Db,
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
}
