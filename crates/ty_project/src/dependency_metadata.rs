use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use ruff_db::system::{SystemPath, SystemPathBuf};
use serde::Deserialize;
use serde_json::{Map, Value};
use ty_module_resolver::{Db as ModuleResolverDb, list_modules};
use ty_python_semantic::dependency::{DependencyMetadata, DependencyProject, ModuleOwner};

#[derive(Debug, Deserialize)]
struct UvWorkspaceMetadata {
    schema: UvSchema,
    members: Vec<UvWorkspaceMember>,
    #[serde(default)]
    resolution: Map<String, Value>,
    #[serde(default)]
    module_owners: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UvSchema {
    version: String,
}

#[derive(Debug, Deserialize)]
struct UvWorkspaceMember {
    name: String,
    path: String,
    id: String,
}

pub fn parse_uv_workspace_metadata(source: &str) -> Result<DependencyMetadata> {
    let snapshot: UvWorkspaceMetadata =
        serde_json::from_str(source).context("Failed to parse dependency metadata JSON")?;

    if snapshot.schema.version != "preview" {
        bail!(
            "Unsupported dependency metadata schema version `{}`; expected `preview`",
            snapshot.schema.version
        );
    }

    let mut projects = Vec::with_capacity(snapshot.members.len());
    for member in snapshot.members {
        let package = snapshot
            .resolution
            .get(&member.id)
            .with_context(|| format!("Missing resolution package node `{}`", member.id))?;
        let direct_dependencies = direct_dependencies(package, &snapshot.resolution)
            .with_context(|| format!("Failed to read dependencies for `{}`", member.name))?;
        let dependency_group_dependencies =
            dependency_group_dependencies(package, &snapshot.resolution).with_context(|| {
                format!("Failed to read dependency groups for `{}`", member.name)
            })?;

        projects.push(DependencyProject::new_with_dependency_groups(
            member.name,
            SystemPathBuf::from(member.path),
            direct_dependencies,
            dependency_group_dependencies,
        ));
    }

    let mut module_owners = Vec::with_capacity(snapshot.module_owners.len());
    for (module, owner_references) in snapshot.module_owners {
        let owners = owner_references
            .iter()
            .map(|reference| {
                resolve_package_reference(reference, &snapshot.resolution)
                    .with_context(|| format!("Missing resolution package node `{reference}`"))
            })
            .collect::<Result<Vec<_>>>()?;

        let Some(module_owner) = ModuleOwner::from_module(&module, owners) else {
            bail!("Invalid module owner `{module}`");
        };

        module_owners.push(module_owner);
    }

    Ok(DependencyMetadata::new(projects, module_owners))
}

pub fn enrich_dependency_metadata_with_editables(
    db: &dyn ModuleResolverDb,
    metadata: DependencyMetadata,
) -> DependencyMetadata {
    let (projects, mut module_owners) = metadata.into_parts();
    let mut modules_with_owners: BTreeSet<_> = module_owners
        .iter()
        .map(|owner| owner.module().clone())
        .collect();

    for module in list_modules(db) {
        let Some(search_path) = module.search_path(db) else {
            continue;
        };

        if !search_path.is_editable() {
            continue;
        }

        let Some(file) = module.file(db) else {
            continue;
        };

        let Some(path) = file.path(db).as_system_path() else {
            continue;
        };

        let Some(project) = project_for_path(&projects, path) else {
            continue;
        };

        let module_name = module.name(db).clone();
        if !modules_with_owners.insert(module_name.clone()) {
            continue;
        }

        module_owners.push(ModuleOwner::new(module_name, [project.name().to_string()]));
    }

    DependencyMetadata::new(projects, module_owners)
}

fn project_for_path<'a>(
    projects: &'a [DependencyProject],
    path: &SystemPath,
) -> Option<&'a DependencyProject> {
    projects
        .iter()
        .filter(|project| path.starts_with(project.path()))
        .max_by_key(|project| project.path().as_str().len())
}

fn direct_dependencies(package: &Value, resolution: &Map<String, Value>) -> Result<Vec<String>> {
    let Some(dependencies) = package.get("dependencies") else {
        return Ok(Vec::new());
    };

    let dependencies = dependencies
        .as_array()
        .context("Expected `dependencies` to be an array")?;

    dependencies
        .iter()
        .map(|dependency| dependency_name(dependency, resolution))
        .collect()
}

fn dependency_group_dependencies(
    package: &Value,
    resolution: &Map<String, Value>,
) -> Result<Vec<String>> {
    let Some(dependency_groups) = package.get("dependency_groups") else {
        return Ok(Vec::new());
    };

    let dependency_groups = dependency_groups
        .as_array()
        .context("Expected `dependency_groups` to be an array")?;

    let mut dependencies = Vec::new();
    for dependency_group in dependency_groups {
        let dependency_group = dependency_group.as_object().with_context(|| {
            format!("Expected dependency group entry to be an object, got `{dependency_group}`")
        })?;
        let group_id = dependency_group
            .get("id")
            .and_then(Value::as_str)
            .context("Dependency group entry does not contain a group id")?;
        let group = resolution
            .get(group_id)
            .with_context(|| format!("Missing resolution dependency group node `{group_id}`"))?;
        dependencies.extend(direct_dependencies(group, resolution).with_context(|| {
            format!("Failed to read dependencies for dependency group `{group_id}`")
        })?);
    }

    Ok(dependencies)
}

fn dependency_name(dependency: &Value, resolution: &Map<String, Value>) -> Result<String> {
    match dependency {
        Value::String(reference) => Ok(resolve_package_reference(reference, resolution)
            .unwrap_or_else(|| reference.to_string())),
        Value::Object(object) => dependency_name_from_object(object, resolution),
        _ => {
            bail!("Expected dependency entry to be a string or object, got `{dependency}`")
        }
    }
}

fn dependency_name_from_object(
    dependency: &Map<String, Value>,
    resolution: &Map<String, Value>,
) -> Result<String> {
    if let Some(name) = string_field(dependency, &["name", "package_name", "package-name"]) {
        return Ok(name.to_string());
    }

    if let Some(reference) = string_field(dependency, &["id", "package_id", "package-id"]) {
        return Ok(resolve_package_reference(reference, resolution)
            .unwrap_or_else(|| reference.to_string()));
    }

    if let Some(package) = dependency.get("package") {
        return dependency_name(package, resolution);
    }

    bail!("Dependency entry does not contain a dependency name or package id")
}

fn resolve_package_reference(reference: &str, resolution: &Map<String, Value>) -> Option<String> {
    resolution
        .get(reference)
        .and_then(|package| package.get("name"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn string_field<'a>(object: &'a Map<String, Value>, fields: &[&str]) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|field| object.get(*field).and_then(Value::as_str))
}

#[cfg(test)]
mod tests {
    use ruff_db::Db as _;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPathBuf};
    use ruff_python_ast::PythonVersion;
    use ruff_python_ast::name::Name;
    use ty_module_resolver::{ModuleName, SearchPathSettings};
    use ty_python_core::platform::PythonPlatform;
    use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
    use ty_python_semantic::PythonVersionWithSource;

    use crate::ProjectMetadata;
    use crate::db::tests::TestDb;

    use super::{enrich_dependency_metadata_with_editables, parse_uv_workspace_metadata};
    use ty_python_semantic::dependency::{
        DependencyMetadata, DependencyProject, DistributionName, ModuleOwner,
    };

    fn setup_db(files: &[(&str, &str)]) -> anyhow::Result<TestDb> {
        let project =
            ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("/workspace"));
        let mut db = TestDb::new(project);

        db.memory_file_system()
            .create_directory_all("/workspace/app")?;
        db.memory_file_system()
            .create_directory_all("/site-packages")?;

        for (path, contents) in files {
            db.write_file(path, contents)?;
        }

        let search_paths = SearchPathSettings {
            src_roots: vec![SystemPathBuf::from("/workspace/app")],
            site_packages_paths: vec![SystemPathBuf::from("/site-packages")],
            ..SearchPathSettings::empty()
        }
        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)?;

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: PythonVersion::latest_ty(),
                    source: ty_python_semantic::PythonVersionSource::Default,
                },
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        );

        Ok(db)
    }

    fn owner_names(metadata: &DependencyMetadata, module: &str) -> Vec<String> {
        let module = ModuleName::new(module).unwrap();

        metadata
            .module_owners()
            .iter()
            .filter(|owner| owner.module() == &module)
            .flat_map(|owner| owner.owners().iter().map(ToString::to_string))
            .collect()
    }

    fn dependency_names<'a>(
        dependencies: impl Iterator<Item = &'a DistributionName>,
    ) -> Vec<String> {
        let mut dependencies = dependencies.map(ToString::to_string).collect::<Vec<_>>();
        dependencies.sort();
        dependencies
    }

    #[test]
    fn parses_uv_workspace_metadata() {
        let metadata = parse_uv_workspace_metadata(
            r#"
{
  "schema": {"version": "preview"},
  "members": [
    {
      "name": "app",
      "path": "/workspace/app",
      "id": "app==0.1.0@editable+/workspace/app/"
    }
  ],
  "resolution": {
    "app==0.1.0@editable+/workspace/app/": {
      "name": "app",
      "dependencies": [
        {"id": "requests==2.32.0@registry+https://pypi.org/simple"},
        "rich==13.0.0",
        {"package": "attrs==24.0.0"}
      ]
    },
    "requests==2.32.0@registry+https://pypi.org/simple": {"name": "requests"},
    "rich==13.0.0": {"name": "rich"},
    "attrs==24.0.0": {"name": "attrs"}
  },
  "module_owners": {
    "requests": ["requests==2.32.0@registry+https://pypi.org/simple"],
    "rich": ["rich==13.0.0"]
  }
}
"#,
        )
        .unwrap();

        let debug = format!("{metadata:?}");
        assert!(debug.contains("requests"));
        assert!(debug.contains("rich"));
        assert!(debug.contains("attrs"));
    }

    #[test]
    fn parses_dependency_group_dependencies_separately() {
        let metadata = parse_uv_workspace_metadata(
            r#"
{
  "schema": {"version": "preview"},
  "members": [
    {
      "name": "app",
      "path": "/workspace/app",
      "id": "app"
    }
  ],
  "resolution": {
    "app": {
      "name": "app",
      "dependencies": [
        {"id": "requests==2.32.0@registry+https://pypi.org/simple"}
      ],
      "dependency_groups": [
        {"name": "dev", "id": "app:dev"}
      ]
    },
    "app:dev": {
      "dependencies": [
        {"id": "inline-snapshot==0.20.8@registry+https://pypi.org/simple"}
      ]
    },
    "requests==2.32.0@registry+https://pypi.org/simple": {"name": "requests"},
    "inline-snapshot==0.20.8@registry+https://pypi.org/simple": {"name": "inline-snapshot"}
  },
  "module_owners": {}
}
"#,
        )
        .unwrap();

        let project = metadata.projects().first().unwrap();
        assert_eq!(
            dependency_names(project.direct_dependencies()),
            ["requests"]
        );
        assert_eq!(
            dependency_names(project.dependency_group_dependencies()),
            ["inline-snapshot"]
        );
    }

    #[test]
    fn parses_nested_module_owner_map() {
        let metadata = parse_uv_workspace_metadata(
            r#"
{
  "schema": {"version": "preview"},
  "members": [],
  "resolution": {
    "gpu-a==0.1.0@path+/workspace/gpu_a-0.1.0-py3-none-any.whl": {"name": "gpu-a"},
    "gpu-b==0.1.0@path+/workspace/gpu_b-0.1.0-py3-none-any.whl": {"name": "gpu-b"}
  },
  "module_owners": {
    "gpu": [
      "gpu-a==0.1.0@path+/workspace/gpu_a-0.1.0-py3-none-any.whl",
      "gpu-b==0.1.0@path+/workspace/gpu_b-0.1.0-py3-none-any.whl"
    ],
    "gpu.a": ["gpu-a==0.1.0@path+/workspace/gpu_a-0.1.0-py3-none-any.whl"],
    "gpu.b": ["gpu-b==0.1.0@path+/workspace/gpu_b-0.1.0-py3-none-any.whl"]
  }
}
"#,
        )
        .unwrap();

        let debug = format!("{metadata:?}");
        assert!(debug.contains("gpu-a"));
        assert!(debug.contains("gpu-b"));
        assert!(debug.contains("gpu.a"));
        assert!(debug.contains("gpu.b"));
        assert!(!debug.contains("@path+"));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let error = parse_uv_workspace_metadata(
            r#"
{
  "schema": {"version": "1"},
  "members": [],
  "resolution": {}
}
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Unsupported dependency metadata schema version")
        );
    }

    #[test]
    fn infers_editable_module_owners() -> anyhow::Result<()> {
        let db = setup_db(&[
            ("/site-packages/_lib.pth", "/workspace/lib/src"),
            ("/workspace/lib/src/lib_module/__init__.py", ""),
        ])?;
        let metadata = DependencyMetadata::new(
            vec![
                DependencyProject::new(
                    "app",
                    SystemPathBuf::from("/workspace/app"),
                    std::iter::empty::<&str>(),
                ),
                DependencyProject::new(
                    "lib-project",
                    SystemPathBuf::from("/workspace/lib"),
                    std::iter::empty::<&str>(),
                ),
            ],
            vec![],
        );

        let metadata = enrich_dependency_metadata_with_editables(&db, metadata);

        assert_eq!(owner_names(&metadata, "lib_module"), ["lib-project"]);
        Ok(())
    }

    #[test]
    fn nested_workspace_member_uses_longest_matching_project_path() -> anyhow::Result<()> {
        let db = setup_db(&[
            ("/site-packages/_parent.pth", "/workspace/parent"),
            ("/workspace/parent/child/__init__.py", ""),
        ])?;
        let metadata = DependencyMetadata::new(
            vec![
                DependencyProject::new(
                    "parent-project",
                    SystemPathBuf::from("/workspace/parent"),
                    std::iter::empty::<&str>(),
                ),
                DependencyProject::new(
                    "child-project",
                    SystemPathBuf::from("/workspace/parent/child"),
                    std::iter::empty::<&str>(),
                ),
            ],
            vec![],
        );

        let metadata = enrich_dependency_metadata_with_editables(&db, metadata);

        assert_eq!(owner_names(&metadata, "child"), ["child-project"]);
        Ok(())
    }

    #[test]
    fn explicit_module_owners_are_preserved() -> anyhow::Result<()> {
        let db = setup_db(&[
            ("/site-packages/_editable.pth", "/workspace/editable/src"),
            ("/workspace/editable/src/editable/__init__.py", ""),
        ])?;
        let metadata = DependencyMetadata::new(
            vec![DependencyProject::new(
                "editable-project",
                SystemPathBuf::from("/workspace/editable"),
                std::iter::empty::<&str>(),
            )],
            vec![ModuleOwner::new(
                ModuleName::new_static("editable").unwrap(),
                ["explicit-owner"],
            )],
        );

        let metadata = enrich_dependency_metadata_with_editables(&db, metadata);

        assert_eq!(owner_names(&metadata, "editable"), ["explicit-owner"]);
        Ok(())
    }

    #[test]
    fn namespace_editable_modules_are_skipped() -> anyhow::Result<()> {
        let db = setup_db(&[
            ("/site-packages/_namespace.pth", "/workspace/namespace/src"),
            ("/workspace/namespace/src/ns_pkg/submodule.py", ""),
        ])?;
        let metadata = DependencyMetadata::new(
            vec![DependencyProject::new(
                "namespace-project",
                SystemPathBuf::from("/workspace/namespace"),
                std::iter::empty::<&str>(),
            )],
            vec![],
        );

        let metadata = enrich_dependency_metadata_with_editables(&db, metadata);

        assert_eq!(owner_names(&metadata, "ns_pkg"), Vec::<String>::new());
        Ok(())
    }
}
