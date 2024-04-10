#![allow(dead_code)]

mod config;
mod execution_environment;
mod host;
mod implicit_imports;
mod import_result;
mod module_descriptor;
mod native_module;
mod py_typed;
mod python_platform;
mod python_version;
mod resolver;
mod search;

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, File};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};

    use log::debug;
    use tempfile::TempDir;

    use crate::config::Config;
    use crate::execution_environment::ExecutionEnvironment;
    use crate::host;
    use crate::import_result::{ImportResult, ImportType};
    use crate::module_descriptor::ImportModuleDescriptor;
    use crate::python_platform::PythonPlatform;
    use crate::python_version::PythonVersion;
    use crate::resolver::resolve_import;

    /// Create a file at the given path with the given content.
    fn create(path: PathBuf, content: &str) -> io::Result<PathBuf> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        let mut f = File::create(&path)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;

        Ok(path)
    }

    /// Create an empty file at the given path.
    fn empty(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "")
    }

    /// Create a partial `py.typed` file at the given path.
    fn partial(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "partial\n")
    }

    /// Create a `py.typed` file at the given path.
    fn typed(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "# typed")
    }

    #[derive(Debug, Default)]
    struct ResolverOptions {
        extra_paths: Vec<PathBuf>,
        library: Option<PathBuf>,
        stub_path: Option<PathBuf>,
        typeshed_path: Option<PathBuf>,
        venv_path: Option<PathBuf>,
        venv: Option<PathBuf>,
    }

    fn resolve_options(
        source_file: impl AsRef<Path>,
        name: &str,
        root: impl Into<PathBuf>,
        options: ResolverOptions,
    ) -> ImportResult {
        let ResolverOptions {
            extra_paths,
            library,
            stub_path,
            typeshed_path,
            venv_path,
            venv,
        } = options;

        let execution_environment = ExecutionEnvironment {
            root: root.into(),
            python_version: PythonVersion::Py37,
            python_platform: PythonPlatform::Darwin,
            extra_paths,
        };

        let module_descriptor = ImportModuleDescriptor {
            leading_dots: name.chars().take_while(|c| *c == '.').count(),
            name_parts: name
                .chars()
                .skip_while(|c| *c == '.')
                .collect::<String>()
                .split('.')
                .map(std::string::ToString::to_string)
                .collect(),
            imported_symbols: Vec::new(),
        };

        let config = Config {
            typeshed_path,
            stub_path,
            venv_path,
            venv,
        };

        let host = host::StaticHost::new(if let Some(library) = library {
            vec![library]
        } else {
            Vec::new()
        });

        resolve_import(
            source_file.as_ref(),
            &execution_environment,
            &module_descriptor,
            &config,
            &host,
        )
    }

    fn setup() {
        env_logger::builder().is_test(true).try_init().ok();
    }

    macro_rules! assert_debug_snapshot_normalize_paths {
        ($value: ident) => {{
            // The debug representation for the backslash are two backslashes (escaping)
            let $value = std::format!("{:#?}", $value).replace("\\\\", "/");
            insta::assert_snapshot!($value);
        }};
    }

    #[test]
    fn partial_stub_file_exists() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_pyi = empty(library.join("myLib-stubs").join("partialStub.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(
            result.resolved_paths,
            // TODO(charlie): Pyright matches on `libraryRoot, 'myLib', 'partialStub.pyi'` here.
            // But that file doesn't exist. There's some kind of transform.
            vec![PathBuf::new(), partial_stub_pyi]
        );

        Ok(())
    }

    #[test]
    fn partial_stub_init_exists() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let partial_stub_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            partial_stub_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(
            result.resolved_paths,
            // TODO(charlie): Pyright matches on `libraryRoot, 'myLib', '__init__.pyi'` here.
            // But that file doesn't exist. There's some kind of transform.
            vec![partial_stub_init_pyi]
        );

        Ok(())
    }

    #[test]
    fn side_by_side_files() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        empty(library.join("myLib/partialStub.pyi"))?;
        empty(library.join("myLib/partialStub.py"))?;
        empty(library.join("myLib/partialStub2.py"))?;
        let my_file = empty(root.join("myFile.py"))?;
        let side_by_side_stub_file = empty(library.join("myLib-stubs/partialStub.pyi"))?;
        let partial_stub_file = empty(library.join("myLib-stubs/partialStub2.pyi"))?;

        // Stub package wins over original package (per PEP 561 rules).
        let side_by_side_result = resolve_options(
            &my_file,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library.clone()),
                ..Default::default()
            },
        );
        assert!(side_by_side_result.is_import_found);
        assert!(side_by_side_result.is_stub_file);
        assert_eq!(
            side_by_side_result.resolved_paths,
            vec![PathBuf::new(), side_by_side_stub_file]
        );

        // Side by side stub doesn't completely disable partial stub.
        let partial_stub_result = resolve_options(
            &my_file,
            "myLib.partialStub2",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );
        assert!(partial_stub_result.is_import_found);
        assert!(partial_stub_result.is_stub_file);
        assert_eq!(
            partial_stub_result.resolved_paths,
            vec![PathBuf::new(), partial_stub_file]
        );

        Ok(())
    }

    #[test]
    fn stub_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib-stubs/stub.pyi"))?;
        empty(library.join("myLib-stubs/__init__.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // If fully typed stub package exists, that wins over the real package.
        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn stub_namespace_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib-stubs/stub.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py.clone(),
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // If fully typed stub package exists, that wins over the real package.
        assert!(result.is_import_found);
        assert!(!result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![PathBuf::new(), partial_stub_py]);

        Ok(())
    }

    #[test]
    fn stub_in_typing_folder_over_partial_stub_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typing_folder = root.join("typing");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        empty(library.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_pyi = empty(typing_folder.join("myLib.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                stub_path: Some(typing_folder),
                ..Default::default()
            },
        );

        // If the package exists in typing folder, that gets picked up first (so we resolve to
        // `myLib.pyi`).
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_pyi]);

        Ok(())
    }

    #[test]
    fn partial_stub_package_in_typing_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typing_folder = root.join("typing");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(typing_folder.join("myLib-stubs/py.typed"))?;
        let my_lib_stubs_init_pyi = empty(typing_folder.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                stub_path: Some(typing_folder),
                ..Default::default()
            },
        );

        // If the package exists in typing folder, that gets picked up first (so we resolve to
        // `myLib.pyi`).
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_stubs_init_pyi]);

        Ok(())
    }

    #[test]
    fn typeshed_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(typeshed_folder.join("stubs/myLibPackage/myLib.pyi"))?;
        partial(library.join("myLib-stubs/py.typed"))?;
        let my_lib_stubs_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        // Stub packages win over typeshed.
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_stubs_init_pyi]);

        Ok(())
    }

    #[test]
    fn py_typed_file() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib/__init__.py"))?;
        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let package_py_typed = typed(library.join("myLib/py.typed"))?;

        let result = resolve_options(
            package_py_typed,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // Partial stub package always overrides original package.
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![partial_stub_init_pyi]);

        Ok(())
    }

    #[test]
    fn py_typed_library() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        typed(library.join("os/py.typed"))?;
        let init_py = empty(library.join("os/__init__.py"))?;
        let typeshed_init_pyi = empty(typeshed_folder.join("stubs/os/os/__init__.pyi"))?;

        let result = resolve_options(
            typeshed_init_pyi,
            "os",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.resolved_paths, vec![init_py]);

        Ok(())
    }

    #[test]
    fn non_py_typed_library() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("os/__init__.py"))?;
        let typeshed_init_pyi = empty(typeshed_folder.join("stubs/os/os/__init__.pyi"))?;

        let result = resolve_options(
            typeshed_init_pyi.clone(),
            "os",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(result.resolved_paths, vec![typeshed_init_pyi]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file1 = empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, "file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let test_init = empty(root.join("test/__init__.py"))?;
        let test_file1 = empty(root.join("test/file1.py"))?;
        let test_file2 = empty(root.join("test/file2.py"))?;

        let result = resolve_options(test_file2, "test.file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![test_init, test_file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_under_src_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let nested_init = empty(root.join("src/nested/__init__.py"))?;
        let nested_file1 = empty(root.join("src/nested/file1.py"))?;
        let nested_file2 = empty(root.join("src/nested/file2.py"))?;

        let result = resolve_options(
            nested_file2,
            "nested.file1",
            root,
            ResolverOptions::default(),
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![nested_init, nested_file1]);

        Ok(())
    }

    #[test]
    fn import_file_sub_under_containing_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let nested_file1 = empty(root.join("src/nested/file1.py"))?;
        let nested_file2 = empty(root.join("src/nested/nested2/file2.py"))?;

        let result = resolve_options(nested_file2, "file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![nested_file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_under_lib_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib/file1.py"))?;
        let file2 = empty(library.join("myLib/file2.py"))?;

        let result = resolve_options(file2, "file1", root, ResolverOptions::default());

        debug!("result: {:?}", result);

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn nested_namespace_package_1() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file = empty(root.join("package1/a/b/c/d.py"))?;
        let package1_init = empty(root.join("package1/a/__init__.py"))?;
        let package2_init = empty(root.join("package2/a/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(
            result.resolved_paths,
            vec![package1_init, PathBuf::new(), PathBuf::new(), file]
        );

        Ok(())
    }

    #[test]
    fn nested_namespace_package_2() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file = empty(root.join("package1/a/b/c/d.py"))?;
        let package1_init = empty(root.join("package1/a/b/c/__init__.py"))?;
        let package2_init = empty(root.join("package2/a/b/c/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(
            result.resolved_paths,
            vec![PathBuf::new(), PathBuf::new(), package1_init, file]
        );

        Ok(())
    }

    #[test]
    fn nested_namespace_package_3() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("package1/a/b/c/d.py"))?;
        let package2_init = empty(root.join("package2/a/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn nested_namespace_package_4() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("package1/a/b/__init__.py"))?;
        empty(root.join("package1/a/b/c.py"))?;
        empty(root.join("package2/a/__init__.py"))?;
        let package2_a_b_init = empty(root.join("package2/a/b/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_a_b_init,
            "a.b.c",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(!result.is_import_found);

        Ok(())
    }

    // New tests, don't exist upstream.
    #[test]
    fn relative_import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file1 = empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, ".file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![file1]);

        Ok(())
    }

    #[test]
    fn invalid_relative_import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, "..file1", root, ResolverOptions::default());

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn airflow_standard_library() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "os",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_first_party() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.jobs.scheduler_job_runner",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_stub_file() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.compat.functools",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_namespace_package() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.providers.google.cloud.hooks.gcs",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_third_party() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "sqlalchemy.orm",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_explicit_native_module() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "_watchdog_fsevents",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }

    #[test]
    fn airflow_implicit_native_module() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "orjson",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot_normalize_paths!(result);
    }
}
