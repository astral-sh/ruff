//! Helpers for simulating uv dependency metadata in project tests.

use std::collections::BTreeMap;

use anyhow::Context;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithTestSystem, System, SystemPath, TestSystem};
use ruff_python_ast::PythonVersion;
use salsa::Setter;
use serde_json::{Map, Value, json};

use crate::Db as _;
use crate::db::testing::TestDb;

use super::{UvMetadata, UvWorkspace, script_environment};

pub const PYTHON_VERSION: PythonVersion = PythonVersion::PY313;

/// Create an environment and uv metadata for the given installed distributions.
/// A module with children becomes a namespace package; all other modules get an empty `.py` file.
pub fn dependency_fixture<S: AsRef<str>>(
    system: &TestSystem,
    root: &SystemPath,
    packages: impl IntoIterator<Item = (S, Vec<S>)>,
) -> anyhow::Result<Value> {
    let environment = root.join(".venv");
    let site_packages = environment.join(if cfg!(windows) {
        "Lib/site-packages"
    } else {
        "lib/python3.13/site-packages"
    });
    let fs = system.memory_file_system();
    fs.create_directory_all(&site_packages)?;
    fs.write_file_all(
        environment.join("pyvenv.cfg"),
        "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
    )?;

    let mut installed = Map::new();
    let mut resolution = Map::new();
    let mut dependencies = Vec::new();
    let mut owners: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (name, modules) in packages {
        let name = name.as_ref();
        installed.insert(name.to_owned(), json!({"name": name, "editable": false}));
        resolution.insert(
            name.to_owned(),
            json!({"kind": "package", "name": name, "dependencies": []}),
        );
        dependencies.push(json!({"id": name}));
        for module in modules {
            owners
                .entry(module.as_ref().to_owned())
                .or_default()
                .push(json!({"package_id": name}));
        }
    }

    for module in owners.keys() {
        let prefix = format!("{module}.");
        if owners.keys().any(|child| child.starts_with(&prefix)) {
            continue;
        }
        fs.write_file_all(
            site_packages.join(format!("{}.py", module.replace('.', "/"))),
            "",
        )?;
    }

    resolution.insert(
        "app".to_owned(),
        json!({"kind": "package", "name": "app", "dependencies": dependencies}),
    );
    let modules: Map<String, Value> = owners
        .into_iter()
        .map(|(module, owners)| (module, json!({"owners": owners})))
        .collect();
    Ok(json!({
        "schema": {"version": "preview"},
        "workspace_root": root.as_str(),
        "members": [{"name": "app", "path": root.as_str(), "id": "app"}],
        "environment": {"root": environment, "packages": installed},
        "resolution": resolution,
        "modules": modules,
    }))
}

/// Use the fixture's direct dependencies as the declarations of a standalone script.
pub fn as_script(input: &mut Value, path: &SystemPath) {
    input["members"] = json!([]);
    input["script"] = json!({"id": "app", "path": path.as_str()});
    let dependencies = input["resolution"]["app"]["dependencies"].take();
    input["resolution"]["app"] = json!({"kind": "script", "dependencies": dependencies});
}

/// Parse simulated uv command output without invoking uv.
pub fn workspace(input: &Value, system: &dyn System) -> anyhow::Result<UvWorkspace> {
    Ok(UvWorkspace {
        metadata: Some(UvMetadata::from_metadata(
            &serde_json::to_vec(input)?,
            system,
        )?),
        error: None,
    })
}

/// Apply a simulated sync to a running test database.
pub fn apply_metadata(db: &mut TestDb, input: &Value) -> anyhow::Result<()> {
    let workspace = workspace(input, db.test_system())?;
    if let Some(path) = input.pointer("/script/path").and_then(Value::as_str) {
        let file = system_path_to_file(db, path)?;
        script_environment(db, file)
            .context("expected a script environment")?
            .set_uv_metadata(db)
            .to(workspace.metadata);
    } else {
        let project = db.project();
        let root = project.root(db).to_path_buf();
        project.rediscover(db, &root, workspace)?;
    }
    Ok(())
}
