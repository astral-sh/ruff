use crate::Db;
use crate::glob::{ExcludeFilter, IncludeExcludeFilter, IncludeFilter, PortableGlobKind};
use crate::metadata::settings::{OverrideSettings, SrcSettings};

use super::settings::{Override, Settings, TerminalSettings};
use crate::metadata::value::{
    RangedValue, RelativeGlobPattern, RelativePathBuf, ValueSource, ValueSourceGuard,
};
use anyhow::Context;
use ordermap::OrderMap;
use ruff_db::RustDoc;
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig, Severity,
    Span, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
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
use ty_combine::Combine;
use ty_module_resolver::{SearchPathSettings, SearchPathSettingsError, SearchPaths};
use ty_python_semantic::lint::{Level, LintSource, RuleSelection};
use ty_python_semantic::{
    AnalysisSettings, MisconfigurationMode, ProgramSettings, PythonEnvironment, PythonPlatform,
    PythonVersionFileSource, PythonVersionSource, PythonVersionWithSource, SitePackagesPaths,
    SysPrefixPathOrigin,
};
use ty_static::EnvVars;

#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
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

    #[serde(skip_serializing_if = "Option::is_none")]
    #[option_group]
    pub analysis: Option<AnalysisOptions>,

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
        vendored: &VendoredFileSystem,
        misconfiguration_mode: MisconfigurationMode,
    ) -> anyhow::Result<ProgramSettings> {
        let environment = self.environment.or_default();

        let options_python_version =
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
                        ValueSource::Editor => PythonVersionSource::Editor,
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

        let python_environment = if let Some(python_path) = environment.python.as_ref() {
            let origin = match python_path.source() {
                ValueSource::Cli => SysPrefixPathOrigin::PythonCliFlag,
                ValueSource::File(path) => {
                    SysPrefixPathOrigin::ConfigFileSetting(path.clone(), python_path.range())
                }
                ValueSource::Editor => SysPrefixPathOrigin::Editor,
            };

            PythonEnvironment::new(python_path.absolute(project_root, system), origin, system)
                .map_err(anyhow::Error::from)
                .map(Some)
        } else {
            PythonEnvironment::discover(project_root, system)
                .context("Failed to discover local Python environment")
        };

        // If in safe-mode, fallback to None if this fails instead of erroring.
        let python_environment = match python_environment {
            Ok(python_environment) => python_environment,
            Err(err) => {
                if misconfiguration_mode == MisconfigurationMode::UseDefault {
                    tracing::debug!("Default settings failed to discover local Python environment");
                    None
                } else {
                    return Err(err);
                }
            }
        };

        let self_site_packages = self_environment_search_paths(
            python_environment
                .as_ref()
                .map(ty_python_semantic::PythonEnvironment::origin)
                .cloned(),
            system,
        )
        .unwrap_or_default();

        let site_packages_paths = if let Some(python_environment) = python_environment.as_ref() {
            let site_packages_paths = python_environment
                .site_packages_paths(system)
                .context("Failed to discover the site-packages directory");
            let site_packages_paths = match site_packages_paths {
                Ok(paths) => paths,
                Err(err) => {
                    if misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!(
                            "Default settings failed to discover site-packages directory"
                        );
                        SitePackagesPaths::default()
                    } else {
                        return Err(err);
                    }
                }
            };
            self_site_packages.concatenate(site_packages_paths)
        } else {
            tracing::debug!("No virtual environment found");
            self_site_packages
        };

        let real_stdlib_path = python_environment.as_ref().and_then(|python_environment| {
            // For now this is considered non-fatal, we don't Need this for anything.
            python_environment.real_stdlib_path(system).map_err(|err| {
                tracing::info!("No real stdlib found, stdlib goto-definition may have degraded quality: {err}");
            }).ok()
        });

        let python_version = options_python_version
            .or_else(|| {
                python_environment
                    .as_ref()?
                    .python_version_from_metadata()
                    .cloned()
            })
            .or_else(|| site_packages_paths.python_version_from_layout())
            .unwrap_or_default();

        // Safe mode is handled inside this function, so we just assume this can't fail
        let search_paths = self.to_search_paths(
            project_root,
            project_name,
            site_packages_paths,
            real_stdlib_path,
            system,
            vendored,
            misconfiguration_mode,
        )?;

        tracing::info!(
            "Python version: Python {python_version}, platform: {python_platform}",
            python_version = python_version.version
        );

        Ok(ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        })
    }

    #[expect(clippy::too_many_arguments)]
    fn to_search_paths(
        &self,
        project_root: &SystemPath,
        project_name: &str,
        site_packages_paths: SitePackagesPaths,
        real_stdlib_path: Option<SystemPathBuf>,
        system: &dyn System,
        vendored: &VendoredFileSystem,
        misconfiguration_mode: MisconfigurationMode,
    ) -> Result<SearchPaths, SearchPathSettingsError> {
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
            let mut roots = vec![];
            let src = project_root.join("src");

            if system.is_directory(&src) {
                // Default to `src` and the project root if `src` exists and the root hasn't been specified.
                // This corresponds to the `src-layout`
                tracing::debug!(
                    "Including `.` and `./src` in `environment.root` because a `./src` directory exists"
                );
                roots.push(src);
            } else if system.is_directory(&project_root.join(project_name).join(project_name)) {
                // `src-layout` but when the folder isn't called `src` but has the same name as the project.
                // For example, the "src" folder for `psycopg` is called `psycopg` and the python files are in `psycopg/psycopg/_adapters_map.py`
                tracing::debug!(
                    "Including `.` and `/{project_name}` in `environment.root` because a `./{project_name}/{project_name}` directory exists"
                );

                roots.push(project_root.join(project_name));
            } else {
                // Default to a [flat project structure](https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/).
                tracing::debug!("Including `.` in `environment.root`");
            }

            let python = project_root.join("python");
            if system.is_directory(&python)
                && !system.is_file(&python.join("__init__.py"))
                && !system.is_file(&python.join("__init__.pyi"))
                && !roots.contains(&python)
            {
                // If a `./python` directory exists, include it as a source root. This is the recommended layout
                // for maturin-based rust/python projects [1].
                //
                // https://github.com/PyO3/maturin/blob/979fe1db42bb9e58bc150fa6fc45360b377288bf/README.md?plain=1#L88-L99
                tracing::debug!(
                    "Including `./python` in `environment.root` because a `./python` directory exists"
                );

                roots.push(python);
            }

            // The project root should always be included, and should always come
            // after any subdirectories such as `./src`, `./tests` and/or `./python`.
            roots.push(project_root.to_path_buf());

            roots
        };

        // collect the existing site packages
        let mut extra_paths: Vec<SystemPathBuf> = environment
            .extra_paths
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|path| path.absolute(project_root, system))
            .collect();

        // read all the paths off the PYTHONPATH environment variable, check
        // they exist as a directory, and add them to the vec of extra_paths
        // as they should be checked before site-packages just like python
        // interpreter does
        if let Ok(python_path) = system.env_var(EnvVars::PYTHONPATH) {
            for path in std::env::split_paths(python_path.as_str()) {
                let path = match SystemPathBuf::from_path_buf(path) {
                    Ok(path) => path,
                    Err(path) => {
                        tracing::debug!(
                            "Skipping `{path}` listed in `PYTHONPATH` because the path is not valid UTF-8",
                            path = path.display()
                        );
                        continue;
                    }
                };

                let abspath = SystemPath::absolute(path, system.current_directory());

                if !system.is_directory(&abspath) {
                    tracing::debug!(
                        "Skipping `{abspath}` listed in `PYTHONPATH` because the path doesn't exist or isn't a directory"
                    );
                    continue;
                }

                tracing::debug!(
                    "Adding `{abspath}` from the `PYTHONPATH` environment variable to `extra_paths`"
                );

                extra_paths.push(abspath);
            }
        }

        let settings = SearchPathSettings {
            extra_paths,
            src_roots,
            custom_typeshed: environment
                .typeshed
                .as_ref()
                .map(|path| path.absolute(project_root, system)),
            site_packages_paths: site_packages_paths.into_vec(),
            real_stdlib_path,
            misconfiguration_mode,
        };

        settings.to_search_paths(system, vendored)
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
                .and_then(|path| system_path_to_file(db, path).ok())
            {
                diagnostic = diagnostic.with_annotation(Some(Annotation::primary(
                    Span::from(file).with_optional_range(src_root.range()),
                )));
            }

            if self.environment.or_default().root.is_some() {
                diagnostic = diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
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

        let analysis = self.analysis.or_default().to_settings();

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
            analysis,
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

/// Return the site-packages from the environment ty is installed in, as derived from ty's
/// executable.
///
/// If there's an existing environment with an origin that does not allow including site-packages
/// from ty's environment, discovery of ty's environment is skipped and [`None`] is returned.
///
/// Since ty may be executed from an arbitrary non-Python location, errors during discovery of ty's
/// environment are not raised, instead [`None`] is returned.
fn self_environment_search_paths(
    existing_origin: Option<SysPrefixPathOrigin>,
    system: &dyn System,
) -> Option<SitePackagesPaths> {
    if existing_origin.is_some_and(|origin| !origin.allows_concatenation_with_self_environment()) {
        return None;
    }

    let Ok(exe_path) = std::env::current_exe() else {
        return None;
    };
    let ty_path = SystemPath::from_std_path(exe_path.as_path())?;

    let environment = PythonEnvironment::new(ty_path, SysPrefixPathOrigin::SelfEnvironment, system)
        .inspect_err(|err| tracing::debug!("Failed to discover ty's environment: {err}"))
        .ok()?;

    let search_paths = environment
        .site_packages_paths(system)
        .inspect_err(|err| {
            tracing::debug!("Failed to discover site-packages in ty's environment: {err}");
        })
        .ok();

    tracing::debug!("Using site-packages from ty's environment");
    search_paths
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
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
    /// Additionally, if a `./python` directory exists and is not a package (i.e. it does not contain an `__init__.py` or `__init__.pyi` file),
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
        default = r#""3.14""#,
        value_type = r#""3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | "3.14" | <major>.<minor>"#,
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

    /// User-provided paths that should take first priority in module resolution.
    ///
    /// This is an advanced option that should usually only be used for first-party or third-party
    /// modules that are not installed into your Python environment in a conventional way.
    /// Use the `python` option to specify the location of your Python environment.
    ///
    /// This option is similar to mypy's `MYPYPATH` environment variable and pyright's `stubPath`
    /// configuration setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            extra-paths = ["./shared/my-search-path"]
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

    /// Path to your project's Python environment or interpreter.
    ///
    /// ty uses the `site-packages` directory of your project's Python environment
    /// to resolve third-party (and, in some cases, first-party) imports in your code.
    ///
    /// If you're using a project management tool such as uv, you should not generally need
    /// to specify this option, as commands such as `uv run` will set the `VIRTUAL_ENV`
    /// environment variable to point to your project's virtual environment. ty can also infer
    /// the location of your environment from an activated Conda environment, and will look for
    /// a `.venv` directory in the project root if none of the above apply.
    ///
    /// Passing a path to a Python executable is supported, but passing a path to a dynamic executable
    /// (such as a shim) is not currently supported.
    ///
    /// This option can be used to point to virtual or system Python environments.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            python = "./custom-venv-location/.venv"
        "#
    )]
    pub python: Option<RelativePathBuf>,
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
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
    /// Additionally, if a `./python` directory exists and is not a package (i.e. it does not contain an `__init__.py` file),
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
    ///
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

#[derive(
    Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize, Hash, get_size2::GetSize,
)]
#[serde(rename_all = "kebab-case", transparent)]
pub struct Rules {
    #[get_size(ignore)] // TODO: Add `GetSize` support for `OrderMap`.
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
                        ValueSource::Editor => LintSource::Editor,
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
                        .and_then(|path| system_path_to_file(db, path).ok());

                    // TODO: Add a note if the value was configured on the CLI
                    let diagnostic = OptionDiagnostic::new(
                        DiagnosticId::UnknownRule,
                        error.to_string(),
                        Severity::Warning,
                    );

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
                SubDiagnosticSeverity::Info,
                "Remove the `include` option to match all files or add a pattern to match specific files",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = include_patterns.source().file() {
                if let Ok(file) = system_path_to_file(db, source_file) {
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
                            if let Ok(file) = system_path_to_file(db, &**file_path) {
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
                                    SubDiagnosticSeverity::Info,
                                    format!("The pattern is defined in the `{}` option in your configuration file", context.include_name()),
                                ))
                            }
                        }
                        ValueSource::Cli => diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "The pattern was specified on the CLI",
                        )),
                        ValueSource::Editor => {
                            diagnostic.sub(SubDiagnostic::new(
                                SubDiagnosticSeverity::Info,
                                "The pattern was specified in the editor settings.",
                            ))
                        }
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
            SubDiagnosticSeverity::Info,
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
                            if let Ok(file) = system_path_to_file(db, &**file_path) {
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
                                    SubDiagnosticSeverity::Info,
                                    format!("The pattern is defined in the `{}` option in your configuration file", context.exclude_name()),
                                ))
                            }
                        }
                        ValueSource::Cli => diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "The pattern was specified on the CLI",
                        )),
                        ValueSource::Editor => diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "The pattern was specified in the editor settings",
                        ))
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
            SubDiagnosticSeverity::Info,
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

/// The diagnostic output format.
#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, get_size2::GetSize,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum OutputFormat {
    /// The default full mode will print "pretty" diagnostics.
    ///
    /// That is, color will be used when printing to a `tty`.
    /// Moreover, diagnostic messages may include additional
    /// context and annotations on the input to help understand
    /// the message.
    #[default]
    Full,
    /// Print diagnostics in a concise mode.
    ///
    /// This will guarantee that each diagnostic is printed on
    /// a single line. Only the most important or primary aspects
    /// of the diagnostic are included. Contextual information is
    /// dropped.
    ///
    /// This may use color when printing to a `tty`.
    Concise,
    /// Print diagnostics in the JSON format expected by GitLab [Code Quality] reports.
    ///
    /// [Code Quality]: https://docs.gitlab.com/ci/testing/code_quality/#code-quality-report-format
    Gitlab,
    /// Print diagnostics in the format used by [GitHub Actions] workflow error annotations.
    ///
    /// [GitHub Actions]: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands#setting-an-error-message
    Github,
}

impl OutputFormat {
    /// Returns `true` if this format is intended for users to read directly, in contrast to
    /// machine-readable or structured formats.
    ///
    /// This can be used to check whether information beyond the diagnostics, such as a header or
    /// `Found N diagnostics` footer, should be included.
    pub const fn is_human_readable(&self) -> bool {
        matches!(self, OutputFormat::Full | OutputFormat::Concise)
    }
}

impl From<OutputFormat> for DiagnosticFormat {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Full => Self::Full,
            OutputFormat::Concise => Self::Concise,
            OutputFormat::Gitlab => Self::Gitlab,
            OutputFormat::Github => Self::Github,
        }
    }
}

impl Combine for OutputFormat {
    #[inline(always)]
    fn combine_with(&mut self, _other: Self) {}

    #[inline]
    fn combine(self, _other: Self) -> Self {
        self
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
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
    pub output_format: Option<RangedValue<OutputFormat>>,
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

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AnalysisOptions {
    /// Whether ty should respect `type: ignore` comments.
    ///
    /// When set to `false`, `type: ignore` comments are treated like any other normal
    /// comment and can't be used to suppress ty errors (you have to use `ty: ignore` instead).
    ///
    /// Setting this option can be useful when using ty alongside other type checkers or when
    /// you prefer using `ty: ignore` over `type: ignore`.
    ///
    /// Defaults to `true`.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
        # Disable support for `type: ignore` comments
        respect-type-ignore-comments = false
        "#
    )]
    respect_type_ignore_comments: Option<bool>,
}

impl AnalysisOptions {
    fn to_settings(&self) -> AnalysisSettings {
        let AnalysisSettings {
            respect_type_ignore_comments: respect_type_ignore_default,
        } = AnalysisSettings::default();

        AnalysisSettings {
            respect_type_ignore_comments: self
                .respect_type_ignore_comments
                .unwrap_or(respect_type_ignore_default),
        }
    }
}

/// Configuration override that applies to specific files based on glob patterns.
///
/// An override allows you to apply different rule configurations to specific
/// files or directories. Multiple overrides can match the same file, with
/// later overrides take precedence. Override rules take precedence over global
/// rules for matching files.
///
/// For example, to relax enforcement of rules in test files:
///
/// ```toml
/// [[tool.ty.overrides]]
/// include = ["tests/**", "**/test_*.py"]
///
/// [tool.ty.overrides.rules]
/// possibly-unresolved-reference = "warn"
/// ```
///
/// Or, to ignore a rule in generated files but retain enforcement in an important file:
///
/// ```toml
/// [[tool.ty.overrides]]
/// include = ["generated/**"]
/// exclude = ["generated/important.py"]
///
/// [tool.ty.overrides.rules]
/// possibly-unresolved-reference = "ignore"
/// ```
#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Combine,
    Serialize,
    Deserialize,
    RustDoc,
    get_size2::GetSize,
)]
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
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Combine,
    Serialize,
    Deserialize,
    OptionsMetadata,
    get_size2::GetSize,
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
                    SubDiagnosticSeverity::Info,
                    "It has no `rules` table",
                ));
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "Add a `[overrides.rules]` table...",
                ))
            } else {
                diagnostic = diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "The rules table is empty",
                ));
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "Add a rule to `[overrides.rules]` to override specific rules...",
                ))
            };

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                "or remove the `[[overrides]]` section if there's nothing to override",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = self.source().file() {
                if let Ok(file) = system_path_to_file(db, source_file) {
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
                    SubDiagnosticSeverity::Info,
                    "It has no `include` or `exclude` option restricting the files",
                ))
            } else {
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "It has no `include` option and `exclude` is empty",
                ))
            };

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                "Restrict the files by adding a pattern to `include` or `exclude`...",
            ));

            diagnostic = diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                "or remove the `[[overrides]]` section and merge the configuration into the root `[rules]` table if the configuration should apply to all files",
            ));

            // Add source annotation if we have source information
            if let Some(source_file) = self.source().file() {
                if let Ok(file) = system_path_to_file(db, source_file) {
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Combine, get_size2::GetSize)]
pub(super) struct InnerOverrideOptions {
    /// Raw rule options as specified in the configuration.
    /// Used when multiple overrides match a file and need to be merged.
    pub(super) rules: Option<Rules>,
}

/// Error returned when the settings can't be resolved because of a hard error.
#[derive(Debug)]
pub struct ToSettingsError {
    diagnostic: Box<OptionDiagnostic>,
    output_format: OutputFormat,
    color: bool,
}

impl ToSettingsError {
    pub fn pretty<'a>(&'a self, db: &'a dyn Db) -> impl fmt::Display + use<'a> {
        struct DisplayPretty<'a> {
            db: &'a dyn ruff_db::Db,
            error: &'a ToSettingsError,
        }

        impl fmt::Display for DisplayPretty<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let display_config = DisplayDiagnosticConfig::default()
                    .format(self.error.output_format.into())
                    .color(self.error.color);

                write!(
                    f,
                    "{}",
                    self.error
                        .diagnostic
                        .to_diagnostic()
                        .display(&self.db, &display_config)
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
    impl schemars::JsonSchema for super::Rules {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            std::borrow::Cow::Borrowed("Rules")
        }

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
            use serde_json::{Map, Value};

            let registry = ty_python_semantic::default_lint_registry();
            let level_schema = generator.subschema_for::<super::Level>();

            let properties: Map<String, Value> = registry
                .lints()
                .iter()
                .map(|lint| {
                    let mut schema = schemars::Schema::default();
                    let object = schema.ensure_object();
                    object.insert(
                        "title".to_string(),
                        Value::String(lint.summary().to_string()),
                    );
                    object.insert(
                        "description".to_string(),
                        Value::String(lint.documentation()),
                    );
                    if lint.status.is_deprecated() {
                        object.insert("deprecated".to_string(), Value::Bool(true));
                    }
                    object.insert(
                        "default".to_string(),
                        Value::String(lint.default_level.to_string()),
                    );
                    object.insert(
                        "oneOf".to_string(),
                        Value::Array(vec![level_schema.clone().into()]),
                    );

                    (lint.name().to_string(), schema.into())
                })
                .collect();

            let mut schema = schemars::json_schema!({ "type": "object" });
            let object = schema.ensure_object();
            object.insert("properties".to_string(), Value::Object(properties));
            // Allow unknown rules: ty will warn about them. It gives a better experience when using an older
            // ty version because the schema will not deny rules that have been removed in newer versions.
            object.insert("additionalProperties".to_string(), level_schema.into());

            schema
        }
    }
}

#[derive(Error, Debug)]
pub enum TyTomlError {
    #[error(transparent)]
    TomlSyntax(#[from] toml::de::Error),
}

#[derive(Debug, PartialEq, Eq, Clone, get_size2::GetSize)]
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct ProjectOptionsOverrides {
    pub config_file_override: Option<SystemPathBuf>,
    pub fallback_python_version: Option<RangedValue<PythonVersion>>,
    pub fallback_python: Option<RelativePathBuf>,
    pub options: Options,
}

impl ProjectOptionsOverrides {
    pub fn new(config_file_override: Option<SystemPathBuf>, options: Options) -> Self {
        Self {
            config_file_override,
            options,
            ..Self::default()
        }
    }

    pub fn apply_to(&self, options: Options) -> Options {
        let mut combined = self.options.clone().combine(options);

        // Set the fallback python version and path if set
        combined.environment.combine_with(Some(EnvironmentOptions {
            python_version: self.fallback_python_version.clone(),
            python: self.fallback_python.clone(),
            ..EnvironmentOptions::default()
        }));

        combined
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
