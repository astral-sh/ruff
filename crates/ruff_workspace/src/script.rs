//! Resolve Ruff settings from inline script metadata (PEP 723).

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use pep440_rs::VersionSpecifiers;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, Severity, Span,
};
use ruff_db::system::SystemPathBuf;
use ruff_linter::fs;
use ruff_linter::message::RenderContext;
use ruff_linter::package::PackageRoot;
use ruff_linter::packaging::detect_package_root;
use ruff_linter::preview::is_script_metadata_enabled;
use ruff_linter::settings::{LinterSettings, types::PreviewMode};
use ruff_python_ast::script::ScriptTag;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_ranged_value::{ValueSource, ValueSourceGuard};
use ruff_source_file::{SourceFile, SourceFileBuilder};

use crate::Settings;
use crate::configuration::Configuration;
use crate::options::Options;
use crate::pyproject::{Tools, check_required_version, get_minimum_supported_version};
use crate::resolver::{
    ConfigurationOrigin, ConfigurationTransformer, Project, resolve_configuration,
};

/// How script configuration participates in settings resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptConfiguration {
    /// Apply inline script configuration and Python requirements.
    Enabled,
    /// Validate script metadata without applying its configuration or Python requirement.
    Disabled,
    /// Validate script metadata and apply only `requires-python`, as in editor-only mode.
    RequiresPythonOnly,
}

/// A Python file with a PEP 723 script metadata header.
#[derive(Debug)]
pub struct Script {
    has_ruff_table: bool,
    settings: ScriptSettings,
    config_path: Option<PathBuf>,
}

impl Script {
    /// Return the inline configuration's path, falling back to the project's path.
    pub fn config_path<'a>(&'a self, project: &'a Project) -> Option<&'a Path> {
        self.config_path
            .as_deref()
            .or_else(|| project.config_path())
    }

    /// Whether the script contains a `[tool.ruff]` table, even if empty or ignored.
    pub fn has_ruff_table(&self) -> bool {
        self.has_ruff_table
    }

    /// Whether the script declares `requires-python` or `[tool.ruff]`, even if ignored.
    pub fn declares_settings(&self) -> bool {
        matches!(self.settings, ScriptSettings::Declared(_))
    }

    /// Return the script's resolved settings, falling back to the project's settings.
    pub fn settings<'a>(&'a self, project: &'a Settings) -> &'a Settings {
        match &self.settings {
            ScriptSettings::Declared(Some(settings)) => settings,
            ScriptSettings::Undeclared | ScriptSettings::Declared(None) => project,
        }
    }

    /// Resolve the script's package root when its namespace settings differ from the project.
    pub fn package_root<'a>(
        &self,
        path: &'a Path,
        project_settings: &LinterSettings,
        package: Option<PackageRoot<'a>>,
        package_roots: &FxHashMap<&Path, Option<PackageRoot<'_>>>,
    ) -> Option<PackageRoot<'a>> {
        // FIXME: Resolver::package_roots runs before inline script settings are read. Its
        // discovery cache records whether a directory is a package, including whether it
        // matches the active namespace-packages setting, but shares that answer across
        // configurations. The subsequent pass also marks nested roots using one map for all
        // configurations, so both cached membership and nested roots can be incorrect when
        // projects disagree about namespace packages.
        //
        // We recompute this script's root with its own namespace settings, but can only
        // check whether it is nested under roots from that earlier discovery pass. If, for
        // example, pkg/run.py declares pkg as a namespace package, that script-specific
        // root is not added to the map used to classify pkg/sub/__init__.py. Conversely,
        // roots discovered under another project's settings can affect this script's nested
        // classification. This can produce incorrect package-related diagnostics or fixes.
        let ScriptSettings::Declared(Some(settings)) = &self.settings else {
            return package;
        };

        let namespaces = &settings.linter.namespace_packages;
        if *namespaces == project_settings.namespace_packages {
            return package;
        }

        let root = detect_package_root(path.parent()?, namespaces)?;
        if let Some(package) = package.filter(|package| package.path() == root) {
            return Some(package);
        }

        let nested = path.ends_with("__init__.py")
            && path.parent() == Some(root)
            && package_roots
                .values()
                .flatten()
                .any(|outer| outer.path() != root && root.starts_with(outer.path()));

        Some(if nested {
            PackageRoot::nested(root)
        } else {
            PackageRoot::root(root)
        })
    }
}

/// Settings declared by a script's inline metadata.
#[derive(Debug)]
enum ScriptSettings {
    /// The script declares neither a `requires-python` constraint nor a `[tool.ruff]` table.
    /// Use the project's settings.
    Undeclared,
    /// The script declares `requires-python` or `[tool.ruff]`, even if the table is empty.
    /// The resolved settings are `None` when the declarations are ignored (for
    /// example, with `--isolated`) or do not yield a supported Python target version.
    Declared(Option<Box<Settings>>),
}

/// Whether to look for inline script metadata in this file.
pub fn should_read_script_metadata(preview: PreviewMode, source_type: SourceType) -> bool {
    is_script_metadata_enabled(preview)
        && matches!(source_type, SourceType::Python(PySourceType::Python))
}

/// Resolve a Python file's inline script settings. Returns `None` if script metadata
/// support is disabled or the file has no PEP 723 script metadata header.
pub fn script(
    project: &Project,
    overrides: &dyn ConfigurationTransformer,
    script_configuration: ScriptConfiguration,
    path: Option<&Path>,
    source_type: SourceType,
    source: &str,
) -> Result<Option<Script>, ScriptSettingsError> {
    if !should_read_script_metadata(project.settings.preview, source_type) {
        return Ok(None);
    }

    let Some(metadata) = script_metadata(path, source, script_configuration)? else {
        return Ok(None);
    };

    let requires_python = metadata.requires_python.as_ref();
    let options = metadata.tool.and_then(|tool| tool.ruff);
    let has_ruff_table = options.is_some();
    let declared = requires_python.is_some() || has_ruff_table;

    let config_error = |err| ScriptSettingsError::configuration(path, &err);
    let resolve = |configuration, root: &Path| {
        overrides
            .transform(configuration)
            .into_settings(root)
            .map_err(config_error)
    };

    let mut config_path = None;
    let settings = if script_configuration == ScriptConfiguration::Disabled {
        None
    } else if script_configuration == ScriptConfiguration::Enabled
        && let Some(options) = options
    {
        let root = fs::normalize_path(path.and_then(Path::parent).unwrap_or(Path::new(".")));
        let configuration =
            inline_configuration(options, path, &root, requires_python).map_err(config_error)?;
        config_path = path.map(fs::normalize_path);
        Some(resolve(configuration, &root)?)
    } else if let Some(version) = requires_python.and_then(get_minimum_supported_version) {
        let mut configuration = project.configuration.clone();
        configuration.target_version = Some(version.into());
        // The script's requirement replaces the project's per-file targets too.
        configuration.per_file_target_version = Some(Vec::new());
        Some(resolve(
            configuration,
            &project.settings.file_resolver.project_root,
        )?)
    } else {
        None
    };

    Ok(Some(Script {
        has_ruff_table,
        config_path,
        settings: if declared {
            ScriptSettings::Declared(settings.map(Box::new))
        } else {
            ScriptSettings::Undeclared
        },
    }))
}

/// The inline metadata that can affect Ruff settings.
#[derive(Deserialize)]
struct ScriptMetadata {
    #[serde(rename = "requires-python")]
    requires_python: Option<VersionSpecifiers>,
    tool: Option<Box<Tools>>,
}

/// Read the declarations that affect Ruff, returning `None` if there is no script tag.
fn script_metadata(
    path: Option<&Path>,
    source: &str,
    script_configuration: ScriptConfiguration,
) -> Result<Option<ScriptMetadata>, ScriptSettingsError> {
    let Some(tag) = ScriptTag::parse(source) else {
        return Ok(None);
    };

    let toml_error = |err: toml::de::Error| {
        let range = tag.source_map().map_toml_span(err.span());
        let span = Span::from(source_file(path, source)).with_optional_range(range);
        ScriptSettingsError::new(span, err.message())
    };

    let root = toml::de::DeTable::parse(tag.metadata()).map_err(toml_error)?;
    // As with project configuration, report a version mismatch before an unknown
    // option introduced in a newer Ruff. Only enforce the version if we use these settings.
    if script_configuration == ScriptConfiguration::Enabled {
        check_required_version(root.get_ref(), &["tool", "ruff"])
            .map_err(|err| ScriptSettingsError::configuration(path, &err))?;
    }

    let source = ValueSource::File(Arc::new(SystemPathBuf::from_path_buf_lossy(
        path.unwrap_or_else(|| Path::new("-")).to_owned(),
    )));
    let _guard = ValueSourceGuard::with_source_map(source, tag.source_map().clone());

    ScriptMetadata::deserialize(toml::de::Deserializer::from(root))
        .map(Some)
        .map_err(toml_error)
}

fn inline_configuration(
    options: Options,
    path: Option<&Path>,
    root: &Path,
    requires_python: Option<&VersionSpecifiers>,
) -> Result<Configuration> {
    let mut configuration = Configuration::from_options(options, path, root)?;

    if configuration.target_version.is_none()
        && let Some(version) = requires_python.and_then(get_minimum_supported_version)
    {
        configuration.target_version = Some(version.into());
    }

    if configuration.target_version.is_some() {
        // Only the script's own per-file targets can override its target version.
        configuration.per_file_target_version =
            Some(configuration.per_file_target_version.unwrap_or_default());
    }

    if let Some(extend) = &configuration.extend {
        let base = resolve_configuration(&root.join(extend), ConfigurationOrigin::Unknown)?;
        configuration = configuration.combine(base);
    }

    Ok(configuration)
}

/// A diagnostic for an error encountered while resolving a script's settings.
#[derive(Debug)]
pub struct ScriptSettingsError {
    pub diagnostic: Diagnostic,
}

impl ScriptSettingsError {
    /// Render the diagnostic as an error for commands that cannot return diagnostics.
    pub fn into_anyhow(self) -> anyhow::Error {
        let rendered = self
            .diagnostic
            .display(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff").hide_severity(true),
            )
            .to_string();
        anyhow::Error::msg(rendered.trim_end().to_owned())
    }

    fn new(span: Span, message: impl fmt::Display) -> Self {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidScriptMetadata,
            Severity::Error,
            message,
        );
        let hide_snippet = span.range().is_none();
        let mut annotation = Annotation::primary(span);
        annotation.hide_snippet(hide_snippet);
        diagnostic.annotate(annotation);
        Self { diagnostic }
    }

    fn configuration(path: Option<&Path>, error: &anyhow::Error) -> Self {
        Self::new(Span::from(source_file(path, "")), format_args!("{error:#}"))
    }
}

fn source_file(path: Option<&Path>, source: &str) -> SourceFile {
    let name = path.map_or_else(|| "-".into(), Path::to_string_lossy);
    SourceFileBuilder::new(name, source).finish()
}
