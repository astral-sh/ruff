use regex::Regex;
use ruff::line_width::{LineLength, TabSize};
use ruff::rules::flake8_pytest_style::settings::SettingsError;
use ruff::rules::flake8_pytest_style::types;
use ruff::rules::flake8_quotes::settings::Quote;
use ruff::rules::flake8_tidy_imports::settings::{ApiBan, Strictness};
use ruff::rules::isort::settings::RelativeImportsOrder;
use ruff::rules::isort::{ImportSection, ImportType};
use ruff::rules::pydocstyle::settings::Convention;
use ruff::rules::pylint::settings::ConstantType;
use ruff::rules::{
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade,
};
use ruff::settings::types::{IdentifierPattern, PythonVersion, SerializationFormat, Version};
use ruff::{warn_user_once, RuleSelector};
use ruff_macros::{CombineOptions, ConfigurationOptions};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::hash::BuildHasherDefault;
use strum::IntoEnumIterator;

#[derive(Debug, PartialEq, Eq, Default, ConfigurationOptions, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow minus-sign (U+2212), greek-small-letter-rho (U+03C1), and the asterisk-operator (U+2217),
            # which could be confused for "-", "p", and "*", respectively.
            allowed-confusables = ["−", "ρ", "∗"]
        "#
    )]
    /// A list of allowed "confusable" Unicode characters to ignore when
    /// enforcing `RUF001`, `RUF002`, and `RUF003`.
    pub allowed_confusables: Option<Vec<char>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            builtins = ["_"]
        "#
    )]
    /// A list of builtins to treat as defined references, in addition to the
    /// system builtins.
    pub builtins: Option<Vec<String>>,
    #[option(
        default = ".ruff_cache",
        value_type = "str",
        example = r#"cache-dir = "~/.cache/ruff""#
    )]
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
    pub cache_dir: Option<String>,
    #[option(
        default = r#""^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$""#,
        value_type = "re.Pattern",
        example = r#"
            # Only ignore variables named "_".
            dummy-variable-rgx = "^_$"
        "#
    )]
    /// A regular expression used to identify "dummy" variables, or those which
    /// should be ignored when enforcing (e.g.) unused-variable rules. The
    /// default expression matches `_`, `__`, and `_var`, but not `_var_`.
    pub dummy_variable_rgx: Option<String>,
    #[option(
        default = r#"[".bzr", ".direnv", ".eggs", ".git", ".git-rewrite", ".hg", ".mypy_cache", ".nox", ".pants.d", ".pytype", ".ruff_cache", ".svn", ".tox", ".venv", "__pypackages__", "_build", "buck-out", "build", "dist", "node_modules", "venv"]"#,
        value_type = "list[str]",
        example = r#"
            exclude = [".venv"]
        "#
    )]
    /// A list of file patterns to exclude from linting.
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
    pub exclude: Option<Vec<String>>,
    #[option(
        default = r#"None"#,
        value_type = "str",
        example = r#"
            # Extend the `pyproject.toml` file in the parent directory.
            extend = "../pyproject.toml"
            # But use a different line length.
            line-length = 100
        "#
    )]
    /// A path to a local `pyproject.toml` file to merge into this
    /// configuration. User home directory and environment variables will be
    /// expanded.
    ///
    /// To resolve the current `pyproject.toml` file, Ruff will first resolve
    /// this base configuration file, then merge in any properties defined
    /// in the current configuration file.
    pub extend: Option<String>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # In addition to the standard set of exclusions, omit all tests, plus a specific file.
            extend-exclude = ["tests", "src/bad.py"]
        "#
    )]
    /// A list of file patterns to omit from linting, in addition to those
    /// specified by `exclude`.
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
    pub extend_exclude: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # In addition to the standard set of inclusions, include `.pyw` files.
            extend-include = ["*.pyw"]
        "#
    )]
    /// A list of file patterns to include when linting, in addition to those
    /// specified by `include`.
    ///
    /// Inclusion are based on globs, and should be single-path patterns, like
    /// `*.pyw`, to include any file with the `.pyw` extension.
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub extend_include: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Skip unused variable rules (`F841`).
            extend-ignore = ["F841"]
        "#
    )]
    /// A list of rule codes or prefixes to ignore, in addition to those
    /// specified by `ignore`.
    ///
    /// This option has been **deprecated** in favor of `ignore`
    /// since its usage is now interchangeable with `ignore`.
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub extend_ignore: Option<Vec<RuleSelector>>,
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # On top of the default `select` (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            extend-select = ["B", "Q"]
        "#
    )]
    /// A list of rule codes or prefixes to enable, in addition to those
    /// specified by `select`.
    pub extend_select: Option<Vec<RuleSelector>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # Enable autofix for flake8-bugbear (`B`), on top of any rules specified by `fixable`.
            extend-fixable = ["B"]
        "#
    )]
    /// A list of rule codes or prefixes to consider autofixable, in addition to those
    /// specified by `fixable`.
    pub extend_fixable: Option<Vec<RuleSelector>>,
    /// A list of rule codes or prefixes to consider non-auto-fixable, in addition to those
    /// specified by `unfixable`.
    ///
    /// This option has been **deprecated** in favor of `unfixable` since its usage is now
    /// interchangeable with `unfixable`.
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub extend_unfixable: Option<Vec<RuleSelector>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # Avoiding flagging (and removing) `V101` from any `# noqa`
            # directives, despite Ruff's lack of support for `vulture`.
            external = ["V101"]
        "#
    )]
    /// A list of rule codes that are unsupported by Ruff, but should be
    /// preserved when (e.g.) validating `# noqa` directives. Useful for
    /// retaining `# noqa` directives that cover plugins not yet implemented
    /// by Ruff.
    pub external: Option<Vec<String>>,
    #[option(default = "false", value_type = "bool", example = "fix = true")]
    /// Enable autofix behavior by-default when running `ruff` (overridden
    /// by the `--fix` and `--no-fix` command-line flags).
    pub fix: Option<bool>,
    #[option(default = "false", value_type = "bool", example = "fix-only = true")]
    /// Like `fix`, but disables reporting on leftover violation. Implies `fix`.
    pub fix_only: Option<bool>,
    #[option(
        default = r#"["ALL"]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # Only allow autofix behavior for `E` and `F` rules.
            fixable = ["E", "F"]
        "#
    )]
    /// A list of rule codes or prefixes to consider autofixable. By default,
    /// all rules are considered autofixable.
    pub fixable: Option<Vec<RuleSelector>>,
    #[option(
        default = r#""text""#,
        value_type = r#""text" | "json" | "junit" | "github" | "gitlab" | "pylint" | "azure""#,
        example = r#"
            # Group violations by containing file.
            format = "grouped"
        "#
    )]
    /// The style in which violation messages should be formatted: `"text"`
    /// (default), `"grouped"` (group messages by file), `"json"`
    /// (machine-readable), `"junit"` (machine-readable XML), `"github"` (GitHub
    /// Actions annotations), `"gitlab"` (GitLab CI code quality report),
    /// `"pylint"` (Pylint text format) or `"azure"` (Azure Pipeline logging commands).
    pub format: Option<SerializationFormat>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-exclude = true
        "#
    )]
    /// Whether to enforce `exclude` and `extend-exclude` patterns, even for
    /// paths that are passed to Ruff explicitly. Typically, Ruff will lint
    /// any paths passed in directly, even if they would typically be
    /// excluded. Setting `force-exclude = true` will cause Ruff to
    /// respect these exclusions unequivocally.
    ///
    /// This is useful for [`pre-commit`](https://pre-commit.com/), which explicitly passes all
    /// changed files to the [`ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit)
    /// plugin, regardless of whether they're marked as excluded by Ruff's own
    /// settings.
    pub force_exclude: Option<bool>,
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Skip unused variable rules (`F841`).
            ignore = ["F841"]
        "#
    )]
    /// A list of rule codes or prefixes to ignore. Prefixes can specify exact
    /// rules (like `F841`), entire categories (like `F`), or anything in
    /// between.
    ///
    /// When breaking ties between enabled and disabled rules (via `select` and
    /// `ignore`, respectively), more specific prefixes override less
    /// specific prefixes.
    pub ignore: Option<Vec<RuleSelector>>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-init-module-imports = true
        "#
    )]
    /// Avoid automatically removing unused imports in `__init__.py` files. Such
    /// imports will still be flagged, but with a dedicated message suggesting
    /// that the import is either added to the module's `__all__` symbol, or
    /// re-exported with a redundant alias (e.g., `import os as os`).
    pub ignore_init_module_imports: Option<bool>,
    #[option(
        default = r#"["*.py", "*.pyi", "**/pyproject.toml"]"#,
        value_type = "list[str]",
        example = r#"
            include = ["*.py"]
        "#
    )]
    /// A list of file patterns to include when linting.
    ///
    /// Inclusion are based on globs, and should be single-path patterns, like
    /// `*.pyw`, to include any file with the `.pyw` extension. `pyproject.toml` is
    /// included here not for configuration but because we lint whether e.g. the
    /// `[project]` matches the schema.
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub include: Option<Vec<String>>,
    #[option(
        default = "88",
        value_type = "int",
        example = r#"
        # Allow lines to be as long as 120 characters.
        line-length = 120
        "#
    )]
    /// The line length to use when enforcing long-lines violations (like
    /// `E501`). Must be greater than `0` and less than or equal to `320`.
    #[cfg_attr(feature = "schemars", schemars(range(min = 1, max = 320)))]
    pub line_length: Option<LineLength>,
    #[option(
        default = "4",
        value_type = "int",
        example = r#"
            tab-size = 8
        "#
    )]
    /// The tabulation size to calculate line length.
    pub tab_size: Option<TabSize>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"logger-objects = ["logging_setup.logger"]"#
    )]
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
    pub logger_objects: Option<Vec<String>>,
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            required-version = "0.0.193"
        "#
    )]
    /// Require a specific version of Ruff to be running (useful for unifying
    /// results across many environments, e.g., with a `pyproject.toml`
    /// file).
    pub required_version: Option<Version>,
    #[option(
        default = "true",
        value_type = "bool",
        example = r#"
            respect-gitignore = false
        "#
    )]
    /// Whether to automatically exclude files that are ignored by `.ignore`,
    /// `.gitignore`, `.git/info/exclude`, and global `gitignore` files.
    /// Enabled by default.
    pub respect_gitignore: Option<bool>,
    #[option(
        default = r#"["E", "F"]"#,
        value_type = "list[RuleSelector]",
        example = r#"
            # On top of the defaults (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            select = ["E", "F", "B", "Q"]
        "#
    )]
    /// A list of rule codes or prefixes to enable. Prefixes can specify exact
    /// rules (like `F841`), entire categories (like `F`), or anything in
    /// between.
    ///
    /// When breaking ties between enabled and disabled rules (via `select` and
    /// `ignore`, respectively), more specific prefixes override less
    /// specific prefixes.
    pub select: Option<Vec<RuleSelector>>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # By default, always show source code snippets.
            show-source = true
        "#
    )]
    /// Whether to show source code snippets when reporting lint violations
    /// (overridden by the `--show-source` command-line flag).
    pub show_source: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enumerate all fixed violations.
            show-fixes = true
        "#
    )]
    /// Whether to show an enumeration of all autofixed lint violations
    /// (overridden by the `--show-fixes` command-line flag).
    pub show_fixes: Option<bool>,
    #[option(
        default = r#"["."]"#,
        value_type = "list[str]",
        example = r#"
            # Allow imports relative to the "src" and "test" directories.
            src = ["src", "test"]
        "#
    )]
    /// The directories to consider when resolving first- vs. third-party
    /// imports.
    ///
    /// As an example: given a Python package structure like:
    ///
    /// ```text
    /// my_project
    /// ├── pyproject.toml
    /// └── src
    ///     └── my_package
    ///         ├── __init__.py
    ///         ├── foo.py
    ///         └── bar.py
    /// ```
    ///
    /// The `./src` directory should be included in the `src` option
    /// (e.g., `src = ["src"]`), such that when resolving imports,
    /// `my_package.foo` is considered a first-party import.
    ///
    /// When omitted, the `src` directory will typically default to the
    /// directory containing the nearest `pyproject.toml`, `ruff.toml`, or
    /// `.ruff.toml` file (the "project root"), unless a configuration file
    /// is explicitly provided (e.g., via the `--config` command-line flag).
    ///
    /// This field supports globs. For example, if you have a series of Python
    /// packages in a `python_modules` directory, `src = ["python_modules/*"]`
    /// would expand to incorporate all of the packages in that directory. User
    /// home directory and environment variables will also be expanded.
    pub src: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            namespace-packages = ["airflow/providers"]
        "#
    )]
    /// Mark the specified directories as namespace packages. For the purpose of
    /// module resolution, Ruff will treat those directories as if they
    /// contained an `__init__.py` file.
    pub namespace_packages: Option<Vec<String>>,
    #[option(
        default = r#""py38""#,
        value_type = r#""py37" | "py38" | "py39" | "py310" | "py311" | "py312""#,
        example = r#"
            # Always generate Python 3.7-compatible code.
            target-version = "py37"
        "#
    )]
    /// The minimum Python version to target, e.g., when considering automatic
    /// code upgrades, like rewriting type annotations. Ruff will not propose
    /// changes using features that are not available in the given version.
    ///
    /// For example, to represent supporting Python >=3.10 or ==3.10
    /// specify `target-version = "py310"`.
    ///
    /// If omitted, and Ruff is configured via a `pyproject.toml` file, the
    /// target version will be inferred from its `project.requires-python`
    /// field (e.g., `requires-python = ">=3.8"`). If Ruff is configured via
    /// `ruff.toml` or `.ruff.toml`, no such inference will be performed.
    pub target_version: Option<PythonVersion>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            # Enable preview features
            preview = true
        "#
    )]
    /// Whether to enable preview mode. When preview mode is enabled, Ruff will
    /// use unstable rules and fixes.
    pub preview: Option<bool>,
    #[option(
        default = r#"["TODO", "FIXME", "XXX"]"#,
        value_type = "list[str]",
        example = r#"task-tags = ["HACK"]"#
    )]
    /// A list of task tags to recognize (e.g., "TODO", "FIXME", "XXX").
    ///
    /// Comments starting with these tags will be ignored by commented-out code
    /// detection (`ERA`), and skipped by line-length rules (`E501`) if
    /// `ignore-overlong-task-comments` is set to `true`.
    pub task_tags: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"typing-modules = ["airflow.typing_compat"]"#
    )]
    /// A list of modules whose exports should be treated equivalently to
    /// members of the `typing` module.
    ///
    /// This is useful for ensuring proper type annotation inference for
    /// projects that re-export `typing` and `typing_extensions` members
    /// from a compatibility module. If omitted, any members imported from
    /// modules apart from `typing` and `typing_extensions` will be treated
    /// as ordinary Python objects.
    pub typing_modules: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[RuleSelector]",
        example = r#"
            # Disable autofix for unused imports (`F401`).
            unfixable = ["F401"]
        "#
    )]
    /// A list of rule codes or prefixes to consider non-autofix-able.
    pub unfixable: Option<Vec<RuleSelector>>,
    #[option_group]
    /// Options for the `flake8-annotations` plugin.
    pub flake8_annotations: Option<Flake8AnnotationsOptions>,
    #[option_group]
    /// Options for the `flake8-bandit` plugin.
    pub flake8_bandit: Option<Flake8BanditOptions>,
    #[option_group]
    /// Options for the `flake8-bugbear` plugin.
    pub flake8_bugbear: Option<Flake8BugbearOptions>,
    #[option_group]
    /// Options for the `flake8-builtins` plugin.
    pub flake8_builtins: Option<Flake8BuiltinsOptions>,
    #[option_group]
    /// Options for the `flake8-comprehensions` plugin.
    pub flake8_comprehensions: Option<Flake8ComprehensionsOptions>,
    #[option_group]
    /// Options for the `flake8-copyright` plugin.
    pub flake8_copyright: Option<Flake8CopyrightOptions>,
    #[option_group]
    /// Options for the `flake8-errmsg` plugin.
    pub flake8_errmsg: Option<Flake8ErrMsgOptions>,
    #[option_group]
    /// Options for the `flake8-quotes` plugin.
    pub flake8_quotes: Option<Flake8QuotesOptions>,
    #[option_group]
    /// Options for the `flake8_self` plugin.
    pub flake8_self: Option<Flake8SelfOptions>,
    #[option_group]
    /// Options for the `flake8-tidy-imports` plugin.
    pub flake8_tidy_imports: Option<Flake8TidyImportsOptions>,
    #[option_group]
    /// Options for the `flake8-type-checking` plugin.
    pub flake8_type_checking: Option<Flake8TypeCheckingOptions>,
    #[option_group]
    /// Options for the `flake8-gettext` plugin.
    pub flake8_gettext: Option<Flake8GetTextOptions>,
    #[option_group]
    /// Options for the `flake8-implicit-str-concat` plugin.
    pub flake8_implicit_str_concat: Option<Flake8ImplicitStrConcatOptions>,
    #[option_group]
    /// Options for the `flake8-import-conventions` plugin.
    pub flake8_import_conventions: Option<Flake8ImportConventionsOptions>,
    #[option_group]
    /// Options for the `flake8-pytest-style` plugin.
    pub flake8_pytest_style: Option<Flake8PytestStyleOptions>,
    #[option_group]
    /// Options for the `flake8-unused-arguments` plugin.
    pub flake8_unused_arguments: Option<Flake8UnusedArgumentsOptions>,
    #[option_group]
    /// Options for the `isort` plugin.
    pub isort: Option<IsortOptions>,
    #[option_group]
    /// Options for the `mccabe` plugin.
    pub mccabe: Option<McCabeOptions>,
    #[option_group]
    /// Options for the `pep8-naming` plugin.
    pub pep8_naming: Option<Pep8NamingOptions>,
    #[option_group]
    /// Options for the `pycodestyle` plugin.
    pub pycodestyle: Option<PycodestyleOptions>,
    #[option_group]
    /// Options for the `pydocstyle` plugin.
    pub pydocstyle: Option<PydocstyleOptions>,
    #[option_group]
    /// Options for the `pyflakes` plugin.
    pub pyflakes: Option<PyflakesOptions>,
    #[option_group]
    /// Options for the `pylint` plugin.
    pub pylint: Option<PylintOptions>,
    #[option_group]
    /// Options for the `pyupgrade` plugin.
    pub pyupgrade: Option<PyUpgradeOptions>,
    // Tables are required to go last.
    #[option(
        default = "{}",
        value_type = "dict[str, list[RuleSelector]]",
        example = r#"
            # Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
            [tool.ruff.per-file-ignores]
            "__init__.py" = ["E402"]
            "path/to/file.py" = ["E402"]
        "#
    )]
    /// A list of mappings from file pattern to rule codes or prefixes to
    /// exclude, when considering any matching files.
    pub per_file_ignores: Option<FxHashMap<String, Vec<RuleSelector>>>,
    #[option(
        default = "{}",
        value_type = "dict[str, list[RuleSelector]]",
        example = r#"
            # Also ignore `E402` in all `__init__.py` files.
            [tool.ruff.extend-per-file-ignores]
            "__init__.py" = ["E402"]
        "#
    )]
    /// A list of mappings from file pattern to rule codes or prefixes to
    /// exclude, in addition to any rules excluded by `per-file-ignores`.
    pub extend_per_file_ignores: Option<FxHashMap<String, Vec<RuleSelector>>>,
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(
    Debug, PartialEq, Eq, Default, ConfigurationOptions, CombineOptions, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Flake8AnnotationsOptions {
    #[option(
        default = "false",
        value_type = "bool",
        example = "mypy-init-return = true"
    )]
    /// Whether to allow the omission of a return type hint for `__init__` if at
    /// least one argument is annotated.
    pub mypy_init_return: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-dummy-args = true"
    )]
    /// Whether to suppress `ANN000`-level violations for arguments matching the
    /// "dummy" variable regex (like `_`).
    pub suppress_dummy_args: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "suppress-none-returning = true"
    )]
    /// Whether to suppress `ANN200`-level violations for functions that meet
    /// either of the following criteria:
    ///
    /// - Contain no `return` statement.
    /// - Explicit `return` statement(s) all return `None` (explicitly or
    ///   implicitly).
    pub suppress_none_returning: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-star-arg-any = true"
    )]
    /// Whether to suppress `ANN401` for dynamically typed `*args` and
    /// `**kwargs` arguments.
    pub allow_star_arg_any: Option<bool>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-fully-untyped = true"
    )]
    /// Whether to suppress `ANN*` rules for any declaration
    /// that hasn't been typed at all.
    /// This makes it easier to gradually add types to a codebase.
    pub ignore_fully_untyped: Option<bool>,
}

impl Flake8AnnotationsOptions {
    pub fn into_settings(self) -> ruff::rules::flake8_annotations::settings::Settings {
        ruff::rules::flake8_annotations::settings::Settings {
            mypy_init_return: self.mypy_init_return.unwrap_or(false),
            suppress_dummy_args: self.suppress_dummy_args.unwrap_or(false),
            suppress_none_returning: self.suppress_none_returning.unwrap_or(false),
            allow_star_arg_any: self.allow_star_arg_any.unwrap_or(false),
            ignore_fully_untyped: self.ignore_fully_untyped.unwrap_or(false),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BanditOptions {
    #[option(
        default = "[\"/tmp\", \"/var/tmp\", \"/dev/shm\"]",
        value_type = "list[str]",
        example = "hardcoded-tmp-directory = [\"/foo/bar\"]"
    )]
    /// A list of directories to consider temporary.
    pub hardcoded_tmp_directory: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "extend-hardcoded-tmp-directory = [\"/foo/bar\"]"
    )]
    /// A list of directories to consider temporary, in addition to those
    /// specified by `hardcoded-tmp-directory`.
    pub hardcoded_tmp_directory_extend: Option<Vec<String>>,
    #[option(
        default = "false",
        value_type = "bool",
        example = "check-typed-exception = true"
    )]
    /// Whether to disallow `try`-`except`-`pass` (`S110`) for specific
    /// exception types. By default, `try`-`except`-`pass` is only
    /// disallowed for `Exception` and `BaseException`.
    pub check_typed_exception: Option<bool>,
}

impl Flake8BanditOptions {
    pub fn into_settings(self) -> ruff::rules::flake8_bandit::settings::Settings {
        ruff::rules::flake8_bandit::settings::Settings {
            hardcoded_tmp_directory: self
                .hardcoded_tmp_directory
                .unwrap_or_else(ruff::rules::flake8_bandit::settings::default_tmp_dirs)
                .into_iter()
                .chain(self.hardcoded_tmp_directory_extend.unwrap_or_default())
                .collect(),
            check_typed_exception: self.check_typed_exception.unwrap_or(false),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8BugbearOptions {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow default arguments like, e.g., `data: List[str] = fastapi.Query(None)`.
            extend-immutable-calls = ["fastapi.Depends", "fastapi.Query"]
        "#
    )]
    /// Additional callable functions to consider "immutable" when evaluating, e.g., the
    /// `function-call-in-default-argument` rule (`B008`) or `function-call-in-dataclass-defaults`
    /// rule (`RUF009`).
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `fastapi.Query`, rather than
    /// `Query`).
    pub extend_immutable_calls: Option<Vec<String>>,
}

impl Flake8BugbearOptions {
    pub fn into_settings(self) -> ruff::rules::flake8_bugbear::settings::Settings {
        ruff::rules::flake8_bugbear::settings::Settings {
            extend_immutable_calls: self.extend_immutable_calls.unwrap_or_default(),
        }
    }
}
#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
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
}

impl Flake8BuiltinsOptions {
    pub fn into_settings(self) -> ruff::rules::flake8_builtins::settings::Settings {
        ruff::rules::flake8_builtins::settings::Settings {
            builtins_ignorelist: self.builtins_ignorelist.unwrap_or_default(),
        }
    }
}
#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ComprehensionsOptions {
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-dict-calls-with-keyword-arguments = true"
    )]
    /// Allow `dict` calls that make use of keyword arguments (e.g., `dict(a=1, b=2)`).
    pub allow_dict_calls_with_keyword_arguments: Option<bool>,
}

impl Flake8ComprehensionsOptions {
    pub fn into_settings(self) -> ruff::rules::flake8_comprehensions::settings::Settings {
        ruff::rules::flake8_comprehensions::settings::Settings {
            allow_dict_calls_with_keyword_arguments: self
                .allow_dict_calls_with_keyword_arguments
                .unwrap_or_default(),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8CopyrightOptions {
    #[option(
        default = r#"(?i)Copyright\s+(\(C\)\s+)?\d{4}([-,]\d{4})*"#,
        value_type = "str",
        example = r#"notice-rgx = "(?i)Copyright \\(C\\) \\d{4}""#
    )]
    /// The regular expression used to match the copyright notice, compiled
    /// with the [`regex`](https://docs.rs/regex/latest/regex/) crate.
    ///
    /// Defaults to `(?i)Copyright\s+(\(C\)\s+)?\d{4}(-\d{4})*`, which matches
    /// the following:
    /// - `Copyright 2023`
    /// - `Copyright (C) 2023`
    /// - `Copyright 2021-2023`
    /// - `Copyright (C) 2021-2023`
    pub notice_rgx: Option<String>,
    #[option(default = "None", value_type = "str", example = r#"author = "Ruff""#)]
    /// Author to enforce within the copyright notice. If provided, the
    /// author must be present immediately following the copyright notice.
    pub author: Option<String>,
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Avoid enforcing a header on files smaller than 1024 bytes.
            min-file-size = 1024
        "#
    )]
    /// A minimum file size (in bytes) required for a copyright notice to
    /// be enforced. By default, all files are validated.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ErrMsgOptions {
    #[option(default = "0", value_type = "int", example = "max-string-length = 20")]
    /// Maximum string length for string literals in exception messages.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8GetTextOptions {
    #[option(
        default = r#"["_", "gettext", "ngettext"]"#,
        value_type = "list[str]",
        example = r#"function-names = ["_", "gettext", "ngettext", "ugettetxt"]"#
    )]
    /// The function names to consider as internationalization calls.
    pub function_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-function-names = ["ugettetxt"]"#
    )]
    /// Additional function names to consider as internationalization calls, in addition to those
    /// included in `function-names`.
    pub extend_function_names: Option<Vec<String>>,
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ImplicitStrConcatOptions {
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            allow-multiline = false
        "#
    )]
    /// Whether to allow implicit string concatenations for multiline strings.
    /// By default, implicit concatenations of multiline strings are
    /// allowed (but continuation lines, delimited with a backslash, are
    /// prohibited).
    ///
    /// Note that setting `allow-multiline = false` should typically be coupled
    /// with disabling `explicit-string-concatenation` (`ISC003`). Otherwise,
    /// both explicit and implicit multiline string concatenations will be seen
    /// as violations.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8ImportConventionsOptions {
    #[option(
        default = r#"{"altair": "alt", "matplotlib": "mpl", "matplotlib.pyplot": "plt", "numpy": "np", "pandas": "pd", "seaborn": "sns", "tensorflow": "tf", "tkinter":  "tk", "holoviews": "hv", "panel": "pn", "plotly.express": "px", "polars": "pl", "pyarrow": "pa"}"#,
        value_type = "dict[str, str]",
        example = r#"
            [tool.ruff.flake8-import-conventions.aliases]
            # Declare the default aliases.
            altair = "alt"
            "matplotlib.pyplot" = "plt"
            numpy = "np"
            pandas = "pd"
            seaborn = "sns"
            scipy = "sp"
        "#
    )]
    /// The conventional aliases for imports. These aliases can be extended by
    /// the `extend_aliases` option.
    pub aliases: Option<FxHashMap<String, String>>,
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, str]",
        example = r#"
            [tool.ruff.flake8-import-conventions.extend-aliases]
            # Declare a custom alias for the `matplotlib` module.
            "dask.dataframe" = "dd"
        "#
    )]
    /// A mapping from module to conventional import alias. These aliases will
    /// be added to the `aliases` mapping.
    pub extend_aliases: Option<FxHashMap<String, String>>,
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, list[str]]",
        example = r#"
            [tool.ruff.flake8-import-conventions.banned-aliases]
            # Declare the banned aliases.
            "tensorflow.keras.backend" = ["K"]
    "#
    )]
    /// A mapping from module to its banned import aliases.
    pub banned_aliases: Option<FxHashMap<String, Vec<String>>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Declare the banned `from` imports.
            banned-from = ["typing"]
    "#
    )]
    /// A list of modules that should not be imported from using the
    /// `from ... import ...` syntax.
    ///
    /// For example, given `banned-from = ["pandas"]`, `from pandas import DataFrame`
    /// would be disallowed, while `import pandas` would be allowed.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8PytestStyleOptions {
    #[option(
        default = "true",
        value_type = "bool",
        example = "fixture-parentheses = true"
    )]
    /// Boolean flag specifying whether `@pytest.fixture()` without parameters
    /// should have parentheses. If the option is set to `true` (the
    /// default), `@pytest.fixture()` is valid and `@pytest.fixture` is
    /// invalid. If set to `false`, `@pytest.fixture` is valid and
    /// `@pytest.fixture()` is invalid.
    pub fixture_parentheses: Option<bool>,
    #[option(
        default = "tuple",
        value_type = r#""csv" | "tuple" | "list""#,
        example = "parametrize-names-type = \"list\""
    )]
    /// Expected type for multiple argument names in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `csv` — a comma-separated list, e.g.
    ///   `@pytest.mark.parametrize('name1,name2', ...)`
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), ...)`
    /// - `list` — e.g. `@pytest.mark.parametrize(['name1', 'name2'], ...)`
    pub parametrize_names_type: Option<types::ParametrizeNameType>,
    #[option(
        default = "list",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-type = \"tuple\""
    )]
    /// Expected type for the list of values rows in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `tuple` — e.g. `@pytest.mark.parametrize('name', (1, 2, 3))`
    /// - `list` (default) — e.g. `@pytest.mark.parametrize('name', [1, 2, 3])`
    pub parametrize_values_type: Option<types::ParametrizeValuesType>,
    #[option(
        default = "tuple",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-row-type = \"list\""
    )]
    /// Expected type for each row of values in `@pytest.mark.parametrize` in
    /// case of multiple parameters. The following values are supported:
    ///
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), [(1, 2), (3, 4)])`
    /// - `list` — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), [[1, 2], [3, 4]])`
    pub parametrize_values_row_type: Option<types::ParametrizeValuesRowType>,
    #[option(
        default = r#"["BaseException", "Exception", "ValueError", "OSError", "IOError", "EnvironmentError", "socket.error"]"#,
        value_type = "list[str]",
        example = "raises-require-match-for = [\"requests.RequestException\"]"
    )]
    /// List of exception names that require a match= parameter in a
    /// `pytest.raises()` call.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub raises_require_match_for: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "raises-extend-require-match-for = [\"requests.RequestException\"]"
    )]
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
    pub raises_extend_require_match_for: Option<Vec<String>>,
    #[option(
        default = "true",
        value_type = "bool",
        example = "mark-parentheses = true"
    )]
    /// Boolean flag specifying whether `@pytest.mark.foo()` without parameters
    /// should have parentheses. If the option is set to `true` (the
    /// default), `@pytest.mark.foo()` is valid and `@pytest.mark.foo` is
    /// invalid. If set to `false`, `@pytest.fixture` is valid and
    /// `@pytest.mark.foo()` is invalid.
    pub mark_parentheses: Option<bool>,
}

impl Flake8PytestStyleOptions {
    pub fn try_into_settings(self) -> anyhow::Result<flake8_pytest_style::settings::Settings> {
        Ok(flake8_pytest_style::settings::Settings {
            fixture_parentheses: self.fixture_parentheses.unwrap_or(true),
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
            mark_parentheses: self.mark_parentheses.unwrap_or(true),
        })
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8QuotesOptions {
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            inline-quotes = "single"
        "#
    )]
    /// Quote style to prefer for inline strings (either "single" or
    /// "double").
    pub inline_quotes: Option<Quote>,
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            multiline-quotes = "single"
        "#
    )]
    /// Quote style to prefer for multiline strings (either "single" or
    /// "double").
    pub multiline_quotes: Option<Quote>,
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            docstring-quotes = "single"
        "#
    )]
    /// Quote style to prefer for docstrings (either "single" or "double").
    pub docstring_quotes: Option<Quote>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            # Don't bother trying to avoid escapes.
            avoid-escape = false
        "#
    )]
    /// Whether to avoid using single quotes if a string contains single quotes,
    /// or vice-versa with double quotes, as per [PEP 8](https://peps.python.org/pep-0008/#string-quotes).
    /// This minimizes the need to escape quotation marks within strings.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8SelfOptions {
    #[option(
        default = r#"["_make", "_asdict", "_replace", "_fields", "_field_defaults", "_name_", "_value_"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["_new"]
        "#
    )]
    /// A list of names to ignore when considering `flake8-self` violations.
    pub ignore_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-ignore-names = ["_base_manager", "_default_manager",  "_meta"]"#
    )]
    /// Additional names to ignore when considering `flake8-self` violations,
    /// in addition to those included in `ignore-names`.
    pub extend_ignore_names: Option<Vec<String>>,
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8TidyImportsOptions {
    #[option(
        default = r#""parents""#,
        value_type = r#""parents" | "all""#,
        example = r#"
            # Disallow all relative imports.
            ban-relative-imports = "all"
        "#
    )]
    /// Whether to ban all relative imports (`"all"`), or only those imports
    /// that extend into the parent module or beyond (`"parents"`).
    pub ban_relative_imports: Option<Strictness>,
    #[option(
        default = r#"{}"#,
        value_type = r#"dict[str, { "msg": str }]"#,
        example = r#"
            [tool.ruff.flake8-tidy-imports.banned-api]
            "cgi".msg = "The cgi module is deprecated, see https://peps.python.org/pep-0594/#cgi."
            "typing.TypedDict".msg = "Use typing_extensions.TypedDict instead."
        "#
    )]
    /// Specific modules or module members that may not be imported or accessed.
    /// Note that this rule is only meant to flag accidental uses,
    /// and can be circumvented via `eval` or `importlib`.
    pub banned_api: Option<FxHashMap<String, ApiBan>>,
    #[option(
        default = r#"[]"#,
        value_type = r#"list[str]"#,
        example = r#"
            # Ban certain modules from being imported at module level, instead requiring
            # that they're imported lazily (e.g., within a function definition).
            banned-module-level-imports = ["torch", "tensorflow"]
        "#
    )]
    /// List of specific modules that may not be imported at module level, and should instead be
    /// imported lazily (e.g., within a function definition, or an `if TYPE_CHECKING:`
    /// block, or some other nested context).
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8TypeCheckingOptions {
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            strict = true
        "#
    )]
    /// Enforce TC001, TC002, and TC003 rules even when valid runtime imports
    /// are present for the same module.
    ///
    /// See flake8-type-checking's [strict](https://github.com/snok/flake8-type-checking#strict) option.
    pub strict: Option<bool>,
    #[option(
        default = "[\"typing\"]",
        value_type = "list[str]",
        example = r#"
            exempt-modules = ["typing", "typing_extensions"]
        "#
    )]
    /// Exempt certain modules from needing to be moved into type-checking
    /// blocks.
    pub exempt_modules: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-base-classes = ["pydantic.BaseModel", "sqlalchemy.orm.DeclarativeBase"]
        "#
    )]
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
    pub runtime_evaluated_base_classes: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            runtime-evaluated-decorators = ["attrs.define", "attrs.frozen"]
        "#
    )]
    /// Exempt classes decorated with any of the enumerated decorators from
    /// needing to be moved into type-checking blocks.
    pub runtime_evaluated_decorators: Option<Vec<String>>,
}

impl Flake8TypeCheckingOptions {
    pub fn into_settings(self) -> flake8_type_checking::settings::Settings {
        flake8_type_checking::settings::Settings {
            strict: self.strict.unwrap_or(false),
            exempt_modules: self
                .exempt_modules
                .unwrap_or_else(|| vec!["typing".to_string()]),
            runtime_evaluated_base_classes: self.runtime_evaluated_base_classes.unwrap_or_default(),
            runtime_evaluated_decorators: self.runtime_evaluated_decorators.unwrap_or_default(),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flake8UnusedArgumentsOptions {
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-variadic-names = true"
    )]
    /// Whether to allow unused variadic arguments, like `*args` and `**kwargs`.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IsortOptions {
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-wrap-aliases = true
            combine-as-imports = true
        "#
    )]
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
    /// `combine-as-imports = true`. When `combine-as-imports` isn't
    /// enabled, every aliased `import from` will be given its own line, in
    /// which case, wrapping is not necessary.
    pub force_wrap_aliases: Option<bool>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"force-single-line = true"#
    )]
    /// Forces all from imports to appear on their own line.
    pub force_single_line: Option<bool>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            single-line-exclusions = ["os", "json"]
        "#
    )]
    /// One or more modules to exclude from the single line rule.
    pub single_line_exclusions: Option<Vec<String>>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            combine-as-imports = true
        "#
    )]
    /// Combines as imports on the same line. See isort's [`combine-as-imports`](https://pycqa.github.io/isort/docs/configuration/options.html#combine-as-imports)
    /// option.
    pub combine_as_imports: Option<bool>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            split-on-trailing-comma = false
        "#
    )]
    /// If a comma is placed after the last member in a multi-line import, then
    /// the imports will never be folded into one line.
    ///
    /// See isort's [`split-on-trailing-comma`](https://pycqa.github.io/isort/docs/configuration/options.html#split-on-trailing-comma) option.
    pub split_on_trailing_comma: Option<bool>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            order-by-type = true
        "#
    )]
    /// Order imports by type, which is determined by case, in addition to
    /// alphabetically.
    pub order_by_type: Option<bool>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-sort-within-sections = true
        "#
    )]
    /// Don't sort straight-style imports (like `import sys`) before from-style
    /// imports (like `from itertools import groupby`). Instead, sort the
    /// imports by module, independent of import style.
    pub force_sort_within_sections: Option<bool>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            case-sensitive = true
        "#
    )]
    /// Sort imports taking into account case sensitivity.
    pub case_sensitive: Option<bool>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            force-to-top = ["src"]
        "#
    )]
    /// Force specific imports to the top of their appropriate section.
    pub force_to_top: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-first-party = ["src"]
        "#
    )]
    /// A list of modules to consider first-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub known_first_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-third-party = ["src"]
        "#
    )]
    /// A list of modules to consider third-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub known_third_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-local-folder = ["src"]
        "#
    )]
    /// A list of modules to consider being a local folder.
    /// Generally, this is reserved for relative imports (`from . import module`).
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub known_local_folder: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            extra-standard-library = ["path"]
        "#
    )]
    /// A list of modules to consider standard-library, in addition to those
    /// known to Ruff in advance.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub extra_standard_library: Option<Vec<String>>,
    #[option(
        default = r#"furthest-to-closest"#,
        value_type = r#""furthest-to-closest" | "closest-to-furthest""#,
        example = r#"
            relative-imports-order = "closest-to-furthest"
        "#
    )]
    /// Whether to place "closer" imports (fewer `.` characters, most local)
    /// before "further" imports (more `.` characters, least local), or vice
    /// versa.
    ///
    /// The default ("furthest-to-closest") is equivalent to isort's
    /// `reverse-relative` default (`reverse-relative = false`); setting
    /// this to "closest-to-furthest" is equivalent to isort's
    /// `reverse-relative = true`.
    pub relative_imports_order: Option<RelativeImportsOrder>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            required-imports = ["from __future__ import annotations"]
        "#
    )]
    /// Add the specified import line to all files.
    pub required_imports: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            classes = ["SVC"]
        "#
    )]
    /// An override list of tokens to always recognize as a Class for
    /// `order-by-type` regardless of casing.
    pub classes: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            constants = ["constant"]
        "#
    )]
    /// An override list of tokens to always recognize as a CONSTANT
    /// for `order-by-type` regardless of casing.
    pub constants: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            variables = ["VAR"]
        "#
    )]
    /// An override list of tokens to always recognize as a var
    /// for `order-by-type` regardless of casing.
    pub variables: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = r#"list["future" | "standard-library" | "third-party" | "first-party" | "local-folder" | str]"#,
        example = r#"
            no-lines-before = ["future", "standard-library"]
        "#
    )]
    /// A list of sections that should _not_ be delineated from the previous
    /// section via empty lines.
    pub no_lines_before: Option<Vec<ImportSection>>,
    #[option(
        default = r#"-1"#,
        value_type = "int",
        example = r#"
            # Use a single line after each import block.
            lines-after-imports = 1
        "#
    )]
    /// The number of blank lines to place after imports.
    /// Use `-1` for automatic determination.
    pub lines_after_imports: Option<isize>,
    #[option(
        default = r#"0"#,
        value_type = "int",
        example = r#"
            # Use a single line between direct and from import
            lines-between-types = 1
        "#
    )]
    /// The number of lines to place between "direct" and `import from` imports.
    pub lines_between_types: Option<usize>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            forced-separate = ["tests"]
        "#
    )]
    /// A list of modules to separate into auxiliary block(s) of imports,
    /// in the order specified.
    pub forced_separate: Option<Vec<String>>,
    #[option(
        default = r#"["future", "standard-library", "third-party", "first-party", "local-folder"]"#,
        value_type = r#"list["future" | "standard-library" | "third-party" | "first-party" | "local-folder" | str]"#,
        example = r#"
            section-order = ["future", "standard-library", "first-party", "local-folder", "third-party"]
        "#
    )]
    /// Override in which order the sections should be output. Can be used to move custom sections.
    pub section_order: Option<Vec<ImportSection>>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            detect-same-package = false
        "#
    )]
    /// Whether to automatically mark imports from within the same package as first-party.
    /// For example, when `detect-same-package = true`, then when analyzing files within the
    /// `foo` package, any imports from within the `foo` package will be considered first-party.
    ///
    /// This heuristic is often unnecessary when `src` is configured to detect all first-party
    /// sources; however, if `src` is _not_ configured, this heuristic can be useful to detect
    /// first-party imports from _within_ (but not _across_) first-party packages.
    pub detect_same_package: Option<bool>,
    // Tables are required to go last.
    #[option(
        default = "{}",
        value_type = "dict[str, list[str]]",
        example = r#"
            # Group all Django imports into a separate section.
            [tool.ruff.isort.sections]
            "django" = ["django"]
        "#
    )]
    /// A list of mappings from section names to modules.
    /// By default custom sections are output last, but this can be overridden with `section-order`.
    pub sections: Option<FxHashMap<ImportSection, Vec<String>>>,
}

impl IsortOptions {
    pub fn try_into_settings(
        self,
    ) -> Result<isort::settings::Settings, isort::settings::SettingsError> {
        // Extract any configuration options that deal with user-defined sections.
        let mut section_order: Vec<_> = self
            .section_order
            .unwrap_or_else(|| ImportType::iter().map(ImportSection::Known).collect());
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
        let mut seen =
            FxHashSet::with_capacity_and_hasher(section_order.len(), BuildHasherDefault::default());
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

        // Add all built-in sections to `section_order`, if not already present.
        for section in ImportType::iter().map(ImportSection::Known) {
            if !section_order.contains(&section) {
                warn_user_once!(
                    "`section-order` is missing built-in section: `{:?}`",
                    section
                );
                section_order.push(section);
            }
        }

        // Add all user-defined sections to `section-order`, if not already present.
        for section_name in sections.keys() {
            let section = ImportSection::UserDefined(section_name.clone());
            if !section_order.contains(&section) {
                warn_user_once!("`section-order` is missing section: `{:?}`", section);
                section_order.push(section);
            }
        }

        Ok(isort::settings::Settings {
            required_imports: BTreeSet::from_iter(self.required_imports.unwrap_or_default()),
            combine_as_imports: self.combine_as_imports.unwrap_or(false),
            force_single_line: self.force_single_line.unwrap_or(false),
            force_sort_within_sections: self.force_sort_within_sections.unwrap_or(false),
            case_sensitive: self.case_sensitive.unwrap_or(false),
            force_wrap_aliases: self.force_wrap_aliases.unwrap_or(false),
            detect_same_package: self.detect_same_package.unwrap_or(true),
            force_to_top: BTreeSet::from_iter(self.force_to_top.unwrap_or_default()),
            known_modules: isort::categorize::KnownModules::new(
                known_first_party,
                known_third_party,
                known_local_folder,
                extra_standard_library,
                sections,
            ),
            order_by_type: self.order_by_type.unwrap_or(true),
            relative_imports_order: self.relative_imports_order.unwrap_or_default(),
            single_line_exclusions: BTreeSet::from_iter(
                self.single_line_exclusions.unwrap_or_default(),
            ),
            split_on_trailing_comma: self.split_on_trailing_comma.unwrap_or(true),
            classes: BTreeSet::from_iter(self.classes.unwrap_or_default()),
            constants: BTreeSet::from_iter(self.constants.unwrap_or_default()),
            variables: BTreeSet::from_iter(self.variables.unwrap_or_default()),
            no_lines_before: BTreeSet::from_iter(no_lines_before),
            lines_after_imports: self.lines_after_imports.unwrap_or(-1),
            lines_between_types: self.lines_between_types.unwrap_or_default(),
            forced_separate: Vec::from_iter(self.forced_separate.unwrap_or_default()),
            section_order,
        })
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct McCabeOptions {
    #[option(
        default = "10",
        value_type = "int",
        example = r#"
            # Flag errors (`C901`) whenever the complexity level exceeds 5.
            max-complexity = 5
        "#
    )]
    /// The maximum McCabe complexity to allow before triggering `C901` errors.
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
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Pep8NamingOptions {
    #[option(
        default = r#"["setUp", "tearDown", "setUpClass", "tearDownClass", "setUpModule", "tearDownModule", "asyncSetUp", "asyncTearDown", "setUpTestData", "failureException", "longMessage", "maxDiff"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["callMethod"]
        "#
    )]
    /// A list of names (or patterns) to ignore when considering `pep8-naming` violations.
    pub ignore_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"extend-ignore-names = ["callMethod"]"#
    )]
    /// Additional names (or patterns) to ignore when considering `pep8-naming` violations,
    /// in addition to those included in `ignore-names`.
    pub extend_ignore_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow Pydantic's `@validator` decorator to trigger class method treatment.
            classmethod-decorators = ["pydantic.validator"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a class method (in addition to the builtin
    /// `@classmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list takes a `cls` argument as its first argument.
    pub classmethod_decorators: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow a shorthand alias, `@stcmthd`, to trigger static method treatment.
            staticmethod-decorators = ["stcmthd"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a static method (in addition to the builtin
    /// `@staticmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list has no `self` or `cls` argument.
    pub staticmethod_decorators: Option<Vec<String>>,
}

impl Pep8NamingOptions {
    pub fn try_into_settings(
        self,
    ) -> Result<pep8_naming::settings::Settings, pep8_naming::settings::SettingsError> {
        Ok(pep8_naming::settings::Settings {
            ignore_names: self
                .ignore_names
                .unwrap_or_else(pep8_naming::settings::default_ignore_names)
                .into_iter()
                .chain(self.extend_ignore_names.unwrap_or_default())
                .map(|name| {
                    IdentifierPattern::new(&name)
                        .map_err(pep8_naming::settings::SettingsError::InvalidIgnoreName)
                })
                .collect::<Result<Vec<_>, pep8_naming::settings::SettingsError>>()?,
            classmethod_decorators: self.classmethod_decorators.unwrap_or_default(),
            staticmethod_decorators: self.staticmethod_decorators.unwrap_or_default(),
        })
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PycodestyleOptions {
    #[option(
        default = "None",
        value_type = "int",
        example = r#"
            max-doc-length = 88
        "#
    )]
    /// The maximum line length to allow for line-length violations within
    /// documentation (`W505`), including standalone comments. By default,
    /// this is set to null which disables reporting violations.
    ///
    /// See the [`doc-line-too-long`](https://beta.ruff.rs/docs/rules/doc-line-too-long/) rule for more information.
    pub max_doc_length: Option<LineLength>,
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-overlong-task-comments = true
        "#
    )]
    /// Whether line-length violations (`E501`) should be triggered for
    /// comments starting with `task-tags` (by default: \["TODO", "FIXME",
    /// and "XXX"\]).
    pub ignore_overlong_task_comments: Option<bool>,
}

impl PycodestyleOptions {
    pub fn into_settings(self) -> pycodestyle::settings::Settings {
        pycodestyle::settings::Settings {
            max_doc_length: self.max_doc_length,
            ignore_overlong_task_comments: self.ignore_overlong_task_comments.unwrap_or_default(),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PydocstyleOptions {
    #[option(
        default = r#"None"#,
        value_type = r#""google" | "numpy" | "pep257""#,
        example = r#"
            # Use Google-style docstrings.
            convention = "google"
        "#
    )]
    /// Whether to use Google-style or NumPy-style conventions or the PEP257
    /// defaults when analyzing docstring sections.
    ///
    /// Enabling a convention will force-disable any rules that are not
    /// included in the specified convention. As such, the intended use is
    /// to enable a convention and then selectively disable any additional
    /// rules on top of it.
    ///
    /// For example, to use Google-style conventions but avoid requiring
    /// documentation for every function parameter:
    ///
    /// ```toml
    /// [tool.ruff]
    /// # Enable all `pydocstyle` rules, limiting to those that adhere to the
    /// # Google convention via `convention = "google"`, below.
    /// select = ["D"]
    ///
    /// # On top of the Google convention, disable `D417`, which requires
    /// # documentation for every function parameter.
    /// ignore = ["D417"]
    ///
    /// [tool.ruff.pydocstyle]
    /// convention = "google"
    /// ```
    ///
    /// As conventions force-disable all rules not included in the convention,
    /// enabling _additional_ rules on top of a convention is currently
    /// unsupported.
    pub convention: Option<Convention>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            ignore-decorators = ["typing.overload"]
        "#
    )]
    /// Ignore docstrings for functions or methods decorated with the
    /// specified fully-qualified decorators.
    pub ignore_decorators: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            property-decorators = ["gi.repository.GObject.Property"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a property (in addition to the builtin
    /// `@property` and standard-library `@functools.cached_property`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list can use a non-imperative summary line.
    pub property_decorators: Option<Vec<String>>,
}

impl PydocstyleOptions {
    pub fn into_settings(self) -> pydocstyle::settings::Settings {
        pydocstyle::settings::Settings {
            convention: self.convention,
            ignore_decorators: BTreeSet::from_iter(self.ignore_decorators.unwrap_or_default()),
            property_decorators: BTreeSet::from_iter(self.property_decorators.unwrap_or_default()),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PyflakesOptions {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "extend-generics = [\"django.db.models.ForeignKey\"]"
    )]
    /// Additional functions or classes to consider generic, such that any
    /// subscripts should be treated as type annotation (e.g., `ForeignKey` in
    /// `django.db.models.ForeignKey["User"]`.
    ///
    /// Expects to receive a list of fully-qualified names (e.g., `django.db.models.ForeignKey`,
    /// rather than `ForeignKey`).
    pub extend_generics: Option<Vec<String>>,
}

impl PyflakesOptions {
    pub fn into_settings(self) -> pyflakes::settings::Settings {
        pyflakes::settings::Settings {
            extend_generics: self.extend_generics.unwrap_or_default(),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PylintOptions {
    #[option(
        default = r#"["str", "bytes"]"#,
        value_type = r#"list["str" | "bytes" | "complex" | "float" | "int" | "tuple"]"#,
        example = r#"
            allow-magic-value-types = ["int"]
        "#
    )]
    /// Constant types to ignore when used as "magic values" (see: `PLR2004`).
    pub allow_magic_value_types: Option<Vec<ConstantType>>,
    #[option(default = r"12", value_type = "int", example = r"max-branches = 12")]
    /// Maximum number of branches allowed for a function or method body (see:
    /// `PLR0912`).
    pub max_branches: Option<usize>,
    #[option(default = r"6", value_type = "int", example = r"max-returns = 6")]
    /// Maximum number of return statements allowed for a function or method
    /// body (see `PLR0911`)
    pub max_returns: Option<usize>,
    #[option(default = r"5", value_type = "int", example = r"max-args = 5")]
    /// Maximum number of arguments allowed for a function or method definition
    /// (see: `PLR0913`).
    pub max_args: Option<usize>,
    #[option(default = r"50", value_type = "int", example = r"max-statements = 50")]
    /// Maximum number of statements allowed for a function or method body (see:
    /// `PLR0915`).
    pub max_statements: Option<usize>,
    #[option(
        default = r"20",
        value_type = "int",
        example = r"max-public-methods = 20"
    )]
    /// Maximum number of public methods allowed for a class (see: `PLR0904`).
    pub max_public_methods: Option<usize>,
}

impl PylintOptions {
    pub fn into_settings(self) -> pylint::settings::Settings {
        let defaults = pylint::settings::Settings::default();
        pylint::settings::Settings {
            allow_magic_value_types: self
                .allow_magic_value_types
                .unwrap_or(defaults.allow_magic_value_types),
            max_args: self.max_args.unwrap_or(defaults.max_args),
            max_returns: self.max_returns.unwrap_or(defaults.max_returns),
            max_branches: self.max_branches.unwrap_or(defaults.max_branches),
            max_statements: self.max_statements.unwrap_or(defaults.max_statements),
            max_public_methods: self
                .max_public_methods
                .unwrap_or(defaults.max_public_methods),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PyUpgradeOptions {
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            # Preserve types, even if a file imports `from __future__ import annotations`.
            keep-runtime-typing = true
        "#
    )]
    /// Whether to avoid PEP 585 (`List[int]` -> `list[int]`) and PEP 604
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
    /// presence of `from __future__ import annotations`, the use of `str| int`
    /// prior to Python 3.10 will cause Pydantic to raise a `TypeError` at
    /// runtime:
    ///
    /// ```python
    /// from __future__ import annotations
    ///
    /// import pydantic
    ///
    /// class Foo(pydantic.BaseModel):
    ///    bar: str | int
    /// ```
    ///
    ///
    pub keep_runtime_typing: Option<bool>,
}

impl PyUpgradeOptions {
    pub fn into_settings(self) -> pyupgrade::settings::Settings {
        pyupgrade::settings::Settings {
            keep_runtime_typing: self.keep_runtime_typing.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::options::Flake8SelfOptions;
    use ruff::rules::flake8_self;

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
            ignore_names: Some(vec!["_foo".to_string()]),
            extend_ignore_names: None,
        };
        let settings = options.into_settings();
        assert_eq!(settings.ignore_names, vec!["_foo".to_string()]);

        // Appends extend_ignore_names to defaults if only extend_ignore_names is specified.
        let options = Flake8SelfOptions {
            ignore_names: None,
            extend_ignore_names: Some(vec!["_bar".to_string()]),
        };
        let settings = options.into_settings();
        assert_eq!(
            settings.ignore_names,
            default_settings
                .ignore_names
                .into_iter()
                .chain(["_bar".to_string()])
                .collect::<Vec<String>>()
        );

        // Appends extend_ignore_names to ignore_names if both are specified.
        let options = Flake8SelfOptions {
            ignore_names: Some(vec!["_foo".to_string()]),
            extend_ignore_names: Some(vec!["_bar".to_string()]),
        };
        let settings = options.into_settings();
        assert_eq!(
            settings.ignore_names,
            vec!["_foo".to_string(), "_bar".to_string()]
        );
    }
}
