use crate::Db;
use crate::metadata::value::{RangedValue, RelativePathBuf, ValueSource, ValueSourceGuard};
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, Severity, Span};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath};
use ruff_macros::{Combine, OptionsMetadata};
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;
use ty_python_semantic::lint::{GetLintError, Level, LintSource, RuleSelection};
use ty_python_semantic::{
    ProgramSettings, PythonPath, PythonPlatform, PythonVersionSource, PythonVersionWithSource,
    SearchPathSettings,
};

use super::settings::{Settings, TerminalSettings};

#[derive(
    Debug, Default, Clone, PartialEq, Eq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    /// Configures the type checking environment.
    #[option_group]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentOptions>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[option_group]
    pub src: Option<SrcOptions>,

    /// Configures the enabled rules and their severity.
    ///
    /// See [the rules documentation](https://ty.dev/rules) for a list of all available rules.
    ///
    /// Valid severities are:
    ///
    /// * `ignore`: Disable the rule.
    /// * `warn`: Enable the rule and create a warning diagnostic.
    /// * `error`: Enable the rule and create an error diagnostic.
    ///   ty will exit with a non-zero code if any error diagnostics are emitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"{...}"#,
        value_type = r#"dict[RuleName, "ignore" | "warn" | "error"]"#,
        example = r#"
            [tool.ty.rules]
            possibly-unresolved-reference = "warn"
            division-by-zero = "ignore"
        "#
    )]
    pub rules: Option<Rules>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[option_group]
    pub terminal: Option<TerminalOptions>,

    /// Whether to automatically exclude files that are ignored by `.ignore`,
    /// `.gitignore`, `.git/info/exclude`, and global `gitignore` files.
    /// Enabled by default.
    #[option(
        default = r#"true"#,
        value_type = r#"bool"#,
        example = r#"
            respect-ignore-files = false
        "#
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_ignore_files: Option<bool>,
}

impl Options {
    pub fn from_toml_str(content: &str, source: ValueSource) -> Result<Self, TyTomlError> {
        let _guard = ValueSourceGuard::new(source, true);
        let options = toml::from_str(content)?;
        Ok(options)
    }

    pub fn deserialize_with<'de, D>(source: ValueSource, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let _guard = ValueSourceGuard::new(source, false);
        Self::deserialize(deserializer)
    }

    pub(crate) fn to_program_settings(
        &self,
        project_root: &SystemPath,
        project_name: &str,
        system: &dyn System,
    ) -> ProgramSettings {
        let python_version = self
            .environment
            .as_ref()
            .and_then(|env| env.python_version.as_ref())
            .map(|ranged_version| PythonVersionWithSource {
                version: **ranged_version,
                source: match ranged_version.source() {
                    ValueSource::Cli => PythonVersionSource::Cli,
                    ValueSource::File(path) => {
                        PythonVersionSource::File(path.clone(), ranged_version.range())
                    }
                },
            })
            .unwrap_or_default();
        let python_platform = self
            .environment
            .as_ref()
            .and_then(|env| env.python_platform.as_deref().cloned())
            .unwrap_or_else(|| {
                let default = PythonPlatform::default();
                tracing::info!("Defaulting to python-platform `{default}`");
                default
            });
        ProgramSettings {
            python_version,
            python_platform,
            search_paths: self.to_search_path_settings(project_root, project_name, system),
        }
    }

    fn to_search_path_settings(
        &self,
        project_root: &SystemPath,
        project_name: &str,
        system: &dyn System,
    ) -> SearchPathSettings {
        let src_roots = if let Some(src_root) = self.src.as_ref().and_then(|src| src.root.as_ref())
        {
            vec![src_root.absolute(project_root, system)]
        } else {
            let src = project_root.join("src");

            if system.is_directory(&src) {
                // Default to `src` and the project root if `src` exists and the root hasn't been specified.
                // This corresponds to the `src-layout`
                tracing::debug!(
                    "Including `./src` in `src.root` because a `./src` directory exists"
                );
                vec![project_root.to_path_buf(), src]
            } else if system.is_directory(&project_root.join(project_name).join(project_name)) {
                // `src-layout` but when the folder isn't called `src` but has the same name as the project.
                // For example, the "src" folder for `psycopg` is called `psycopg` and the python files are in `psycopg/psycopg/_adapters_map.py`
                tracing::debug!(
                    "Including `./{project_name}` in `src.root` because a `./{project_name}/{project_name}` directory exists"
                );

                vec![project_root.to_path_buf(), project_root.join(project_name)]
            } else {
                // Default to a [flat project structure](https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/).
                tracing::debug!("Defaulting `src.root` to `.`");
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
                    PythonPath::from_cli_flag(python_path.absolute(project_root, system))
                })
                .or_else(|| {
                    std::env::var("VIRTUAL_ENV")
                        .ok()
                        .map(PythonPath::from_virtual_env_var)
                })
                .or_else(|| {
                    std::env::var("CONDA_PREFIX")
                        .ok()
                        .map(PythonPath::from_conda_prefix_var)
                })
                .unwrap_or_else(|| PythonPath::Discover(project_root.to_path_buf())),
        }
    }

    #[must_use]
    pub(crate) fn to_settings(&self, db: &dyn Db) -> (Settings, Vec<OptionDiagnostic>) {
        let (rules, diagnostics) = self.to_rule_selection(db);

        let mut settings = Settings::new(rules, self.respect_ignore_files);

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

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EnvironmentOptions {
    /// Specifies the version of Python that will be used to analyze the source code.
    /// The version should be specified as a string in the format `M.m` where `M` is the major version
    /// and `m` is the minor (e.g. `"3.0"` or `"3.6"`).
    /// If a version is provided, ty will generate errors if the source code makes use of language features
    /// that are not supported in that version.
    /// It will also understand conditionals based on comparisons with `sys.version_info`, such
    /// as are commonly found in typeshed to reflect the differing contents of the standard
    /// library across Python versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#""3.13""#,
        value_type = r#""3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | <major>.<minor>"#,
        example = r#"
            python-version = "3.12"
        "#
    )]
    pub python_version: Option<RangedValue<PythonVersion>>,

    /// Specifies the target platform that will be used to analyze the source code.
    /// If specified, ty will understand conditions based on comparisons with `sys.platform`, such
    /// as are commonly found in typeshed to reflect the differing contents of the standard library across platforms.
    ///
    /// If no platform is specified, ty will use the current platform:
    /// - `win32` for Windows
    /// - `darwin` for macOS
    /// - `android` for Android
    /// - `ios` for iOS
    /// - `linux` for everything else
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"<current-platform>"#,
        value_type = r#""win32" | "darwin" | "android" | "ios" | "linux" | str"#,
        example = r#"
        # Tailor type stubs and conditionalized type definitions to windows.
        python-platform = "win32"
        "#
    )]
    pub python_platform: Option<RangedValue<PythonPlatform>>,

    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's `MYPYPATH` environment variable,
    /// or pyright's `stubPath` configuration setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            extra-paths = ["~/shared/my-search-path"]
        "#
    )]
    pub extra_paths: Option<Vec<RelativePathBuf>>,

    /// Optional path to a "typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            typeshed = "/path/to/custom/typeshed"
        "#
    )]
    pub typeshed: Option<RelativePathBuf>,

    /// Path to the Python installation from which ty resolves type information and third-party dependencies.
    ///
    /// ty will search in the path's `site-packages` directories for type information and
    /// third-party imports.
    ///
    /// This option is commonly used to specify the path to a virtual environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            python = "./.venv"
        "#
    )]
    pub python: Option<RelativePathBuf>,
}

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SrcOptions {
    /// The root of the project, used for finding first-party modules.
    ///
    /// If left unspecified, ty will try to detect common project layouts and initialize `src.root` accordingly:
    ///
    /// * if a `./src` directory exists, include `.` and `./src` in the first party search path (src layout or flat)
    /// * if a `./<project-name>/<project-name>` directory exists, include `.` and `./<project-name>` in the first party search path
    /// * otherwise, default to `.` (flat layout)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            root = "./app"
        "#
    )]
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

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TerminalOptions {
    /// The format to use for printing diagnostic messages.
    ///
    /// Defaults to `full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"full"#,
        value_type = "full | concise",
        example = r#"
            output-format = "concise"
        "#
    )]
    pub output_format: Option<RangedValue<DiagnosticFormat>>,
    /// Use exit code 1 if there are any warning-level diagnostics.
    ///
    /// Defaults to `false`.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
        # Error if ty emits any warning-level diagnostics.
        error-on-warning = true
        "#
    )]
    pub error_on_warning: Option<bool>,
}

#[cfg(feature = "schemars")]
mod schema {
    use crate::DEFAULT_LINT_REGISTRY;
    use schemars::JsonSchema;
    use schemars::r#gen::SchemaGenerator;
    use schemars::schema::{
        InstanceType, Metadata, ObjectValidation, Schema, SchemaObject, SubschemaValidation,
    };
    use ty_python_semantic::lint::Level;

    pub(super) struct Rules;

    impl JsonSchema for Rules {
        fn schema_name() -> String {
            "Rules".to_string()
        }

        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            let registry = &*DEFAULT_LINT_REGISTRY;

            let level_schema = generator.subschema_for::<Level>();

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
                    // Allow unknown rules: ty will warn about them.
                    // It gives a better experience when using an older ty version because
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
pub enum TyTomlError {
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

    pub(crate) fn to_diagnostic(&self) -> Diagnostic {
        if let Some(ref span) = self.span {
            let mut diag = Diagnostic::new(self.id, self.severity, "");
            diag.annotate(Annotation::primary(span.clone()).message(&self.message));
            diag
        } else {
            Diagnostic::new(self.id, self.severity, &self.message)
        }
    }
}
