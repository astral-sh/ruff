use anyhow::Context;
use ruff_db::diagnostic::render::{FullRenderer, Renderer};
use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig, Severity, UnifiedFile};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
use ruff_db::testing::{assert_function_query_was_not_run, assert_function_query_was_run};
use ruff_python_trivia::textwrap::dedent;
use ruff_ranged_value::ValueSource;
use serde_json::json;
use ty_combine::Combine;
use ty_python_core::program::FallibleStrategy;
use ty_python_semantic::Db as _;

use crate::db::testing::TestDb;
use crate::metadata::options::Options;
use crate::script::Script;
use crate::uv::testing::{
    apply_metadata, create_environment, dependency_metadata, script_metadata,
};
use crate::{Db as _, ProjectMetadata, UseUv};

use super::{member_dependency_usage, project_dependency_diagnostics, script_dependency_usage};

fn database(modules: &[&str], options: &str) -> anyhow::Result<TestDb> {
    let root = SystemPathBuf::from("/project");
    let mut metadata = ProjectMetadata::new("app", root.clone()).with_use_uv(UseUv::On);
    metadata.set_override_options(
        Options::from_toml_str(options, ValueSource::File(root.join("ty.toml").into()))?.combine(
            Options::from_toml_str("[rules]\nunused-dependency = 'warn'", ValueSource::Cli)?,
        ),
    );
    let mut db = TestDb::new(metadata);
    db.write_file("/project/pyproject.toml", "[project]\n")?;
    create_environment(&mut db, modules)?;
    Ok(db)
}

#[test]
fn declaration_file_rule_override_reuses_usage() -> anyhow::Result<()> {
    let mut db = database(&["unused_lib"], "")?;
    db.write_file("/project/main.py", "")?;
    let metadata = dependency_metadata(&db, &[("unused-lib", &["unused_lib"])]);
    apply_metadata(&mut db, &metadata)?;
    let manifest = system_path_to_file(&db, "/project/pyproject.toml")?;

    for (level, severity) in [
        ("error", Some(Severity::Error)),
        ("warn", Some(Severity::Warning)),
        ("ignore", None),
    ] {
        let project = db.project();
        let mut metadata = project.metadata(&db).clone();
        metadata.set_override_options(Options::from_toml_str(
            &format!(
                r"
                [[overrides]]
                include = ['pyproject.toml']
                [overrides.rules]
                unused-dependency = '{level}'
                "
            ),
            ValueSource::File(SystemPathBuf::from("/project/ty.toml").into()),
        )?);
        let (settings, diagnostics) = metadata
            .to_merged_options()
            .to_settings(&db, &FallibleStrategy)?;
        project.reload(&mut db, metadata, Some(settings), diagnostics);
        assert_eq!(
            project_dependency_diagnostics(&db)
                .first()
                .map(Diagnostic::severity),
            severity
        );
        let events = db.take_salsa_events();
        if level != "error" {
            assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);
        }
    }
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
    let mut metadata = dependency_metadata(
        &db,
        &[("used-lib", &["used_lib"]), ("dev-tool", &["dev_tool"])],
    );
    metadata["resolution"]["app"]["dependencies"] = json!([{"id": "used-lib"}]);
    metadata["resolution"]["app"]["dependency_groups"] = json!([{"id": "dev"}]);
    metadata["resolution"]["dev"] =
        json!({"kind": {"group": "dev"}, "dependencies": [{"id": "dev-tool"}]});
    apply_metadata(&mut db, &metadata)?;
    assert!(project_dependency_diagnostics(&db).is_empty());

    db.take_salsa_events();
    db.write_file("/project/worker.py", "")?;
    let diagnostics = project_dependency_diagnostics(&db);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id().as_str(), "unused-dependency");
    assert_eq!(
        diagnostics[0].headline_message(),
        "Dependency `used-lib` is declared but never imported"
    );
    assert_eq!(diagnostics[0].severity(), Severity::Error);
    let events = db.take_salsa_events();
    let manifest = system_path_to_file(&db, "/project/pyproject.toml")?;
    assert_function_query_was_run(&db, member_dependency_usage, manifest, &events);
    Ok(())
}

#[test]
fn source_filters_limit_dependency_usage() -> anyhow::Result<()> {
    for filter in ["include = ['src']", "exclude = ['worker.py']"] {
        let mut db = database(&["used_lib"], &format!("[src]\n{filter}"))?;
        db.write_file("/project/src/main.py", "import worker\n")?;
        db.write_file("/project/worker.py", "import used_lib\n")?;
        let project_metadata = dependency_metadata(&db, &[("used-lib", &["used_lib"])]);
        apply_metadata(&mut db, &project_metadata)?;

        let worker = system_path_to_file(&db, "/project/worker.py")?;
        db.project().open_file(&mut db, worker);
        let diagnostics = project_dependency_diagnostics(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].headline_message(),
            "Dependency `used-lib` is declared but never imported"
        );

        db.take_salsa_events();
        db.write_file("/project/worker.py", "")?;
        assert_eq!(project_dependency_diagnostics(&db).len(), 1);
        let events = db.take_salsa_events();
        let manifest = system_path_to_file(&db, "/project/pyproject.toml")?;
        assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);

        db.write_file("/project/src/main.py", "import used_lib\n")?;
        assert!(project_dependency_diagnostics(&db).is_empty());
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
    db.write_dedented(
        "/project/a/script.py",
        r"
        # /// script
        # dependencies = ['shared-lib']
        # ///
        import shared_lib
        ",
    )?;
    let mut metadata = dependency_metadata(&db, &[("shared-lib", &["shared_lib"])]);
    metadata["members"] = ["a", "b"]
        .map(|name| {
            json!({
                "name": name, "path": format!("/project/{name}"), "id": name,
            })
        })
        .into();
    for name in ["a", "b"] {
        metadata["resolution"][name] = metadata["resolution"]["app"].clone();
        metadata["resolution"][name]["name"] = json!(name);
    }
    apply_metadata(&mut db, &metadata)?;
    let manifest = system_path_to_file(&db, "/project/a/pyproject.toml")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    db.take_salsa_events();
    db.write_file("/project/b/main.py", "")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 2);
    let events = db.take_salsa_events();
    assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);
    db.write_file("/project/b/main.py", "import shared_lib\n")?;

    for included in ["/project", "/project/a"] {
        db.project()
            .set_included_paths(&mut db, vec![SystemPathBuf::from(included)]);
        let diagnostics = project_dependency_diagnostics(&db);
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
    assert!(project_dependency_diagnostics(&db).is_empty());
    Ok(())
}

#[test]
fn incomplete_project_analysis_does_not_report_unused_dependencies() -> anyhow::Result<()> {
    let mut db = database(&["unused_lib"], "")?;
    db.write_file("/project/main.py", "def incomplete(\n")?;
    let metadata = dependency_metadata(&db, &[("unused-lib", &["unused_lib"])]);
    apply_metadata(&mut db, &metadata)?;
    assert!(project_dependency_diagnostics(&db).is_empty());

    db.write_file("/project/main.py", "")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    db.remove_file("/project/main.py")?;
    assert!(project_dependency_diagnostics(&db).is_empty());
    Ok(())
}

#[test]
fn script_reachable_helpers_cycles_and_edits() -> anyhow::Result<()> {
    let mut db = database(&["used_lib"], "")?;
    let source = dedent(
        r"
        # /// script
        # dependencies = ['used-lib']
        # [tool.ty.environment]
        # root = ['/script-helpers']
        # ///
        import helper
        ",
    );
    let source = source.trim_start();
    db.write_files([
        ("/project/script.py", source),
        ("/project/helper.py", ""),
        ("/script-helpers/helper.pyi", ""),
        ("/script-helpers/helper.py", "import other\n"),
        (
            "/script-helpers/other.py",
            "import helper\nimport used_lib\n",
        ),
        ("/project/unrelated.py", "import used_lib\n"),
    ])?;
    let file = system_path_to_file(&db, "/project/script.py")?;
    let metadata = script_metadata(
        &db,
        SystemPath::new("/project/script.py"),
        &[("used-lib", &["used_lib"])],
    );
    apply_metadata(&mut db, &metadata)?;
    let diagnostics = db.check_file(file);
    assert!(
        diagnostics.is_empty(),
        "{}",
        FullRenderer::new(&db, &DisplayDiagnosticConfig::new("ty")).display(&diagnostics)
    );
    db.take_salsa_events();
    db.write_file("/project/unrelated.py", "")?;
    assert!(db.check_file(file).is_empty());
    let events = db.take_salsa_events();
    let script = Script::for_file(&db, file).context("script")?;
    assert_function_query_was_not_run(&db, script_dependency_usage, script, &events);

    db.write_file("/script-helpers/other.py", "import helper\n")?;
    let diagnostics = db.check_file(file);
    insta::assert_snapshot!(
        "unused_script_dependency",
        FullRenderer::new(&db, &DisplayDiagnosticConfig::new("ty"))
            .display(&diagnostics)
            .to_string()
            .replace('\\', "/")
    );
    db.write_file(
        "/project/script.py",
        format!(
            "# ty: ignore[unused-dependency]\n{source}missing # ty: ignore[unresolved-reference]\n"
        ),
    )?;
    assert!(db.check_file(file).is_empty());

    db.write_file(
        "/script-helpers/other.py",
        "import helper\nimport used_lib\n",
    )?;
    let diagnostics = db.check_file(file);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id().as_str(), "unused-ignore-comment");
    let range = diagnostics[0]
        .primary_span()
        .and_then(|span| span.range())
        .context("unused suppression range")?;
    assert!(range.end().to_usize() <= "# ty: ignore[unused-dependency]".len());
    Ok(())
}
