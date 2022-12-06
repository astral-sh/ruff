//! Options that the user can provide via pyproject.toml.

use ruff_macros::ConfigurationOptions;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::checks_gen::CheckCodePrefix;
use crate::settings::types::{PythonVersion, SerializationFormat};
use crate::{
    flake8_annotations, flake8_bugbear, flake8_import_conventions, flake8_quotes,
    flake8_tidy_imports, isort, mccabe, pep8_naming, pyupgrade,
};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = r#"
            A list of allowed "confusable" Unicode characters to ignore when enforcing `RUF001`,
            `RUF002`, and `RUF003`.
        "#,
        default = r#"[]"#,
        value_type = "Vec<char>",
        example = r#"
            # Allow minus-sign (U+2212), greek-small-letter-rho (U+03C1), and the asterisk-operator (U+2217),
            # which could be confused for "-", "p", and "*", respectively.
            allowed-confusables = ["−", "ρ", "∗"]
        "#
    )]
    pub allowed_confusables: Option<Vec<char>>,
    #[option(
        doc = r#"
            A regular expression used to identify "dummy" variables, or those which should be
            ignored when evaluating (e.g.) unused-variable checks. The default expression matches
            `_`, `__`, and `_var`, but not `_var_`.
        "#,
        default = r#""^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$""#,
        value_type = "Regex",
        example = r#"
            # Only ignore variables named "_".
            dummy_variable_rgx = "^_$"
        "#
    )]
    pub dummy_variable_rgx: Option<String>,
    #[option(
        doc = r#"
            A list of file patterns to exclude from linting.

            Exclusions are based on globs, and can be either:

            - Single-path patterns, like `.mypy_cache` (to exclude any directory named `.mypy_cache` in the
              tree), `foo.py` (to exclude any file named `foo.py`), or `foo_*.py` (to exclude any file matching
              `foo_*.py` ).
            - Relative patterns, like `directory/foo.py` (to exclude that specific file) or `directory/*.py`
              (to exclude any Python files in `directory`). Note that these paths are relative to the
              project root (e.g., the directory containing your `pyproject.toml`).

            Note that you'll typically want to use [`extend_exclude`](#extend_exclude) to modify
            the excluded paths.
        "#,
        default = r#"[".bzr", ".direnv", ".eggs", ".git", ".hg", ".mypy_cache", ".nox", ".pants.d", ".ruff_cache", ".svn", ".tox", ".venv", "__pypackages__", "_build", "buck-out", "build", "dist", "node_modules", "venv"]"#,
        value_type = "Vec<FilePattern>",
        example = r#"
            exclude = [".venv"]
        "#
    )]
    pub exclude: Option<Vec<String>>,
    #[option(
        doc = "A list of file patterns to omit from linting, in addition to those specified by \
               `exclude`.",
        default = "[]",
        value_type = "Vec<FilePattern>",
        example = r#"
            # In addition to the standard set of exclusions, omit all tests, plus a specific file.
            extend-exclude = ["tests", "src/bad.py"]
        "#
    )]
    pub extend_exclude: Option<Vec<String>>,
    #[option(
        doc = "A list of check code prefixes to ignore, in addition to those specified by \
               `ignore`.",
        default = "[]",
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # Skip unused variable checks (`F841`).
            extend-ignore = ["F841"]
        "#
    )]
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    #[option(
        doc = "A list of check code prefixes to enable, in addition to those specified by \
               `select`.",
        default = "[]",
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # On top of the default `select` (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            extend-select = ["B", "Q"]
        "#
    )]
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    #[option(
        doc = r#"
            A list of check codes that are unsupported by Ruff, but should be preserved when (e.g.)
            validating `# noqa` directives. Useful for retaining `# noqa` directives that cover plugins not
            yet implemented in Ruff.
        "#,
        default = "[]",
        value_type = "Vec<String>",
        example = r#"
            # Avoiding flagging (and removing) `V101` from any `# noqa`
            # directives, despite Ruff's lack of support for `vulture`.
            external = ["V101"]
        "#
    )]
    pub external: Option<Vec<String>>,
    #[option(
        doc = r#"
            Enable autofix behavior by-default when running `ruff` (overridden
            by the `--fix` and `--no-fix` command-line flags).
        "#,
        default = "false",
        value_type = "bool",
        example = "fix = true"
    )]
    pub fix: Option<bool>,
    #[option(
        doc = "A list of check code prefixes to consider autofix-able.",
        default = r#"["A", "ANN", "B", "BLE", "C", "D", "E", "F", "FBT", "I", "M", "N", "Q", "RUF", "S", "T", "U", "W", "YTT"]"#,
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # Only allow autofix behavior for `E` and `F` checks.
            fixable = ["E", "F"]
        "#
    )]
    pub fixable: Option<Vec<CheckCodePrefix>>,
    #[option(
        doc = r#"
            The style in which violation messages should be formatted: `"text"` (default),
            `"grouped"` (group messages by file), `"json"` (machine-readable), `"junit"`
            (machine-readable XML), or `"github"` (GitHub Actions annotations).
        "#,
        default = r#""text""#,
        value_type = "SerializationType",
        example = r#"
            # Group violations by containing file.
            format = "grouped"
        "#
    )]
    pub format: Option<SerializationFormat>,
    #[option(
        doc = r"
            A list of check code prefixes to ignore. Prefixes can specify exact checks (like
            `F841`), entire categories (like `F`), or anything in between.

            When breaking ties between enabled and disabled checks (via `select` and `ignore`,
            respectively), more specific prefixes override less specific prefixes.
        ",
        default = "[]",
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # Skip unused variable checks (`F841`).
            ignore = ["F841"]
        "#
    )]
    pub ignore: Option<Vec<CheckCodePrefix>>,
    #[option(
        doc = r#"
            Avoid automatically removing unused imports in `__init__.py` files. Such imports will
            still be +flagged, but with a dedicated message suggesting that the import is either
            added to the module' +`__all__` symbol, or re-exported with a redundant alias (e.g.,
            `import os as os`).
        "#,
        default = "false",
        value_type = "bool",
        example = r#"
            ignore-init-module-imports = true
        "#
    )]
    pub ignore_init_module_imports: Option<bool>,
    #[option(
        doc = "The line length to use when enforcing long-lines violations (like E501).",
        default = "88",
        value_type = "usize",
        example = r#"
            # Allow lines to be as long as 120 characters.
            line-length = 120
        "#
    )]
    pub line_length: Option<usize>,
    #[option(
        doc = r#"
            A list of check code prefixes to enable. Prefixes can specify exact checks (like
            `F841`), entire categories (like `F`), or anything in between.

            When breaking ties between enabled and disabled checks (via `select` and `ignore`,
            respectively), more specific prefixes override less specific prefixes.
        "#,
        default = r#"["E", "F"]"#,
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # On top of the defaults (`E`, `F`), enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
            select = ["E", "F", "B", "Q"]
        "#
    )]
    pub select: Option<Vec<CheckCodePrefix>>,
    #[option(
        doc = r#"
            Whether to show source code snippets when reporting lint error violations (overridden by
            the `--show-source` command-line flag).
        "#,
        default = "false",
        value_type = "bool",
        example = r#"
            # By default, always show source code snippets.
            show-source = true
        "#
    )]
    pub show_source: Option<bool>,
    #[option(
        doc = "The source code paths to consider, e.g., when resolving first- vs. third-party \
               imports.",
        default = r#"["."]"#,
        value_type = "Vec<PathBuf>",
        example = r#"
            # Allow imports relative to the "src" and "test" directories.
            src = ["src", "test"]
        "#
    )]
    pub src: Option<Vec<String>>,
    #[option(
        doc = r#"
            The Python version to target, e.g., when considering automatic code upgrades, like
            rewriting type annotations. Note that the target version will _not_ be inferred from the
            _current_ Python version, and instead must be specified explicitly (as seen below).
        "#,
        default = r#""py310""#,
        value_type = "PythonVersion",
        example = r#"
            # Always generate Python 3.7-compatible code.
            target-version = "py37"
        "#
    )]
    pub target_version: Option<PythonVersion>,
    #[option(
        doc = "A list of check code prefixes to consider un-autofix-able.",
        default = "[]",
        value_type = "Vec<CheckCodePrefix>",
        example = r#"
            # Disable autofix for unused imports (`F401`).
            unfixable = ["F401"]
        "#
    )]
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    // Plugins
    #[option_group]
    pub flake8_annotations: Option<flake8_annotations::settings::Options>,
    #[option_group]
    pub flake8_bugbear: Option<flake8_bugbear::settings::Options>,
    #[option_group]
    pub flake8_quotes: Option<flake8_quotes::settings::Options>,
    #[option_group]
    pub flake8_tidy_imports: Option<flake8_tidy_imports::settings::Options>,
    #[option_group]
    pub flake8_import_conventions: Option<flake8_import_conventions::settings::Options>,
    #[option_group]
    pub isort: Option<isort::settings::Options>,
    #[option_group]
    pub mccabe: Option<mccabe::settings::Options>,
    #[option_group]
    pub pep8_naming: Option<pep8_naming::settings::Options>,
    #[option_group]
    pub pyupgrade: Option<pyupgrade::settings::Options>,
    // Tables are required to go last.
    #[option(
        doc = r#"
            A list of mappings from file pattern to check code prefixes to exclude, when considering
            any matching files.
        "#,
        default = "{}",
        value_type = "HashMap<String, Vec<CheckCodePrefix>>",
        example = r#"
            # Ignore `E402` (import violations) in all `__init__.py` files, and in `path/to/file.py`.
            [tool.ruff.per-file-ignores]
            "__init__.py" = ["E402"]
            "path/to/file.py" = ["E402"]
        "#
    )]
    pub per_file_ignores: Option<FxHashMap<String, Vec<CheckCodePrefix>>>,
}
