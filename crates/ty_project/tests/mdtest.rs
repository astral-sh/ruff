//! Project-level mdtests run a full project check. `[packages]` maps installed distributions to
//! the modules they provide; other configuration uses `ty.toml` syntax. Package names in
//! `pyproject.toml` must match the fixture keys.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::Context;
use camino::Utf8Path;
use mdtest::parser::{self, MarkdownTest};
use mdtest::{Failures, MarkdownEdit, OutputFormat, RunOptions, TestFile, TestOutcome};
use ruff_db::diagnostic::UnifiedFile;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{SystemPath, TestSystem};
use ruff_ranged_value::{ValueSource, ValueSourceGuard};
use serde::Deserialize;
use serde_json::json;
use ty_project::metadata::options::Options;
use ty_project::{ProjectDatabase, ProjectMetadata, UseUv, uv_test};

const PROJECT_ROOT: &str = if cfg!(windows) {
    "C:/project"
} else {
    "/project"
};

#[derive(Clone, Default, Deserialize)]
struct FixtureConfig {
    #[serde(default)]
    packages: BTreeMap<String, Vec<String>>,
    #[serde(flatten)]
    options: Options,
}

#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, source: String) -> datatest_stable::Result<()> {
    let filename = fixture_path.file_name().context("fixture filename")?;
    let suite = {
        let config = SystemPath::new(PROJECT_ROOT).join("ty.toml");
        let _source = ValueSourceGuard::new(ValueSource::File(config.into()), false);
        parser::parse::<FixtureConfig>(filename, &source, |_| Ok(()))?
    };
    let absolute = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture_path);
    let relative = Utf8Path::new("crates/ty_project")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));
    mdtest::run(
        &absolute,
        &relative,
        &source,
        filename,
        "ty_project",
        &suite,
        run_case,
    )?;
    Ok(())
}

fn run_case(
    test: &MarkdownTest<'_, '_, FixtureConfig>,
    assertion: &mut String,
    _output_format: OutputFormat,
) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures> {
    let (db, files) = setup(test).map_err(|error| {
        let _ = write!(assertion, "Failed to set up project: {error:#}");
        Vec::new()
    })?;
    let mut diagnostics = db.check();
    let mut edits = Vec::new();
    let mut failures = Vec::new();
    for file in &files {
        let file_diagnostics = diagnostics
            .extract_if(.., |diagnostic| {
                diagnostic
                    .primary_span()
                    .is_some_and(|span| span.file() == &UnifiedFile::Ty(file.file))
            })
            .collect::<Vec<_>>();
        match file.check_diagnostics(
            &db,
            "ty",
            uv_test::PYTHON_VERSION,
            &file_diagnostics,
            RunOptions::default(),
            |rendered| rendered.replace("--> C:/project/", "--> project/"),
        ) {
            Ok(updates) => edits.extend(updates),
            Err(failure) => failures.push(failure),
        }
    }
    if !diagnostics.is_empty() {
        assertion.push_str("Diagnostics without an embedded test file:\n");
        for diagnostic in diagnostics {
            let _ = writeln!(
                assertion,
                "{}",
                mdtest::render_diagnostic(&db, "ty", &diagnostic)
            );
        }
        return Err(failures);
    }
    if failures.is_empty() {
        Ok((TestOutcome::Success, edits))
    } else {
        Err(failures)
    }
}

fn setup<'s>(
    test: &MarkdownTest<'_, 's, FixtureConfig>,
) -> anyhow::Result<(ProjectDatabase, Vec<TestFile<'s>>)> {
    let root = SystemPath::new(PROJECT_ROOT);
    let system = TestSystem::default();
    let mut metadata =
        uv_test::dependency_fixture(&system, root, test.configuration().packages.clone())?;
    let fs = system.memory_file_system();
    let mut embedded = Vec::new();
    for file in test.files() {
        if file.lang == "ignore" {
            continue;
        }
        let path = file.full_path(root);
        let path = mdtest::expand_site_packages_placeholder(&path, uv_test::PYTHON_VERSION)
            .unwrap_or(path);
        fs.write_file_all(&path, file.code.as_bytes())?;
        if path.starts_with(root) && !path.starts_with(root.join(".venv")) {
            embedded.push((path, file.code_blocks.clone()));
        }
    }

    let manifest: toml::Value = toml::from_str(&fs.read_to_string(root.join("pyproject.toml"))?)?;
    metadata["resolution"]["app"]["dependencies"] = manifest
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| {
            let name = value.as_str().context("dependency must be a string")?;
            anyhow::ensure!(
                test.configuration().packages.contains_key(name),
                "declare `{name}` in the mdtest's [packages] table"
            );
            Ok(json!({"id": name}))
        })
        .collect::<anyhow::Result<_>>()?;

    let mut project = ProjectMetadata::discover(root, &system)?.with_use_uv(UseUv::On);
    project.set_fallback_options(test.configuration().options.clone());
    project.apply_uv_workspace(&system, uv_test::workspace(&metadata, &system)?)?;
    let db = ProjectDatabase::fallible(project, system)?;
    let files = embedded
        .into_iter()
        .map(|(path, code_blocks)| {
            Ok(TestFile {
                file: system_path_to_file(&db, path)?,
                code_blocks,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    Ok((db, files))
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/mdtest", pattern = r"\.md$" },
}
