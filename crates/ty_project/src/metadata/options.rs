use crate::Db;
use crate::combine::Combine;
use crate::glob::{ExcludeFilter, IncludeExcludeFilter, IncludeFilter, PortableGlobKind};
use crate::metadata::settings::{OverrideSettings, SrcSettings};
use crate::metadata::value::{
    RangedValue, RelativeGlobPattern, RelativePathBuf, ValueSource, ValueSourceGuard,
};

use ordermap::OrderMap;
use ruff_db::RustDoc;
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, Severity,
    Span, SubDiagnostic,
};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_macros::{Combine, OptionsMetadata, RustDoc};
use ruff_options_metadata::{OptionSet, OptionsMetadata, Visit};
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{self, Debug, Display};
use std::hash::BuildHasherDefault;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use ty_python_semantic::lint::{GetLintError, Level, LintSource, RuleSelection};
use ty_python_semantic::{
    ProgramSettings, PythonPath, PythonPlatform, PythonVersionFileSource, PythonVersionSource,
    PythonVersionWithSource, SearchPathSettings, SysPrefixPathOrigin,
};

use super::settings::{Override, Settings, TerminalSettings};

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

    /// Override configurations for specific file patterns.
    ///
    /// Each override specifies include/exclude patterns and rule configurations
    /// that apply to matching files. Multiple overrides can match the same file,
    /// with later overrides taking precedence.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option_group]
    pub overrides: Option<OverridesOptions>,
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
        let environment = self.environment.or_default();

        let python_version =
            environment
                .python_version
                .as_ref()
                .map(|ranged_version| PythonVersionWithSource {
                    version: **ranged_version,
                    source: match ranged_version.source() {
                        ValueSource::Cli => PythonVersionSource::Cli,
                        ValueSource::File(path) => PythonVersionSource::ConfigFile(
                            PythonVersionFileSource::new(path.clone(), ranged_version.range()),
                        ),
                    },
                });
        let python_platform = environment
            .python_platform
            .as_deref()
            .cloned()
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
        let environment = self.environment.or_default();
        let src = self.src.or_default();

        #[allow(deprecated)]
        let src_roots = if let Some(roots) = environment
            .root
            .as_deref()
            .or_else(|| Some(std::slice::from_ref(src.root.as_ref()?)))
        {
            roots
                .iter()
                .map(|root| root.absolute(project_root, system))
                .collect()
        } else {
            let src = project_root.join("src");

            let mut roots = if system.is_directory(&src) {
                // Default to `src` and the project root if `src` exists and the root hasn't been specified.
                // This corresponds to the `src-layout`
                tracing::debug!(
                    "Including `.` and `./src` in `environment.root` because a `./src` directory exists"
                );
                vec![project_root.to_path_buf(), src]
            } else if system.is_directory(&project_root.join(project_name).join(project_name)) {
                // `src-layout` but when the folder isn't called `src` but has the same name as the project.
                // For example, the "src" folder for `psycopg` is called `psycopg` and the python files are in `psycopg/psycopg/_adapters_map.py`
                tracing::debug!(
                    "Including `.` and `/{project_name}` in `environment.root` because a `./{project_name}/{project_name}` directory exists"
                );

                vec![project_root.to_path_buf(), project_root.join(project_name)]
            } else {
                // Default to a [flat project structure](https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/).
                tracing::debug!("Including `.` in `environment.root`");
                vec![project_root.to_path_buf()]
            };

            // Considering pytest test discovery conventions,
            // we also include the `tests` directory if it exists and is not a package.
            let tests_dir = project_root.join("tests");
            if system.is_directory(&tests_dir)
                && !system.is_file(&tests_dir.join("__init__.py"))
                && !roots.contains(&tests_dir)
            {
                // If the `tests` directory exists and is not a package, include it as a source root.
                tracing::debug!(
                    "Including `./tests` in `environment.root` because a `./tests` directory exists"
                );

                roots.push(tests_dir);
            }

            roots
        };

        SearchPathSettings {
            extra_paths: environment
                .extra_paths
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|path| path.absolute(project_root, system))
                .collect(),
            src_roots,
            custom_typeshed: environment
                .typeshed
                .as_ref()
                .map(|path| path.absolute(project_root, system)),
            python_path: environment
                .python
                .as_ref()
                .map(|python_path| {
                    let origin = match python_path.source() {
                        ValueSource::Cli => SysPrefixPathOrigin::PythonCliFlag,
                        ValueSource::File(path) => SysPrefixPathOrigin::ConfigFileSetting(
                            path.clone(),
                            python_path.range(),
                        ),
                    };
                    PythonPath::sys_prefix(python_path.absolute(project_root, system), origin)
                })
                .or_else(|| {
                    system.env_var("VIRTUAL_ENV").ok().map(|virtual_env| {
                        PythonPath::sys_prefix(virtual_env, SysPrefixPathOrigin::VirtualEnvVar)
                    })
                })
                .or_else(|| {
                    system.env_var("CONDA_PREFIX").ok().map(|path| {
                        PythonPath::sys_prefix(path, SysPrefixPathOrigin::CondaPrefixVar)
                    })
                })
                .unwrap_or_else(|| {
                    PythonPath::sys_prefix(
                        project_root.to_path_buf(),
                        SysPrefixPathOrigin::LocalVenv,
                    )
                }),
        }
    }

    pub(crate) fn to_settings(
        &self,
        db: &dyn Db,
        project_root: &SystemPath,
    ) -> Result<(Settings, Vec<OptionDiagnostic>), ToSettingsError> {
        let mut diagnostics = Vec::new();
        let rules = self.to_rule_selection(db, &mut diagnostics);

        let terminal_options = self.terminal.or_default();
        let terminal = TerminalSettings {
            output_format: terminal_options
                .output_format
                .as_deref()
                .copied()
                .unwrap_or_default(),
            error_on_warning: terminal_options.error_on_warning.unwrap_or_default(),
        };

        let src_options = self.src.or_default();

        #[allow(deprecated)]
        if let Some(src_root) = src_options.root.as_ref() {
            let mut diagnostic = OptionDiagnostic::new(
                DiagnosticId::DeprecatedSetting,
                "The `src.root` setting is deprecated. Use `environment.root` instead.".to_string(),
                Severity::Warning,
            );

            if let Some(file) = src_root
                .source()
                .file()
                .and_then(|path| system_path_to_file(db.upcast(), path).ok())
            {
                diagnostic = diagnostic.with_annotation(Some(Annotation::primary(
                    Span::from(file).with_optional_range(src_root.range()),
                )));
            }

            if self.environment.or_default().root.is_some() {
                diagnostic = diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "The `src.root` setting was ignored in favor of the `environment.root` setting",
                ));
            }

            diagnostics.push(diagnostic);
        }

        let src = src_options
            .to_settings(db, project_root, &mut diagnostics)
            .map_err(|err| ToSettingsError {
                diagnostic: err,
                output_format: terminal.output_format,
                color: colored::control::SHOULD_COLORIZE.should_colorize(),
            })?;

        let overrides = self
            .to_overrides_settings(db, project_root, &mut diagnostics)
            .map_err(|err| ToSettingsError {
                diagnostic: err,
                output_format: terminal.output_format,
                color: colored::control::SHOULD_COLORIZE.should_colorize(),
            })?;

        let settings = Settings {
            rules: Arc::new(rules),
            terminal,
            src,
            overrides,
        };

        Ok((settings, diagnostics))
    }

    #[must_use]
    fn to_rule_selection(
        &self,
        db: &dyn Db,
        diagnostics: &mut Vec<OptionDiagnostic>,
    ) -> RuleSelection {
        self.rules.or_default().to_rule_selection(db, diagnostics)
    }

    fn to_overrides_settings(
        &self,
        db: &dyn Db,
        project_root: &SystemPath,
        diagnostics: &mut Vec<OptionDiagnostic>,
    ) -> Result<Vec<Override>, Box<OptionDiagnostic>> {
        let override_options = &**self.overrides.or_default();

        let mut overrides = Vec::with_capacity(override_options.len());

        for override_option in override_options {
            let override_instance =
                override_option.to_override(db, project_root, self.rules.as_ref(), diagnostics)?;

            if let Some(value) = override_instance {
                overrides.push(value);
            }
        }

        Ok(overrides)
    }
}

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EnvironmentOptions {
    /// The root paths of the project, used for finding first-party modules.
    ///
    /// Accepts a list of directory paths searched in priority order (first has highest priority).
    ///
    /// If left unspecified, ty will try to detect common project layouts and initialize `root` accordingly:
    ///
    /// * if a `./src` directory exists, include `.` and `./src` in the first party search path (src layout or flat)
    /// * if a `./<project-name>/<project-name>` directory exists, include `.` and `./<project-name>` in the first party search path
    /// * otherwise, default to `.` (flat layout)
    ///
    /// Besides, if a `./tests` directory exists and is not a package (i.e. it does not contain an `__init__.py` file),
    /// it will also be included in the first party search path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "list[str]",
        example = r#"
            # Multiple directories (priority order)
            root = ["./src", "./lib", "./vendor"]
        "#
    )]
    pub root: Option<Vec<RelativePathBuf>>,

    /// Specifies the version of Python that will be used to analyze the source code.
    /// The version should be specified as a string in the format `M.m` where `M` is the major version
    /// and `m` is the minor (e.g. `"3.0"` or `"3.6"`).
    /// If a version is provided, ty will generate errors if the source code makes use of language features
    /// that are not supported in that version.
    ///
    /// If a version is not specified, ty will try the following techniques in order of preference
    /// to determine a value:
    /// 1. Check for the `project.requires-python` setting in a `pyproject.toml` file
    ///    and use the minimum version from the specified range
    /// 2. Check for an activated or configured Python environment
    ///    and attempt to infer the Python version of that environment
    /// 3. Fall back to the default value (see below)
    ///
    /// For some language features, ty can also understand conditionals based on comparisons
    /// with `sys.version_info`. These are commonly found in typeshed, for example,
    /// to reflect the differing contents of the standard library across Python versions.
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
    /// If `all` is specified, ty will assume that the source code can run on any platform.
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
        value_type = r#""win32" | "darwin" | "android" | "ios" | "linux" | "all" | str"#,
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
    ///
    /// Besides, if a `./tests` directory exists and is not a package (i.e. it does not contain an `__init__.py` file),
    /// it will also be included in the first party search path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            root = "./app"
        "#
    )]
    #[deprecated(note = "Use `environment.root` instead.")]
    pub root: Option<RelativePathBuf>,

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

    /// A list of files and directories to check. The `include` option
    /// follows a similar syntax to `.gitignore` but reversed:
    /// Including a file or directory will make it so that it (and its contents)
    /// are type checked.
    ///
    /// - `./src/` matches only a directory
    /// - `./src` matches both files and directories
    /// - `src` matches a file or directory named `src`
    /// - `*` matches any (possibly empty) sequence of characters (except `/`).
    /// - `**` matches zero or more path components.
    ///   This sequence **must** form a single path component, so both `**a` and `b**` are invalid and will result in an error.
    ///   A sequence of more than two consecutive `*` characters is also invalid.
    /// - `?` matches any single character except `/`
    /// - `[abc]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as ordered by Unicode,
    ///   so e.g. `[0-9]` specifies any character between `0` and `9` inclusive. An unclosed bracket is invalid.
    ///
    /// All paths are anchored relative to the project root (`src` only
    /// matches `<project_root>/src` and not `<project_root>/test/src`).
    ///
    /// `exclude` takes precedence over `include`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = r#"list[str]"#,
        example = r#"
            include = [
                "src",
                "tests",
            ]
        "#
    )]
    pub include: Option<RangedValue<Vec<RelativeGlobPattern>>>,

    /// A list of file and directory patterns to exclude from type checking.
    ///
    /// Patterns follow a syntax similar to `.gitignore`:
    /// - `./src/` matches only a directory
    /// - `./src` matches both files and directories
    /// - `src` matches files or directories named `src`
    /// - `*` matches any (possibly empty) sequence of characters (except `/`).
    /// - `**` matches zero or more path components.
    ///   This sequence **must** form a single path component, so both `**a` and `b**` are invalid and will result in an error.
    ///   A sequence of more than two consecutive `*` characters is also invalid.
    /// - `?` matches any single character except `/`
    /// - `[abc]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as ordered by Unicode,
    ///   so e.g. `[0-9]` specifies any character between `0` and `9` inclusive. An unclosed bracket is invalid.
    /// - `!pattern` negates a pattern (undoes the exclusion of files that would otherwise be excluded)
    ///
    /// All paths are anchored relative to the project root (`src` only
    /// matches `<project_root>/src` and not `<project_root>/test/src`).
    /// To exclude any directory or file named `src`, use `**/src` instead.
    ///
    /// By default, ty excludes commonly ignored directories:
    ///
    /// - `**/.bzr/`
    /// - `**/.direnv/`
    /// - `**/.eggs/`
    /// - `**/.git/`
    /// - `**/.git-rewrite/`
    /// - `**/.hg/`
    /// - `**/.mypy_cache/`
    /// - `**/.nox/`
    /// - `**/.pants.d/`
    /// - `**/.pytype/`
    /// - `**/.ruff_cache/`
    /// - `**/.svn/`
    /// - `**/.tox/`
    /// - `**/.venv/`
    /// - `**/__pypackages__/`
    /// - `**/_build/`
    /// - `**/buck-out/`
    /// - `**/dist/`
    /// - `**/node_modules/`
    /// - `**/venv/`
    ///
    /// You can override any default exclude by using a negated pattern. For example,
    /// to re-include `dist` use `exclude = ["!dist"]`
    #[option(
        default = r#"null"#,
        value_type = r#"list[str]"#,
        example = r#"
            exclude = [
                "generated",
                "*.proto",
                "tests/fixtures/**",
                "!tests/fixtures/important.py"  # Include this one file
            ]
        "#
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<RangedValue<Vec<RelativeGlobPattern>>>,
}

impl SrcOptions {
    fn to_settings(
        &self,
        db: &dyn Db,
        project_root: &SystemPath,
        diagnostics: &mut Vec<OptionDiagnostic>,
    ) -> Result<SrcSettings, Box<OptionDiagnostic>> {
        let include = build_include_filter(
            db,
            project_root,
            self.include.as_ref(),
            GlobFilterContext::SrcRoot,
            diagnostics,
        )?;
        let exclude = build_exclude_filter(
            db,
            project_root,
            self.exclude.as_ref(),
            DEFAULT_SRC_EXCLUDES,
            GlobFilterContext::SrcRoot,
        )?;
        let files = IncludeExcludeFilter::new(include, exclude);

        Ok(SrcSettings {
            respect_ignore_files: self.respect_ignore_files.unwrap_or(true),
            files,
        })
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case", transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Rules {
    #[cfg_attr(feature = "schemars", schemars(with = "schema::Rules"))]
    inner: OrderMap<RangedValue<String>, RangedValue<Level>, BuildHasherDefault<FxHasher>>,
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

impl Rules {
    /// Convert the rules to a `RuleSelection` with diagnostics.
    pub fn to_rule_selection(
        &self,
        db: &dyn Db,
        diagnostics: &mut Vec<OptionDiagnostic>,
    ) -> RuleSelection {
        let registry = db.lint_registry();

        // Initialize the selection with the defaults
        let mut selection = RuleSelection::from_registry(registry);

        for (rule_name, level) in &self.inner {
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

                    let annotation = file.map(Span::from).map(|span| {
                        Annotation::primary(span.with_optional_range(rule_name.range()))
                    });
                    diagnostics.push(diagnostic.with_annotation(annotation));
                }
            }
        }

        selection
    }

    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Default exclude patterns for src options.
const DEFAULT_SRC_EXCLUDES: &[&str] = &[
    "**/.bzr/",
    "**/.direnv/",
    "**/.eggs/",
    "**/.git/",
    "**/.git-rewrite/",
    "**/.hg/",
    "**/.mypy_cache/",
    "**/.nox/",
    "**/.pants.d/",
    "**/.pytype/",
    "**/.ruff_cache/",
    "**/.svn/",
    "**/.tox/",
    "**/.venv/",
    "**/__pypackages__/",
    "**/_build/",
    "**/buck-out/",
    "**/dist/",
    "**/node_modules/",
    "**/venv/",
];

/// Helper function to build an include filter from patterns with proper error handling.
fn build_include_filter(
    db: &dyn Db,
    project_root: &SystemPath,
    include_patterns: Option<&RangedValue<Vec<RelativeGlobPattern>>>,
    context: GlobFilterContext,
    diagnostics: &mut Vec<OptionDiagnostic>,
) -> Result<IncludeFilter, Box<OptionDiagnostic>> {
    use crate::glob::{IncludeFilterBuilder, PortableGlobPattern};

    let system = db.system();
    let mut includes = IncludeFilterBuilder::new();

    if let Some(include_patterns) = include_patterns {
        if include_patterns.is_empty() {
            // An override with an empty include `[]` won't match any files.
            let mut diagnostic = OptionDiagnostic::new(
                DiagnosticId::EmptyInclude,
                "Empty include matches no files".to_string(),
                Severity::Warning,
            )
            .sub(SubDiagnostic::new(
                Severity::Info,
                "Remove the `include` option to match all files or add a pattern to match specific files",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = include_patterns.source().file() {
                if let Ok(file) = system_path_to_file(db.upcast(), source_file) {
                    let annotation = Annotation::primary(
                        Span::from(file).with_optional_range(include_patterns.range()),
                    )
                    .message("This `include` list is empty");
                    diagnostic = diagnostic.with_annotation(Some(annotation));
                }
            }

            diagnostics.push(diagnostic);
        }

        for pattern in include_patterns {
            pattern.absolute(project_root, system, PortableGlobKind::Include)
                .and_then(|include| Ok(includes.add(&include)?))
                .map_err(|err| {
                    let diagnostic = OptionDiagnostic::new(
                        DiagnosticId::InvalidGlob,
                        format!("Invalid include pattern `{pattern}`: {err}"),
                        Severity::Error,
                    );

                    match pattern.source() {
                        ValueSource::File(file_path) => {
                            if let Ok(file) = system_path_to_file(db.upcast(), &**file_path) {
                                diagnostic
                                    .with_message("Invalid include pattern")
                                    .with_annotation(Some(
                                        Annotation::primary(
                                            Span::from(file)
                                                .with_optional_range(pattern.range()),
                                        )
                                            .message(err.to_string()),
                                    ))
                            } else {
                                diagnostic.sub(SubDiagnostic::new(
                                    Severity::Info,
                                    format!("The pattern is defined in the `{}` option in your configuration file", context.include_name()),
                                ))
                            }
                        }
                        ValueSource::Cli => diagnostic.sub(SubDiagnostic::new(
                            Severity::Info,
                            "The pattern was specified on the CLI",
                        )),
                    }
                })?;
        }
    } else {
        includes
            .add(
                &PortableGlobPattern::parse("**", PortableGlobKind::Include)
                    .unwrap()
                    .into_absolute(""),
            )
            .unwrap();
    }

    includes.build().map_err(|_| {
        let diagnostic = OptionDiagnostic::new(
            DiagnosticId::InvalidGlob,
            format!("The `{}` patterns resulted in a regex that is too large", context.include_name()),
            Severity::Error,
        );
        Box::new(diagnostic.sub(SubDiagnostic::new(
            Severity::Info,
            "Please open an issue on the ty repository and share the patterns that caused the error.",
        )))
    })
}

/// Helper function to build an exclude filter from patterns with proper error handling.
fn build_exclude_filter(
    db: &dyn Db,
    project_root: &SystemPath,
    exclude_patterns: Option<&RangedValue<Vec<RelativeGlobPattern>>>,
    default_patterns: &[&str],
    context: GlobFilterContext,
) -> Result<ExcludeFilter, Box<OptionDiagnostic>> {
    use crate::glob::{ExcludeFilterBuilder, PortableGlobPattern};

    let system = db.system();
    let mut excludes = ExcludeFilterBuilder::new();

    for pattern in default_patterns {
        PortableGlobPattern::parse(pattern, PortableGlobKind::Exclude)
            .and_then(|exclude| Ok(excludes.add(&exclude.into_absolute(""))?))
            .unwrap_or_else(|err| {
                panic!("Expected default exclude to be valid glob but adding it failed with: {err}")
            });
    }

    // Add user-specified excludes
    if let Some(exclude_patterns) = exclude_patterns {
        for exclude in exclude_patterns {
            exclude.absolute(project_root, system, PortableGlobKind::Exclude)
                .and_then(|pattern| Ok(excludes.add(&pattern)?))
                .map_err(|err| {
                    let diagnostic = OptionDiagnostic::new(
                        DiagnosticId::InvalidGlob,
                        format!("Invalid exclude pattern `{exclude}`: {err}"),
                        Severity::Error,
                    );

                    match exclude.source() {
                        ValueSource::File(file_path) => {
                            if let Ok(file) = system_path_to_file(db.upcast(), &**file_path) {
                                diagnostic
                                    .with_message("Invalid exclude pattern")
                                    .with_annotation(Some(
                                        Annotation::primary(
                                            Span::from(file)
                                                .with_optional_range(exclude.range()),
                                        )
                                            .message(err.to_string()),
                                    ))
                            } else {
                                diagnostic.sub(SubDiagnostic::new(
                                    Severity::Info,
                                    format!("The pattern is defined in the `{}` option in your configuration file", context.exclude_name()),
                                ))
                            }
                        }
                        ValueSource::Cli => diagnostic.sub(SubDiagnostic::new(
                            Severity::Info,
                            "The pattern was specified on the CLI",
                        )),
                    }
                })?;
        }
    }

    excludes.build().map_err(|_| {
        let diagnostic = OptionDiagnostic::new(
            DiagnosticId::InvalidGlob,
            format!("The `{}` patterns resulted in a regex that is too large", context.exclude_name()),
            Severity::Error,
        );
        Box::new(diagnostic.sub(SubDiagnostic::new(
            Severity::Info,
            "Please open an issue on the ty repository and share the patterns that caused the error.",
        )))
    })
}

/// Context for filter operations, used in error messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobFilterContext {
    /// Source root configuration context
    SrcRoot,
    /// Override configuration context
    Overrides,
}

impl GlobFilterContext {
    fn include_name(self) -> &'static str {
        match self {
            Self::SrcRoot => "src.include",
            Self::Overrides => "overrides.include",
        }
    }

    fn exclude_name(self) -> &'static str {
        match self {
            Self::SrcRoot => "src.exclude",
            Self::Overrides => "overrides.exclude",
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

/// Configuration override that applies to specific files based on glob patterns.
///
/// An override allows you to apply different rule configurations to specific
/// files or directories. Multiple overrides can match the same file, with
/// later overrides take precedence.
///
/// ### Precedence
///
/// - Later overrides in the array take precedence over earlier ones
/// - Override rules take precedence over global rules for matching files
///
/// ### Examples
///
/// ```toml
/// # Relax rules for test files
/// [[tool.ty.overrides]]
/// include = ["tests/**", "**/test_*.py"]
///
/// [tool.ty.overrides.rules]
/// possibly-unresolved-reference = "warn"
///
/// # Ignore generated files but still check important ones
/// [[tool.ty.overrides]]
/// include = ["generated/**"]
/// exclude = ["generated/important.py"]
///
/// [tool.ty.overrides.rules]
/// possibly-unresolved-reference = "ignore"
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Combine, Serialize, Deserialize, RustDoc)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct OverridesOptions(Vec<RangedValue<OverrideOptions>>);

impl OptionsMetadata for OverridesOptions {
    fn documentation() -> Option<&'static str> {
        Some(<Self as RustDoc>::rust_doc())
    }

    fn record(visit: &mut dyn Visit) {
        OptionSet::of::<OverrideOptions>().record(visit);
    }
}

impl Deref for OverridesOptions {
    type Target = [RangedValue<OverrideOptions>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OverrideOptions {
    /// A list of file and directory patterns to include for this override.
    ///
    /// The `include` option follows a similar syntax to `.gitignore` but reversed:
    /// Including a file or directory will make it so that it (and its contents)
    /// are affected by this override.
    ///
    /// If not specified, defaults to `["**"]` (matches all files).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = r#"list[str]"#,
        example = r#"
            [[tool.ty.overrides]]
            include = [
                "src",
                "tests",
            ]
        "#
    )]
    pub include: Option<RangedValue<Vec<RelativeGlobPattern>>>,

    /// A list of file and directory patterns to exclude from this override.
    ///
    /// Patterns follow a syntax similar to `.gitignore`.
    /// Exclude patterns take precedence over include patterns within the same override.
    ///
    /// If not specified, defaults to `[]` (excludes no files).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = r#"list[str]"#,
        example = r#"
            [[tool.ty.overrides]]
            exclude = [
                "generated",
                "*.proto",
                "tests/fixtures/**",
                "!tests/fixtures/important.py"  # Include this one file
            ]
        "#
    )]
    pub exclude: Option<RangedValue<Vec<RelativeGlobPattern>>>,

    /// Rule overrides for files matching the include/exclude patterns.
    ///
    /// These rules will be merged with the global rules, with override rules
    /// taking precedence for matching files. You can set rules to different
    /// severity levels or disable them entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"{...}"#,
        value_type = r#"dict[RuleName, "ignore" | "warn" | "error"]"#,
        example = r#"
            [[tool.ty.overrides]]
            include = ["src"]

            [tool.ty.overrides.rules]
            possibly-unresolved-reference = "ignore"
        "#
    )]
    pub rules: Option<Rules>,
}

impl RangedValue<OverrideOptions> {
    fn to_override(
        &self,
        db: &dyn Db,
        project_root: &SystemPath,
        global_rules: Option<&Rules>,
        diagnostics: &mut Vec<OptionDiagnostic>,
    ) -> Result<Option<Override>, Box<OptionDiagnostic>> {
        let rules = self.rules.or_default();

        // First, warn about incorrect or useless overrides.
        if rules.is_empty() {
            let mut diagnostic = OptionDiagnostic::new(
                DiagnosticId::UselessOverridesSection,
                "Useless `overrides` section".to_string(),
                Severity::Warning,
            );

            diagnostic = if self.rules.is_none() {
                diagnostic = diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "It has no `rules` table",
                ));
                diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "Add a `[overrides.rules]` table...",
                ))
            } else {
                diagnostic = diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "The rules table is empty",
                ));
                diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "Add a rule to `[overrides.rules]` to override specific rules...",
                ))
            };

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                "or remove the `[[overrides]]` section if there's nothing to override",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = self.source().file() {
                if let Ok(file) = system_path_to_file(db.upcast(), source_file) {
                    let annotation =
                        Annotation::primary(Span::from(file).with_optional_range(self.range()))
                            .message("This overrides section configures no rules");
                    diagnostic = diagnostic.with_annotation(Some(annotation));
                }
            }

            diagnostics.push(diagnostic);
            // Return `None`, because this override doesn't override anything
            return Ok(None);
        }

        let include_missing = self.include.is_none();
        let exclude_empty = self
            .exclude
            .as_ref()
            .is_none_or(|exclude| exclude.is_empty());

        if include_missing && exclude_empty {
            // Neither include nor exclude specified - applies to all files
            let mut diagnostic = OptionDiagnostic::new(
                DiagnosticId::UnnecessaryOverridesSection,
                "Unnecessary `overrides` section".to_string(),
                Severity::Warning,
            );

            diagnostic = if self.exclude.is_none() {
                diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "It has no `include` or `exclude` option restricting the files",
                ))
            } else {
                diagnostic.sub(SubDiagnostic::new(
                    Severity::Info,
                    "It has no `include` option and `exclude` is empty",
                ))
            };

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                "Restrict the files by adding a pattern to `include` or `exclude`...",
            ));

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                "or remove the `[[overrides]]` section and merge the configuration into the root `[rules]` table if the configuration should apply to all files",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = self.source().file() {
                if let Ok(file) = system_path_to_file(db.upcast(), source_file) {
                    let annotation =
                        Annotation::primary(Span::from(file).with_optional_range(self.range()))
                            .message("This overrides section applies to all files");
                    diagnostic = diagnostic.with_annotation(Some(annotation));
                }
            }

            diagnostics.push(diagnostic);
        }

        // The override is at least (partially) valid.
        // Construct the matcher and resolve the settings.
        let include = build_include_filter(
            db,
            project_root,
            self.include.as_ref(),
            GlobFilterContext::Overrides,
            diagnostics,
        )?;

        let exclude = build_exclude_filter(
            db,
            project_root,
            self.exclude.as_ref(),
            &[],
            GlobFilterContext::Overrides,
        )?;

        let files = IncludeExcludeFilter::new(include, exclude);

        // Merge global rules with override rules, with override rules taking precedence
        let mut merged_rules = rules.into_owned();

        if let Some(global_rules) = global_rules {
            merged_rules = merged_rules.combine(global_rules.clone());
        }

        // Convert merged rules to rule selection
        let rule_selection = merged_rules.to_rule_selection(db, diagnostics);

        let override_instance = Override {
            files,
            options: Arc::new(InnerOverrideOptions {
                rules: self.rules.clone(),
            }),
            settings: Arc::new(OverrideSettings {
                rules: rule_selection,
            }),
        };

        Ok(Some(override_instance))
    }
}

/// The options for an override but without the include/exclude patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Combine)]
pub(super) struct InnerOverrideOptions {
    /// Raw rule options as specified in the configuration.
    /// Used when multiple overrides match a file and need to be merged.
    pub(super) rules: Option<Rules>,
}

/// Error returned when the settings can't be resolved because of a hard error.
#[derive(Debug)]
pub struct ToSettingsError {
    diagnostic: Box<OptionDiagnostic>,
    output_format: DiagnosticFormat,
    color: bool,
}

impl ToSettingsError {
    pub fn pretty<'a>(&'a self, db: &'a dyn Db) -> impl fmt::Display + use<'a> {
        struct DisplayPretty<'a> {
            db: &'a dyn Db,
            error: &'a ToSettingsError,
        }

        impl fmt::Display for DisplayPretty<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let display_config = DisplayDiagnosticConfig::default()
                    .format(self.error.output_format)
                    .color(self.error.color);

                write!(
                    f,
                    "{}",
                    self.error
                        .diagnostic
                        .to_diagnostic()
                        .display(&self.db.upcast(), &display_config)
                )
            }
        }

        DisplayPretty { db, error: self }
    }

    pub fn into_diagnostic(self) -> OptionDiagnostic {
        *self.diagnostic
    }
}

impl Display for ToSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic.message)
    }
}

impl std::error::Error for ToSettingsError {}

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
    annotation: Option<Annotation>,
    sub: Vec<SubDiagnostic>,
}

impl OptionDiagnostic {
    pub fn new(id: DiagnosticId, message: String, severity: Severity) -> Self {
        Self {
            id,
            message,
            severity,
            annotation: None,
            sub: Vec::new(),
        }
    }

    #[must_use]
    fn with_message(self, message: impl Display) -> Self {
        OptionDiagnostic {
            message: message.to_string(),
            ..self
        }
    }

    #[must_use]
    fn with_annotation(self, annotation: Option<Annotation>) -> Self {
        OptionDiagnostic { annotation, ..self }
    }

    #[must_use]
    fn sub(mut self, sub: SubDiagnostic) -> Self {
        self.sub.push(sub);
        self
    }

    pub(crate) fn to_diagnostic(&self) -> Diagnostic {
        let mut diag = Diagnostic::new(self.id, self.severity, &self.message);
        if let Some(annotation) = self.annotation.clone() {
            diag.annotate(annotation);
        }

        for sub in &self.sub {
            diag.sub(sub.clone());
        }

        diag
    }
}

/// This is a wrapper for options that actually get loaded from configuration files
/// and the CLI, which also includes a `config_file_override` option that overrides
/// default configuration discovery with an explicitly-provided path to a configuration file
pub struct ProjectOptionsOverrides {
    pub config_file_override: Option<SystemPathBuf>,
    pub options: Options,
}

impl ProjectOptionsOverrides {
    pub fn new(config_file_override: Option<SystemPathBuf>, options: Options) -> Self {
        Self {
            config_file_override,
            options,
        }
    }
}

trait OrDefault {
    type Target: ToOwned;

    fn or_default(&self) -> Cow<'_, Self::Target>;
}

impl<T> OrDefault for Option<T>
where
    T: Default + ToOwned<Owned = T>,
{
    type Target = T;

    fn or_default(&self) -> Cow<'_, Self::Target> {
        match self {
            Some(value) => Cow::Borrowed(value),
            None => Cow::Owned(T::default()),
        }
    }
}
