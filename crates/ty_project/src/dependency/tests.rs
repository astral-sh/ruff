use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use char_str::CharStr;
use ruff_db::Db as _;
use ruff_db::diagnostic::{DisplayDiagnosticConfig, DisplayDiagnostics, Severity, UnifiedFile};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPathBuf};
use ruff_ranged_value::ValueSource;
use ty_combine::Combine;
use ty_module_resolver::{ModuleName, SearchPathSettings};
use ty_python_core::program::{FallibleStrategy, Program};
use ty_python_semantic::dependency::{
    DependencyDistribution, DependencyMetadata, DependencyProject, DependencyProjectKind,
};

use crate::db::testing::TestDb;
use crate::metadata::options::Options;
use crate::script::Script;
use crate::{Db as _, ProjectMetadata};

use super::{project_diagnostics, script_diagnostics};

fn database(modules: &[&str], options: &str) -> anyhow::Result<TestDb> {
    let root = SystemPathBuf::from("/project");
    let mut metadata = ProjectMetadata::new("app", root.clone());
    metadata.set_override_options(
        Options::from_toml_str(options, ValueSource::File(root.join("ty.toml").into()))?.combine(
            Options::from_toml_str("[rules]\nunused-dependency = 'warn'", ValueSource::Cli)?,
        ),
    );
    let mut db = TestDb::new(metadata);
    db.write_file("/project/pyproject.toml", "[project]\n")?;
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

fn metadata(packages: &[(&str, &[&str])]) -> anyhow::Result<DependencyMetadata> {
    let mut distributions = BTreeMap::new();
    let mut module_owners: BTreeMap<ModuleName, Vec<CharStr>> = BTreeMap::new();
    let mut dependencies = BTreeSet::new();
    for (name, modules) in packages {
        let id = CharStr::from(format!("distribution:{name}"));
        distributions.insert(
            id.clone(),
            DependencyDistribution {
                name: CharStr::from(*name),
                editable_path: None,
            },
        );
        for module in *modules {
            module_owners
                .entry(ModuleName::new(module).context("valid module name")?)
                .or_default()
                .push(id.clone());
        }
        dependencies.insert(id);
    }
    Ok(DependencyMetadata {
        projects: Box::new([DependencyProject {
            path: SystemPathBuf::from("/project"),
            kind: DependencyProjectKind::Project,
            distribution: None,
            dependencies,
            group_dependencies: BTreeSet::new(),
        }]),
        distributions,
        module_owners: module_owners
            .into_iter()
            .map(|(module, owners)| (module, owners.into_boxed_slice()))
            .collect(),
    })
}

#[salsa::tracked(returns(copy))]
fn script_with_program<'db>(
    db: &'db dyn crate::Db,
    file: File,
    program: Program<'db>,
) -> Script<'db> {
    let project = db.project();
    Script::new(
        db,
        file,
        project.settings(db).clone(),
        program,
        project.program_settings(db).python_version.clone(),
        true,
        Box::default(),
    )
}

#[test]
fn declaration_file_rule_override() -> anyhow::Result<()> {
    let mut db = database(
        &["unused_lib"],
        r#"[rules]
unused-dependency = 'error'

[[overrides]]
include = ['pyproject.toml']
[overrides.rules]
unused-dependency = 'ignore'
"#,
    )?;
    db.write_file("/project/main.py", "")?;
    let metadata = metadata(&[("unused-lib", &["unused_lib"])])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());
    Ok(())
}

#[test]
fn runtime_ownership_distinguishes_stubs_and_local_modules() -> anyhow::Result<()> {
    let mut db = database(&["shared_lib"], "")?;
    db.write_file("/site-packages/shared_lib-stubs/__init__.pyi", "")?;
    db.write_file("/project/main.py", "import shared_lib\n")?;
    let metadata = metadata(&[("shared-lib", &["shared_lib"]), ("shared-lib-stubs", &[])])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/shared_lib.py", "")?;
    let diagnostics = project_diagnostics(&db, &metadata);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].headline_message().contains("`shared-lib`"));
    Ok(())
}

#[test]
fn namespace_import_credits_all_owners_but_child_import_is_specific() -> anyhow::Result<()> {
    let mut db = database(&[], "")?;
    db.write_file("/site-packages/shared/a.py", "")?;
    db.write_file("/site-packages/shared/b.py", "")?;
    db.write_file("/project/main.py", "import shared\n")?;
    let metadata = metadata(&[
        ("a-lib", &["shared", "shared.a"]),
        ("b-lib", &["shared", "shared.b"]),
    ])?;
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/main.py", "import shared.a\n")?;
    let diagnostics = project_diagnostics(&db, &metadata);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].headline_message().contains("`b-lib`"));
    Ok(())
}

#[test]
fn unopened_imports_count_and_dependency_groups_are_skipped() -> anyhow::Result<()> {
    let mut db = database(
        &["used_lib", "dev_tool"],
        "[rules]\nunused-dependency = 'error'",
    )?;
    db.write_file("/project/main.py", "")?;
    db.write_file("/project/worker.py", "import used_lib\n")?;
    let main = system_path_to_file(&db, "/project/main.py")?;
    db.project().open_file(&mut db, main);
    let mut metadata = metadata(&[("used-lib", &["used_lib"]), ("dev-tool", &["dev_tool"])])?;
    let member = &mut metadata.projects[0];
    member.dependencies.remove("distribution:dev-tool");
    member
        .group_dependencies
        .insert(CharStr::from("distribution:dev-tool"));
    assert!(project_diagnostics(&db, &metadata).is_empty());

    db.write_file("/project/worker.py", "")?;
    let diagnostics = project_diagnostics(&db, &metadata);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id().as_str(), "unused-dependency");
    assert_eq!(
        diagnostics[0].headline_message(),
        "Dependency `used-lib` is declared but never imported"
    );
    assert_eq!(diagnostics[0].severity(), Severity::Error);
    Ok(())
}

#[test]
fn custom_source_filters_do_not_prove_unused_dependencies() -> anyhow::Result<()> {
    for filter in ["include = ['src']", "exclude = ['worker.py']"] {
        let mut db = database(&["used_lib"], &format!("[src]\n{filter}"))?;
        db.write_file("/project/src/main.py", "")?;
        let project_metadata = metadata(&[("used-lib", &["used_lib"])])?;
        assert!(project_diagnostics(&db, &project_metadata).is_empty());
    }
    Ok(())
}

#[test]
fn complete_member_check_and_import_isolation() -> anyhow::Result<()> {
    let mut db = database(&["shared_lib"], "")?;
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
        r#"# /// script
# dependencies = ['shared-lib']
# ///
import shared_lib
"#,
    )?;
    let mut metadata = metadata(&[("shared-lib", &["shared_lib"])])?;
    metadata.projects = ["/project/a", "/project/b"]
        .map(|path| DependencyProject {
            path: SystemPathBuf::from(path),
            ..metadata.projects[0].clone()
        })
        .into();
    let manifest = system_path_to_file(&db, "/project/a/pyproject.toml")?;
    for included in ["/project", "/project/a"] {
        db.project()
            .set_included_paths(&mut db, vec![SystemPathBuf::from(included)]);
        let diagnostics = project_diagnostics(&db, &metadata);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0]
                .primary_span()
                .context("declaration span")?
                .file(),
            &UnifiedFile::Ty(manifest)
        );
    }
    db.project()
        .set_included_paths(&mut db, vec![SystemPathBuf::from("/project/a/main.py")]);
    assert!(project_diagnostics(&db, &metadata).is_empty());
    Ok(())
}

#[test]
fn incomplete_project_analysis_does_not_report_unused_dependencies() -> anyhow::Result<()> {
    let mut db = database(&["unused_lib"], "")?;
    db.write_file("/project/main.py", "def incomplete(\n")?;
    let metadata = metadata(&[("unused-lib", &["unused_lib"])])?;
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
    let mut db = database(&["used_lib"], "")?;
    db.write_files([
        (
            "/project/script.py",
            "# /// script\n# dependencies = ['used-lib']\n# ///\nimport helper\n",
        ),
        ("/project/helper.py", ""),
        ("/script-helpers/helper.pyi", ""),
        ("/script-helpers/helper.py", "import other\n"),
        (
            "/script-helpers/other.py",
            "import helper\nimport used_lib\n",
        ),
        ("/project/unrelated.py", "import used_lib\n"),
    ])?;
    let mut paths = SearchPathSettings::new(vec![SystemPathBuf::from("/script-helpers")]);
    paths.site_packages_paths = vec![SystemPathBuf::from("/site-packages")];
    let mut settings = db.project().program_settings(&db).clone();
    settings.search_paths = paths.to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?;
    settings.search_paths.try_register_static_roots(&db);
    let file = system_path_to_file(&db, "/project/script.py")?;
    let mut metadata = metadata(&[("used-lib", &["used_lib"])])?;
    metadata.projects[0].path = SystemPathBuf::from("/project/script.py");
    metadata.projects[0].kind = DependencyProjectKind::Script;
    let script = script_with_program(&db, file, Program::from_settings(&db, &settings));
    assert!(script_diagnostics(&db, script, &metadata).is_empty());

    db.write_file("/script-helpers/other.py", "import helper\n")?;
    let script = script_with_program(&db, file, Program::from_settings(&db, &settings));
    let diagnostics = script_diagnostics(&db, script, &metadata);
    insta::assert_snapshot!(
        "unused_script_dependency",
        DisplayDiagnostics::new(&db, &DisplayDiagnosticConfig::new("ty"), &diagnostics)
            .to_string()
            .replace('\\', "/")
    );
    Ok(())
}
