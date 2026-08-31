use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use compact_str::CompactString;
use ruff_db::Db as _;
use ruff_db::diagnostic::{Diagnostic, Severity, UnifiedFile};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPathBuf};
use ruff_ranged_value::ValueSource;
use ty_module_resolver::{ModuleName, SearchPathSettings};
use ty_python_core::program::FallibleStrategy;
use ty_python_semantic::dependency::{
    DependencyDistribution, DependencyMetadata, DependencyProject, DependencyProjectKind,
};

use crate::db::testing::TestDb;
use crate::metadata::options::Options;
use crate::script::Script;
use crate::{Db as _, ProjectMetadata};

use super::{project_diagnostics, script_diagnostics};

fn database(rule: &str, modules: &[&str]) -> anyhow::Result<TestDb> {
    let options = format!("[rules]\nunused-dependency = '{rule}'");
    database_with_options(&options, modules, ValueSource::Cli)
}

fn database_with_options(
    options: &str,
    modules: &[&str],
    source: ValueSource,
) -> anyhow::Result<TestDb> {
    let root = SystemPathBuf::from("/project");
    let mut metadata = ProjectMetadata::new("app", root.clone());
    metadata.apply_override_options(Options::from_toml_str(options, source)?);
    let mut db = TestDb::new(metadata);
    for module in modules {
        db.write_file(format!("/site-packages/{module}.py"), "")?;
    }
    db.memory_file_system()
        .create_directory_all("/site-packages")?;
    let mut paths = SearchPathSettings::new(vec![root]);
    paths.site_packages_paths = vec![SystemPathBuf::from("/site-packages")];
    let project = db.project();
    let mut settings = project.program_settings(&db).clone();
    settings.search_paths = paths.to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?;
    project.update_program(&mut db, settings);
    Ok(db)
}

fn metadata(
    projects: &[(&str, DependencyProjectKind, &[&str])],
) -> anyhow::Result<DependencyMetadata> {
    let mut distributions = BTreeMap::new();
    let mut module_owners = BTreeMap::new();
    let mut dependency_projects = Vec::new();
    for (path, kind, dependencies) in projects {
        let mut ids = BTreeSet::new();
        for name in *dependencies {
            let id = CompactString::from(format!("distribution:{name}"));
            distributions.insert(
                id.clone(),
                DependencyDistribution {
                    name: CompactString::new(name),
                    editable_path: None,
                },
            );
            module_owners.insert(
                ModuleName::new(&name.replace('-', "_")).context("valid module name")?,
                vec![id.clone()].into_boxed_slice(),
            );
            ids.insert(id);
        }
        dependency_projects.push(DependencyProject {
            path: SystemPathBuf::from(*path),
            kind: *kind,
            distribution: None,
            dependencies: ids,
            group_dependencies: BTreeSet::new(),
        });
    }
    Ok(DependencyMetadata {
        projects: dependency_projects.into_boxed_slice(),
        distributions,
        module_owners,
    })
}

fn declarations(
    db: &TestDb,
    diagnostics: &[Diagnostic],
) -> anyhow::Result<Vec<(SystemPathBuf, String)>> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.id().as_str(), "unused-dependency");
            let span = diagnostic.primary_span().context("declaration span")?;
            let UnifiedFile::Ty(file) = span.file() else {
                anyhow::bail!("expected a ty file");
            };
            let range = span.range().context("declaration range")?;
            Ok((
                file.path(db)
                    .as_system_path()
                    .context("declaration path")?
                    .to_path_buf(),
                source_text(db, *file)[range].to_owned(),
            ))
        })
        .collect()
}

fn script<'db>(db: &'db TestDb, path: &str) -> anyhow::Result<Script<'db>> {
    let file = system_path_to_file(db, path)?;
    Ok(script_with_project_program(db, file))
}

#[salsa::tracked(returns(copy))]
fn script_with_project_program(db: &dyn crate::Db, file: File) -> Script<'_> {
    let project = db.project();
    Script::new(
        db,
        file,
        project.settings(db).clone(),
        project.program(db),
        project.program_settings(db).python_version.clone(),
        true,
        Box::default(),
    )
}

#[test]
fn declaration_file_rule_override() -> anyhow::Result<()> {
    let mut db = database_with_options(
        "[rules]\nunused-dependency = 'error'\n\n[[overrides]]\ninclude = ['pyproject.toml']\n[overrides.rules]\nunused-dependency = 'ignore'\n",
        &["unused_lib"],
        ValueSource::File(SystemPathBuf::from("/project/ty.toml").into()),
    )?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['unused-lib']\n",
    )?;
    db.write_file("/project/main.py", "")?;
    let metadata = metadata(&[("/project", DependencyProjectKind::Project, &["unused-lib"])])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());
    Ok(())
}

#[test]
fn unopened_file_import_counts_for_project() -> anyhow::Result<()> {
    let mut db = database("warn", &["used_lib"])?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['used-lib']\n",
    )?;
    db.write_file("/project/main.py", "")?;
    db.write_file("/project/worker.py", "import used_lib\n")?;
    let main = system_path_to_file(&db, "/project/main.py")?;
    db.project().open_file(&mut db, main);
    let metadata = metadata(&[("/project", DependencyProjectKind::Project, &["used-lib"])])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/worker.py", "")?;
    assert_eq!(project_diagnostics(&db, &metadata).len(), 1);
    Ok(())
}

#[test]
fn local_module_shadowing_does_not_use_installed_distribution() -> anyhow::Result<()> {
    let mut db = database("warn", &["shared_lib"])?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['shared-lib']\n",
    )?;
    db.write_file("/project/main.py", "import shared_lib\n")?;
    db.write_file("/project/shared_lib.py", "")?;
    let metadata = metadata(&[("/project", DependencyProjectKind::Project, &["shared-lib"])])?;
    assert_eq!(
        declarations(&db, &project_diagnostics(&db, &metadata))?,
        [("/project/pyproject.toml".into(), "'shared-lib'".into())]
    );
    Ok(())
}

#[test]
fn stub_only_dependencies_without_module_ownership_are_skipped() -> anyhow::Result<()> {
    let mut db = database("warn", &["shared_lib"])?;
    db.write_file("/site-packages/shared_lib-stubs/__init__.pyi", "")?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['shared-lib', 'shared-lib-stubs']\n",
    )?;
    db.write_file("/project/main.py", "import shared_lib\n")?;
    let mut metadata = metadata(&[(
        "/project",
        DependencyProjectKind::Project,
        &["shared-lib", "shared-lib-stubs"],
    )])?;
    metadata.module_owners.clear();
    metadata.module_owners.insert(
        ModuleName::new("shared_lib").context("valid module name")?,
        vec![CompactString::new("distribution:shared-lib")].into_boxed_slice(),
    );
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/main.py", "")?;
    assert_eq!(
        declarations(&db, &project_diagnostics(&db, &metadata))?,
        [("/project/pyproject.toml".into(), "'shared-lib'".into())]
    );
    Ok(())
}

#[test]
fn namespace_import_credits_all_owners_but_child_import_is_specific() -> anyhow::Result<()> {
    let mut db = database("warn", &[])?;
    db.write_file("/site-packages/shared/a.py", "")?;
    db.write_file("/site-packages/shared/b.py", "")?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['a-lib', 'b-lib']\n",
    )?;
    db.write_file("/project/main.py", "import shared\n")?;
    let mut metadata = metadata(&[(
        "/project",
        DependencyProjectKind::Project,
        &["a-lib", "b-lib"],
    )])?;
    metadata.module_owners.clear();
    for (module, owners) in [
        ("shared", &["a-lib", "b-lib"][..]),
        ("shared.a", &["a-lib"][..]),
        ("shared.b", &["b-lib"][..]),
    ] {
        metadata.module_owners.insert(
            ModuleName::new(module).context("valid module name")?,
            owners
                .iter()
                .map(|name| CompactString::from(format!("distribution:{name}")))
                .collect(),
        );
    }
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/main.py", "import shared.a\n")?;
    assert_eq!(
        declarations(&db, &project_diagnostics(&db, &metadata))?,
        [("/project/pyproject.toml".into(), "'b-lib'".into())]
    );
    Ok(())
}

#[test]
fn runtime_and_optional_declarations_exclude_groups_and_markers() -> anyhow::Result<()> {
    let mut db = database(
        "error",
        &["unused_lib", "optional_lib", "conditional_lib", "dev_tool"],
    )?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['Unused.Lib>=1', \"conditional-lib; python_version >= '3.0'\"]\n\n[project.optional-dependencies]\nfeature = ['optional-lib']\n\n[dependency-groups]\ndev = ['dev-tool', 'unused-lib']\n\n[build-system]\nrequires = ['unused-lib']\n",
    )?;
    db.write_file("/project/main.py", "")?;
    let mut metadata = metadata(&[(
        "/project",
        DependencyProjectKind::Project,
        &["unused-lib", "optional-lib", "conditional-lib", "dev-tool"],
    )])?;
    let member = &mut metadata.projects[0];
    member.dependencies.remove("distribution:dev-tool");
    member
        .group_dependencies
        .insert(CompactString::new("distribution:dev-tool"));
    let diagnostics = project_diagnostics(&db, &metadata);
    assert_eq!(
        declarations(&db, &diagnostics)?,
        [
            ("/project/pyproject.toml".into(), "'Unused.Lib>=1'".into()),
            ("/project/pyproject.toml".into(), "'optional-lib'".into()),
        ]
    );
    assert_eq!(diagnostics[0].severity(), Severity::Error);
    Ok(())
}

#[test]
fn custom_source_filters_do_not_prove_unused_dependencies() -> anyhow::Result<()> {
    for filter in ["include = ['src']", "exclude = ['worker.py']"] {
        let options = format!("[rules]\nunused-dependency = 'warn'\n[src]\n{filter}\n");
        let mut db = database_with_options(
            &options,
            &["used_lib"],
            ValueSource::File(SystemPathBuf::from("/project/ty.toml").into()),
        )?;
        db.write_file(
            "/project/pyproject.toml",
            "[project]\ndependencies = ['used-lib']\n",
        )?;
        db.write_file("/project/src/main.py", "")?;
        db.write_file("/project/worker.py", "import used_lib\n")?;
        let worker = system_path_to_file(&db, "/project/worker.py")?;
        assert!(!db.project().files(&db).contains(worker));
        let project_metadata =
            metadata(&[("/project", DependencyProjectKind::Project, &["used-lib"])])?;
        assert!(project_diagnostics(&db, &project_metadata).is_empty());
    }
    Ok(())
}

#[test]
fn complete_member_check_and_import_isolation() -> anyhow::Result<()> {
    let mut db = database("warn", &["shared_lib"])?;
    for member in ["a", "b"] {
        db.write_file(
            format!("/project/{member}/pyproject.toml"),
            "[project]\ndependencies = ['shared-lib']\n",
        )?;
    }
    db.write_file("/project/a/main.py", "import b.main\n")?;
    db.write_file("/project/b/main.py", "import shared_lib\n")?;
    db.write_file(
        "/project/a/script.py",
        "# /// script\n# dependencies = ['shared-lib']\n# ///\nimport shared_lib\n",
    )?;
    let metadata = metadata(&[
        (
            "/project/a",
            DependencyProjectKind::Project,
            &["shared-lib"],
        ),
        (
            "/project/b",
            DependencyProjectKind::Project,
            &["shared-lib"],
        ),
    ])?;
    assert_eq!(
        declarations(&db, &project_diagnostics(&db, &metadata))?,
        [("/project/a/pyproject.toml".into(), "'shared-lib'".into())]
    );

    db.project()
        .set_included_paths(&mut db, vec![SystemPathBuf::from("/project/a")]);
    assert_eq!(
        declarations(&db, &project_diagnostics(&db, &metadata))?,
        [("/project/a/pyproject.toml".into(), "'shared-lib'".into())]
    );
    db.project()
        .set_included_paths(&mut db, vec![SystemPathBuf::from("/project/a/main.py")]);
    assert!(project_diagnostics(&db, &metadata).is_empty());
    Ok(())
}

#[test]
fn incomplete_project_analysis_does_not_report_unused_dependencies() -> anyhow::Result<()> {
    let mut db = database("warn", &["unused_lib"])?;
    db.write_file(
        "/project/pyproject.toml",
        "[project]\ndependencies = ['unused-lib']\n",
    )?;
    db.write_file("/project/main.py", "def incomplete(\n")?;
    let metadata = metadata(&[("/project", DependencyProjectKind::Project, &["unused-lib"])])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/main.py", "")?;
    assert_eq!(project_diagnostics(&db, &metadata).len(), 1);
    let file = system_path_to_file(&db, "/project/main.py")?;
    db.memory_file_system().remove_file("/project/main.py")?;
    file.sync(&mut db);
    assert!(project_diagnostics(&db, &metadata).is_empty());
    Ok(())
}

#[test]
fn script_reachable_helpers_cycles_and_edits() -> anyhow::Result<()> {
    let mut db = database("warn", &["used_lib"])?;
    db.write_file(
        "/project/script.py",
        "# /// script\n# dependencies = ['used-lib']\n# ///\nimport helper\n",
    )?;
    db.write_file("/project/helper.pyi", "")?;
    db.write_file("/project/helper.py", "import other\n")?;
    db.write_file("/project/other.py", "import helper\nimport used_lib\n")?;
    db.write_file("/project/unrelated.py", "import used_lib\n")?;
    let metadata = metadata(&[(
        "/project/script.py",
        DependencyProjectKind::Script,
        &["used-lib"],
    )])?;
    assert!(script_diagnostics(&db, script(&db, "/project/script.py")?, &metadata).is_empty());

    db.write_file("/project/other.py", "import helper\n")?;
    assert_eq!(
        declarations(
            &db,
            &script_diagnostics(&db, script(&db, "/project/script.py")?, &metadata)
        )?,
        [("/project/script.py".into(), "'used-lib'".into())]
    );
    Ok(())
}
