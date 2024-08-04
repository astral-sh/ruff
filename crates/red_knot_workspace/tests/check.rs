use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::lint::lint_semantic;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::files::system_path_to_file;
use ruff_db::program::{RawProgramSettings, RawSearchPathSettings, TargetVersion};
use ruff_db::system::{OsSystem, SystemPathBuf};
use std::fs;
use std::path::PathBuf;

fn setup_db(workspace_root: SystemPathBuf) -> anyhow::Result<RootDatabase> {
    let system = OsSystem::new(&workspace_root);
    let workspace = WorkspaceMetadata::from_path(&workspace_root, &system)?;
    let search_paths = RawSearchPathSettings {
        extra_paths: vec![],
        src_root: workspace_root,
        custom_typeshed: None,
        site_packages: vec![],
    };
    let settings = RawProgramSettings {
        target_version: TargetVersion::default(),
        search_paths,
    };
    let db = RootDatabase::new(workspace, settings, system)?;
    Ok(db)
}

/// Test that all snippets in testcorpus can be checked without panic
#[test]
#[allow(clippy::print_stdout)]
fn corpus_no_panic() -> anyhow::Result<()> {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/test/corpus");
    let system_corpus =
        SystemPathBuf::from_path_buf(corpus.clone()).expect("corpus path to be UTF8");
    let db = setup_db(system_corpus.clone())?;

    for path in fs::read_dir(&corpus).expect("corpus to be a directory") {
        let path = path.expect("path to not be an error").path();
        println!("checking {path:?}");
        let path = SystemPathBuf::from_path_buf(path.clone()).expect("path to be UTF-8");
        // this test is only asserting that we can run the lint without a panic
        let file = system_path_to_file(&db, path).expect("file to exist");
        lint_semantic(&db, file);
    }
    Ok(())
}
