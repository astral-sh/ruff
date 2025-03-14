use crate::metadata::value::{RangedValue, RelativePathBuf, ValueSource, ValueSourceGuard};
use crate::Db;
use red_knot_python_semantic::lint::{GetLintError, Level, LintSource, RuleSelection};
use red_knot_python_semantic::{ProgramSettings, PythonPath, PythonPlatform, SearchPathSettings};
use ruff_db::diagnostic::{DiagnosticFormat, DiagnosticId, OldDiagnosticTrait, Severity, Span};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath};
use ruff_macros::Combine;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use thiserror::Error;

use super::settings::{Settings, TerminalSettings};

/// The options for the project.
#[derive(Debug, Default, Clone, PartialEq, Eq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    /// Configures the type checking environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentOptions>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<SrcOptions>,

    /// Configures the enabled lints and their severity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<Rules>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalOptions>,
}

impl Options {
    pub(crate) fn from_toml_str(content: &str, source: ValueSource) -> Result<Self, KnotTomlError> {
        let _guard = ValueSourceGuard::new(source);
        let options = toml::from_str(content)?;
        Ok(options)
    }

    pub(crate) fn to_program_settings(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
    ) -> ProgramSettings {
        let (python_version, python_platform) = self
            .environment
            .as_ref()
            .map(|env| {
                (
                    env.python_version.as_deref().copied(),
                    env.python_platform.as_deref(),
                )
            })
            .unwrap_or_default();

        ProgramSettings {
            python_version: python_version.unwrap_or_default(),
            python_platform: python_platform.cloned().unwrap_or_default(),
            search_paths: self.to_search_path_settings(project_root, system),
        }
    }

    fn to_search_path_settings(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
    ) -> SearchPathSettings {
        let src_roots = if let Some(src_root) = self.src.as_ref().and_then(|src| src.root.as_ref())
        {
            vec![src_root.absolute(project_root, system)]
        } else {
            let src = project_root.join("src");

            // Default to `src` and the project root if `src` exists and the root hasn't been specified.
            if system.is_directory(&src) {
                vec![project_root.to_path_buf(), src]
            } else {
                vec![project_root.to_path_buf()]
            }
        };

        let (extra_paths, python, typeshed) = self
            .environment
            .as_ref()
            .map(|env| {
                (
                    env.extra_paths.clone(),
                    env.python.clone(),
                    env.typeshed.clone(),
                )
            })
            .unwrap_or_default();

        SearchPathSettings {
            extra_paths: extra_paths
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.absolute(project_root, system))
                .collect(),
            src_roots,
            custom_typeshed: typeshed.map(|path| path.absolute(project_root, system)),
            python_path: python
                .map(|python_path| {
                    PythonPath::SysPrefix(python_path.absolute(project_root, system))
                })
                .unwrap_or(PythonPath::KnownSitePackages(vec![])),
        }
    }

    #[must_use]
    pub(crate) fn to_settings(&self, db: &dyn Db) -> (Settings, Vec<OptionDiagnostic>) {
        let (rules, diagnostics) = self.to_rule_selection(db);

        let mut settings = Settings::new(rules);

        if let Some(terminal) = self.terminal.as_ref() {
            settings.set_terminal(TerminalSettings {
                output_format: terminal
                    .output_format
                    .as_deref()
                    .copied()
                    .unwrap_or_default(),
                error_on_warning: terminal.error_on_warning.unwrap_or_default(),
            });
        }

        (settings, diagnostics)
    }

    #[must_use]
    fn to_rule_selection(&self, db: &dyn Db) -> (RuleSelection, Vec<OptionDiagnostic>) {
        let registry = db.lint_registry();
        let mut diagnostics = Vec::new();

        // Initialize the selection with the defaults
        let mut selection = RuleSelection::from_registry(registry);

        let rules = self
            .rules
            .as_ref()
            .into_iter()
            .flat_map(|rules| rules.inner.iter());

        for (rule_name, level) in rules {
            let source = rule_name.source();
            match registry.get(rule_name) {
                Ok(lint) => {
                    let lint_source = match source {
                        ValueSource::File(_) => LintSource::File,
                        ValueSource::Cli => LintSource::Cli,
                    };
                    if let Ok(severity) = Severity::try_from(**level) {
                        selection.enable(lint, severity, lint_source);
                    } else {
                        selection.disable(lint);
                    }
                }
                Err(error) => {
                    // `system_path_to_file` can return `Err` if the file was deleted since the configuration
                    // was read. This should be rare and it should be okay to default to not showing a configuration
                    // file in that case.
                    let file = source
                        .file()
                        .and_then(|path| system_path_to_file(db.upcast(), path).ok());

                    // TODO: Add a note if the value was configured on the CLI
                    let diagnostic = match error {
                        GetLintError::Unknown(_) => OptionDiagnostic::new(
                            DiagnosticId::UnknownRule,
                            format!("Unknown lint rule `{rule_name}`"),
                            Severity::Warning,
                        ),
                        GetLintError::PrefixedWithCategory { suggestion, .. } => {
                            OptionDiagnostic::new(
                                DiagnosticId::UnknownRule,
                                format!(
                                    "Unknown lint rule `{rule_name}`. Did you mean `{suggestion}`?"
                                ),
                                Severity::Warning,
                            )
                        }

                        GetLintError::Removed(_) => OptionDiagnostic::new(
                            DiagnosticId::UnknownRule,
                            format!("Unknown lint rule `{rule_name}`"),
                            Severity::Warning,
                        ),
                    };

                    let span = file.map(Span::from).map(|span| {
                        if let Some(range) = rule_name.range() {
                            span.with_range(range)
                        } else {
                            span
                        }
                    });
                    diagnostics.push(diagnostic.with_span(span));
                }
            }
        }

        (selection, diagnostics)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EnvironmentOptions {
    /// Specifies the version of Python that will be used to execute the source code.
    /// The version should be specified as a string in the format `M.m` where `M` is the major version
    /// and `m` is the minor (e.g. "3.0" or "3.6").
    /// If a version is provided, knot will generate errors if the source code makes use of language features
    /// that are not supported in that version.
    /// It will also tailor its use of type stub files, which conditionalizes type definitions based on the version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_version: Option<RangedValue<PythonVersion>>,

    /// Specifies the target platform that will be used to execute the source code.
    /// If specified, Red Knot will tailor its use of type stub files,
    /// which conditionalize type definitions based on the platform.
    ///
    /// If no platform is specified, knot will use `all` or the current platform in the LSP use case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_platform: Option<RangedValue<PythonPlatform>>,

    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_paths: Option<Vec<RelativePathBuf>>,

    /// Optional path to a "typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typeshed: Option<RelativePathBuf>,

    /// Path to the Python installation from which Red Knot resolves type information and third-party dependencies.
    ///
    /// Red Knot will search in the path's `site-packages` directories for type information and
    /// third-party imports.
    ///
    /// This option is commonly used to specify the path to a virtual environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python: Option<RelativePathBuf>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SrcOptions {
    /// The root of the project, used for finding first-party modules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<RelativePathBuf>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Rules {
    #[cfg_attr(feature = "schemars", schemars(with = "schema::Rules"))]
    inner: FxHashMap<RangedValue<String>, RangedValue<Level>>,
}

impl FromIterator<(RangedValue<String>, RangedValue<Level>)> for Rules {
    fn from_iter<T: IntoIterator<Item = (RangedValue<String>, RangedValue<Level>)>>(
        iter: T,
    ) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TerminalOptions {
    /// The format to use for printing diagnostic messages.
    ///
    /// Defaults to `full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<RangedValue<DiagnosticFormat>>,
    /// Use exit code 1 if there are any warning-level diagnostics.
    ///
    /// Defaults to `false`.
    pub error_on_warning: Option<bool>,
}

#[cfg(feature = "schemars")]
mod schema {
    use crate::DEFAULT_LINT_REGISTRY;
    use red_knot_python_semantic::lint::Level;
    use schemars::gen::SchemaGenerator;
    use schemars::schema::{
        InstanceType, Metadata, ObjectValidation, Schema, SchemaObject, SubschemaValidation,
    };
    use schemars::JsonSchema;

    pub(super) struct Rules;

    impl JsonSchema for Rules {
        fn schema_name() -> String {
            "Rules".to_string()
        }

        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            let registry = &*DEFAULT_LINT_REGISTRY;

            let level_schema = gen.subschema_for::<Level>();

            let properties: schemars::Map<String, Schema> = registry
                .lints()
                .iter()
                .map(|lint| {
                    (
                        lint.name().to_string(),
                        Schema::Object(SchemaObject {
                            metadata: Some(Box::new(Metadata {
                                title: Some(lint.summary().to_string()),
                                description: Some(lint.documentation()),
                                deprecated: lint.status.is_deprecated(),
                                default: Some(lint.default_level.to_string().into()),
                                ..Metadata::default()
                            })),
                            subschemas: Some(Box::new(SubschemaValidation {
                                one_of: Some(vec![level_schema.clone()]),
                                ..Default::default()
                            })),
                            ..Default::default()
                        }),
                    )
                })
                .collect();

            Schema::Object(SchemaObject {
                instance_type: Some(InstanceType::Object.into()),
                object: Some(Box::new(ObjectValidation {
                    properties,
                    // Allow unknown rules: Red Knot will warn about them.
                    // It gives a better experience when using an older Red Knot version because
                    // the schema will not deny rules that have been removed in newer versions.
                    additional_properties: Some(Box::new(level_schema)),
                    ..ObjectValidation::default()
                })),

                ..Default::default()
            })
        }
    }
}

#[derive(Error, Debug)]
pub enum KnotTomlError {
    #[error(transparent)]
    TomlSyntax(#[from] toml::de::Error),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OptionDiagnostic {
    id: DiagnosticId,
    message: String,
    severity: Severity,
    span: Option<Span>,
}

impl OptionDiagnostic {
    pub fn new(id: DiagnosticId, message: String, severity: Severity) -> Self {
        Self {
            id,
            message,
            severity,
            span: None,
        }
    }

    #[must_use]
    fn with_span(self, span: Option<Span>) -> Self {
        OptionDiagnostic { span, ..self }
    }
}

impl OldDiagnosticTrait for OptionDiagnostic {
    fn id(&self) -> DiagnosticId {
        self.id
    }

    fn message(&self) -> Cow<str> {
        Cow::Borrowed(&self.message)
    }

    fn span(&self) -> Option<Span> {
        self.span.clone()
    }

    fn severity(&self) -> Severity {
        self.severity
    }
}
