//! Resolve Ruff settings from inline script metadata (PEP 723).

use std::fmt;
use std::path::Path;
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
use ruff_linter::preview::is_script_metadata_enabled;
use ruff_python_ast::SourceType;
use ruff_python_ast::script::ScriptTag;
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
}

/// Settings resolved for a single Python file from its inline metadata.
#[derive(Debug)]
pub struct Script {
    pub settings: Settings,
}

/// Resolve settings that depend on the contents of a Python script. An inline
/// `[tool.ruff]` is a configuration file in its own right, even when empty.
/// Returns `None` when the caller can reuse the project's settings.
pub fn script(
    project: &Project,
    overrides: &dyn ConfigurationTransformer,
    script_configuration: ScriptConfiguration,
    path: Option<&Path>,
    source_type: SourceType,
    source: &str,
) -> Result<Option<Script>, ScriptSettingsError> {
    if !is_script_metadata_enabled(project.settings.preview, source_type) {
        return Ok(None);
    }
    let Some(metadata) = script_metadata(path, source)? else {
        return Ok(None);
    };
    metadata.resolve(project, overrides, script_configuration, path)
}

/// The inline metadata that can affect Ruff settings.
#[derive(Deserialize)]
struct ScriptMetadata {
    #[serde(rename = "requires-python")]
    requires_python: Option<VersionSpecifiers>,
    tool: Option<Box<Tools>>,
}

impl ScriptMetadata {
    /// Resolve settings from parsed metadata, reusing the project settings when possible.
    fn resolve(
        self,
        project: &Project,
        overrides: &dyn ConfigurationTransformer,
        script_configuration: ScriptConfiguration,
        path: Option<&Path>,
    ) -> Result<Option<Script>, ScriptSettingsError> {
        if script_configuration == ScriptConfiguration::Disabled {
            return Ok(None);
        }
        let config_error = |err| ScriptSettingsError::configuration(path, &err);
        let requires_python = self.requires_python.as_ref();
        let resolve = |configuration, root: &Path| {
            overrides
                .transform(configuration)
                .into_settings(root)
                .map_err(config_error)
        };
        if let Some(options) = self.tool.and_then(|tool| tool.ruff) {
            let root = fs::normalize_path(path.and_then(Path::parent).unwrap_or(Path::new(".")));
            let configuration = inline_configuration(options, path, &root, requires_python)
                .map_err(config_error)?;
            let mut settings = resolve(configuration, &root)?;
            // Package roots are discovered before inline metadata is read.
            settings
                .linter
                .namespace_packages
                .clone_from(&project.settings.linter.namespace_packages);
            return Ok(Some(Script { settings }));
        }

        let Some(version) = requires_python.and_then(get_minimum_supported_version) else {
            return Ok(None);
        };
        let mut configuration = project.configuration.clone();
        configuration.target_version = Some(version.into());
        // The script's requirement replaces the project's per-file targets too.
        configuration.per_file_target_version = Some(Vec::new());
        Ok(Some(Script {
            settings: resolve(configuration, &project.settings.file_resolver.project_root)?,
        }))
    }
}

/// Read the declarations that affect Ruff, returning `None` if there is no script tag.
fn script_metadata(
    path: Option<&Path>,
    source: &str,
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
    check_required_version(root.get_ref(), &["tool", "ruff"])
        .map_err(|err| ScriptSettingsError::configuration(path, &err))?;

    let source = path.map_or(ValueSource::Cli, |path| {
        ValueSource::File(Arc::new(SystemPathBuf::from_path_buf_lossy(
            path.to_owned(),
        )))
    });
    let _guard = ValueSourceGuard::with_source_map(source, tag.source_map().clone());
    ScriptMetadata::deserialize(toml::de::Deserializer::from(root))
        .map(Some)
        .map_err(toml_error)
}

fn inline_configuration(
    mut options: Options,
    path: Option<&Path>,
    root: &Path,
    requires_python: Option<&VersionSpecifiers>,
) -> Result<Configuration> {
    options.namespace_packages = None;
    let mut configuration = Configuration::from_options(options, path, root)?;
    if configuration.target_version.is_none()
        && let Some(version) = requires_python.and_then(get_minimum_supported_version)
    {
        configuration.target_version = Some(version.into());
        // Only the script's own per-file targets can override its requirement.
        configuration
            .per_file_target_version
            .get_or_insert_default();
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

impl fmt::Display for ScriptSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = self
            .diagnostic
            .display(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff").hide_severity(true),
            )
            .to_string();
        f.write_str(rendered.trim_end())
    }
}

impl std::error::Error for ScriptSettingsError {}

fn source_file(path: Option<&Path>, source: &str) -> SourceFile {
    let name = path.map_or_else(|| "-".into(), Path::to_string_lossy);
    SourceFileBuilder::new(name, source).finish()
}
