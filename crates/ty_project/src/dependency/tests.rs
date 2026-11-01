use anyhow::Context;
use char_str::CharStr;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity, UnifiedFile};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPath};
use ruff_db::testing::assert_function_query_was_not_run;
use ruff_ranged_value::ValueSource;
use serde_json::{Value, json};
use ty_python_semantic::Db as _;

use crate::db::testing::TestDb;
use crate::metadata::options::Options;
use crate::script::Script;
use crate::uv::testing::{apply_metadata, as_script, dependency_fixture, workspace};
use crate::{Db as _, ProjectMetadata, UseUv};

use super::{
    candidate_declarations, importable_ids, member_dependency_usage, member_files,
    project_dependency_diagnostics, script_dependency_usage, unused_names,
};

const ROOT: &str = if cfg!(windows) {
    "C:/project"
} else {
    "/project"
};
const ENABLED: &str = "[rules]\nunused-dependency = 'warn'";

fn fixture(options: &str) -> anyhow::Result<(TestDb, Value)> {
    let root = SystemPath::new(ROOT);
    let mut project = ProjectMetadata::new("app", root.to_path_buf()).with_use_uv(UseUv::On);
    project.set_override_options(Options::from_toml_str(
        options,
        ValueSource::File(root.join("ty.toml").into()),
    )?);
    let mut db = TestDb::new(project);
    db.write_file(root.join("pyproject.toml"), "[project]\n")?;
    let metadata =
        dependency_fixture(db.test_system(), root, [("shared-lib", vec!["shared_lib"])])?;
    Ok((db, metadata))
}

#[test]
fn alternatives_use_declared_ids_before_grouping_by_name() -> anyhow::Result<()> {
    let (mut db, mut input) = fixture(ENABLED)?;
    let packages = &mut input["environment"]["packages"];
    packages["unmanaged"] = packages["shared-lib"].clone();
    input["resolution"]["other-version"] = input["resolution"]["shared-lib"].clone();
    input["resolution"]["app"]["dependencies"] =
        json!([{"id": "shared-lib"}, {"id": "other-version"}]);
    apply_metadata(&mut db, &input)?;
    let project = db.project();
    assert!(!project.has_dependency_checks(&db));
    assert!(project_dependency_diagnostics(&db).is_empty());
    let events = db.take_salsa_events();
    assert_function_query_was_not_run(&db, member_files, project, &events);

    let metadata = project
        .dependency_metadata(&db)
        .as_ref()
        .map_err(Clone::clone)?
        .as_deref()
        .context("dependency metadata")?;
    let member = &metadata.projects[0];
    let mut known = importable_ids(metadata);
    let alternative = CharStr::from("other-version");
    known.insert(&alternative);
    let candidates = candidate_declarations(metadata, member, &known);
    assert!(
        unused_names(&candidates, std::slice::from_ref(&alternative))
            .next()
            .is_none()
    );
    let unrelated = [CharStr::from("unmanaged")];
    let names: Vec<_> = unused_names(&candidates, &unrelated)
        .map(CharStr::as_str)
        .collect();
    assert_eq!(names, ["shared-lib"]);
    Ok(())
}

#[test]
fn each_member_uses_its_own_indexed_files() -> anyhow::Result<()> {
    let root = SystemPath::new(ROOT);
    let (mut db, mut metadata) = fixture("")?;
    let manifest_rules = "[[overrides]]\ninclude = ['pyproject.toml', 'child/pyproject.toml']\n[overrides.rules]\nunused-dependency = 'error'";
    db.write_files([
        (root.join("ty.toml"), manifest_rules),
        (root.join("main.py"), "import child.worker\n"),
        (
            root.join("script.py"),
            "# /// script\n# dependencies = ['shared-lib']\n# ///\nimport shared_lib\n",
        ),
        (root.join("child/pyproject.toml"), "[project]\n"),
        (root.join("child/worker.py"), "import shared_lib\n"),
    ])?;
    metadata["members"] = json!([
        {"name": "app", "id": "app", "path": ROOT},
        {"name": "child", "id": "child", "path": root.join("child")},
    ]);
    metadata["resolution"]["child"] = metadata["resolution"]["app"].clone();
    metadata["resolution"]["child"]["name"] = json!("child");
    apply_metadata(&mut db, &metadata)?;

    let manifest = system_path_to_file(&db, root.join("pyproject.toml"))?;
    let diagnostics = project_dependency_diagnostics(&db);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity(), Severity::Error);
    assert_eq!(
        diagnostics[0]
            .primary_span()
            .context("manifest span")?
            .file(),
        &UnifiedFile::Ty(manifest)
    );
    db.take_salsa_events();
    db.write_file(
        root.join("ty.toml"),
        manifest_rules.replace("error", "warn"),
    )?;
    apply_metadata(&mut db, &metadata)?;
    assert_eq!(
        project_dependency_diagnostics(&db)[0].severity(),
        Severity::Warning
    );
    let events = db.take_salsa_events();
    assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);

    db.write_file(root.join("child/worker.py"), "")?;
    let added = root.join("child/added.py");
    db.write_file(&added, "import shared_lib\n")?;
    let added_file = system_path_to_file(&db, &added)?;
    db.project().add_file(&mut db, added_file, false);
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    let events = db.take_salsa_events();
    assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);
    db.write_file(added, "")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 2);
    db.write_file(root.join("main.py"), "def unfinished(\n")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);

    db.project()
        .set_included_paths(&mut db, vec![root.join("child/worker.py")]);
    assert!(project_dependency_diagnostics(&db).is_empty());
    db.project()
        .set_included_paths(&mut db, vec![root.join("child")]);
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    Ok(())
}

#[test]
fn opening_an_excluded_file_does_not_count_its_imports() -> anyhow::Result<()> {
    let root = SystemPath::new(ROOT);
    let (mut db, metadata) = fixture(&format!("{ENABLED}\n[src]\ninclude = ['src']"))?;
    db.write_files([
        (root.join("src/main.py"), "import generated\n"),
        (root.join("generated.py"), "import shared_lib\n"),
    ])?;
    let generated = system_path_to_file(&db, root.join("generated.py"))?;
    db.project().open_file(&mut db, generated);
    apply_metadata(&mut db, &metadata)?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    db.take_salsa_events();
    db.write_file(root.join("generated.py"), "")?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    let manifest = system_path_to_file(&db, root.join("pyproject.toml"))?;
    let events = db.take_salsa_events();
    assert_function_query_was_not_run(&db, member_dependency_usage, manifest, &events);
    db.write_file(root.join("src/main.py"), "import shared_lib\n")?;
    assert!(project_dependency_diagnostics(&db).is_empty());
    Ok(())
}

#[test]
fn older_uv_reports_missing_inventory_when_the_rule_is_enabled() -> anyhow::Result<()> {
    for (options, expected) in [("", None), (ENABLED, Some(DiagnosticId::UvMetadata))] {
        let (mut db, mut input) = fixture(options)?;
        input["environment"] = json!({"root": input["environment"]["root"]});
        apply_metadata(&mut db, &input)?;
        let diagnostics = db.project().manifest_diagnostics(&db);
        assert_eq!(
            diagnostics.iter().map(Diagnostic::id).collect::<Vec<_>>(),
            expected.as_slice()
        );
    }
    Ok(())
}

#[test]
fn failed_uv_refresh_skips_stale_declarations() -> anyhow::Result<()> {
    let root = SystemPath::new(ROOT);
    let (mut db, metadata) = fixture(ENABLED)?;
    db.write_file(root.join("main.py"), "")?;
    apply_metadata(&mut db, &metadata)?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    let mut stale = workspace(&metadata, db.test_system())?;
    stale.error = Some(Diagnostic::new(
        DiagnosticId::UvMetadata,
        Severity::Warning,
        "uv metadata refresh failed",
    ));
    db.project().rediscover(&mut db, root, stale)?;
    assert!(project_dependency_diagnostics(&db).is_empty());
    apply_metadata(&mut db, &metadata)?;
    assert_eq!(project_dependency_diagnostics(&db).len(), 1);
    Ok(())
}

#[test]
fn script_usage_follows_reachable_stubs_and_implementations() -> anyhow::Result<()> {
    let root = SystemPath::new(ROOT);
    let path = root.join("script.py");
    let tag = "# /// script
# dependencies = ['shared-lib']
# [tool.ty.rules]
# unused-dependency = 'warn'
# [tool.ty.environment]
# root = ['helpers']
# ///";
    let source = format!("{tag}\nimport helper\n");
    let (mut db, mut metadata) = fixture("")?;
    let implementation = root.join("helpers/helper.py");
    let stub = root.join("helpers/helper.pyi");
    db.write_files([
        (path.clone(), source.as_str()),
        (root.join("helper.py"), ""),
        (stub.clone(), ""),
        (implementation.clone(), "import other\n"),
        (
            root.join("helpers/other.py"),
            "import helper\nimport shared_lib\n",
        ),
        (root.join("unrelated.py"), "import shared_lib\n"),
    ])?;
    as_script(&mut metadata, &path);
    apply_metadata(&mut db, &metadata)?;
    let file = system_path_to_file(&db, &path)?;
    assert!(db.check_file(file).is_empty());
    db.take_salsa_events();
    db.write_file(root.join("unrelated.py"), "")?;
    assert!(db.check_file(file).is_empty());
    let events = db.take_salsa_events();
    let script = Script::for_file(&db, file).context("standalone script")?;
    assert_function_query_was_not_run(&db, script_dependency_usage, script, &events);

    db.write_file(&implementation, "")?;
    let diagnostics = db.check_file(file);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].id().is_lint_named("unused-dependency"));
    let range = diagnostics[0]
        .primary_span()
        .and_then(|span| span.range())
        .context("script metadata span")?;
    assert_eq!(&source[range], tag);
    db.write_file(&stub, "import shared_lib\n")?;
    assert!(db.check_file(file).is_empty());
    db.write_file(&stub, "")?;
    db.write_file(&path, format!("# ty: ignore[unused-dependency]\n{source}"))?;
    assert!(db.check_file(file).is_empty());
    db.write_file(&implementation, "import shared_lib\n")?;
    let diagnostics = db.check_file(file);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].id().is_lint_named("unused-ignore-comment"));
    Ok(())
}
