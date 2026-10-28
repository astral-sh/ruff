//! Import classification using uv workspace metadata.

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};
use itertools::Itertools;
use ruff_linter::fs;
use ruff_linter::registry::Rule;
use ruff_linter::rules::isort::categorize::{ImportSection, ImportType, WorkspaceImports};
use ruff_linter::warn_user;
use ruff_python_stdlib::identifiers::is_identifier;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;

use crate::Settings;

/// Shares a workspace's metadata across Ruff configurations in a single discovery pass.
#[derive(Debug, Default)]
pub struct UvImportDiscovery {
    workspaces: Mutex<FxHashMap<PathBuf, Option<WorkspaceImports>>>,
}

impl UvImportDiscovery {
    pub fn enabled() -> bool {
        matches!(env::var("RUFF_UV").as_deref(), Ok("1" | "true"))
    }

    pub(crate) fn is_project(directory: &Path) -> bool {
        directory.join("pyproject.toml").is_file() || directory.join("uv.lock").is_file()
    }

    pub(crate) fn apply(&self, settings: &mut Settings, directory: &Path) {
        if !Self::uses_import_sections(settings) {
            return;
        }
        let directory = fs::normalize_path(directory);
        settings.linter.isort.known_modules.workspace =
            Self::project_root(&directory).and_then(|project| self.discover(project));
    }

    fn project_root(directory: &Path) -> Option<&Path> {
        directory.ancestors().find(|dir| Self::is_project(dir))
    }

    fn discover(&self, root: &Path) -> Option<WorkspaceImports> {
        let mut workspaces = self.workspaces.lock().unwrap();
        if !workspaces.contains_key(root) {
            let imports = match read_metadata(root) {
                Ok(metadata) => {
                    let imports = metadata.imports();
                    workspaces.insert(metadata.workspace_root, Some(imports.clone()));
                    for member in metadata.members {
                        workspaces.insert(member.path, Some(imports.clone()));
                    }
                    Some(imports)
                }
                Err(err) => {
                    warn_user!(
                        "Failed to read uv workspace metadata from `{}`: {err:#}",
                        root.display()
                    );
                    None
                }
            };
            workspaces.insert(root.to_owned(), imports);
        }
        workspaces.get(root).cloned().flatten()
    }

    pub(crate) fn uses_import_sections(settings: &Settings) -> bool {
        settings.linter.rules.any_enabled(&[
            Rule::UnsortedImports,
            Rule::UnusedImport,
            Rule::TypingOnlyFirstPartyImport,
            Rule::TypingOnlyThirdPartyImport,
            Rule::TypingOnlyStandardLibraryImport,
        ])
    }
}

fn read_metadata(root: &Path) -> Result<Metadata> {
    let uv = match env::var_os("UV") {
        Some(executable) => PathBuf::from(executable),
        None => which::which("uv").context("failed to find `uv` on PATH")?,
    };
    let output = Command::new(uv)
        .args(["workspace", "metadata", "--quiet", "--active"])
        .current_dir(root)
        .output()
        .context("failed to invoke `uv workspace metadata`")?;
    if !output.status.success() {
        bail!(
            "`uv workspace metadata` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    // Older uv versions suppress the JSON output when passed `--quiet`.
    ensure!(
        !output.stdout.is_empty(),
        "`uv workspace metadata` returned no output (requires uv 0.12.3 or later)"
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).context("invalid `uv workspace metadata` JSON")?;
    ensure!(
        metadata.schema.version == "preview",
        "unsupported uv metadata schema `{}`",
        metadata.schema.version
    );
    ensure!(
        metadata.workspace_root.is_absolute()
            && metadata
                .members
                .iter()
                .all(|member| member.path.is_absolute()),
        "uv returned a relative workspace or member path"
    );
    Ok(metadata)
}

#[derive(Deserialize)]
struct Metadata {
    schema: Schema,
    workspace_root: PathBuf,
    #[serde(default)]
    members: Vec<Member>,
    #[serde(default)]
    environment: Option<Environment>,
    #[serde(default)]
    modules: BTreeMap<String, Module>,
    // uv versions before the introduction of `modules` return the owner lists directly.
    #[serde(default)]
    module_owners: BTreeMap<String, Vec<Owner>>,
}

#[derive(Deserialize)]
struct Schema {
    version: String,
}

#[derive(Deserialize)]
struct Member {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct Environment {
    #[serde(default)]
    packages: BTreeMap<String, InstalledPackage>,
}

#[derive(Deserialize)]
struct InstalledPackage {
    path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Module {
    owners: Vec<Owner>,
}

#[derive(Deserialize)]
struct Owner {
    package_id: String,
}

impl Metadata {
    fn imports(&self) -> WorkspaceImports {
        let member_ids: FxHashSet<_> = self.members.iter().map(|member| &member.id).collect();
        let mut sections = self
            .module_owners
            .iter()
            .chain(
                self.modules
                    .iter()
                    .map(|(name, module)| (name, &module.owners)),
            )
            .map(|(name, owners)| {
                // Shared namespaces only have a section if all their owners agree.
                let section = owners
                    .iter()
                    .map(|owner| {
                        if member_ids.contains(&owner.package_id) {
                            ImportType::FirstParty
                        } else {
                            ImportType::ThirdParty
                        }
                    })
                    .all_equal_value()
                    .ok()
                    .map(ImportSection::Known);
                (name.clone(), section)
            })
            .collect::<BTreeMap<_, _>>();

        let roots = self
            .members
            .iter()
            .filter_map(|member| {
                let package = self.environment.as_ref()?.packages.get(&member.id)?;
                Some(package.editable_roots(&member.path))
            })
            .flatten()
            .unique();
        for root in roots {
            add_source_modules(&root, &mut sections);
        }
        WorkspaceImports::new(sections)
    }
}

impl InstalledPackage {
    fn editable_roots(&self, member: &Path) -> Vec<PathBuf> {
        let Some(dist_info) = self.path.as_deref().filter(|path| path.is_absolute()) else {
            return Vec::new();
        };
        let Some(site_packages) = dist_info.parent() else {
            return Vec::new();
        };
        let Ok(mut record) = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_path(dist_info.join("RECORD"))
        else {
            return Vec::new();
        };
        let mut roots = Vec::new();
        // Python processes .pth files installed directly in site-packages. The wheel's RECORD
        // identifies which distribution installed the file, even when its name differs.
        for row in record.records().flatten() {
            let Some(pth) = row.get(0).map(Path::new) else {
                continue;
            };
            if pth.parent() != Some(Path::new("")) || pth.extension().is_none_or(|ext| ext != "pth")
            {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(site_packages.join(pth)) else {
                continue;
            };
            for line in contents.lines().map(str::trim_end) {
                if line.is_empty()
                    || line.starts_with('#')
                    || line.starts_with("import ")
                    || line.starts_with("import\t")
                {
                    continue;
                }
                let root = fs::normalize_path_to(line, site_packages);
                if root.starts_with(member) {
                    roots.push(root);
                }
            }
        }
        roots
    }
}

/// Index the names exposed by a member's editable search path. A regular package owns its
/// children. For a namespace, inspect its children to avoid claiming unrelated directories.
fn add_source_modules(root: &Path, sections: &mut BTreeMap<String, Option<ImportSection>>) {
    let first = Some(ImportSection::Known(ImportType::FirstParty));
    let mut entries = walkdir::WalkDir::new(root)
        .follow_links(true)
        .min_depth(1)
        .into_iter();
    while let Some(entry) = entries.next() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let is_dir = entry.file_type().is_dir();
        let module = if is_dir {
            if relative
                .file_name()
                .and_then(|s| s.to_str())
                .is_none_or(|s| s == "__pycache__" || !is_identifier(s))
            {
                entries.skip_current_dir();
                continue;
            }
            if !path.join("__init__.py").is_file() && !path.join("__init__.pyi").is_file() {
                continue;
            }
            relative.to_owned()
        } else if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("py" | "pyi")
        ) && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s != "__init__" && is_identifier(s))
        {
            relative.with_extension("")
        } else {
            continue;
        };
        let name = module.iter().filter_map(|part| part.to_str()).join(".");
        if is_dir && sections.get(&name).is_none_or(|section| *section == first) {
            // Children inherit this package's section unless uv has more specific owners.
            let prefix = format!("{name}.");
            if !sections
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(child, _)| child.starts_with(&prefix))
            {
                entries.skip_current_dir();
            }
        }
        // uv's explicit owners take precedence, including mixed namespaces.
        for (dot, _) in name.match_indices('.') {
            sections
                .entry(name[..dot].to_owned())
                .or_insert_with(|| first.clone());
        }
        sections.entry(name).or_insert_with(|| first.clone());
    }
}
