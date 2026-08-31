//! In-memory uv environments and metadata fixtures for project tests.

use std::collections::BTreeMap;

use anyhow::Context;
use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPath};
use salsa::Setter;
use serde_json::{Value, json};
use ty_python_core::program::FallibleStrategy;

use crate::Db as _;
use crate::db::testing::TestDb;

use super::{UvMetadata, UvWorkspace, script_environment};

pub const SITE_PACKAGES: &str = if cfg!(windows) {
    "/venv/Lib/site-packages"
} else {
    "/venv/lib/python3.13/site-packages"
};

pub fn create_environment(db: &mut TestDb, modules: &[&str]) -> anyhow::Result<()> {
    db.write_file(
        "/venv/pyvenv.cfg",
        "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
    )?;
    db.memory_file_system()
        .create_directory_all(SITE_PACKAGES)?;
    for module in modules {
        db.write_file(format!("{SITE_PACKAGES}/{module}.py"), "")?;
    }
    Ok(())
}

pub fn dependency_metadata(db: &TestDb, packages: &[(&str, &[&str])]) -> Value {
    let root = db.project().root(db).as_str();
    let mut metadata = json!({
        "schema": {"version": "preview"},
        "workspace_root": root,
        "environment": {"root": "/venv", "packages": {}},
        "members": [{"name": "app", "path": root, "id": "app"}],
        "resolution": {"app": {"kind": "package", "name": "app", "dependencies": []}},
        "modules": {},
    });
    let mut module_owners: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for (name, modules) in packages {
        metadata["resolution"][name] = json!({"kind": "package", "name": name, "dependencies": []});
        metadata["environment"]["packages"][name] = json!({"name": name, "editable": false});
        for module in *modules {
            module_owners
                .entry(module)
                .or_default()
                .push(json!({"package_id": name}));
        }
    }
    metadata["modules"] = module_owners
        .into_iter()
        .map(|(module, owners)| (module, json!({ "owners": owners })))
        .collect();
    metadata["resolution"]["app"]["dependencies"] = packages
        .iter()
        .map(|(name, _)| json!({"id": name}))
        .collect();
    metadata
}

pub fn script_metadata(db: &TestDb, path: &SystemPath, packages: &[(&str, &[&str])]) -> Value {
    let mut metadata = dependency_metadata(db, packages);
    metadata["members"] = json!([]);
    metadata["script"] = json!({"path": path.as_str(), "id": "app"});
    metadata["resolution"]["app"]["kind"] = json!("script");
    metadata
}

pub fn apply_metadata(db: &mut TestDb, input: &Value) -> anyhow::Result<()> {
    let metadata = UvMetadata::from_metadata(&serde_json::to_vec(input)?, db.system())?;
    if let Some(path) = input["script"]["path"].as_str() {
        let file = system_path_to_file(db, path)?;
        let environment = script_environment(db, file).context("script environment")?;
        environment.set_uv_metadata(db).to(Some(metadata));
    } else {
        let project = db.project();
        let mut project_metadata = project.metadata(db).clone();
        project_metadata.apply_uv_workspace(
            db.system(),
            UvWorkspace {
                metadata: Some(metadata),
                error: None,
            },
        )?;
        let (program, _) = project_metadata.to_merged_options().to_program_settings(
            db.system(),
            db.vendored(),
            &FallibleStrategy,
        )?;
        project.reload(db, project_metadata, None, vec![]);
        project.update_program(db, program);
    }
    Ok(())
}
