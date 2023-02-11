//! Options that the user can provide via pyproject.toml.

use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::rule_selector::RuleSelector;
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_errmsg,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_pytest_style, flake8_quotes,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pylint, pyupgrade,
};
use crate::settings::types::{PythonVersion, SerializationFormat, Version};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
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
        default = r#"[".bzr", ".direnv", ".eggs", ".git", ".hg", ".mypy_cache", ".nox", ".pants.d", ".ruff_cache", ".svn", ".tox", ".venv", "__pypackages__", "_build", "buck-out", "build", "dist", "node_modules", "venv"]"#,
        value_type = "list[str]",
        example = r#"
            exclude = [".venv"]
        "#
    )]
    /// A list of file patterns to exclude from linting.
    ///
    /// Exclusions are based on globs, and can be either:
    ///
    /// * Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// * Relative patterns, like `directory/foo.py` (to exclude that specific
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
    /// * Single-path patterns, like `.mypy_cache` (to exclude any directory
    ///   named `.mypy_cache` in the tree), `foo.py` (to exclude any file named
    ///   `foo.py`), or `foo_*.py` (to exclude any file matching `foo_*.py` ).
    /// * Relative patterns, like `directory/foo.py` (to exclude that specific
    ///   file) or `directory/*.py` (to exclude any Python files in
    ///   `directory`). Note that these paths are relative to the project root
    ///   (e.g., the directory containing your `pyproject.toml`).
    ///
    /// For more information on the glob syntax, refer to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub extend_exclude: Option<Vec<String>>,
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
    #[schemars(skip)]
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
        default = r#"["A", "ANN", "ARG", "B", "BLE", "C", "COM", "D", "DTZ", "E", "EM", "ERA", "EXE", "F", "FBT", "G", "I", "ICN", "INP", "ISC", "N", "PD", "PGH", "PIE", "PL", "PT", "PTH", "Q", "RET", "RUF", "S", "SIM", "T", "TCH", "TID", "TRY", "UP", "W", "YTT"]"#,
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
        value_type = r#""text" | "json" | "junit" | "github" | "gitlab" | "pylint""#,
        example = r#"
            # Group violations by containing file.
            format = "grouped"
        "#
    )]
    /// The style in which violation messages should be formatted: `"text"`
    /// (default), `"grouped"` (group messages by file), `"json"`
    /// (machine-readable), `"junit"` (machine-readable XML), `"github"` (GitHub
    /// Actions annotations), `"gitlab"` (GitLab CI code quality report), or
    /// `"pylint"` (Pylint text format).
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
    /// changed files to the [`ruff-pre-commit`](https://github.com/charliermarsh/ruff-pre-commit)
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
        default = "88",
        value_type = "int",
        example = r#"
            # Allow lines to be as long as 120 characters.
            line-length = 120
        "#
    )]
    /// The line length to use when enforcing long-lines violations (like
    /// `E501`).
    pub line_length: Option<usize>,
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
        default = r#"["."]"#,
        value_type = "list[str]",
        example = r#"
            # Allow imports relative to the "src" and "test" directories.
            src = ["src", "test"]
        "#
    )]
    /// The source code paths to consider, e.g., when resolving first- vs.
    /// third-party imports.
    ///
    /// As an example: given a Python package structure like:
    ///
    /// ```text
    /// my_package/
    ///   pyproject.toml
    ///   src/
    ///     my_package/
    ///       __init__.py
    ///       foo.py
    ///       bar.py
    /// ```
    ///
    /// The `src` directory should be included in the `src` option (e.g., `src =
    /// ["src"]`), such that when resolving imports, `my_package.foo` is
    /// considered a first-party import.
    ///
    /// This field supports globs. For example, if you have a series of Python
    /// packages in a `python_modules` directory, `src =
    /// ["python_modules/*"]` would expand to incorporate all of the
    /// packages in that directory. User home directory and environment
    /// variables will also be expanded.
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
        default = r#""py310""#,
        value_type = r#""py37" | "py38" | "py39" | "py310" | "py311""#,
        example = r#"
            # Always generate Python 3.7-compatible code.
            target-version = "py37"
        "#
    )]
    /// The Python version to target, e.g., when considering automatic code
    /// upgrades, like rewriting type annotations. Note that the target
    /// version will _not_ be inferred from the _current_ Python version,
    /// and instead must be specified explicitly (as seen below).
    pub target_version: Option<PythonVersion>,
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
    /// A list of modules whose imports should be treated equivalently to
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
    #[option(
        default = "false",
        value_type = "bool",
        example = "update-check = true"
    )]
    /// Enable or disable automatic update checks (overridden by the
    /// `--update-check` and `--no-update-check` command-line flags).
    pub update_check: Option<bool>,
    #[option_group]
    /// Options for the `flake8-annotations` plugin.
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    #[option_group]
    /// Options for the `flake8-bandit` plugin.
    pub flake8_bandit: Option<flake8_bandit::settings::Options>,
    #[option_group]
    /// Options for the `flake8-bugbear` plugin.
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    #[option_group]
    /// Options for the `flake8-builtins` plugin.
    pub flake8_builtins: Option<flake8_builtins::settings::Options>,
    #[option_group]
    /// Options for the `flake8-errmsg` plugin.
    pub flake8_errmsg: Option<flake8_errmsg::settings::Options>,
    #[option_group]
    /// Options for the `flake8-quotes` plugin.
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    #[option_group]
    /// Options for the `flake8-tidy-imports` plugin.
    pub flake8_tidy_imports: Option<flake8_tidy_imports::options::Options>,
    #[option_group]
    /// Options for the `flake8-type-checking` plugin.
    pub flake8_type_checking: Option<flake8_type_checking::settings::Options>,
    #[option_group]
    /// Options for the `flake8-implicit-str-concat` plugin.
    pub flake8_implicit_str_concat: Option<flake8_implicit_str_concat::settings::Options>,
    #[option_group]
    /// Options for the `flake8-import-conventions` plugin.
    pub flake8_import_conventions: Option<flake8_import_conventions::settings::Options>,
    #[option_group]
    /// Options for the `flake8-pytest-style` plugin.
    pub flake8_pytest_style: Option<flake8_pytest_style::settings::Options>,
    #[option_group]
    /// Options for the `flake8-unused-arguments` plugin.
    pub flake8_unused_arguments: Option<flake8_unused_arguments::settings::Options>,
    #[option_group]
    /// Options for the `isort` plugin.
    pub isort: Option<isort::settings::Options>,
    #[option_group]
    /// Options for the `mccabe` plugin.
    pub mccabe: Option<mccabe::settings::Options>,
    #[option_group]
    /// Options for the `pep8-naming` plugin.
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    #[option_group]
    /// Options for the `pycodestyle` plugin.
    pub pycodestyle: Option<pycodestyle::settings::Options>,
    #[option_group]
    /// Options for the `pydocstyle` plugin.
    pub pydocstyle: Option<pydocstyle::settings::Options>,
    #[option_group]
    /// Options for the `pylint` plugin.
    pub pylint: Option<pylint::settings::Options>,
    #[option_group]
    /// Options for the `pyupgrade` plugin.
    pub pyupgrade: Option<pyupgrade::settings::Options>,
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
}
