use std::collections::BTreeMap;

use anyhow::{Context, ensure};
use camino::Utf8Path;
use mdtest::{TestFile, TestOutcome, parser};
use ruff_db::diagnostic::render::{FullRenderer, Renderer};
use ruff_db::diagnostic::{DisplayDiagnosticConfig, UnifiedFile};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};
use ruff_ranged_value::{ValueSource, ValueSourceGuard};
use serde::Deserialize;
use ty_project::dependency::project_dependency_diagnostics;
use ty_project::metadata::options::Options;
use ty_project::testing::{SITE_PACKAGES, apply_metadata, create_environment, dependency_metadata};
use ty_project::{Db as _, ProjectMetadata, SemanticDb as _, TestDb, UseUv};

#[derive(Clone, Default, Deserialize)]
struct Configuration {
    #[serde(default)]
    packages: BTreeMap<String, Vec<String>>,
    #[serde(flatten)]
    options: Options,
}

#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let title = fixture_path.file_name().context("fixture file name")?;
    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let absolute = crate_dir.join(fixture_path);
    let relative = Utf8Path::new("crates/ty_project")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));
    let name = fixture_path.strip_prefix("./resources/mdtest")?.as_str();
    let suite = {
        let _guard = ValueSourceGuard::new(
            ValueSource::File(SystemPathBuf::from("/project/ty.toml").into()),
            false,
        );
        parser::parse::<Configuration>(title, &content, |_| Ok(()))?
    };

    mdtest::run(
        &absolute,
        &relative,
        &content,
        name,
        "ty_project",
        &suite,
        |test, assertion, _output_format| {
            let (db, files) = match setup(test) {
                Ok(setup) => setup,
                Err(error) => {
                    *assertion = format!("Failed to set up test: {error:#}");
                    return Err(Vec::new());
                }
            };
            let project_diagnostics = project_dependency_diagnostics(&db);
            let mut edits = Vec::new();
            let mut failures = Vec::new();
            for test_file in &files {
                let file = test_file.file;
                let mut diagnostics = if file.path(&db).extension() == Some("toml") {
                    Vec::new()
                } else {
                    db.check_file(file)
                };
                diagnostics.extend(
                    project_diagnostics
                        .iter()
                        .filter(|diagnostic| {
                            diagnostic
                                .primary_span()
                                .is_some_and(|span| span.file() == &UnifiedFile::Ty(file))
                        })
                        .cloned(),
                );
                match mdtest::check_diagnostics(
                    &db,
                    "ty",
                    test_file,
                    db.python_version_with_source(file).version,
                    &diagnostics,
                    mdtest::RunOptions::default(),
                    str::to_owned,
                ) {
                    Ok(file_edits) => edits.extend(file_edits),
                    Err(failure) => failures.push(failure),
                }
            }
            if failures.is_empty() {
                Ok((TestOutcome::Success, edits))
            } else {
                Err(failures)
            }
        },
    )?;
    Ok(())
}

fn setup<'a>(
    test: &parser::MarkdownTest<'_, 'a, Configuration>,
) -> anyhow::Result<(TestDb, Vec<TestFile<'a>>)> {
    let root = SystemPathBuf::from("/project");
    let mut project = ProjectMetadata::new("app", root.clone()).with_use_uv(UseUv::On);
    project.set_override_options(test.configuration().options.clone());
    let mut db = TestDb::new(project);
    create_environment(&mut db, &[])?;
    let mut files = Vec::new();
    for embedded in test.files() {
        let path = SystemPathBuf::from(
            embedded
                .full_path(&root)
                .as_str()
                .replace("/venv/<path-to-site-packages>", SITE_PACKAGES),
        );
        db.write_file(&path, &embedded.code)?;
        if path.starts_with(&root) {
            files.push(TestFile {
                file: system_path_to_file(&db, &path)?,
                code_blocks: embedded.code_blocks.clone(),
            });
        }
    }
    ensure!(
        files.iter().any(|file| file.file.path(&db).as_system_path()
            == Some(root.join("pyproject.toml").as_path())),
        "project mdtests must include a pyproject.toml fixture"
    );

    let packages: Vec<_> = test
        .configuration()
        .packages
        .iter()
        .map(|(name, modules)| {
            (
                name.as_str(),
                modules.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .collect();
    let packages: Vec<_> = packages
        .iter()
        .map(|(name, modules)| (*name, modules.as_slice()))
        .collect();
    let metadata = dependency_metadata(&db, &packages);
    apply_metadata(&mut db, &metadata)?;
    let diagnostics = db.project().check_settings(&db);
    ensure!(
        diagnostics.is_empty(),
        "invalid project settings: {}",
        FullRenderer::new(&db, &DisplayDiagnosticConfig::new("ty")).display(&diagnostics)
    );
    Ok((db, files))
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/mdtest", pattern = r"\.md$" },
}
