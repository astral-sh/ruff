use std::collections::btree_map::{BTreeMap, Entry};

use ruff_python_ast::PythonVersion;

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::{ModulePath, SearchPath, SystemOrVendoredPathRef};
use crate::resolve::{ModuleResolveMode, ResolverContext, resolve_file_module, search_paths};

/// List all available modules, including all sub-modules, sorted in lexicographic order.
pub fn all_modules(db: &dyn Db) -> Vec<Module<'_>> {
    let mut modules = list_modules(db);
    let mut stack = modules.clone();
    while let Some(module) = stack.pop() {
        for &submodule in module.all_submodules(db) {
            modules.push(submodule);
            stack.push(submodule);
        }
    }
    modules.sort_by_key(|module| module.name(db));
    modules
}

/// List all available top-level modules.
#[salsa::tracked]
pub fn list_modules(db: &dyn Db) -> Vec<Module<'_>> {
    let mut modules = BTreeMap::new();
    for search_path in search_paths(db, ModuleResolveMode::StubsAllowed) {
        for module in list_modules_in(db, SearchPathIngredient::new(db, search_path.clone())) {
            match modules.entry(module.name(db)) {
                Entry::Vacant(entry) => {
                    entry.insert(module);
                }
                Entry::Occupied(mut entry) => {
                    // The only case where a module can override
                    // a module with the same name in a higher
                    // precedent search path is if the higher
                    // precedent search path contained a namespace
                    // package and the lower precedent search path
                    // contained a "regular" module.
                    if let (None, Some(_)) = (entry.get().search_path(db), module.search_path(db)) {
                        entry.insert(module);
                    }
                }
            }
        }
    }
    modules.into_values().collect()
}

#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
struct SearchPathIngredient<'db> {
    #[returns(ref)]
    path: SearchPath,
}

/// List all available top-level modules in the given `SearchPath`.
#[salsa::tracked]
fn list_modules_in<'db>(
    db: &'db dyn Db,
    search_path: SearchPathIngredient<'db>,
) -> Vec<Module<'db>> {
    tracing::debug!("Listing modules in search path '{}'", search_path.path(db));
    let mut lister = Lister::new(db, search_path.path(db));
    match search_path.path(db).as_path() {
        SystemOrVendoredPathRef::System(system_search_path) => {
            // Read the revision on the corresponding file root to
            // register an explicit dependency on this directory. When
            // the revision gets bumped, the cache that Salsa creates
            // for this routine will be invalidated.
            let root = db.files().expect_root(db, system_search_path);
            let _ = root.revision(db);

            let Ok(it) = db.system().read_directory(system_search_path) else {
                return vec![];
            };
            for result in it {
                let Ok(entry) = result else { continue };
                lister.add_path(&entry.path().into(), entry.file_type().into());
            }
        }
        SystemOrVendoredPathRef::Vendored(vendored_search_path) => {
            for entry in db.vendored().read_directory(vendored_search_path) {
                lister.add_path(&entry.path().into(), entry.file_type().into());
            }
        }
    }
    lister.into_modules()
}

/// An implementation helper for "list all modules."
///
/// This is responsible for accumulating modules indexed by
/// module name. It also handles precedence by implementing the
/// rules that determine which module gets priority when there is
/// otherwise ambiguity (e.g., `foo.py` versus `foo/__init__.py`
/// in the same directory).
struct Lister<'db> {
    db: &'db dyn Db,
    search_path: &'db SearchPath,
    modules: BTreeMap<&'db ModuleName, Module<'db>>,
}

impl<'db> Lister<'db> {
    /// Create new state that can accumulate modules from a list
    /// of file paths.
    fn new(db: &'db dyn Db, search_path: &'db SearchPath) -> Lister<'db> {
        Lister {
            db,
            search_path,
            modules: BTreeMap::new(),
        }
    }

    /// Returns the modules collected, sorted by module name.
    fn into_modules(self) -> Vec<Module<'db>> {
        self.modules.into_values().collect()
    }

    /// Add the given `path` as a possible module to this lister. The
    /// `file_type` should be the type of `path` (file, directory or
    /// symlink).
    ///
    /// This may decide that the given path does not correspond to
    /// a valid Python module. In which case, it is dropped and this
    /// is a no-op.
    ///
    /// Callers must ensure that the path given came from the same
    /// `SearchPath` used to create this `Lister`.
    fn add_path(&mut self, path: &SystemOrVendoredPathRef<'_>, file_type: FileType) {
        let mut has_py_extension = false;
        // We must have no extension, a Python source file extension (`.py`)
        // or a Python stub file extension (`.pyi`).
        if let Some(ext) = path.extension() {
            has_py_extension = is_python_extension(ext);
            if !has_py_extension {
                return;
            }
        }

        let Some(name) = path.file_name() else { return };
        let mut module_path = self.search_path.to_module_path();
        module_path.push(name);
        let Some(module_name) = module_path.to_module_name() else {
            return;
        };

        // Some modules cannot shadow a subset of special
        // modules from the standard library.
        if !self.search_path.is_standard_library() && self.is_non_shadowable(&module_name) {
            return;
        }

        if file_type.is_possibly_directory() {
            if module_path.is_regular_package(&self.context()) {
                module_path.push("__init__");
                if let Some(file) = resolve_file_module(&module_path, &self.context()) {
                    self.add_module(
                        &module_path,
                        Module::file_module(
                            self.db,
                            module_name,
                            ModuleKind::Package,
                            self.search_path.clone(),
                            file,
                        ),
                    );
                    return;
                }
                module_path.pop();
            }

            // Otherwise, we kind of have to assume that we have a
            // namespace package, which can be any directory that
            // *doesn't* contain an `__init__.{py,pyi}`. We do need to
            // know if we have a real directory or not. If we have a
            // symlink, then this requires hitting the file system.
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
            let is_dir =
                file_type.is_definitely_directory() || module_path.is_directory(&self.context());
            if is_dir {
                if !self.search_path.is_standard_library() {
                    self.add_module(
                        &module_path,
                        Module::namespace_package(self.db, module_name),
                    );
                }
                return;
            }
            // At this point, we have a symlink that we know is not a
            // directory, so press on as if it were a regular file...
        }

        // At this point, we're looking for a file module.
        // For a file module, we require a `.py` or `.pyi`
        // extension.
        if !has_py_extension {
            return;
        }
        // We also require stub packages to be packages, not
        // single-file modules.
        if module_path.is_stub_package() {
            return;
        }

        let Some(file) = module_path.to_file(&self.context()) else {
            return;
        };
        self.add_module(
            &module_path,
            Module::file_module(
                self.db,
                module_name,
                ModuleKind::Module,
                self.search_path.clone(),
                file,
            ),
        );
    }

    /// Adds the given module to the collection.
    ///
    /// If the module had already been added and shouldn't override any
    /// existing entry, then this is a no-op. That is, this assumes that the
    /// caller looks for modules in search path priority order.
    fn add_module(&mut self, path: &ModulePath, module: Module<'db>) {
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
            }
            (Some(_), Some(_)) => {
                // Merging across search paths is only necessary for
                // namespace packages. For all other modules, entries
                // from earlier search paths take precedence. Thus, all
                // of the cases below require that we're in the same
                // directory. ... Which is true here, because a `Lister`
                // only works for one specific search path.

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
                    && path.is_stub_file()
                {
                    entry.insert(module);
                    return;
                }
                // Or... if we have a stub package, the stub package
                // always gets priority.
                if path.is_stub_package() {
                    entry.insert(module);
                }
            }
            _ => {}
        }
    }

    /// Returns true if the given module name cannot be shadowable.
    fn is_non_shadowable(&self, name: &ModuleName) -> bool {
        ModuleResolveMode::StubsAllowed
            .is_non_shadowable(self.python_version().minor, name.as_str())
    }

    /// Returns the Python version we want to perform module resolution
    /// with.
    fn python_version(&self) -> PythonVersion {
        self.db.python_version()
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

/// The type of a file.
#[derive(Clone, Copy, Debug)]
enum FileType {
    File,
    Directory,
    Symlink,
}

impl FileType {
    fn is_possibly_directory(self) -> bool {
        matches!(self, FileType::Directory | FileType::Symlink)
    }

    fn is_definitely_directory(self) -> bool {
        matches!(self, FileType::Directory)
    }
}

impl From<ruff_db::vendored::FileType> for FileType {
    fn from(ft: ruff_db::vendored::FileType) -> FileType {
        match ft {
            ruff_db::vendored::FileType::File => FileType::File,
            ruff_db::vendored::FileType::Directory => FileType::Directory,
        }
    }
}

impl From<ruff_db::system::FileType> for FileType {
    fn from(ft: ruff_db::system::FileType) -> FileType {
        match ft {
            ruff_db::system::FileType::File => FileType::File,
            ruff_db::system::FileType::Directory => FileType::Directory,
            ruff_db::system::FileType::Symlink => FileType::Symlink,
        }
    }
}

/// Returns true if and only if the given file extension corresponds
/// to a Python source or stub file.
fn is_python_extension(ext: &str) -> bool {
    matches!(ext, "py" | "pyi")
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
    use crate::resolve::{
        ModuleResolveMode, ModuleResolveModeIngredient, dynamic_resolution_paths,
    };
    use crate::settings::SearchPathSettings;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::list_modules;

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
                        FilePath::System(path) => path.as_path().components(),
                        FilePath::Vendored(path) => path.as_path().components(),
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

    fn sorted_list(db: &dyn Db) -> Vec<Module<'_>> {
        let mut modules = list_modules(db);
        modules.sort_by(|m1, m2| m1.name(db).cmp(m2.name(db)));
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
                .to_search_paths(db.system(), db.vendored())
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
            Module::File("functools", "std-custom", "/typeshed/stdlib/functools.pyi", Module, None),
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
            ModuleResolveModeIngredient::new(&db, ModuleResolveMode::StubsAllowed),
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
                .to_search_paths(db.system(), db.vendored())
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
                .to_search_paths(db.system(), db.vendored())
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
            .to_search_paths(db.system(), db.vendored())
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
                .to_search_paths(db.system(), db.vendored())
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
            @r#"
        []
        "#,
        );
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
            .to_search_paths(db.system(), db.vendored())
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
}
