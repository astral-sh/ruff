use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use ruff_db::system::SystemPathBuf;
use serde::Deserialize;
use serde_json::{Map, Value};
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

pub(crate) fn parse_uv_workspace_metadata(source: &str) -> Result<DependencyMetadata> {
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

        projects.push(DependencyProject::new(
            member.name,
            SystemPathBuf::from(member.path),
            direct_dependencies,
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
    use super::parse_uv_workspace_metadata;

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
}
