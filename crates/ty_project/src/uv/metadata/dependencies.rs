use std::collections::{BTreeMap, BTreeSet};

use compact_str::CompactString;
use ruff_db::diagnostic::{
    Diagnostic, DiagnosticId, Severity, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::system::SystemPathBuf;
use thiserror::Error;
use ty_module_resolver::ModuleName;
use ty_python_semantic::dependency::{
    DependencyDistribution, DependencyMetadata, DependencyProject,
};

use super::{NodeKind, ResolutionNode, UvMetadata};

impl UvMetadata {
    pub(crate) fn dependency_metadata(
        &self,
    ) -> Result<DependencyMetadata, DependencyMetadataError> {
        let root = self.workspace_root();
        let mut distributions = BTreeMap::new();
        let mut extra_packages = BTreeMap::new();

        for (id, node) in &self.resolution {
            if node.kind != NodeKind::Package {
                continue;
            }

            let editable_path = node
                .source
                .as_ref()
                .and_then(|source| source.editable.clone());
            if let Some(path) = &editable_path
                && !path.is_absolute()
            {
                return Err(DependencyMetadataError::RelativePath {
                    kind: "editable package",
                    id: id.clone(),
                    path: path.clone(),
                });
            }

            distributions.insert(
                id.clone(),
                DependencyDistribution {
                    name: node
                        .name
                        .clone()
                        .ok_or_else(|| DependencyMetadataError::MissingPackageName(id.clone()))?,
                    editable_path,
                },
            );

            for extra in &node.optional_dependencies {
                let extra_node = self.node(&extra.id)?;
                if !matches!(extra_node.kind, NodeKind::Extra(_)) {
                    return Err(DependencyMetadataError::UnexpectedNodeKind {
                        id: extra.id.clone(),
                        expected: "extra",
                    });
                }
                if let Some(previous) = extra_packages.insert(&extra.id, id)
                    && previous != id
                {
                    return Err(DependencyMetadataError::SharedExtra {
                        id: extra.id.clone(),
                        first: previous.clone(),
                        second: id.clone(),
                    });
                }
            }
        }

        let package_id = |id: &CompactString| {
            if distributions.contains_key(id) {
                Ok(id.clone())
            } else {
                extra_packages
                    .get(id)
                    .copied()
                    .cloned()
                    .ok_or_else(|| DependencyMetadataError::UnknownDependency(id.clone()))
            }
        };

        let dependencies = |node: &ResolutionNode| {
            node.dependencies
                .iter()
                .map(|dependency| package_id(&dependency.id))
                .collect::<Result<BTreeSet<_>, _>>()
        };

        let group_dependencies = |node: &ResolutionNode| {
            let mut groups = BTreeSet::new();
            for group in &node.dependency_groups {
                let group_node = self.node(&group.id)?;
                if !matches!(group_node.kind, NodeKind::Group(_)) {
                    return Err(DependencyMetadataError::UnexpectedNodeKind {
                        id: group.id.clone(),
                        expected: "dependency group",
                    });
                }
                groups.extend(dependencies(group_node)?);
            }
            Ok(groups)
        };

        let workspace_groups = match &self.workspace {
            Some(workspace) => {
                let node = self.node(&workspace.id)?;
                if node.kind != NodeKind::Workspace {
                    return Err(DependencyMetadataError::UnexpectedNodeKind {
                        id: workspace.id.clone(),
                        expected: "workspace",
                    });
                }
                group_dependencies(node)?
            }
            None => BTreeSet::new(),
        };

        let mut projects = Vec::new();
        let mut member_paths = BTreeSet::new();
        for member in &self.members {
            if !member.path.is_absolute() {
                return Err(DependencyMetadataError::RelativePath {
                    kind: "workspace member",
                    id: member.id.clone(),
                    path: member.path.clone(),
                });
            }
            if !member_paths.insert(&member.path) {
                return Err(DependencyMetadataError::DuplicateMemberPath(
                    member.path.clone(),
                ));
            }
            let node = self.node(&member.id)?;
            if node.kind != NodeKind::Package {
                return Err(DependencyMetadataError::UnexpectedNodeKind {
                    id: member.id.clone(),
                    expected: "package",
                });
            }

            let mut direct = dependencies(node)?;
            for extra in &node.optional_dependencies {
                // A member's own extras declare dependencies directly. By contrast, requesting an
                // extra of another package only declares that package, not its extra's dependencies.
                direct.extend(dependencies(self.node(&extra.id)?)?);
            }
            // Extra nodes also point back to their own package. A project's own imports are
            // accounted for by `distribution`, not by listing itself as a dependency.
            direct.remove(&member.id);

            let mut groups = group_dependencies(node)?;
            groups.extend(workspace_groups.iter().cloned());

            projects.push(DependencyProject {
                path: member.path.clone(),
                distribution: Some(member.id.clone()),
                dependencies: direct,
                group_dependencies: groups,
            });
        }

        if self.workspace.is_some()
            && !projects
                .iter()
                .any(|project| project.path.as_path() == root)
        {
            projects.push(DependencyProject {
                path: root.to_path_buf(),
                distribution: None,
                dependencies: BTreeSet::new(),
                group_dependencies: workspace_groups,
            });
        }

        if projects.is_empty() {
            return Err(DependencyMetadataError::MissingProjects);
        }

        let mut module_owners: BTreeMap<ModuleName, Box<[CompactString]>> = BTreeMap::new();
        for (module, owners) in &self.module_owners {
            let Some(module) = ModuleName::new(module) else {
                continue;
            };
            // Keep an empty entry when any owner is unknown. Omitting the entry would allow a
            // caller to use a known parent module's owner for this incomplete child module.
            let owners = owners
                .iter()
                .map(|owner| {
                    distributions
                        .contains_key(&owner.package_id)
                        .then(|| owner.package_id.clone())
                })
                .collect::<Option<BTreeSet<_>>>();
            let owners = owners.map_or_else(Box::default, |owners| owners.into_iter().collect());
            module_owners.insert(module, owners);
        }

        if module_owners.values().all(|owners| owners.is_empty())
            && !distributions
                .values()
                .any(|distribution| distribution.editable_path.is_some())
            // Check for a package outside the workspace. A dependency-free virtual workspace
            // has no modules to attribute, so its empty ownership map is valid.
            && distributions
                .keys()
                .any(|id| !self.members.iter().any(|member| member.id == id))
        {
            return Err(DependencyMetadataError::MissingModuleOwnership);
        }

        projects.sort_by(|left, right| left.path.cmp(&right.path));

        Ok(DependencyMetadata {
            projects: projects.into_boxed_slice(),
            distributions,
            module_owners,
        })
    }

    fn node(&self, id: &CompactString) -> Result<&ResolutionNode, DependencyMetadataError> {
        self.resolution
            .get(id)
            .ok_or_else(|| DependencyMetadataError::MissingNode(id.clone()))
    }
}

/// Why uv's dependency metadata cannot be used for the selected Python environment.
#[derive(Debug, Clone, PartialEq, Eq, Error, get_size2::GetSize)]
pub(crate) enum DependencyMetadataError {
    #[error("resolution node `{0}` is missing")]
    MissingNode(CompactString),
    #[error("package node `{0}` is missing its name")]
    MissingPackageName(CompactString),
    #[error("resolution node `{id}` is not a {expected} node")]
    UnexpectedNodeKind {
        id: CompactString,
        expected: &'static str,
    },
    #[error("dependency `{0}` is not a known package or extra")]
    UnknownDependency(CompactString),
    #[error("extra node `{id}` belongs to both package `{first}` and package `{second}`")]
    SharedExtra {
        id: CompactString,
        first: CompactString,
        second: CompactString,
    },
    #[error("{kind} `{id}` has a non-absolute path `{path}`")]
    RelativePath {
        kind: &'static str,
        id: CompactString,
        path: SystemPathBuf,
    },
    #[error("multiple workspace members use path `{0}`")]
    DuplicateMemberPath(SystemPathBuf),
    #[error("no workspace project information is available in uv metadata")]
    MissingProjects,
    #[error("uv metadata has no module ownership or editable source paths")]
    MissingModuleOwnership,
    #[error("uv did not provide a Python environment")]
    MissingEnvironment,
    #[error("could not read uv's Python environment `{path}`: {message}")]
    InvalidEnvironment {
        path: SystemPathBuf,
        message: Box<str>,
    },
    #[error("could not resolve the selected Python environment: {0}")]
    EnvironmentResolution(Box<str>),
    #[error("no Python environment is configured")]
    MissingSelectedEnvironment,
    #[error(
        "selected Python environment `{selected}` (from {selected_origin}) differs from uv's environment `{uv}`"
    )]
    EnvironmentMismatch {
        selected: SystemPathBuf,
        selected_origin: Box<str>,
        uv: SystemPathBuf,
    },
}

impl DependencyMetadataError {
    pub(crate) fn to_diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::UvMetadata,
            Severity::Warning,
            "Failed to load uv dependency metadata",
        );
        diagnostic.set_concise_message(format_args!(
            "Failed to load uv dependency metadata: {self}"
        ));
        diagnostic.sub(SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            self.to_string(),
        ));
        match self {
            Self::MissingModuleOwnership
            | Self::MissingEnvironment
            | Self::MissingSelectedEnvironment => {
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Help,
                    "Synchronize the environment with `uv sync` or run ty through `uv check`",
                ));
            }
            Self::EnvironmentMismatch { uv, .. } => {
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Help,
                    format_args!("Use `--python` to select uv's Python environment at `{uv}`"),
                ));
            }
            _ => {}
        }
        diagnostic
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::Context;
    use compact_str::CompactString;
    use ruff_db::system::{SystemPathBuf, TestSystem};
    use serde_json::{Value, json};
    use ty_module_resolver::ModuleName;
    use ty_python_semantic::dependency::{DependencyMetadata, DependencyProject};

    use super::UvMetadata;

    fn absolute(path: &str) -> SystemPathBuf {
        if cfg!(windows) {
            SystemPathBuf::from(format!("C:{path}"))
        } else {
            SystemPathBuf::from(path)
        }
    }

    fn metadata() -> Value {
        json!({
            "schema": {"version": "preview"},
            "workspace_root": absolute("/app"),
            "workspace": {"id": "workspace"},
            "members": [{"id": "member", "name": "app", "path": absolute("/app")}],
            "module_owners": {
                "direct": [{"package_id": "direct"}],
                "indirect": [{"package_id": "indirect"}],
                "namespace": [{"package_id": "direct"}, {"package_id": "indirect"}],
                "namespace.direct": [{"package_id": "direct"}],
                "namespace.indirect": [{"package_id": "indirect"}]
            },
            "resolution": {
                "workspace": {
                    "kind": "workspace", "dependencies": [],
                    "dependency_groups": [{"id": "workspace-group"}]
                },
                "workspace-group": {
                    "kind": {"group": "dev"},
                    "dependencies": [{"id": "workspace-tool"}]
                },
                "member": {
                    "kind": "package", "name": "app", "source": {"virtual": absolute("/app")},
                    "dependencies": [{"id": "required-extra"}],
                    "optional_dependencies": [{"id": "member-extra"}],
                    "dependency_groups": [{"id": "member-group"}]
                },
                "member-extra": {
                    "kind": {"extra": "feature"},
                    "dependencies": [{"id": "member"}, {"id": "optional"}]
                },
                "member-group": {
                    "kind": {"group": "test"},
                    "dependencies": [{"id": "development"}]
                },
                "direct": {
                    "kind": "package", "name": "a-different-distribution-name",
                    "dependencies": [{"id": "indirect"}],
                    "optional_dependencies": [{"id": "required-extra"}]
                },
                "required-extra": {
                    "kind": {"extra": "feature"},
                    "dependencies": [{"id": "direct"}, {"id": "indirect"}]
                },
                "indirect": {"kind": "package", "name": "indirect", "dependencies": []},
                "optional": {"kind": "package", "name": "optional", "dependencies": []},
                "development": {
                    "kind": "package", "name": "development",
                    "dependencies": [{"id": "indirect"}]
                },
                "workspace-tool": {
                    "kind": "package", "name": "workspace-tool", "dependencies": []
                }
            }
        })
    }

    fn extract(metadata: &Value) -> anyhow::Result<DependencyMetadata> {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .write_file_all(absolute("/app/pyproject.toml"), "[tool.uv.workspace]")?;
        let metadata = UvMetadata::from_metadata(&serde_json::to_vec(metadata)?, &system)?;
        Ok(metadata.dependency_metadata()?)
    }

    fn project<'a>(
        metadata: &'a DependencyMetadata,
        path: &str,
    ) -> anyhow::Result<&'a DependencyProject> {
        let path = absolute(path);
        metadata
            .projects
            .iter()
            .find(|project| project.path == path)
            .context("expected a project at this path")
    }

    fn ids<const N: usize>(ids: [&str; N]) -> BTreeSet<CompactString> {
        ids.into_iter().map(CompactString::from).collect()
    }

    #[test]
    fn separates_direct_dependencies_from_transitive_packages() -> anyhow::Result<()> {
        let metadata = extract(&metadata())?;
        let project = project(&metadata, "/app")?;

        assert_eq!(project.distribution.as_deref(), Some("member"));
        assert_eq!(project.dependencies, ids(["direct", "optional"]));
        assert_eq!(
            project.group_dependencies,
            ids(["development", "workspace-tool"])
        );
        assert_eq!(
            metadata
                .distributions
                .get("direct")
                .map(|distribution| distribution.name.as_str()),
            Some("a-different-distribution-name")
        );

        Ok(())
    }

    #[test]
    fn preserves_ambiguous_namespace_owners_and_submodules() -> anyhow::Result<()> {
        let metadata = extract(&metadata())?;

        for (name, expected) in [
            ("namespace", vec!["direct", "indirect"]),
            ("namespace.direct", vec!["direct"]),
            ("namespace.indirect", vec!["indirect"]),
        ] {
            let name = ModuleName::new(name).context("expected a valid module name")?;
            let owners = metadata
                .module_owners
                .get(&name)
                .context("expected module ownership")?;
            assert_eq!(
                owners.iter().map(CompactString::as_str).collect::<Vec<_>>(),
                expected
            );
        }

        Ok(())
    }

    #[test]
    fn workspace_groups_apply_to_each_member_and_virtual_root() -> anyhow::Result<()> {
        let mut input = metadata();
        input["members"] = json!([
            {"id": "member", "name": "app", "path": absolute("/app/packages/member")},
            {"id": "sibling", "name": "sibling", "path": absolute("/app/packages/sibling")}
        ]);
        input["resolution"]["sibling"] = json!({
            "kind": "package", "name": "sibling", "dependencies": []
        });
        let metadata = extract(&input)?;

        assert_eq!(metadata.projects.len(), 3);
        let root = project(&metadata, "/app")?;
        assert_eq!(root.distribution, None);
        assert_eq!(root.dependencies, ids([]));
        assert_eq!(root.group_dependencies, ids(["workspace-tool"]));

        let member = project(&metadata, "/app/packages/member")?;
        assert_eq!(member.dependencies, ids(["direct", "optional"]));
        assert_eq!(
            member.group_dependencies,
            ids(["development", "workspace-tool"])
        );

        let sibling = project(&metadata, "/app/packages/sibling")?;
        assert_eq!(sibling.dependencies, ids([]));
        assert_eq!(sibling.group_dependencies, ids(["workspace-tool"]));

        Ok(())
    }

    #[test]
    fn workspace_without_members_can_supply_groups() -> anyhow::Result<()> {
        let metadata = extract(&json!({
            "schema": {"version": "preview"},
            "workspace_root": absolute("/app"),
            "workspace": {"id": "workspace"},
            "module_owners": {"tool": [{"package_id": "tool"}]},
            "resolution": {
                "workspace": {
                    "kind": "workspace", "dependencies": [],
                    "dependency_groups": [{"id": "group"}]
                },
                "group": {"kind": {"group": "dev"}, "dependencies": [{"id": "tool"}]},
                "tool": {"kind": "package", "name": "tool", "dependencies": []}
            }
        }))?;

        let root = project(&metadata, "/app")?;
        assert_eq!(root.distribution, None);
        assert_eq!(root.group_dependencies, ids(["tool"]));

        Ok(())
    }

    #[test]
    fn editable_paths_supply_ownership_without_recorded_modules() -> anyhow::Result<()> {
        let mut input = metadata();
        input["module_owners"] = json!({});
        input["resolution"]["direct"]["source"] =
            json!({"editable": absolute("/editable-package")});
        let metadata = extract(&input)?;

        assert!(metadata.module_owners.is_empty());
        assert_eq!(
            metadata
                .distributions
                .get("direct")
                .and_then(|distribution| distribution.editable_path.as_deref()),
            Some(absolute("/editable-package").as_path())
        );

        Ok(())
    }

    #[test]
    fn unknown_module_owners_prevent_parent_fallback() -> anyhow::Result<()> {
        let mut input = metadata();
        input["module_owners"]["namespace"] = json!([{"package_id": "direct"}]);
        input["module_owners"]["namespace.indirect"] = json!([
            {"package_id": "indirect"}, {"package_id": "missing-package"}
        ]);
        input["module_owners"]["namespace.empty"] = json!([]);
        input["module_owners"]["not-a-module"] = json!([{"package_id": "direct"}]);
        let metadata = extract(&input)?;

        let namespace = ModuleName::new("namespace").context("expected a valid module name")?;
        assert_eq!(
            metadata.module_owners.get(&namespace).map(AsRef::as_ref),
            Some([CompactString::from("direct")].as_slice())
        );
        for child in ["namespace.indirect", "namespace.empty"] {
            let child = ModuleName::new(child).context("expected a valid module name")?;
            assert!(
                metadata
                    .module_owners
                    .get(&child)
                    .is_some_and(|owners| owners.is_empty())
            );
        }
        assert_eq!(metadata.module_owners.len(), 6);

        Ok(())
    }

    #[test]
    fn virtual_projects_without_dependencies_do_not_require_module_ownership() -> anyhow::Result<()>
    {
        for input in [
            json!({
                "schema": {"version": "preview"},
                "workspace_root": absolute("/app"),
                "workspace": {"id": "workspace"},
                "resolution": {
                    "workspace": {"kind": "workspace", "dependencies": []}
                }
            }),
            json!({
                "schema": {"version": "preview"},
                "workspace_root": absolute("/app"),
                "members": [{"id": "app", "name": "app", "path": absolute("/app")}],
                "resolution": {
                    "app": {
                        "kind": "package", "name": "app", "source": {"virtual": absolute("/app")},
                        "dependencies": []
                    }
                }
            }),
        ] {
            let metadata = extract(&input)?;
            assert!(project(&metadata, "/app")?.dependencies.is_empty());
            assert!(metadata.module_owners.is_empty());
        }

        Ok(())
    }

    #[test]
    fn unavailable_dependency_information_has_a_reason() -> anyhow::Result<()> {
        let mut missing_graph = metadata();
        missing_graph["resolution"] = json!({});
        let mut missing_projects = metadata();
        missing_projects["workspace"] = json!(null);
        missing_projects["members"] = json!([]);
        let mut missing_owners = metadata();
        missing_owners["module_owners"] = json!({});

        for (input, expected) in [
            (missing_graph, "resolution node `workspace` is missing"),
            (
                missing_projects,
                "no workspace project information is available in uv metadata",
            ),
            (
                missing_owners,
                "uv metadata has no module ownership or editable source paths",
            ),
        ] {
            let error = extract(&input)
                .err()
                .context("expected unavailable information to disable dependency checks")?;
            assert_eq!(format!("{error:#}"), expected, "{input}");
        }

        Ok(())
    }
}
