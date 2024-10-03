use regex::Regex;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use strum::IntoEnumIterator;

use crate::options_base::{OptionsMetadata, Visit};
use crate::settings::LineEnding;
use ruff_formatter::IndentStyle;
use ruff_graph::Direction;
use ruff_linter::line_width::{IndentWidth, LineLength};
use ruff_linter::rules::flake8_import_conventions::settings::BannedAliases;
use ruff_linter::rules::flake8_pytest_style::settings::SettingsError;
use ruff_linter::rules::flake8_pytest_style::types;
use ruff_linter::rules::flake8_quotes::settings::Quote;
use ruff_linter::rules::flake8_tidy_imports::settings::{ApiBan, Strictness};
use ruff_linter::rules::isort::settings::RelativeImportsOrder;
use ruff_linter::rules::isort::{ImportSection, ImportType};
use ruff_linter::rules::pep8_naming::settings::IgnoreNames;
use ruff_linter::rules::pydocstyle::settings::Convention;
use ruff_linter::rules::pylint::settings::ConstantType;
use ruff_linter::rules::{
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade, ruff,
};
use ruff_linter::settings::types::{
    IdentifierPattern, OutputFormat, PreviewMode, PythonVersion, RequiredVersion,
};
use ruff_linter::{warn_user_once, RuleSelector};
use ruff_macros::{CombineOptions, OptionsMetadata};
use ruff_python_ast::name::Name;
use ruff_python_formatter::{DocstringCodeLineWidth, QuoteStyle};
use ruff_python_semantic::NameImports;

#[derive(Clone, Debug, PartialEq, Eq, Default, OptionsMetadata, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    /// A path to the cache directory.
    ///
    /// By default, Ruff stores cache results in a `.ruff_cache` directory in
    /// the current project root.
    ///
    /// However, Ruff will also respect the `RUFF_CACHE_DIR` environment
    /// variable, which takes precedence over that default.
    ///
    /// This setting will override even the `RUFF_CACHE_DIR` environment
    /// variable, if set.
    #[option(
        default = r#"".ruff_cache""#,
        value_type = "str",
        example = r#"cache-dir = "~/.cache/ruff""#
    )]
    pub cache_dir: Option<String>,

    /// A path to a local `pyproject.toml` file to merge into this
    /// configuration. User home directory and environment variables will be
    /// expanded.
    ///
    /// To resolve the current `pyproject.toml` file, Ruff will first resolve
    /// this base configuration file, then merge in any properties defined
    /// in the current configuration file.
    #[option(
        default = r#"null"#,
        value_type = "str",
        example = r#"
            # Extend the `pyproject.toml` file in the parent directory.
            extend = "../pyproject.toml"
            # But use a different line length.
            line-length = 100
        "#
    )]
    pub extend: Option<String>,

    /// The style in which violation messages should be formatted: `"full"` (default)
    /// (shows source), `"concise"`, `"grouped"` (group messages by file), `"json"`
    /// (machine-readable), `"junit"` (machine-readable XML), `"github"` (GitHub
    /// Actions annotations), `"gitlab"` (GitLab CI code quality report),
    /// `"pylint"` (Pylint text format) or `"azure"` (Azure Pipeline logging commands).
    #[option(
        default = r#""full""#,
        value_type = r#""full" | "concise" | "grouped" | "json" | "junit" | "github" | "gitlab" | "pylint" | "azure""#,
        example = r#"
            # Group violations by containing file.
            output-format = "grouped"
        "#
    )]
    pub output_format: Option<OutputFormat>,

    /// Enable fix behavior by-default when running `ruff` (overridden
    /// by the `--fix` and `--no-fix` command-line flags).
    /// Only includes automatic fixes unless `--unsafe-fixes` is provided.
    #[option(default = "false", value_type = "bool", example = "fix = true")]
    pub fix: Option<bool>,

    /// Enable application of unsafe fixes.
    /// If excluded, a hint will be displayed when unsafe fixes are available.
    /// If set to false, the hint will be hidden.
    #[option(
        default = r#"null"#,
        value_type = "bool",
        example = "unsafe-fixes = true"
    )]
    pub unsafe_fixes: Option<bool>,

    /// Like [`fix`](#fix), but disables reporting on leftover violation. Implies [`fix`](#fix).
    #[option(default = "false", value_type = "bool", example = "fix-only = true")]
    pub fix_only: Option<bool>,

    /// Whether to show an enumeration of all fixed lint violations
    /// (overridden by the `--show-fixes` command-line flag).
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enumerate all fixed violations.
            show-fixes = true
        "#
    )]
    pub show_fixes: Option<bool>,

    /// Enforce a requirement on the version of Ruff, to enforce at runtime.
    /// If the version of Ruff does not meet the requirement, Ruff will exit
    /// with an error.
    ///
    /// Useful for unifying results across many environments, e.g., with a
    /// `pyproject.toml` file.
    ///
    /// Accepts a [PEP 440](https://peps.python.org/pep-0440/) specifier, like `==0.3.1` or `>=0.3.1`.
    #[option(
        default = "null",
        value_type = "str",
        example = r#"
            required-version = ">=0.0.193"
        "#
    )]
    pub required_version: Option<RequiredVersion>,

    /// Whether to enable preview mode. When preview mode is enabled, Ruff will
    /// use unstable rules, fixes, and formatting.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable preview features.
            preview = true
        "#
    )]
    pub preview: Option<bool>,

    // File resolver options
    /// A list of file patterns to exclude from formatting and linting.
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// - Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// - Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    ///
    /// Note that you'll typically want to use
    /// [`extend-exclude`](#extend-exclude) to modify the excluded paths.
    #[option(
        default = r#"[".bzr", ".direnv", ".eggs", ".git", ".git-rewrite", ".hg", ".mypy_cache", ".nox", ".pants.d", ".pytype", ".ruff_cache", ".svn", ".tox", ".venv", "__pypackages__", "_build", "buck-out", "dist", "node_modules", "venv"]"#,
        value_type = "list[str]",
        example = r#"
            exclude = [".venv"]
        "#
    )]
    pub exclude: Option<Vec<String>>,

    /// A list of file patterns to omit from formatting and linting, in addition to those
    /// specified by [`exclude`](#exclude).
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// - Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// - Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # In addition to the standard set of exclusions, omit all tests, plus a specific file.
            extend-exclude = ["tests", "src/bad.py"]
        "#
    )]
    pub extend_exclude: Option<Vec<String>>,

    /// A list of file patterns to include when linting, in addition to those
    /// specified by [`include`](#include).
    ///
    /// Inclusion are based on globs, and should be single-path patterns, like
    /// `*.pyw`, to include any file with the `.pyw` extension.
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # In addition to the standard set of inclusions, include `.pyw` files.
            extend-include = ["*.pyw"]
        "#
    )]
    pub extend_include: Option<Vec<String>>,

    /// Whether to enforce [`exclude`](#exclude) and [`extend-exclude`](#extend-exclude) patterns,
    /// even for paths that are passed to Ruff explicitly. Typically, Ruff will lint
    /// any paths passed in directly, even if they would typically be
    /// excluded. Setting `force-exclude = true` will cause Ruff to
    /// respect these exclusions unequivocally.
    ///
    /// This is useful for [`pre-commit`](https://pre-commit.com/), which explicitly passes all
    /// changed files to the [`ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit)
    /// plugin, regardless of whether they're marked as excluded by Ruff's own
    /// settings.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-exclude = true
        "#
    )]
    pub force_exclude: Option<bool>,

    /// A list of file patterns to include when linting.
    ///
    /// Inclusion are based on globs, and should be single-path patterns, like
    /// `*.pyw`, to include any file with the `.pyw` extension. `pyproject.toml` is
    /// included here not for configuration but because we lint whether e.g. the
    /// `[project]` matches the schema.
    ///
    /// Notebook files (`.ipynb` extension) are included by default on Ruff 0.6.0+.
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"["*.py", "*.pyi", "*.ipynb", "**/pyproject.toml"]"#,
        value_type = "list[str]",
        example = r#"
            include = ["*.py"]
        "#
    )]
    pub include: Option<Vec<String>>,

    /// Whether to automatically exclude files that are ignored by `.ignore`,
    /// `.gitignore`, `.git/info/exclude`, and global `gitignore` files.
    /// Enabled by default.
    #[option(
        default = "true",
        value_type = "bool",
        example = r#"
            respect-gitignore = false
        "#
    )]
    pub respect_gitignore: Option<bool>,

    // Generic python options
    /// A list of builtins to treat as defined references, in addition to the
    /// system builtins.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            builtins = ["_"]
        "#
    )]
    pub builtins: Option<Vec<String>>,

    /// Mark the specified directories as namespace packages. For the purpose of
    /// module resolution, Ruff will treat those directories and all their subdirectories
    /// as if they contained an `__init__.py` file.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            namespace-packages = ["airflow/providers"]
        "#
    )]
    pub namespace_packages: Option<Vec<String>>,

    /// The minimum Python version to target, e.g., when considering automatic
    /// code upgrades, like rewriting type annotations. Ruff will not propose
    /// changes using features that are not available in the given version.
    ///
    /// For example, to represent supporting Python >=3.10 or ==3.10
    /// specify `target-version = "py310"`.
    ///
    /// If you're already using a `pyproject.toml` file, we recommend
    /// `project.requires-python` instead, as it's based on Python packaging
    /// standards, and will be respected by other tools. For example, Ruff
    /// treats the following as identical to `target-version = "py38"`:
    ///
    /// ```toml
    /// [project]
    /// requires-python = ">=3.8"
    /// ```
    ///
    /// If both are specified, `target-version` takes precedence over
    /// `requires-python`.
    ///
    /// Note that a stub file can [sometimes make use of a typing feature](https://typing.readthedocs.io/en/latest/spec/distributing.html#syntax)
    /// before it is available at runtime, as long as the stub does not make
    /// use of new *syntax*. For example, a type checker will understand
    /// `int | str` in a stub as being a `Union` type annotation, even if the
    /// type checker is run using Python 3.9, despite the fact that the `|`
    /// operator can only be used to create union types at runtime on Python
    /// 3.10+. As such, Ruff will often recommend newer features in a stub
    /// file than it would for an equivalent runtime file with the same target
    /// version.
    #[option(
        default = r#""py38""#,
        value_type = r#""py37" | "py38" | "py39" | "py310" | "py311" | "py312""#,
        example = r#"
            # Always generate Python 3.7-compatible code.
            target-version = "py37"
        "#
    )]
    pub target_version: Option<PythonVersion>,

    /// The directories to consider when resolving first- vs. third-party
    /// imports.
    ///
    /// When omitted, the `src` directory will typically default to including both:
    ///
    /// 1. The directory containing the nearest `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file (the "project root").
    /// 2. The `"src"` subdirectory of the project root.
    ///
    /// These defaults ensure that uv supports both flat layouts and `src` layouts out-of-the-box.
    /// (If a configuration file is explicitly provided (e.g., via the `--config` command-line
    /// flag), the current working directory will be considered the project root.)
    ///
    /// As an example, consider an alternative project structure, like:
    ///
    /// ```text
    /// my_project
    /// ├── pyproject.toml
    /// └── lib
    ///     └── my_package
    ///         ├── __init__.py
    ///         ├── foo.py
    ///         └── bar.py
    /// ```
    ///
    /// In this case, the `./lib` directory should be included in the `src` option
    /// (e.g., `src = ["lib"]`), such that when resolving imports, `my_package.foo`
    /// is considered first-party.
    ///
    /// This field supports globs. For example, if you have a series of Python
    /// packages in a `python_modules` directory, `src = ["python_modules/*"]`
    /// would expand to incorporate all packages in that directory. User home
    /// directory and environment variables will also be expanded.
    #[option(
        default = r#"[".", "src"]"#,
        value_type = "list[str]",
        example = r#"
            # Allow imports relative to the "src" and "test" directories.
            src = ["src", "test"]
        "#
    )]
    pub src: Option<Vec<String>>,

    // Global Formatting options
    /// The line length to use when enforcing long-lines violations (like `E501`)
    /// and at which `isort` and the formatter prefers to wrap lines.
    ///
    /// The length is determined by the number of characters per line, except for lines containing East Asian characters or emojis.
    /// For these lines, the [unicode width](https://unicode.org/reports/tr11/) of each character is added up to determine the length.
    ///
    /// The value must be greater than `0` and less than or equal to `320`.
    ///
    /// Note: While the formatter will attempt to format lines such that they remain
    /// within the `line-length`, it isn't a hard upper bound, and formatted lines may
    /// exceed the `line-length`.
    ///
    /// See [`pycodestyle.max-line-length`](#lint_pycodestyle_max-line-length) to configure different lengths for `E501` and the formatter.
    #[option(
        default = "88",
        value_type = "int",
        example = r#"
        # Allow lines to be as long as 120.
        line-length = 120
        "#
    )]
    pub line_length: Option<LineLength>,

    /// The number of spaces per indentation level (tab).
    ///
    /// Used by the formatter and when enforcing long-line violations (like `E501`) to determine the visual
    /// width of a tab.
    ///
    /// This option changes the number of spaces the formatter inserts when
    /// using soft-tabs (`indent-style = space`).
    ///
    /// PEP 8 recommends using 4 spaces per [indentation level](https://peps.python.org/pep-0008/#indentation).
    #[option(
        default = "4",
        value_type = "int",
        example = r#"
            indent-width = 2
        "#
    )]
    pub indent_width: Option<IndentWidth>,

    /// The number of spaces a tab is equal to when enforcing long-line violations (like `E501`)
    /// or formatting code with the formatter.
    ///
    /// This option changes the number of spaces inserted by the formatter when
    /// using soft-tabs (`indent-style = space`).
    #[deprecated(
        since = "0.1.2",
        note = "The `tab-size` option has been renamed to `indent-width` to emphasize that it configures the indentation used by the formatter as well as the tab width. Please update your configuration to use `indent-width = <value>` instead."
    )]
    pub tab_size: Option<IndentWidth>,

    #[option_group]
    pub lint: Option<LintOptions>,

    /// The lint sections specified at the top level.
    #[serde(flatten)]
    pub lint_top_level: DeprecatedTopLevelLintOptions,

    /// Options to configure code formatting.
    #[option_group]
    pub format: Option<FormatOptions>,

    /// Options to configure import map generation.
    #[option_group]
    pub analyze: Option<AnalyzeOptions>,
}

/// Configures how Ruff checks your code.
///
/// Options specified in the `lint` section take precedence over the deprecated top-level settings.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Default, OptionsMetadata, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct LintOptions {
    #[serde(flatten)]
    pub common: LintCommonOptions,

    /// A list of file patterns to exclude from linting in addition to the files excluded globally (see [`exclude`](#exclude), and [`extend-exclude`](#extend-exclude)).
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// - Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// - Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            exclude = ["generated"]
        "#
    )]
    pub exclude: Option<Vec<String>>,

    /// Options for the `ruff` plugin
    #[option_group]
    pub ruff: Option<RuffOptions>,

    /// Whether to enable preview mode. When preview mode is enabled, Ruff will
    /// use unstable rules and fixes.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable preview features.
            preview = true
        "#
    )]
    pub preview: Option<bool>,
}

/// Newtype wrapper for [`LintCommonOptions`] that allows customizing the JSON schema and omitting the fields from the [`OptionsMetadata`].
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeprecatedTopLevelLintOptions(pub LintCommonOptions);

impl OptionsMetadata for DeprecatedTopLevelLintOptions {
    fn record(_visit: &mut dyn Visit) {
        // Intentionally empty. Omit all fields from the documentation and instead promote the options under the `lint.` section.
        // This doesn't create an empty 'common' option  because the field in the `Options` struct is marked with `#[serde(flatten)]`.
        // Meaning, the code here flattens no-properties into the parent, which is what we want.
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for DeprecatedTopLevelLintOptions {
    fn schema_name() -> String {
        "DeprecatedTopLevelLintOptions".to_owned()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(concat!(
            module_path!(),
            "::",
            "DeprecatedTopLevelLintOptions"
        ))
    }
    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::Schema;

        let common_schema = LintCommonOptions::json_schema(gen);
        let mut schema_obj = common_schema.into_object();

        if let Some(object) = schema_obj.object.as_mut() {
            for property in object.properties.values_mut() {
                if let Schema::Object(property_object) = property {
                    if let Some(metadata) = &mut property_object.metadata {
                        metadata.deprecated = true;
                    } else {
                        property_object.metadata = Some(Box::new(schemars::schema::Metadata {
                            deprecated: true,
                            ..schemars::schema::Metadata::default()
                        }));
                    }
                }
            }
        }

        Schema::Object(schema_obj)
    }
}

// Note: This struct should be inlined into [`LintOptions`] once support for the top-level lint settings
// is removed.
// Don't add any new options to this struct. Add them to [`LintOptions`] directly to avoid exposing them in the
// global settings.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(
    Clone, Debug, PartialEq, Eq, Default, OptionsMetadata, CombineOptions, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct LintCommonOptions {
    // WARNING: Don't add new options to this type. Add them to `LintOptions` instead.
    /// A list of allowed "confusable" Unicode characters to ignore when
    /// enforcing `RUF001`, `RUF002`, and `RUF003`.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow minus-sign (U+2212), greek-small-letter-rho (U+03C1), and the asterisk-operator (U+2217),
            # which could be confused for "-", "p", and "*", respectively.
            allowed-confusables = ["−", "ρ", "∗"]
        "#
    )]
    pub allowed_confusables: Option<Vec<char>>,

    /// A regular expression used to identify "dummy" variables, or those which
    /// should be ignored when enforcing (e.g.) unused-variable rules. The
    /// default expression matches `_`, `__`, and `_var`, but not `_var_`.
    #[option(
        default = r#""^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$""#,
        value_type = "str",
        example = r#"
            # Only ignore variables named "_".
            dummy-variable-rgx = "^_$"
        "#
    )]
    pub dummy_variable_rgx: Option<String>,

    /// A list of rule codes or prefixes to ignore, in addition to those
    /// specified by `ignore`.
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Skip unused variable rules (`F841`).
            extend-ignore = ["F841"]
        "#
    )]
    #[deprecated(
        note = "The `extend-ignore` option is now interchangeable with [`ignore`](#lint_ignore). Please update your configuration to use the [`ignore`](#lint_ignore) option instead."
    )]
    pub extend_ignore: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes to enable, in addition to those
    /// specified by [`select`](#lint_select).
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # On top of the default `select` (`E4`, E7`, `E9`, and `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            extend-select = ["B", "Q"]
        "#
    )]
    pub extend_select: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes to consider fixable, in addition to those
    /// specified by [`fixable`](#lint_fixable).
    #[option(
        default = r#"[]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # Enable fix for flake8-bugbear (`B`), on top of any rules specified by `fixable`.
            extend-fixable = ["B"]
        "#
    )]
    pub extend_fixable: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes to consider non-auto-fixable, in addition to those
    /// specified by [`unfixable`](#lint_unfixable).
    #[deprecated(
        note = "The `extend-unfixable` option is now interchangeable with [`unfixable`](#lint_unfixable). Please update your configuration to use the `unfixable` option instead."
    )]
    pub extend_unfixable: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes that are unsupported by Ruff, but should be
    /// preserved when (e.g.) validating `# noqa` directives. Useful for
    /// retaining `# noqa` directives that cover plugins not yet implemented
    /// by Ruff.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # Avoiding flagging (and removing) any codes starting with `V` from any
            # `# noqa` directives, despite Ruff's lack of support for `vulture`.
            external = ["V"]
        "#
    )]
    pub external: Option<Vec<String>>,

    /// A list of rule codes or prefixes to consider fixable. By default,
    /// all rules are considered fixable.
    #[option(
        default = r#"["ALL"]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # Only allow fix behavior for `E` and `F` rules.
            fixable = ["E", "F"]
        "#
    )]
    pub fixable: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes to ignore. Prefixes can specify exact
    /// rules (like `F841`), entire categories (like `F`), or anything in
    /// between.
    ///
    /// When breaking ties between enabled and disabled rules (via `select` and
    /// `ignore`, respectively), more specific prefixes override less
    /// specific prefixes.
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Skip unused variable rules (`F841`).
            ignore = ["F841"]
        "#
    )]
    pub ignore: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes for which unsafe fixes should be considered
    /// safe.
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Allow applying all unsafe fixes in the `E` rules and `F401` without the `--unsafe-fixes` flag
            extend-safe-fixes = ["E", "F401"]
        "#
    )]
    pub extend_safe_fixes: Option<Vec<RuleSelector>>,

    /// A list of rule codes or prefixes for which safe fixes should be considered
    /// unsafe.
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Require the `--unsafe-fixes` flag when fixing the `E` rules and `F401`
            extend-unsafe-fixes = ["E", "F401"]
        "#
    )]
    pub extend_unsafe_fixes: Option<Vec<RuleSelector>>,

    /// Avoid automatically removing unused imports in `__init__.py` files. Such
    /// imports will still be flagged, but with a dedicated message suggesting
    /// that the import is either added to the module's `__all__` symbol, or
    /// re-exported with a redundant alias (e.g., `import os as os`).
    ///
    /// This option is enabled by default, but you can opt-in to removal of imports
    /// via an unsafe fix.
    #[option(
        default = "true",
        value_type = "bool",
        example = r#"
            ignore-init-module-imports = false
        "#
    )]
    #[deprecated(
        since = "0.4.4",
        note = "`ignore-init-module-imports` will be removed in a future version because F401 now recommends appropriate fixes for unused imports in `__init__.py` (currently in preview mode). See documentation for more information and please update your configuration."
    )]
    pub ignore_init_module_imports: Option<bool>,

    /// A list of objects that should be treated equivalently to a
    /// `logging.Logger` object.
    ///
    /// This is useful for ensuring proper diagnostics (e.g., to identify
    /// `logging` deprecations and other best-practices) for projects that
    /// re-export a `logging.Logger` object from a common module.
    ///
    /// For example, if you have a module `logging_setup.py` with the following
    /// contents:
    /// ```python
    /// import logging
    ///
    /// logger = logging.getLogger(__name__)
    /// ```
    ///
    /// Adding `"logging_setup.logger"` to `logger-objects` will ensure that
    /// `logging_setup.logger` is treated as a `logging.Logger` object when
    /// imported from other modules (e.g., `from logging_setup import logger`).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"logger-objects = ["logging_setup.logger"]"#
    )]
    pub logger_objects: Option<Vec<String>>,

    /// A list of rule codes or prefixes to enable. Prefixes can specify exact
    /// rules (like `F841`), entire categories (like `F`), or anything in
    /// between.
    ///
    /// When breaking ties between enabled and disabled rules (via `select` and
    /// `ignore`, respectively), more specific prefixes override less
    /// specific prefixes.
    #[option(
        default = r#"["E4", "E7", "E9", "F"]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # On top of the defaults (`E4`, E7`, `E9`, and `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            select = ["E4", "E7", "E9", "F", "B", "Q"]
        "#
    )]
    pub select: Option<Vec<RuleSelector>>,

    /// Whether to require exact codes to select preview rules. When enabled,
    /// preview rules will not be selected by prefixes — the full code of each
    /// preview rule will be required to enable the rule.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Require explicit selection of preview rules.
            explicit-preview-rules = true
        "#
    )]
    pub explicit_preview_rules: Option<bool>,

    /// A list of task tags to recognize (e.g., "TODO", "FIXME", "XXX").
    ///
    /// Comments starting with these tags will be ignored by commented-out code
    /// detection (`ERA`), and skipped by line-length rules (`E501`) if
    /// [`ignore-overlong-task-comments`](#lint_pycodestyle_ignore-overlong-task-comments) is set to `true`.
    #[option(
        default = r#"["TODO", "FIXME", "XXX"]"#,
        value_type = "list[str]",
        example = r#"
            task-tags = ["HACK"]
        "#
    )]
    pub task_tags: Option<Vec<String>>,

    /// A list of modules whose exports should be treated equivalently to
    /// members of the `typing` module.
    ///
    /// This is useful for ensuring proper type annotation inference for
    /// projects that re-export `typing` and `typing_extensions` members
    /// from a compatibility module. If omitted, any members imported from
    /// modules apart from `typing` and `typing_extensions` will be treated
    /// as ordinary Python objects.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"typing-modules = ["airflow.typing_compat"]"#
    )]
    pub typing_modules: Option<Vec<String>>,

    /// A list of modules which is allowed even thought they are not used
    /// in the code.
    ///
    /// This is useful when a module has a side effect when imported.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"allowed-unused-imports = ["hvplot.pandas"]"#
    )]
    pub allowed_unused_imports: Option<Vec<String>>,
    /// A list of rule codes or prefixes to consider non-fixable.
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Disable fix for unused imports (`F401`).
            unfixable = ["F401"]
        "#
    )]
    pub unfixable: Option<Vec<RuleSelector>>,

    // WARNING: Don't add new options to this type. Add them to `LintOptions` instead.
    /// Options for the `flake8-annotations` plugin.
    #[option_group]
    pub flake8_annotations: Option<Flake8AnnotationsOptions>,

    /// Options for the `flake8-bandit` plugin.
    #[option_group]
    pub flake8_bandit: Option<Flake8BanditOptions>,

    /// Options for the `flake8-boolean-trap` plugin.
    #[option_group]
    pub flake8_boolean_trap: Option<Flake8BooleanTrapOptions>,

    /// Options for the `flake8-bugbear` plugin.
    #[option_group]
    pub flake8_bugbear: Option<Flake8BugbearOptions>,

    /// Options for the `flake8-builtins` plugin.
    #[option_group]
    pub flake8_builtins: Option<Flake8BuiltinsOptions>,

    /// Options for the `flake8-comprehensions` plugin.
    #[option_group]
    pub flake8_comprehensions: Option<Flake8ComprehensionsOptions>,

    /// Options for the `flake8-copyright` plugin.
    #[option_group]
    pub flake8_copyright: Option<Flake8CopyrightOptions>,

    /// Options for the `flake8-errmsg` plugin.
    #[option_group]
    pub flake8_errmsg: Option<Flake8ErrMsgOptions>,

    /// Options for the `flake8-quotes` plugin.
    #[option_group]
    pub flake8_quotes: Option<Flake8QuotesOptions>,

    /// Options for the `flake8_self` plugin.
    #[option_group]
    pub flake8_self: Option<Flake8SelfOptions>,

    /// Options for the `flake8-tidy-imports` plugin.
    #[option_group]
    pub flake8_tidy_imports: Option<Flake8TidyImportsOptions>,

    /// Options for the `flake8-type-checking` plugin.
    #[option_group]
    pub flake8_type_checking: Option<Flake8TypeCheckingOptions>,

    /// Options for the `flake8-gettext` plugin.
    #[option_group]
    pub flake8_gettext: Option<Flake8GetTextOptions>,

    /// Options for the `flake8-implicit-str-concat` plugin.
    #[option_group]
    pub flake8_implicit_str_concat: Option<Flake8ImplicitStrConcatOptions>,

    /// Options for the `flake8-import-conventions` plugin.
    #[option_group]
    pub flake8_import_conventions: Option<Flake8ImportConventionsOptions>,

    /// Options for the `flake8-pytest-style` plugin.
    #[option_group]
    pub flake8_pytest_style: Option<Flake8PytestStyleOptions>,

    /// Options for the `flake8-unused-arguments` plugin.
    #[option_group]
    pub flake8_unused_arguments: Option<Flake8UnusedArgumentsOptions>,

    /// Options for the `isort` plugin.
    #[option_group]
    pub isort: Option<IsortOptions>,

    /// Options for the `mccabe` plugin.
    #[option_group]
    pub mccabe: Option<McCabeOptions>,

    /// Options for the `pep8-naming` plugin.
    #[option_group]
    pub pep8_naming: Option<Pep8NamingOptions>,

    /// Options for the `pycodestyle` plugin.
    #[option_group]
    pub pycodestyle: Option<PycodestyleOptions>,

    /// Options for the `pydocstyle` plugin.
    #[option_group]
    pub pydocstyle: Option<PydocstyleOptions>,

    /// Options for the `pyflakes` plugin.
    #[option_group]
    pub pyflakes: Option<PyflakesOptions>,

    /// Options for the `pylint` plugin.
    #[option_group]
    pub pylint: Option<PylintOptions>,

    /// Options for the `pyupgrade` plugin.
    #[option_group]
    pub pyupgrade: Option<PyUpgradeOptions>,

    // WARNING: Don't add new options to this type. Add them to `LintOptions` instead.

    // Tables are required to go last.
    /// A list of mappings from file pattern to rule codes or prefixes to
    /// exclude, when considering any matching files. An initial '!' negates
    /// the file pattern.
    #[option(
        default = "{}",
        value_type = "dict[str, list[RuleSelector]]",
        scope = "per-file-ignores",
        example = r#"
            # Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
            "__init__.py" = ["E402"]
            "path/to/file.py" = ["E402"]
            # Ignore `D` rules everywhere except for the `src/` directory.
            "!src/**.py" = ["D"]
        "#
    )]
    pub per_file_ignores: Option<FxHashMap<String, Vec<RuleSelector>>>,

    /// A list of mappings from file pattern to rule codes or prefixes to
    /// exclude, in addition to any rules excluded by [`per-file-ignores`](#lint_per-file-ignores).
    #[option(
        default = "{}",
        value_type = "dict[str, list[RuleSelector]]",
        scope = "extend-per-file-ignores",
        example = r#"
            # Also ignore `E402` in all `__init__.py` files.
            "__init__.py" = ["E402"]
        "#
    )]
    pub extend_per_file_ignores: Option<FxHashMap<String, Vec<RuleSelector>>>,
    // WARNING: Don't add new options to this type. Add them to `LintOptions` instead.
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(
    Clone, Debug, PartialEq, Eq, Default, OptionsMetadata, CombineOptions, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Flake8AnnotationsOptions {
    /// Whether to allow the omission of a return type hint for `__init__` if at
    /// least one argument is annotated.
    #[option(
        default = "false",
        value_type = "bool",
        example = "mypy-init-return = true"
    )]
    pub mypy_init_return: Option<bool>,

    /// Whether to suppress `ANN000`-level violations for arguments matching the
    /// "dummy" variable regex (like `_`).
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-dummy-args = true"
    )]
    pub suppress_dummy_args: Option<bool>,

    /// Whether to suppress `ANN200`-level violations for functions that meet
    /// either of the following criteria:
    ///
    /// - Contain no `return` statement.
    /// - Explicit `return` statement(s) all return `None` (explicitly or
    ///   implicitly).
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-none-returning = true"
    )]
    pub suppress_none_returning: Option<bool>,

    /// Whether to suppress `ANN401` for dynamically typed `*args` and
    /// `**kwargs` arguments.
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-star-arg-any = true"
    )]
    pub allow_star_arg_any: Option<bool>,

    /// Whether to suppress `ANN*` rules for any declaration
    /// that hasn't been typed at all.
    /// This makes it easier to gradually add types to a codebase.
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-fully-untyped = true"
    )]
    pub ignore_fully_untyped: Option<bool>,
}

impl Flake8AnnotationsOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_annotations::settings::Settings {
        ruff_linter::rules::flake8_annotations::settings::Settings {
            mypy_init_return: self.mypy_init_return.unwrap_or(false),
            suppress_dummy_args: self.suppress_dummy_args.unwrap_or(false),
            suppress_none_returning: self.suppress_none_returning.unwrap_or(false),
            allow_star_arg_any: self.allow_star_arg_any.unwrap_or(false),
            ignore_fully_untyped: self.ignore_fully_untyped.unwrap_or(false),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BanditOptions {
    /// A list of directories to consider temporary.
    #[option(
        default = "[\"/tmp\", \"/var/tmp\", \"/dev/shm\"]",
        value_type = "list[str]",
        example = "hardcoded-tmp-directory = [\"/foo/bar\"]"
    )]
    pub hardcoded_tmp_directory: Option<Vec<String>>,

    /// A list of directories to consider temporary, in addition to those
    /// specified by [`hardcoded-tmp-directory`](#lint_flake8-bandit_hardcoded-tmp-directory).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "hardcoded-tmp-directory-extend = [\"/foo/bar\"]"
    )]
    pub hardcoded_tmp_directory_extend: Option<Vec<String>>,

    /// Whether to disallow `try`-`except`-`pass` (`S110`) for specific
    /// exception types. By default, `try`-`except`-`pass` is only
    /// disallowed for `Exception` and `BaseException`.
    #[option(
        default = "false",
        value_type = "bool",
        example = "check-typed-exception = true"
    )]
    pub check_typed_exception: Option<bool>,
}

impl Flake8BanditOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_bandit::settings::Settings {
        ruff_linter::rules::flake8_bandit::settings::Settings {
            hardcoded_tmp_directory: self
                .hardcoded_tmp_directory
                .unwrap_or_else(ruff_linter::rules::flake8_bandit::settings::default_tmp_dirs)
                .into_iter()
                .chain(self.hardcoded_tmp_directory_extend.unwrap_or_default())
                .collect(),
            check_typed_exception: self.check_typed_exception.unwrap_or(false),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BooleanTrapOptions {
    /// Additional callable functions with which to allow boolean traps.
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `pydantic.Field`, rather than
    /// `Field`).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "extend-allowed-calls = [\"pydantic.Field\", \"django.db.models.Value\"]"
    )]
    pub extend_allowed_calls: Option<Vec<String>>,
}

impl Flake8BooleanTrapOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_boolean_trap::settings::Settings {
        ruff_linter::rules::flake8_boolean_trap::settings::Settings {
            extend_allowed_calls: self.extend_allowed_calls.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BugbearOptions {
    /// Additional callable functions to consider "immutable" when evaluating, e.g., the
    /// `function-call-in-default-argument` rule (`B008`) or `function-call-in-dataclass-defaults`
    /// rule (`RUF009`).
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `fastapi.Query`, rather than
    /// `Query`).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow default arguments like, e.g., `data: List[str] = fastapi.Query(None)`.
            extend-immutable-calls = ["fastapi.Depends", "fastapi.Query"]
        "#
    )]
    pub extend_immutable_calls: Option<Vec<String>>,
}

impl Flake8BugbearOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_bugbear::settings::Settings {
        ruff_linter::rules::flake8_bugbear::settings::Settings {
            extend_immutable_calls: self.extend_immutable_calls.unwrap_or_default(),
        }
    }
}
#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BuiltinsOptions {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "builtins-ignorelist = [\"id\"]"
    )]
    /// Ignore list of builtins.
    pub builtins_ignorelist: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "builtins-allowed-modules = [\"id\"]"
    )]
    /// List of builtin module names to allow.
    pub builtins_allowed_modules: Option<Vec<String>>,
}

impl Flake8BuiltinsOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_builtins::settings::Settings {
        ruff_linter::rules::flake8_builtins::settings::Settings {
            builtins_ignorelist: self.builtins_ignorelist.unwrap_or_default(),
            builtins_allowed_modules: self.builtins_allowed_modules.unwrap_or_default(),
        }
    }
}
#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ComprehensionsOptions {
    /// Allow `dict` calls that make use of keyword arguments (e.g., `dict(a=1, b=2)`).
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-dict-calls-with-keyword-arguments = true"
    )]
    pub allow_dict_calls_with_keyword_arguments: Option<bool>,
}

impl Flake8ComprehensionsOptions {
    pub fn into_settings(self) -> ruff_linter::rules::flake8_comprehensions::settings::Settings {
        ruff_linter::rules::flake8_comprehensions::settings::Settings {
            allow_dict_calls_with_keyword_arguments: self
                .allow_dict_calls_with_keyword_arguments
                .unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8CopyrightOptions {
    /// The regular expression used to match the copyright notice, compiled
    /// with the [`regex`](https://docs.rs/regex/latest/regex/) crate.
    /// Defaults to `(?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*`, which matches
    /// the following:
    ///
    /// - `Copyright 2023`
    /// - `Copyright (C) 2023`
    /// - `Copyright 2021-2023`
    /// - `Copyright (C) 2021-2023`
    /// - `Copyright (C) 2021, 2023`
    #[option(
        default = r#""(?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*""#,
        value_type = "str",
        example = r#"notice-rgx = "(?i)Copyright \\(C\\) \\d{4}""#
    )]
    pub notice_rgx: Option<String>,

    /// Author to enforce within the copyright notice. If provided, the
    /// author must be present immediately following the copyright notice.
    #[option(default = "null", value_type = "str", example = r#"author = "Ruff""#)]
    pub author: Option<String>,

    /// A minimum file size (in bytes) required for a copyright notice to
    /// be enforced. By default, all files are validated.
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Avoid enforcing a header on files smaller than 1024 bytes.
            min-file-size = 1024
        "#
    )]
    pub min_file_size: Option<usize>,
}

impl Flake8CopyrightOptions {
    pub fn try_into_settings(self) -> anyhow::Result<flake8_copyright::settings::Settings> {
        Ok(flake8_copyright::settings::Settings {
            notice_rgx: self
                .notice_rgx
                .map(|pattern| Regex::new(&pattern))
                .transpose()?
                .unwrap_or_else(|| flake8_copyright::settings::COPYRIGHT.clone()),
            author: self.author,
            min_file_size: self.min_file_size.unwrap_or_default(),
        })
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ErrMsgOptions {
    /// Maximum string length for string literals in exception messages.
    #[option(default = "0", value_type = "int", example = "max-string-length = 20")]
    pub max_string_length: Option<usize>,
}

impl Flake8ErrMsgOptions {
    pub fn into_settings(self) -> flake8_errmsg::settings::Settings {
        flake8_errmsg::settings::Settings {
            max_string_length: self.max_string_length.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8GetTextOptions {
    /// The function names to consider as internationalization calls.
    #[option(
        default = r#"["_", "gettext", "ngettext"]"#,
        value_type = "list[str]",
        example = r#"function-names = ["_", "gettext", "ngettext", "ugettetxt"]"#
    )]
    pub function_names: Option<Vec<Name>>,

    /// Additional function names to consider as internationalization calls, in addition to those
    /// included in [`function-names`](#lint_flake8-gettext_function-names).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-function-names = ["ugettetxt"]"#
    )]
    pub extend_function_names: Option<Vec<Name>>,
}

impl Flake8GetTextOptions {
    pub fn into_settings(self) -> flake8_gettext::settings::Settings {
        flake8_gettext::settings::Settings {
            functions_names: self
                .function_names
                .unwrap_or_else(flake8_gettext::settings::default_func_names)
                .into_iter()
                .chain(self.extend_function_names.unwrap_or_default())
                .collect(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ImplicitStrConcatOptions {
    /// Whether to allow implicit string concatenations for multiline strings.
    /// By default, implicit concatenations of multiline strings are
    /// allowed (but continuation lines, delimited with a backslash, are
    /// prohibited).
    ///
    /// Setting `allow-multiline = false` will automatically disable the
    /// `explicit-string-concatenation` (`ISC003`) rule. Otherwise, both
    /// implicit and explicit multiline string concatenations would be seen
    /// as violations, making it impossible to write a linter-compliant multiline
    /// string.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            allow-multiline = false
        "#
    )]
    pub allow_multiline: Option<bool>,
}

impl Flake8ImplicitStrConcatOptions {
    pub fn into_settings(self) -> flake8_implicit_str_concat::settings::Settings {
        flake8_implicit_str_concat::settings::Settings {
            allow_multiline: self.allow_multiline.unwrap_or(true),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ImportConventionsOptions {
    /// The conventional aliases for imports. These aliases can be extended by
    /// the [`extend-aliases`](#lint_flake8-import-conventions_extend-aliases) option.
    #[option(
        default = r#"{"altair": "alt", "matplotlib": "mpl", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns", "tensorflow": "tf", "tkinter":  "tk", "holoviews": "hv", "panel": "pn", "plotly.express": "px", "polars": "pl", "pyarrow": "pa", "xml.etree.ElementTree": "ET"}"#,
        value_type = "dict[str, str]",
        scope = "aliases",
        example = r#"
            # Declare the default aliases.
            altair = "alt"
            "matplotlib.pyplot" = "plt"
            numpy = "np"
            pandas = "pd"
            seaborn = "sns"
            scipy = "sp"
        "#
    )]
    pub aliases: Option<FxHashMap<String, String>>,

    /// A mapping from module to conventional import alias. These aliases will
    /// be added to the [`aliases`](#lint_flake8-import-conventions_aliases) mapping.
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, str]",
        scope = "extend-aliases",
        example = r#"
            # Declare a custom alias for the `dask` module.
            "dask.dataframe" = "dd"
        "#
    )]
    pub extend_aliases: Option<FxHashMap<String, String>>,

    /// A mapping from module to its banned import aliases.
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, list[str]]",
        scope = "banned-aliases",
        example = r#"
            # Declare the banned aliases.
            "tensorflow.keras.backend" = ["K"]
    "#
    )]
    pub banned_aliases: Option<FxHashMap<String, BannedAliases>>,

    /// A list of modules that should not be imported from using the
    /// `from ... import ...` syntax.
    ///
    /// For example, given `banned-from = ["pandas"]`, `from pandas import DataFrame`
    /// would be disallowed, while `import pandas` would be allowed.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Declare the banned `from` imports.
            banned-from = ["typing"]
    "#
    )]
    pub banned_from: Option<FxHashSet<String>>,
}

impl Flake8ImportConventionsOptions {
    pub fn into_settings(self) -> flake8_import_conventions::settings::Settings {
        let mut aliases = match self.aliases {
            Some(options_aliases) => options_aliases,
            None => flake8_import_conventions::settings::default_aliases(),
        };
        if let Some(extend_aliases) = self.extend_aliases {
            aliases.extend(extend_aliases);
        }

        flake8_import_conventions::settings::Settings {
            aliases,
            banned_aliases: self.banned_aliases.unwrap_or_default(),
            banned_from: self.banned_from.unwrap_or_default(),
        }
    }
}
#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8PytestStyleOptions {
    /// Boolean flag specifying whether `@pytest.fixture()` without parameters
    /// should have parentheses. If the option is set to `false` (the default),
    /// `@pytest.fixture` is valid and `@pytest.fixture()` is invalid. If set
    /// to `true`, `@pytest.fixture()` is valid and `@pytest.fixture` is
    /// invalid.
    #[option(
        default = "false",
        value_type = "bool",
        example = "fixture-parentheses = true"
    )]
    pub fixture_parentheses: Option<bool>,

    /// Expected type for multiple argument names in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `csv` — a comma-separated list, e.g.
    ///   `@pytest.mark.parametrize("name1,name2", ...)`
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(("name1", "name2"), ...)`
    /// - `list` — e.g. `@pytest.mark.parametrize(["name1", "name2"], ...)`
    #[option(
        default = "tuple",
        value_type = r#""csv" | "tuple" | "list""#,
        example = "parametrize-names-type = \"list\""
    )]
    pub parametrize_names_type: Option<types::ParametrizeNameType>,

    /// Expected type for the list of values rows in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `tuple` — e.g. `@pytest.mark.parametrize("name", (1, 2, 3))`
    /// - `list` (default) — e.g. `@pytest.mark.parametrize("name", [1, 2, 3])`
    #[option(
        default = "list",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-type = \"tuple\""
    )]
    pub parametrize_values_type: Option<types::ParametrizeValuesType>,

    /// Expected type for each row of values in `@pytest.mark.parametrize` in
    /// case of multiple parameters. The following values are supported:
    ///
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(("name1", "name2"), [(1, 2), (3, 4)])`
    /// - `list` — e.g.
    ///   `@pytest.mark.parametrize(("name1", "name2"), [[1, 2], [3, 4]])`
    #[option(
        default = "tuple",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-row-type = \"list\""
    )]
    pub parametrize_values_row_type: Option<types::ParametrizeValuesRowType>,

    /// List of exception names that require a match= parameter in a
    /// `pytest.raises()` call.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"["BaseException", "Exception", "ValueError", "OSError", "IOError", "EnvironmentError", "socket.error"]"#,
        value_type = "list[str]",
        example = "raises-require-match-for = [\"requests.RequestException\"]"
    )]
    pub raises_require_match_for: Option<Vec<String>>,

    /// List of additional exception names that require a match= parameter in a
    /// `pytest.raises()` call. This extends the default list of exceptions
    /// that require a match= parameter.
    /// This option is useful if you want to extend the default list of
    /// exceptions that require a match= parameter without having to specify
    /// the entire list.
    /// Note that this option does not remove any exceptions from the default
    /// list.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "raises-extend-require-match-for = [\"requests.RequestException\"]"
    )]
    pub raises_extend_require_match_for: Option<Vec<String>>,

    /// Boolean flag specifying whether `@pytest.mark.foo()` without parameters
    /// should have parentheses. If the option is set to `false` (the
    /// default), `@pytest.mark.foo` is valid and `@pytest.mark.foo()` is
    /// invalid. If set to `true`, `@pytest.mark.foo()` is valid and
    /// `@pytest.mark.foo` is invalid.
    #[option(
        default = "false",
        value_type = "bool",
        example = "mark-parentheses = true"
    )]
    pub mark_parentheses: Option<bool>,
}

impl Flake8PytestStyleOptions {
    pub fn try_into_settings(
        self,
        preview: PreviewMode,
    ) -> anyhow::Result<flake8_pytest_style::settings::Settings> {
        Ok(flake8_pytest_style::settings::Settings {
            fixture_parentheses: self.fixture_parentheses.unwrap_or(preview.is_disabled()),
            parametrize_names_type: self.parametrize_names_type.unwrap_or_default(),
            parametrize_values_type: self.parametrize_values_type.unwrap_or_default(),
            parametrize_values_row_type: self.parametrize_values_row_type.unwrap_or_default(),
            raises_require_match_for: self
                .raises_require_match_for
                .map(|patterns| {
                    patterns
                        .into_iter()
                        .map(|pattern| IdentifierPattern::new(&pattern))
                        .collect()
                })
                .transpose()
                .map_err(SettingsError::InvalidRaisesRequireMatchFor)?
                .unwrap_or_else(flake8_pytest_style::settings::default_broad_exceptions),
            raises_extend_require_match_for: self
                .raises_extend_require_match_for
                .map(|patterns| {
                    patterns
                        .into_iter()
                        .map(|pattern| IdentifierPattern::new(&pattern))
                        .collect()
                })
                .transpose()
                .map_err(SettingsError::InvalidRaisesExtendRequireMatchFor)?
                .unwrap_or_default(),
            mark_parentheses: self.mark_parentheses.unwrap_or(preview.is_disabled()),
        })
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8QuotesOptions {
    /// Quote style to prefer for inline strings (either "single" or
    /// "double").
    ///
    /// When using the formatter, ensure that [`format.quote-style`](#format_quote-style) is set to
    /// the same preferred quote style.
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            inline-quotes = "single"
        "#
    )]
    pub inline_quotes: Option<Quote>,

    /// Quote style to prefer for multiline strings (either "single" or
    /// "double").
    ///
    /// When using the formatter, only "double" is compatible, as the formatter
    /// enforces double quotes for multiline strings.
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            multiline-quotes = "single"
        "#
    )]
    pub multiline_quotes: Option<Quote>,

    /// Quote style to prefer for docstrings (either "single" or "double").
    ///
    /// When using the formatter, only "double" is compatible, as the formatter
    /// enforces double quotes for docstrings strings.
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            docstring-quotes = "single"
        "#
    )]
    pub docstring_quotes: Option<Quote>,

    /// Whether to avoid using single quotes if a string contains single quotes,
    /// or vice-versa with double quotes, as per [PEP 8](https://peps.python.org/pep-0008/#string-quotes).
    /// This minimizes the need to escape quotation marks within strings.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            # Don't bother trying to avoid escapes.
            avoid-escape = false
        "#
    )]
    pub avoid_escape: Option<bool>,
}

impl Flake8QuotesOptions {
    pub fn into_settings(self) -> flake8_quotes::settings::Settings {
        flake8_quotes::settings::Settings {
            inline_quotes: self.inline_quotes.unwrap_or_default(),
            multiline_quotes: self.multiline_quotes.unwrap_or_default(),
            docstring_quotes: self.docstring_quotes.unwrap_or_default(),
            avoid_escape: self.avoid_escape.unwrap_or(true),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8SelfOptions {
    /// A list of names to ignore when considering `flake8-self` violations.
    #[option(
        default = r#"["_make", "_asdict", "_replace", "_fields", "_field_defaults", "_name_", "_value_"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["_new"]
        "#
    )]
    pub ignore_names: Option<Vec<Name>>,

    /// Additional names to ignore when considering `flake8-self` violations,
    /// in addition to those included in [`ignore-names`](#lint_flake8-self_ignore-names).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-ignore-names = ["_base_manager", "_default_manager",  "_meta"]"#
    )]
    pub extend_ignore_names: Option<Vec<Name>>,
}

impl Flake8SelfOptions {
    pub fn into_settings(self) -> flake8_self::settings::Settings {
        let defaults = flake8_self::settings::Settings::default();
        flake8_self::settings::Settings {
            ignore_names: self
                .ignore_names
                .unwrap_or(defaults.ignore_names)
                .into_iter()
                .chain(self.extend_ignore_names.unwrap_or_default())
                .collect(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8TidyImportsOptions {
    /// Whether to ban all relative imports (`"all"`), or only those imports
    /// that extend into the parent module or beyond (`"parents"`).
    #[option(
        default = r#""parents""#,
        value_type = r#""parents" | "all""#,
        example = r#"
            # Disallow all relative imports.
            ban-relative-imports = "all"
        "#
    )]
    pub ban_relative_imports: Option<Strictness>,

    /// Specific modules or module members that may not be imported or accessed.
    /// Note that this rule is only meant to flag accidental uses,
    /// and can be circumvented via `eval` or `importlib`.
    #[option(
        default = r#"{}"#,
        value_type = r#"dict[str, { "msg": str }]"#,
        scope = "banned-api",
        example = r#"
            "cgi".msg = "The cgi module is deprecated, see https://peps.python.org/pep-0594/#cgi."
            "typing.TypedDict".msg = "Use typing_extensions.TypedDict instead."
        "#
    )]
    pub banned_api: Option<FxHashMap<String, ApiBan>>,

    /// List of specific modules that may not be imported at module level, and should instead be
    /// imported lazily (e.g., within a function definition, or an `if TYPE_CHECKING:`
    /// block, or some other nested context).
    #[option(
        default = r#"[]"#,
        value_type = r#"list[str]"#,
        example = r#"
            # Ban certain modules from being imported at module level, instead requiring
            # that they're imported lazily (e.g., within a function definition).
            banned-module-level-imports = ["torch", "tensorflow"]
        "#
    )]
    pub banned_module_level_imports: Option<Vec<String>>,
}

impl Flake8TidyImportsOptions {
    pub fn into_settings(self) -> flake8_tidy_imports::settings::Settings {
        flake8_tidy_imports::settings::Settings {
            ban_relative_imports: self.ban_relative_imports.unwrap_or(Strictness::Parents),
            banned_api: self.banned_api.unwrap_or_default(),
            banned_module_level_imports: self.banned_module_level_imports.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8TypeCheckingOptions {
    /// Enforce `TC001`, `TC002`, and `TC003` rules even when valid runtime imports
    /// are present for the same module.
    ///
    /// See flake8-type-checking's [strict](https://github.com/snok/flake8-type-checking#strict) option.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            strict = true
        "#
    )]
    pub strict: Option<bool>,

    /// Exempt certain modules from needing to be moved into type-checking
    /// blocks.
    #[option(
        default = "[\"typing\"]",
        value_type = "list[str]",
        example = r#"
            exempt-modules = ["typing", "typing_extensions"]
        "#
    )]
    pub exempt_modules: Option<Vec<String>>,

    /// Exempt classes that list any of the enumerated classes as a base class
    /// from needing to be moved into type-checking blocks.
    ///
    /// Common examples include Pydantic's `pydantic.BaseModel` and SQLAlchemy's
    /// `sqlalchemy.orm.DeclarativeBase`, but can also support user-defined
    /// classes that inherit from those base classes. For example, if you define
    /// a common `DeclarativeBase` subclass that's used throughout your project
    /// (e.g., `class Base(DeclarativeBase) ...` in `base.py`), you can add it to
    /// this list (`runtime-evaluated-base-classes = ["base.Base"]`) to exempt
    /// models from being moved into type-checking blocks.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-base-classes = ["pydantic.BaseModel", "sqlalchemy.orm.DeclarativeBase"]
        "#
    )]
    pub runtime_evaluated_base_classes: Option<Vec<String>>,

    /// Exempt classes and functions decorated with any of the enumerated
    /// decorators from being moved into type-checking blocks.
    ///
    /// Common examples include Pydantic's `@pydantic.validate_call` decorator
    /// (for functions) and attrs' `@attrs.define` decorator (for classes).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-decorators = ["pydantic.validate_call", "attrs.define"]
        "#
    )]
    pub runtime_evaluated_decorators: Option<Vec<String>>,

    /// Whether to add quotes around type annotations, if doing so would allow
    /// the corresponding import to be moved into a type-checking block.
    ///
    /// For example, in the following, Python requires that `Sequence` be
    /// available at runtime, despite the fact that it's only used in a type
    /// annotation:
    ///
    /// ```python
    /// from collections.abc import Sequence
    ///
    ///
    /// def func(value: Sequence[int]) -> None:
    ///     ...
    /// ```
    ///
    /// In other words, moving `from collections.abc import Sequence` into an
    /// `if TYPE_CHECKING:` block above would cause a runtime error, as the
    /// type would no longer be available at runtime.
    ///
    /// By default, Ruff will respect such runtime semantics and avoid moving
    /// the import to prevent such runtime errors.
    ///
    /// Setting `quote-annotations` to `true` will instruct Ruff to add quotes
    /// around the annotation (e.g., `"Sequence[int]"`), which in turn enables
    /// Ruff to move the import into an `if TYPE_CHECKING:` block, like so:
    ///
    /// ```python
    /// from typing import TYPE_CHECKING
    ///
    /// if TYPE_CHECKING:
    ///     from collections.abc import Sequence
    ///
    ///
    /// def func(value: "Sequence[int]") -> None:
    ///     ...
    /// ```
    ///
    /// Note that this setting has no effect when `from __future__ import annotations`
    /// is present, as `__future__` annotations are always treated equivalently
    /// to quoted annotations.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Add quotes around type annotations, if doing so would allow
            # an import to be moved into a type-checking block.
            quote-annotations = true
        "#
    )]
    pub quote_annotations: Option<bool>,
}

impl Flake8TypeCheckingOptions {
    pub fn into_settings(self) -> flake8_type_checking::settings::Settings {
        flake8_type_checking::settings::Settings {
            strict: self.strict.unwrap_or(false),
            exempt_modules: self
                .exempt_modules
                .unwrap_or_else(|| vec!["typing".to_string()]),
            runtime_required_base_classes: self.runtime_evaluated_base_classes.unwrap_or_default(),
            runtime_required_decorators: self.runtime_evaluated_decorators.unwrap_or_default(),
            quote_annotations: self.quote_annotations.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8UnusedArgumentsOptions {
    /// Whether to allow unused variadic arguments, like `*args` and `**kwargs`.
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-variadic-names = true"
    )]
    pub ignore_variadic_names: Option<bool>,
}

impl Flake8UnusedArgumentsOptions {
    pub fn into_settings(self) -> flake8_unused_arguments::settings::Settings {
        flake8_unused_arguments::settings::Settings {
            ignore_variadic_names: self.ignore_variadic_names.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IsortOptions {
    /// Force `import from` statements with multiple members and at least one
    /// alias (e.g., `import A as B`) to wrap such that every line contains
    /// exactly one member. For example, this formatting would be retained,
    /// rather than condensing to a single line:
    ///
    /// ```python
    /// from .utils import (
    ///     test_directory as test_directory,
    ///     test_id as test_id
    /// )
    /// ```
    ///
    /// Note that this setting is only effective when combined with
    /// `combine-as-imports = true`. When [`combine-as-imports`](#lint_isort_combine-as-imports) isn't
    /// enabled, every aliased `import from` will be given its own line, in
    /// which case, wrapping is not necessary.
    ///
    /// When using the formatter, ensure that [`format.skip-magic-trailing-comma`](#format_skip-magic-trailing-comma) is set to `false` (default)
    /// when enabling `force-wrap-aliases` to avoid that the formatter collapses members if they all fit on a single line.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-wrap-aliases = true
            combine-as-imports = true
        "#
    )]
    pub force_wrap_aliases: Option<bool>,

    /// Forces all from imports to appear on their own line.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"force-single-line = true"#
    )]
    pub force_single_line: Option<bool>,

    /// One or more modules to exclude from the single line rule.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            single-line-exclusions = ["os", "json"]
        "#
    )]
    pub single_line_exclusions: Option<Vec<String>>,

    /// Combines as imports on the same line. See isort's [`combine-as-imports`](https://pycqa.github.io/isort/docs/configuration/options.html#combine-as-imports)
    /// option.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            combine-as-imports = true
        "#
    )]
    pub combine_as_imports: Option<bool>,

    /// If a comma is placed after the last member in a multi-line import, then
    /// the imports will never be folded into one line.
    ///
    /// See isort's [`split-on-trailing-comma`](https://pycqa.github.io/isort/docs/configuration/options.html#split-on-trailing-comma) option.
    ///
    /// When using the formatter, ensure that [`format.skip-magic-trailing-comma`](#format_skip-magic-trailing-comma) is set to `false` (default) when enabling `split-on-trailing-comma`
    /// to avoid that the formatter removes the trailing commas.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            split-on-trailing-comma = false
        "#
    )]
    pub split_on_trailing_comma: Option<bool>,

    /// Order imports by type, which is determined by case, in addition to
    /// alphabetically.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            order-by-type = true
        "#
    )]
    pub order_by_type: Option<bool>,

    /// Don't sort straight-style imports (like `import sys`) before from-style
    /// imports (like `from itertools import groupby`). Instead, sort the
    /// imports by module, independent of import style.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-sort-within-sections = true
        "#
    )]
    pub force_sort_within_sections: Option<bool>,

    /// Sort imports taking into account case sensitivity.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            case-sensitive = true
        "#
    )]
    pub case_sensitive: Option<bool>,

    /// Force specific imports to the top of their appropriate section.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            force-to-top = ["src"]
        "#
    )]
    pub force_to_top: Option<Vec<String>>,

    /// A list of modules to consider first-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-first-party = ["src"]
        "#
    )]
    pub known_first_party: Option<Vec<String>>,

    /// A list of modules to consider third-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-third-party = ["src"]
        "#
    )]
    pub known_third_party: Option<Vec<String>>,

    /// A list of modules to consider being a local folder.
    /// Generally, this is reserved for relative imports (`from . import module`).
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-local-folder = ["src"]
        "#
    )]
    pub known_local_folder: Option<Vec<String>>,

    /// A list of modules to consider standard-library, in addition to those
    /// known to Ruff in advance.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            extra-standard-library = ["path"]
        "#
    )]
    pub extra_standard_library: Option<Vec<String>>,

    /// Whether to place "closer" imports (fewer `.` characters, most local)
    /// before "further" imports (more `.` characters, least local), or vice
    /// versa.
    ///
    /// The default ("furthest-to-closest") is equivalent to isort's
    /// [`reverse-relative`](https://pycqa.github.io/isort/docs/configuration/options.html#reverse-relative) default (`reverse-relative = false`); setting
    /// this to "closest-to-furthest" is equivalent to isort's
    /// `reverse-relative = true`.
    #[option(
        default = r#""furthest-to-closest""#,
        value_type = r#""furthest-to-closest" | "closest-to-furthest""#,
        example = r#"
            relative-imports-order = "closest-to-furthest"
        "#
    )]
    pub relative_imports_order: Option<RelativeImportsOrder>,

    /// Add the specified import line to all files.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            required-imports = ["from __future__ import annotations"]
        "#
    )]
    pub required_imports: Option<Vec<NameImports>>,

    /// An override list of tokens to always recognize as a Class for
    /// [`order-by-type`](#lint_isort_order-by-type) regardless of casing.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            classes = ["SVC"]
        "#
    )]
    pub classes: Option<Vec<String>>,

    /// An override list of tokens to always recognize as a CONSTANT
    /// for [`order-by-type`](#lint_isort_order-by-type) regardless of casing.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            constants = ["constant"]
        "#
    )]
    pub constants: Option<Vec<String>>,

    /// An override list of tokens to always recognize as a var
    /// for [`order-by-type`](#lint_isort_order-by-type) regardless of casing.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            variables = ["VAR"]
        "#
    )]
    pub variables: Option<Vec<String>>,

    /// A list of sections that should _not_ be delineated from the previous
    /// section via empty lines.
    #[option(
        default = r#"[]"#,
        value_type = r#"list["future" | "standard-library" | "third-party" | "first-party" | "local-folder" | str]"#,
        example = r#"
            no-lines-before = ["future", "standard-library"]
        "#
    )]
    pub no_lines_before: Option<Vec<ImportSection>>,

    /// The number of blank lines to place after imports.
    /// Use `-1` for automatic determination.
    ///
    /// Ruff uses at most one blank line after imports in typing stub files (files with `.pyi` extension) in accordance to
    /// the typing style recommendations ([source](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)).
    ///
    /// When using the formatter, only the values `-1`, `1`, and `2` are compatible because
    /// it enforces at least one empty and at most two empty lines after imports.
    #[option(
        default = r#"-1"#,
        value_type = "int",
        example = r#"
            # Use a single line after each import block.
            lines-after-imports = 1
        "#
    )]
    pub lines_after_imports: Option<isize>,

    /// The number of lines to place between "direct" and `import from` imports.
    ///
    /// When using the formatter, only the values `0` and `1` are compatible because
    /// it preserves up to one empty line after imports in nested blocks.
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Use a single line between direct and from import.
            lines-between-types = 1
        "#
    )]
    pub lines_between_types: Option<usize>,

    /// A list of modules to separate into auxiliary block(s) of imports,
    /// in the order specified.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            forced-separate = ["tests"]
        "#
    )]
    pub forced_separate: Option<Vec<String>>,

    /// Override in which order the sections should be output. Can be used to move custom sections.
    #[option(
        default = r#"["future", "standard-library", "third-party", "first-party", "local-folder"]"#,
        value_type = r#"list["future" | "standard-library" | "third-party" | "first-party" | "local-folder" | str]"#,
        example = r#"
            section-order = ["future", "standard-library", "first-party", "local-folder", "third-party"]
        "#
    )]
    pub section_order: Option<Vec<ImportSection>>,

    /// Define a default section for any imports that don't fit into the specified [`section-order`](#lint_isort_section-order).
    #[option(
        default = r#""third-party""#,
        value_type = "str",
        example = r#"
            default-section = "first-party"
        "#
    )]
    pub default_section: Option<ImportSection>,

    /// Put all imports into the same section bucket.
    ///
    /// For example, rather than separating standard library and third-party imports, as in:
    /// ```python
    /// import os
    /// import sys
    ///
    /// import numpy
    /// import pandas
    /// ```
    ///
    /// Setting `no-sections = true` will instead group all imports into a single section:
    /// ```python
    /// import numpy
    /// import os
    /// import pandas
    /// import sys
    /// ```
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            no-sections = true
        "#
    )]
    pub no_sections: Option<bool>,

    /// Whether to automatically mark imports from within the same package as first-party.
    /// For example, when `detect-same-package = true`, then when analyzing files within the
    /// `foo` package, any imports from within the `foo` package will be considered first-party.
    ///
    /// This heuristic is often unnecessary when `src` is configured to detect all first-party
    /// sources; however, if `src` is _not_ configured, this heuristic can be useful to detect
    /// first-party imports from _within_ (but not _across_) first-party packages.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            detect-same-package = false
        "#
    )]
    pub detect_same_package: Option<bool>,

    /// Whether to place `import from` imports before straight imports when sorting.
    ///
    /// For example, by default, imports will be sorted such that straight imports appear
    /// before `import from` imports, as in:
    /// ```python
    /// import os
    /// import sys
    /// from typing import List
    /// ```
    ///
    /// Setting `from-first = true` will instead sort such that `import from` imports appear
    /// before straight imports, as in:
    /// ```python
    /// from typing import List
    /// import os
    /// import sys
    /// ```
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            from-first = true
        "#
    )]
    pub from_first: Option<bool>,

    /// Sort imports by their string length, such that shorter imports appear
    /// before longer imports. For example, by default, imports will be sorted
    /// alphabetically, as in:
    /// ```python
    /// import collections
    /// import os
    /// ```
    ///
    /// Setting `length-sort = true` will instead sort such that shorter imports
    /// appear before longer imports, as in:
    /// ```python
    /// import os
    /// import collections
    /// ```
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            length-sort = true
        "#
    )]
    pub length_sort: Option<bool>,

    /// Sort straight imports by their string length. Similar to [`length-sort`](#lint_isort_length-sort),
    /// but applies only to straight imports and doesn't affect `from` imports.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            length-sort-straight = true
        "#
    )]
    pub length_sort_straight: Option<bool>,

    // Tables are required to go last.
    /// A list of mappings from section names to modules.
    ///
    /// By default, imports are categorized according to their type (e.g., `future`, `third-party`,
    /// and so on). This setting allows you to group modules into custom sections, to augment or
    /// override the built-in sections.
    ///
    /// For example, to group all testing utilities, you could create a `testing` section:
    /// ```toml
    /// testing = ["pytest", "hypothesis"]
    /// ```
    ///
    /// The values in the list are treated as glob patterns. For example, to match all packages in
    /// the LangChain ecosystem (`langchain-core`, `langchain-openai`, etc.):
    /// ```toml
    /// langchain = ["langchain-*"]
    /// ```
    ///
    /// Custom sections should typically be inserted into the [`section-order`](#lint_isort_section-order) list to ensure that
    /// they're displayed as a standalone group and in the intended order, as in:
    /// ```toml
    /// section-order = [
    ///   "future",
    ///   "standard-library",
    ///   "third-party",
    ///   "first-party",
    ///   "local-folder",
    ///   "testing"
    /// ]
    /// ```
    ///
    /// If a custom section is omitted from [`section-order`](#lint_isort_section-order), imports in that section will be
    /// assigned to the [`default-section`](#lint_isort_default-section) (which defaults to `third-party`).
    #[option(
        default = "{}",
        value_type = "dict[str, list[str]]",
        scope = "sections",
        example = r#"
            # Group all Django imports into a separate section.
            "django" = ["django"]
        "#
    )]
    pub sections: Option<FxHashMap<ImportSection, Vec<String>>>,
}

impl IsortOptions {
    pub fn try_into_settings(
        self,
    ) -> Result<isort::settings::Settings, isort::settings::SettingsError> {
        // Verify that if `no_sections` is set, then `section_order` is empty.
        let no_sections = self.no_sections.unwrap_or_default();
        if no_sections && self.section_order.is_some() {
            warn_user_once!("`section-order` is ignored when `no-sections` is set to `true`");
        }
        if no_sections && self.default_section.is_some() {
            warn_user_once!("`default-section` is ignored when `no-sections` is set to `true`");
        }
        if no_sections && self.sections.is_some() {
            warn_user_once!("`sections` is ignored when `no-sections` is set to `true`");
        }

        // Verify that if `force_sort_within_sections` is `True`, then `lines_between_types` is set to `0`.
        let force_sort_within_sections = self.force_sort_within_sections.unwrap_or_default();
        let lines_between_types = self.lines_between_types.unwrap_or_default();
        if force_sort_within_sections && lines_between_types != 0 {
            warn_user_once!("`lines-between-types` is ignored when `force-sort-within-sections` is set to `true`");
        }

        // Extract any configuration options that deal with user-defined sections.
        let mut section_order: Vec<_> = self
            .section_order
            .unwrap_or_else(|| ImportType::iter().map(ImportSection::Known).collect());
        let default_section = self
            .default_section
            .unwrap_or(ImportSection::Known(ImportType::ThirdParty));

        let known_first_party = self
            .known_first_party
            .map(|names| {
                names
                    .into_iter()
                    .map(|name| IdentifierPattern::new(&name))
                    .collect()
            })
            .transpose()
            .map_err(isort::settings::SettingsError::InvalidKnownFirstParty)?
            .unwrap_or_default();
        let known_third_party = self
            .known_third_party
            .map(|names| {
                names
                    .into_iter()
                    .map(|name| IdentifierPattern::new(&name))
                    .collect()
            })
            .transpose()
            .map_err(isort::settings::SettingsError::InvalidKnownThirdParty)?
            .unwrap_or_default();
        let known_local_folder = self
            .known_local_folder
            .map(|names| {
                names
                    .into_iter()
                    .map(|name| IdentifierPattern::new(&name))
                    .collect()
            })
            .transpose()
            .map_err(isort::settings::SettingsError::InvalidKnownLocalFolder)?
            .unwrap_or_default();
        let extra_standard_library = self
            .extra_standard_library
            .map(|names| {
                names
                    .into_iter()
                    .map(|name| IdentifierPattern::new(&name))
                    .collect()
            })
            .transpose()
            .map_err(isort::settings::SettingsError::InvalidExtraStandardLibrary)?
            .unwrap_or_default();
        let no_lines_before = self.no_lines_before.unwrap_or_default();
        let from_first = self.from_first.unwrap_or_default();
        let sections = self.sections.unwrap_or_default();

        // Verify that `sections` doesn't contain any built-in sections.
        let sections: FxHashMap<String, Vec<glob::Pattern>> = sections
            .into_iter()
            .filter_map(|(section, modules)| match section {
                ImportSection::Known(section) => {
                    warn_user_once!("`sections` contains built-in section: `{:?}`", section);
                    None
                }
                ImportSection::UserDefined(section) => Some((section, modules)),
            })
            .map(|(section, modules)| {
                let modules = modules
                    .into_iter()
                    .map(|module| {
                        IdentifierPattern::new(&module)
                            .map_err(isort::settings::SettingsError::InvalidUserDefinedSection)
                    })
                    .collect::<Result<Vec<_>, isort::settings::SettingsError>>()?;
                Ok((section, modules))
            })
            .collect::<Result<_, _>>()?;

        // Verify that `section_order` doesn't contain any duplicates.
        let mut seen = FxHashSet::with_capacity_and_hasher(section_order.len(), FxBuildHasher);
        for section in &section_order {
            if !seen.insert(section) {
                warn_user_once!(
                    "`section-order` contains duplicate section: `{:?}`",
                    section
                );
            }
        }

        // Verify that all sections listed in `section_order` are defined in `sections`.
        for section in &section_order {
            if let ImportSection::UserDefined(section_name) = section {
                if !sections.contains_key(section_name) {
                    warn_user_once!("`section-order` contains unknown section: `{:?}`", section,);
                }
            }
        }

        // Verify that all sections listed in `no_lines_before` are defined in `sections`.
        for section in &no_lines_before {
            if let ImportSection::UserDefined(section_name) = section {
                if !sections.contains_key(section_name) {
                    warn_user_once!(
                        "`no-lines-before` contains unknown section: `{:?}`",
                        section,
                    );
                }
            }
        }

        // Verify that `default_section` is in `section_order`.
        if !section_order.contains(&default_section) {
            warn_user_once!(
                "`section-order` must contain `default-section`: {:?}",
                default_section,
            );
            section_order.push(default_section.clone());
        }

        Ok(isort::settings::Settings {
            required_imports: self
                .required_imports
                .unwrap_or_default()
                .into_iter()
                .flat_map(NameImports::into_imports)
                .collect(),
            combine_as_imports: self.combine_as_imports.unwrap_or(false),
            force_single_line: self.force_single_line.unwrap_or(false),
            force_sort_within_sections,
            case_sensitive: self.case_sensitive.unwrap_or(false),
            force_wrap_aliases: self.force_wrap_aliases.unwrap_or(false),
            detect_same_package: self.detect_same_package.unwrap_or(true),
            force_to_top: FxHashSet::from_iter(self.force_to_top.unwrap_or_default()),
            known_modules: isort::categorize::KnownModules::new(
                known_first_party,
                known_third_party,
                known_local_folder,
                extra_standard_library,
                sections,
            ),
            order_by_type: self.order_by_type.unwrap_or(true),
            relative_imports_order: self.relative_imports_order.unwrap_or_default(),
            single_line_exclusions: FxHashSet::from_iter(
                self.single_line_exclusions.unwrap_or_default(),
            ),
            split_on_trailing_comma: self.split_on_trailing_comma.unwrap_or(true),
            classes: FxHashSet::from_iter(self.classes.unwrap_or_default()),
            constants: FxHashSet::from_iter(self.constants.unwrap_or_default()),
            variables: FxHashSet::from_iter(self.variables.unwrap_or_default()),
            no_lines_before: FxHashSet::from_iter(no_lines_before),
            lines_after_imports: self.lines_after_imports.unwrap_or(-1),
            lines_between_types,
            forced_separate: Vec::from_iter(self.forced_separate.unwrap_or_default()),
            section_order,
            default_section,
            no_sections,
            from_first,
            length_sort: self.length_sort.unwrap_or(false),
            length_sort_straight: self.length_sort_straight.unwrap_or(false),
        })
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct McCabeOptions {
    /// The maximum McCabe complexity to allow before triggering `C901` errors.
    #[option(
        default = "10",
        value_type = "int",
        example = r#"
            # Flag errors (`C901`) whenever the complexity level exceeds 5.
            max-complexity = 5
        "#
    )]
    pub max_complexity: Option<usize>,
}

impl McCabeOptions {
    pub fn into_settings(self) -> mccabe::settings::Settings {
        mccabe::settings::Settings {
            max_complexity: self
                .max_complexity
                .unwrap_or(mccabe::settings::DEFAULT_MAX_COMPLEXITY),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Pep8NamingOptions {
    /// A list of names (or patterns) to ignore when considering `pep8-naming` violations.
    ///
    /// Supports glob patterns. For example, to ignore all names starting with `test_`
    /// or ending with `_test`, you could use `ignore-names = ["test_*", "*_test"]`.
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"["setUp", "tearDown", "setUpClass", "tearDownClass", "setUpModule", "tearDownModule", "asyncSetUp", "asyncTearDown", "setUpTestData", "failureException", "longMessage", "maxDiff"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["callMethod"]
        "#
    )]
    pub ignore_names: Option<Vec<String>>,

    /// Additional names (or patterns) to ignore when considering `pep8-naming` violations,
    /// in addition to those included in [`ignore-names`](#lint_pep8-naming_ignore-names).
    ///
    /// Supports glob patterns. For example, to ignore all names starting with `test_`
    /// or ending with `_test`, you could use `ignore-names = ["test_*", "*_test"]`.
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-ignore-names = ["callMethod"]"#
    )]
    pub extend_ignore_names: Option<Vec<String>>,

    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a class method (in addition to the builtin
    /// `@classmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list takes a `cls` argument as its first argument.
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `pydantic.validator`,
    /// rather than `validator`) or alternatively a plain name which is then matched against
    /// the last segment in case the decorator itself consists of a dotted name.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            classmethod-decorators = [
                # Allow Pydantic's `@validator` decorator to trigger class method treatment.
                "pydantic.validator",
                # Allow SQLAlchemy's dynamic decorators, like `@field.expression`, to trigger class method treatment.
                "declared_attr",
                "expression",
                "comparator",
            ]
        "#
    )]
    pub classmethod_decorators: Option<Vec<String>>,

    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a static method (in addition to the builtin
    /// `@staticmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list has no `self` or `cls` argument.
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `belay.Device.teardown`,
    /// rather than `teardown`) or alternatively a plain name which is then matched against
    /// the last segment in case the decorator itself consists of a dotted name.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow Belay's `@Device.teardown` decorator to trigger static method treatment.
            staticmethod-decorators = ["belay.Device.teardown"]
        "#
    )]
    pub staticmethod_decorators: Option<Vec<String>>,
}

impl Pep8NamingOptions {
    pub fn try_into_settings(
        self,
    ) -> Result<pep8_naming::settings::Settings, pep8_naming::settings::SettingsError> {
        Ok(pep8_naming::settings::Settings {
            ignore_names: IgnoreNames::from_options(self.ignore_names, self.extend_ignore_names)?,
            classmethod_decorators: self.classmethod_decorators.unwrap_or_default(),
            staticmethod_decorators: self.staticmethod_decorators.unwrap_or_default(),
        })
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PycodestyleOptions {
    /// The maximum line length to allow for [`line-too-long`](https://docs.astral.sh/ruff/rules/line-too-long/) violations. By default,
    /// this is set to the value of the [`line-length`](#line-length) option.
    ///
    /// Use this option when you want to detect extra-long lines that the formatter can't automatically split by setting
    /// `pycodestyle.line-length` to a value larger than [`line-length`](#line-length).
    ///
    /// ```toml
    /// # The formatter wraps lines at a length of 88.
    /// line-length = 88
    ///
    /// [pycodestyle]
    /// # E501 reports lines that exceed the length of 100.
    /// max-line-length = 100
    /// ```
    ///
    /// The length is determined by the number of characters per line, except for lines containing East Asian characters or emojis.
    /// For these lines, the [unicode width](https://unicode.org/reports/tr11/) of each character is added up to determine the length.
    ///
    /// See the [`line-too-long`](https://docs.astral.sh/ruff/rules/line-too-long/) rule for more information.
    #[option(
        default = "null",
        value_type = "int",
        example = r#"
            max-line-length = 100
        "#
    )]
    pub max_line_length: Option<LineLength>,

    /// The maximum line length to allow for [`doc-line-too-long`](https://docs.astral.sh/ruff/rules/doc-line-too-long/) violations within
    /// documentation (`W505`), including standalone comments. By default,
    /// this is set to `null` which disables reporting violations.
    ///
    /// The length is determined by the number of characters per line, except for lines containing Asian characters or emojis.
    /// For these lines, the [unicode width](https://unicode.org/reports/tr11/) of each character is added up to determine the length.
    ///
    /// See the [`doc-line-too-long`](https://docs.astral.sh/ruff/rules/doc-line-too-long/) rule for more information.
    #[option(
        default = "null",
        value_type = "int",
        example = r#"
            max-doc-length = 88
        "#
    )]
    pub max_doc_length: Option<LineLength>,

    /// Whether line-length violations (`E501`) should be triggered for
    /// comments starting with [`task-tags`](#lint_task-tags) (by default: "TODO", "FIXME",
    /// and "XXX").
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-overlong-task-comments = true
        "#
    )]
    pub ignore_overlong_task_comments: Option<bool>,
}

impl PycodestyleOptions {
    pub fn into_settings(self, global_line_length: LineLength) -> pycodestyle::settings::Settings {
        pycodestyle::settings::Settings {
            max_doc_length: self.max_doc_length,
            max_line_length: self.max_line_length.unwrap_or(global_line_length),
            ignore_overlong_task_comments: self.ignore_overlong_task_comments.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PydocstyleOptions {
    /// Whether to use Google-style, NumPy-style conventions, or the [PEP 257](https://peps.python.org/pep-0257/)
    /// defaults when analyzing docstring sections.
    ///
    /// Enabling a convention will disable all rules that are not included in
    /// the specified convention. As such, the intended workflow is to enable a
    /// convention and then selectively enable or disable any additional rules
    /// on top of it.
    ///
    /// For example, to use Google-style conventions but avoid requiring
    /// documentation for every function parameter:
    ///
    /// ```toml
    /// [tool.ruff.lint]
    /// # Enable all `pydocstyle` rules, limiting to those that adhere to the
    /// # Google convention via `convention = "google"`, below.
    /// select = ["D"]
    ///
    /// # On top of the Google convention, disable `D417`, which requires
    /// # documentation for every function parameter.
    /// ignore = ["D417"]
    ///
    /// [tool.ruff.lint.pydocstyle]
    /// convention = "google"
    /// ```
    ///
    /// To enable an additional rule that's excluded from the convention,
    /// select the desired rule via its fully qualified rule code (e.g.,
    /// `D400` instead of `D4` or `D40`):
    ///
    /// ```toml
    /// [tool.ruff.lint]
    /// # Enable D400 on top of the Google convention.
    /// extend-select = ["D400"]
    ///
    /// [tool.ruff.lint.pydocstyle]
    /// convention = "google"
    /// ```
    #[option(
        default = r#"null"#,
        value_type = r#""google" | "numpy" | "pep257""#,
        example = r#"
            # Use Google-style docstrings.
            convention = "google"
        "#
    )]
    pub convention: Option<Convention>,

    /// Ignore docstrings for functions or methods decorated with the
    /// specified fully-qualified decorators.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            ignore-decorators = ["typing.overload"]
        "#
    )]
    pub ignore_decorators: Option<Vec<String>>,

    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a property (in addition to the builtin
    /// `@property` and standard-library `@functools.cached_property`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list can use a non-imperative summary line.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            property-decorators = ["gi.repository.GObject.Property"]
        "#
    )]
    pub property_decorators: Option<Vec<String>>,
}

impl PydocstyleOptions {
    pub fn into_settings(self) -> pydocstyle::settings::Settings {
        let PydocstyleOptions {
            convention,
            ignore_decorators,
            property_decorators,
        } = self;
        pydocstyle::settings::Settings::new(
            convention,
            ignore_decorators.unwrap_or_default(),
            property_decorators.unwrap_or_default(),
        )
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PyflakesOptions {
    /// Additional functions or classes to consider generic, such that any
    /// subscripts should be treated as type annotation (e.g., `ForeignKey` in
    /// `django.db.models.ForeignKey["User"]`.
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `django.db.models.ForeignKey`,
    /// rather than `ForeignKey`).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "extend-generics = [\"django.db.models.ForeignKey\"]"
    )]
    pub extend_generics: Option<Vec<String>>,

    /// A list of modules to ignore when considering unused imports.
    ///
    /// Used to prevent violations for specific modules that are known to have side effects on
    /// import (e.g., `hvplot.pandas`).
    ///
    /// Modules in this list are expected to be fully-qualified names (e.g., `hvplot.pandas`). Any
    /// submodule of a given module will also be ignored (e.g., given `hvplot`, `hvplot.pandas`
    /// will also be ignored).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"allowed-unused-imports = ["hvplot.pandas"]"#
    )]
    pub allowed_unused_imports: Option<Vec<String>>,
}

impl PyflakesOptions {
    pub fn into_settings(self) -> pyflakes::settings::Settings {
        pyflakes::settings::Settings {
            extend_generics: self.extend_generics.unwrap_or_default(),
            allowed_unused_imports: self.allowed_unused_imports.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PylintOptions {
    /// Constant types to ignore when used as "magic values" (see `PLR2004`).
    #[option(
        default = r#"["str", "bytes"]"#,
        value_type = r#"list["str" | "bytes" | "complex" | "float" | "int"]"#,
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    pub allow_magic_value_types: Option<Vec<ConstantType>>,

    /// Dunder methods name to allow, in addition to the default set from the
    /// Python standard library (see `PLW3201`).
    #[option(
        default = r#"[]"#,
        value_type = r#"list[str]"#,
        example = r#"
            allow-dunder-method-names = ["__tablename__", "__table_args__"]
        "#
    )]
    pub allow_dunder_method_names: Option<FxHashSet<String>>,

    /// Maximum number of branches allowed for a function or method body (see `PLR0912`).
    #[option(default = r"12", value_type = "int", example = r"max-branches = 15")]
    pub max_branches: Option<usize>,

    /// Maximum number of return statements allowed for a function or method
    /// body (see `PLR0911`)
    #[option(default = r"6", value_type = "int", example = r"max-returns = 10")]
    pub max_returns: Option<usize>,

    /// Maximum number of arguments allowed for a function or method definition
    /// (see `PLR0913`).
    #[option(default = r"5", value_type = "int", example = r"max-args = 10")]
    pub max_args: Option<usize>,

    /// Maximum number of positional arguments allowed for a function or method definition
    /// (see `PLR0917`).
    ///
    /// If not specified, defaults to the value of `max-args`.
    #[option(
        default = r"5", // Needs to be in sync with default of `max-args`.
        value_type = "int",
        example = r"max-positional-args = 3"
    )]
    pub max_positional_args: Option<usize>,

    /// Maximum number of local variables allowed for a function or method body (see `PLR0914`).
    #[option(default = r"15", value_type = "int", example = r"max-locals = 20")]
    pub max_locals: Option<usize>,

    /// Maximum number of statements allowed for a function or method body (see `PLR0915`).
    #[option(default = r"50", value_type = "int", example = r"max-statements = 75")]
    pub max_statements: Option<usize>,

    /// Maximum number of public methods allowed for a class (see `PLR0904`).
    #[option(
        default = r"20",
        value_type = "int",
        example = r"max-public-methods = 30"
    )]
    pub max_public_methods: Option<usize>,

    /// Maximum number of Boolean expressions allowed within a single `if` statement
    /// (see `PLR0916`).
    #[option(default = r"5", value_type = "int", example = r"max-bool-expr = 10")]
    pub max_bool_expr: Option<usize>,

    /// Maximum number of nested blocks allowed within a function or method body
    /// (see `PLR1702`).
    #[option(
        default = r"5",
        value_type = "int",
        example = r"max-nested-blocks = 10"
    )]
    pub max_nested_blocks: Option<usize>,
}

impl PylintOptions {
    pub fn into_settings(self) -> pylint::settings::Settings {
        let defaults = pylint::settings::Settings::default();
        pylint::settings::Settings {
            allow_magic_value_types: self
                .allow_magic_value_types
                .unwrap_or(defaults.allow_magic_value_types),
            allow_dunder_method_names: self.allow_dunder_method_names.unwrap_or_default(),
            max_args: self.max_args.unwrap_or(defaults.max_args),
            max_positional_args: self
                .max_positional_args
                .or(self.max_args)
                .unwrap_or(defaults.max_positional_args),
            max_bool_expr: self.max_bool_expr.unwrap_or(defaults.max_bool_expr),
            max_returns: self.max_returns.unwrap_or(defaults.max_returns),
            max_branches: self.max_branches.unwrap_or(defaults.max_branches),
            max_statements: self.max_statements.unwrap_or(defaults.max_statements),
            max_public_methods: self
                .max_public_methods
                .unwrap_or(defaults.max_public_methods),
            max_locals: self.max_locals.unwrap_or(defaults.max_locals),
            max_nested_blocks: self.max_nested_blocks.unwrap_or(defaults.max_nested_blocks),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PyUpgradeOptions {
    /// Whether to avoid [PEP 585](https://peps.python.org/pep-0585/) (`List[int]` -> `list[int]`) and [PEP 604](https://peps.python.org/pep-0604/)
    /// (`Union[str, int]` -> `str | int`) rewrites even if a file imports
    /// `from __future__ import annotations`.
    ///
    /// This setting is only applicable when the target Python version is below
    /// 3.9 and 3.10 respectively, and is most commonly used when working with
    /// libraries like Pydantic and FastAPI, which rely on the ability to parse
    /// type annotations at runtime. The use of `from __future__ import annotations`
    /// causes Python to treat the type annotations as strings, which typically
    /// allows for the use of language features that appear in later Python
    /// versions but are not yet supported by the current version (e.g., `str |
    /// int`). However, libraries that rely on runtime type annotations will
    /// break if the annotations are incompatible with the current Python
    /// version.
    ///
    /// For example, while the following is valid Python 3.8 code due to the
    /// presence of `from __future__ import annotations`, the use of `str | int`
    /// prior to Python 3.10 will cause Pydantic to raise a `TypeError` at
    /// runtime:
    ///
    /// ```python
    /// from __future__ import annotations
    ///
    /// import pydantic
    ///
    /// class Foo(pydantic.BaseModel):
    ///     bar: str | int
    /// ```
    ///
    ///
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            # Preserve types, even if a file imports `from __future__ import annotations`.
            keep-runtime-typing = true
        "#
    )]
    pub keep_runtime_typing: Option<bool>,
}

impl PyUpgradeOptions {
    pub fn into_settings(self) -> pyupgrade::settings::Settings {
        pyupgrade::settings::Settings {
            keep_runtime_typing: self.keep_runtime_typing.unwrap_or_default(),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RuffOptions {
    /// Whether to prefer accessing items keyed by tuples with
    /// parentheses around the tuple (see `RUF031`).
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
        # Make it a violation to use a tuple in a subscript without parentheses.
        parenthesize-tuple-in-subscript = true
        "#
    )]
    pub parenthesize_tuple_in_subscript: Option<bool>,
}

impl RuffOptions {
    pub fn into_settings(self) -> ruff::settings::Settings {
        ruff::settings::Settings {
            parenthesize_tuple_in_subscript: self
                .parenthesize_tuple_in_subscript
                .unwrap_or_default(),
        }
    }
}

/// Configures the way Ruff formats your code.
#[derive(
    Clone, Debug, PartialEq, Eq, Default, Deserialize, Serialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct FormatOptions {
    /// A list of file patterns to exclude from formatting in addition to the files excluded globally (see [`exclude`](#exclude), and [`extend-exclude`](#extend-exclude)).
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// - Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// - Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            exclude = ["generated"]
        "#
    )]
    pub exclude: Option<Vec<String>>,

    /// Whether to enable the unstable preview style formatting.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable preview style formatting.
            preview = true
        "#
    )]
    pub preview: Option<bool>,

    /// Whether to use spaces or tabs for indentation.
    ///
    /// `indent-style = "space"` (default):
    ///
    /// ```python
    /// def f():
    ///     print("Hello") #  Spaces indent the `print` statement.
    /// ```
    ///
    /// `indent-style = "tab"`:
    ///
    /// ```python
    /// def f():
    ///     print("Hello") #  A tab `\t` indents the `print` statement.
    /// ```
    ///
    /// PEP 8 recommends using spaces for [indentation](https://peps.python.org/pep-0008/#indentation).
    /// We care about accessibility; if you do not need tabs for accessibility, we do not recommend you use them.
    ///
    /// See [`indent-width`](#indent-width) to configure the number of spaces per indentation and the tab width.
    #[option(
        default = r#""space""#,
        value_type = r#""space" | "tab""#,
        example = r#"
            # Use tabs instead of 4 space indentation.
            indent-style = "tab"
        "#
    )]
    pub indent_style: Option<IndentStyle>,

    /// Configures the preferred quote character for strings. The recommended options are
    ///
    /// * `double` (default): Use double quotes `"`
    /// * `single`: Use single quotes `'`
    ///
    /// In compliance with [PEP 8](https://peps.python.org/pep-0008/) and [PEP 257](https://peps.python.org/pep-0257/),
    /// Ruff prefers double quotes for triple quoted strings and docstrings even when using `quote-style = "single"`.
    ///
    /// Ruff deviates from using the configured quotes if doing so prevents the need for
    /// escaping quote characters inside the string:
    ///
    /// ```python
    /// a = "a string without any quotes"
    /// b = "It's monday morning"
    /// ```
    ///
    /// Ruff will change the quotes of the string assigned to `a` to single quotes when using `quote-style = "single"`.
    /// However, Ruff uses double quotes for the string assigned to `b` because using single quotes would require escaping the `'`,
    /// which leads to the less readable code: `'It\'s monday morning'`.
    ///
    /// In addition, Ruff supports the quote style `preserve` for projects that already use
    /// a mixture of single and double quotes and can't migrate to the `double` or `single` style.
    /// The quote style `preserve` leaves the quotes of all strings unchanged.
    #[option(
        default = r#""double""#,
        value_type = r#""double" | "single" | "preserve""#,
        example = r#"
            # Prefer single quotes over double quotes.
            quote-style = "single"
        "#
    )]
    pub quote_style: Option<QuoteStyle>,

    /// Ruff uses existing trailing commas as an indication that short lines should be left separate.
    /// If this option is set to `true`, the magic trailing comma is ignored.
    ///
    /// For example, Ruff leaves the arguments separate even though
    /// collapsing the arguments to a single line doesn't exceed the line length if `skip-magic-trailing-comma = false`:
    ///
    /// ```python
    /// # The arguments remain on separate lines because of the trailing comma after `b`
    /// def test(
    ///     a,
    ///     b,
    /// ): pass
    /// ```
    ///
    /// Setting `skip-magic-trailing-comma = true` changes the formatting to:
    ///
    /// ```python
    /// # The arguments remain on separate lines because of the trailing comma after `b`
    /// def test(a, b):
    ///     pass
    /// ```
    #[option(
        default = r#"false"#,
        value_type = r#"bool"#,
        example = "skip-magic-trailing-comma = true"
    )]
    pub skip_magic_trailing_comma: Option<bool>,

    /// The character Ruff uses at the end of a line.
    ///
    /// * `auto`: The newline style is detected automatically on a file per file basis. Files with mixed line endings will be converted to the first detected line ending. Defaults to `\n` for files that contain no line endings.
    /// * `lf`: Line endings will be converted to `\n`. The default line ending on Unix.
    /// * `cr-lf`: Line endings will be converted to `\r\n`. The default line ending on Windows.
    /// * `native`: Line endings will be converted to `\n` on Unix and `\r\n` on Windows.
    #[option(
        default = r#""auto""#,
        value_type = r#""auto" | "lf" | "cr-lf" | "native""#,
        example = r#"
            # Use `\n` line endings for all files
            line-ending = "lf"
        "#
    )]
    pub line_ending: Option<LineEnding>,

    /// Whether to format code snippets in docstrings.
    ///
    /// When this is enabled, Python code examples within docstrings are
    /// automatically reformatted.
    ///
    /// For example, when this is enabled, the following code:
    ///
    /// ```python
    /// def f(x):
    ///     """
    ///     Something about `f`. And an example in doctest format:
    ///
    ///     >>> f(  x  )
    ///
    ///     Markdown is also supported:
    ///
    ///     ```py
    ///     f(  x  )
    ///     ```
    ///
    ///     As are reStructuredText literal blocks::
    ///
    ///         f(  x  )
    ///
    ///
    ///     And reStructuredText code blocks:
    ///
    ///     .. code-block:: python
    ///
    ///         f(  x  )
    ///     """
    ///     pass
    /// ```
    ///
    /// ... will be reformatted (assuming the rest of the options are set to
    /// their defaults) as:
    ///
    /// ```python
    /// def f(x):
    ///     """
    ///     Something about `f`. And an example in doctest format:
    ///
    ///     >>> f(x)
    ///
    ///     Markdown is also supported:
    ///
    ///     ```py
    ///     f(x)
    ///     ```
    ///
    ///     As are reStructuredText literal blocks::
    ///
    ///         f(x)
    ///
    ///
    ///     And reStructuredText code blocks:
    ///
    ///     .. code-block:: python
    ///
    ///         f(x)
    ///     """
    ///     pass
    /// ```
    ///
    /// If a code snippet in a docstring contains invalid Python code or if the
    /// formatter would otherwise write invalid Python code, then the code
    /// example is ignored by the formatter and kept as-is.
    ///
    /// Currently, doctest, Markdown, reStructuredText literal blocks, and
    /// reStructuredText code blocks are all supported and automatically
    /// recognized. In the case of unlabeled fenced code blocks in Markdown and
    /// reStructuredText literal blocks, the contents are assumed to be Python
    /// and reformatted. As with any other format, if the contents aren't valid
    /// Python, then the block is left untouched automatically.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable reformatting of code snippets in docstrings.
            docstring-code-format = true
        "#
    )]
    pub docstring_code_format: Option<bool>,

    /// Set the line length used when formatting code snippets in docstrings.
    ///
    /// This only has an effect when the `docstring-code-format` setting is
    /// enabled.
    ///
    /// The default value for this setting is `"dynamic"`, which has the effect
    /// of ensuring that any reformatted code examples in docstrings adhere to
    /// the global line length configuration that is used for the surrounding
    /// Python code. The point of this setting is that it takes the indentation
    /// of the docstring into account when reformatting code examples.
    ///
    /// Alternatively, this can be set to a fixed integer, which will result
    /// in the same line length limit being applied to all reformatted code
    /// examples in docstrings. When set to a fixed integer, the indent of the
    /// docstring is not taken into account. That is, this may result in lines
    /// in the reformatted code example that exceed the globally configured
    /// line length limit.
    ///
    /// For example, when this is set to `20` and [`docstring-code-format`](#docstring-code-format)
    /// is enabled, then this code:
    ///
    /// ```python
    /// def f(x):
    ///     '''
    ///     Something about `f`. And an example:
    ///
    ///     .. code-block:: python
    ///
    ///         foo, bar, quux = this_is_a_long_line(lion, hippo, lemur, bear)
    ///     '''
    ///     pass
    /// ```
    ///
    /// ... will be reformatted (assuming the rest of the options are set
    /// to their defaults) as:
    ///
    /// ```python
    /// def f(x):
    ///     """
    ///     Something about `f`. And an example:
    ///
    ///     .. code-block:: python
    ///
    ///         (
    ///             foo,
    ///             bar,
    ///             quux,
    ///         ) = this_is_a_long_line(
    ///             lion,
    ///             hippo,
    ///             lemur,
    ///             bear,
    ///         )
    ///     """
    ///     pass
    /// ```
    #[option(
        default = r#""dynamic""#,
        value_type = r#"int | "dynamic""#,
        example = r#"
            # Format all docstring code snippets with a line length of 60.
            docstring-code-line-length = 60
        "#
    )]
    pub docstring_code_line_length: Option<DocstringCodeLineWidth>,
}

/// Configures Ruff's `analyze` command.
#[derive(
    Clone, Debug, PartialEq, Eq, Default, Deserialize, Serialize, OptionsMetadata, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AnalyzeOptions {
    /// A list of file patterns to exclude from analysis in addition to the files excluded globally (see [`exclude`](#exclude), and [`extend-exclude`](#extend-exclude)).
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// - Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// - Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            exclude = ["generated"]
        "#
    )]
    pub exclude: Option<Vec<String>>,
    /// Whether to enable preview mode. When preview mode is enabled, Ruff will expose unstable
    /// commands.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable preview features.
            preview = true
        "#
    )]
    pub preview: Option<bool>,
    /// Whether to generate a map from file to files that it depends on (dependencies) or files that
    /// depend on it (dependents).
    #[option(
        default = r#""dependencies""#,
        value_type = r#""dependents" | "dependencies""#,
        example = r#"
            direction = "dependencies"
        "#
    )]
    pub direction: Option<Direction>,
    /// Whether to detect imports from string literals. When enabled, Ruff will search for string
    /// literals that "look like" import paths, and include them in the import map, if they resolve
    /// to valid Python modules.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            detect-string-imports = true
        "#
    )]
    pub detect_string_imports: Option<bool>,
    /// A map from file path to the list of file paths or globs that should be considered
    /// dependencies of that file, regardless of whether relevant imports are detected.
    #[option(
        default = "{}",
        value_type = "dict[str, list[str]]",
        example = r#"
            include-dependencies = {
                "foo/bar.py": ["foo/baz/*.py"],
            }
        "#
    )]
    pub include_dependencies: Option<BTreeMap<PathBuf, Vec<String>>>,
}

#[cfg(test)]
mod tests {
    use crate::options::Flake8SelfOptions;
    use ruff_linter::rules::flake8_self;
    use ruff_python_ast::name::Name;

    #[test]
    fn flake8_self_options() {
        let default_settings = flake8_self::settings::Settings::default();

        // Uses defaults if no options are specified.
        let options = Flake8SelfOptions {
            ignore_names: None,
            extend_ignore_names: None,
        };
        let settings = options.into_settings();
        assert_eq!(settings.ignore_names, default_settings.ignore_names);

        // Uses ignore_names if specified.
        let options = Flake8SelfOptions {
            ignore_names: Some(vec![Name::new_static("_foo")]),
            extend_ignore_names: None,
        };
        let settings = options.into_settings();
        assert_eq!(settings.ignore_names, vec![Name::new_static("_foo")]);

        // Appends extend_ignore_names to defaults if only extend_ignore_names is specified.
        let options = Flake8SelfOptions {
            ignore_names: None,
            extend_ignore_names: Some(vec![Name::new_static("_bar")]),
        };
        let settings = options.into_settings();
        assert_eq!(
            settings.ignore_names,
            default_settings
                .ignore_names
                .into_iter()
                .chain([Name::new_static("_bar")])
                .collect::<Vec<_>>()
        );

        // Appends extend_ignore_names to ignore_names if both are specified.
        let options = Flake8SelfOptions {
            ignore_names: Some(vec![Name::new_static("_foo")]),
            extend_ignore_names: Some(vec![Name::new_static("_bar")]),
        };
        let settings = options.into_settings();
        assert_eq!(
            settings.ignore_names,
            vec![Name::new_static("_foo"), Name::new_static("_bar")]
        );
    }
}
