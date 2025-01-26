use crate::metadata::value::{RangedValue, RelativePathBuf, ValueSource, ValueSourceGuard};
use crate::Db;
use red_knot_python_semantic::lint::{GetLintError, Level, LintSource, RuleSelection};
use red_knot_python_semantic::{
    ProgramSettings, PythonPlatform, PythonVersion, SearchPathSettings, SitePackages,
};
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::{System, SystemPath};
use ruff_macros::Combine;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use thiserror::Error;

/// The options for the project.
#[derive(Debug, Default, Clone, PartialEq, Eq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Options {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentOptions>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<SrcOptions>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<Rules>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_on_warning: Option<bool>,
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
                    env.venv_path.clone(),
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
            site_packages: python
                .map(|venv_path| SitePackages::Derived {
                    venv_path: venv_path.absolute(project_root, system),
                })
                .unwrap_or(SitePackages::Known(vec![])),
        }
    }

    #[must_use]
    pub(crate) fn to_rule_selection(&self, db: &dyn Db) -> (RuleSelection, Vec<OptionDiagnostic>) {
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

                    diagnostics.push(diagnostic.with_file(file).with_range(rule_name.range()));
                }
            }
        }

        (selection, diagnostics)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_version: Option<RangedValue<PythonVersion>>,

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

    // TODO: Rename to python, see https://github.com/astral-sh/ruff/issues/15530
    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venv_path: Option<RelativePathBuf>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SrcOptions {
    /// The root of the project, used for finding first-party modules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<RelativePathBuf>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", transparent)]
pub struct Rules {
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
    file: Option<File>,
    range: Option<TextRange>,
}

impl OptionDiagnostic {
    pub fn new(id: DiagnosticId, message: String, severity: Severity) -> Self {
        Self {
            id,
            message,
            severity,
            file: None,
            range: None,
        }
    }

    #[must_use]
    fn with_file(mut self, file: Option<File>) -> Self {
        self.file = file;
        self
    }

    #[must_use]
    fn with_range(mut self, range: Option<TextRange>) -> Self {
        self.range = range;
        self
    }
}

impl Diagnostic for OptionDiagnostic {
    fn id(&self) -> DiagnosticId {
        self.id
    }

    fn message(&self) -> Cow<str> {
        Cow::Borrowed(&self.message)
    }

    fn file(&self) -> Option<File> {
        self.file
    }

    fn range(&self) -> Option<TextRange> {
        self.range
    }

    fn severity(&self) -> Severity {
        self.severity
    }
}
