use std::fs;

use anyhow::Result;
use ruff_db::system::{OsSystem, SystemPathBuf};
use ruff_python_ast::{PySourceType, PythonVersion};
use tempfile::TempDir;

use crate::{AnalyzeOptions, ImportDb, analyze_file};

fn system_path(path: &std::path::Path) -> SystemPathBuf {
    SystemPathBuf::from_path_buf(path.to_path_buf()).expect("expected UTF-8 path")
}

fn write_file(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn occurrence_range_preserved() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    write_file(&root.join("foo.py"), "");

    let importer_path = root.join("main.py");
    let source = "import foo\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 1);
    assert_eq!(&source[imports[0].occurrence.range], "foo");
    assert_eq!(
        imports[0]
            .resolved_module
            .as_ref()
            .map(ty_module_resolver::ModuleName::as_str),
        Some("foo")
    );

    Ok(())
}

#[test]
fn winning_root_reported() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    let first = root.join("first");
    let second = root.join("second");
    write_file(&first.join("foo.py"), "");
    write_file(&second.join("foo.py"), "");

    let importer_path = first.join("main.py");
    let source = "import foo\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(&first), system_path(&second)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].winning_root, Some(0));
    assert_eq!(
        imports[0].resolved_path.as_ref(),
        Some(&system_path(&first.join("foo.py")))
    );
    Ok(())
}

#[test]
fn unresolved_import_preserved() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    let importer_path = root.join("main.py");
    let source = "import missing\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].occurrence.requested.as_str(), "missing");
    assert!(imports[0].resolved_module.is_none());
    assert!(imports[0].resolved_path.is_none());
    assert!(imports[0].winning_root.is_none());

    Ok(())
}

#[test]
fn site_packages_root_index_reported_via_from_roots() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    let src = root.join("src");
    let site_packages = root.join("site-packages");
    write_file(&src.join("myapp.py"), "");
    write_file(&site_packages.join("dep/__init__.py"), "");

    let importer_path = src.join("main.py");
    let source = "import myapp\nimport dep\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_roots(
        OsSystem::default(),
        vec![system_path(&src)],
        vec![system_path(&site_packages)],
        PythonVersion::PY312,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 2);

    assert_eq!(imports[0].occurrence.requested.as_str(), "myapp");
    assert_eq!(imports[0].winning_root, Some(0));

    assert_eq!(imports[1].occurrence.requested.as_str(), "dep");
    assert_eq!(imports[1].winning_root, Some(1));

    Ok(())
}

#[test]
fn equivalent_roots_are_canonicalized_before_indexing() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    let first = root.join("first");
    let second = root.join("second");
    write_file(&first.join("foo.py"), "");
    write_file(&second.join("bar.py"), "");

    let importer_path = second.join("main.py");
    let source = "import bar\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_roots(
        OsSystem::default(),
        vec![
            system_path(&first),
            system_path(&root.join("first/./../first")),
            system_path(&second),
        ],
        Vec::new(),
        PythonVersion::PY312,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].occurrence.requested.as_str(), "bar");
    assert_eq!(imports[0].winning_root, Some(1));

    Ok(())
}

#[test]
fn type_checking_occurrences_are_marked() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    write_file(&root.join("foo.py"), "");
    write_file(&root.join("bar.py"), "");

    let importer_path = root.join("main.py");
    let source = "if TYPE_CHECKING:\n    import foo\n\nimport bar\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0].occurrence.requested.as_str(), "foo");
    assert!(imports[0].occurrence.in_type_checking);
    assert_eq!(imports[1].occurrence.requested.as_str(), "bar");
    assert!(!imports[1].occurrence.in_type_checking);

    Ok(())
}

#[test]
fn type_checking_else_branch_is_marked_after_not_type_checking() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    write_file(&root.join("foo.py"), "");

    let importer_path = root.join("main.py");
    let source = "if not TYPE_CHECKING:\n    pass\nelse:\n    import foo\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions::default(),
    )?;

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].occurrence.requested.as_str(), "foo");
    assert!(imports[0].occurrence.in_type_checking);

    Ok(())
}

#[test]
fn string_imports_in_if_tests_are_collected() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    write_file(&root.join("alpha.py"), "");
    write_file(&root.join("beta.py"), "");

    let importer_path = root.join("main.py");
    let source = "if importlib.util.find_spec(\"alpha\"):\n    pass\nelif importlib.util.find_spec(\"beta\"):\n    pass\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions {
            string_imports: crate::StringImports {
                enabled: true,
                min_dots: 0,
            },
            type_checking_imports: true,
        },
    )?;

    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0].occurrence.requested.as_str(), "alpha");
    assert_eq!(imports[1].occurrence.requested.as_str(), "beta");

    Ok(())
}

#[test]
fn type_checking_imports_can_be_excluded_in_else_branch() -> Result<()> {
    let tempdir = TempDir::new()?;
    let root = tempdir.path();

    write_file(&root.join("foo.py"), "");

    let importer_path = root.join("main.py");
    let source = "if not TYPE_CHECKING:\n    pass\nelse:\n    import foo\n";
    write_file(&importer_path, source);

    let db = ImportDb::from_src_roots(
        OsSystem::default(),
        vec![system_path(root)],
        PythonVersion::PY312,
        None,
    )?;

    let importer_system_path = system_path(&importer_path);
    let imports = analyze_file(
        &db,
        &importer_system_path,
        None,
        source,
        PySourceType::Python,
        &AnalyzeOptions {
            string_imports: crate::StringImports::default(),
            type_checking_imports: false,
        },
    )?;

    assert!(imports.is_empty());

    Ok(())
}
