# Configuring Ruff

Ruff can be configured through a `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file.

Whether you're using Ruff as a linter, formatter, or both, the underlying configuration strategy and
semantics are the same.

For a complete enumeration of the available configuration options, see [_Settings_](settings.md).

If left unspecified, Ruff's default configuration is equivalent to:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    # Exclude a variety of commonly ignored directories.
    exclude = [
        ".bzr",
        ".direnv",
        ".eggs",
        ".git",
        ".git-rewrite",
        ".hg",
        ".ipynb_checkpoints",
        ".mypy_cache",
        ".nox",
        ".pants.d",
        ".pyenv",
        ".pytest_cache",
        ".pytype",
        ".ruff_cache",
        ".svn",
        ".tox",
        ".venv",
        ".vscode",
        "__pypackages__",
        "_build",
        "buck-out",
        "build",
        "dist",
        "node_modules",
        "site-packages",
        "venv",
    ]

    # Same as Black.
    line-length = 88
    indent-width = 4

    # Assume Python 3.10
    target-version = "py310"

    [tool.ruff.lint]
    select = [
        "ASYNC100", "ASYNC105", "ASYNC115", "ASYNC116", "ASYNC210", "ASYNC220",
        "ASYNC221", "ASYNC222", "ASYNC230", "ASYNC251", "B002", "B003", "B004",
        "B005", "B006", "B008", "B009", "B010", "B012", "B013", "B014", "B015",
        "B016", "B017", "B018", "B019", "B020", "B021", "B022", "B023", "B025",
        "B026", "B029", "B030", "B031", "B032", "B033", "B035", "B039",
        "BLE001", "C400", "C401", "C402", "C403", "C404", "C405", "C406",
        "C408", "C409", "C410", "C411", "C413", "C414", "C415", "C417", "C418",
        "C419", "D419", "DTZ001", "DTZ002", "DTZ003", "DTZ004", "DTZ005",
        "DTZ006", "DTZ007", "DTZ011", "DTZ012", "DTZ901", "E722", "E902",
        "EXE001", "EXE002", "EXE004", "EXE005", "F401", "F402", "F404", "F407",
        "F501", "F502", "F503", "F504", "F505", "F506", "F507", "F508", "F509",
        "F521", "F522", "F523", "F524", "F525", "F541", "F601", "F602", "F621",
        "F622", "F631", "F632", "F633", "F634", "F701", "F702", "F704", "F706",
        "F707", "F811", "F821", "F822", "F823", "F841", "F842", "F901", "FA100",
        "FA102", "FLY002", "FURB105", "FURB122", "FURB129", "FURB132",
        "FURB136", "FURB157", "FURB161", "FURB162", "FURB163", "FURB166",
        "FURB167", "FURB168", "FURB169", "FURB177", "FURB181", "FURB188",
        "G010", "G101", "G201", "G202", "I001", "INT001", "INT002", "INT003",
        "LOG001", "LOG002", "LOG009", "LOG014", "LOG015", "N999", "PERF101",
        "PERF102", "PERF402", "PGH005", "PIE790", "PIE794", "PIE796", "PIE800",
        "PIE804", "PIE807", "PIE808", "PIE810", "PLC0105", "PLC0131", "PLC0132",
        "PLC0205", "PLC0206", "PLC0208", "PLC0414", "PLC3002", "PLE0100",
        "PLE0101", "PLE0115", "PLE0116", "PLE0117", "PLE0118", "PLE0303",
        "PLE0305", "PLE0307", "PLE0308", "PLE0309", "PLE0604", "PLE0605",
        "PLE0643", "PLE0704", "PLE1132", "PLE1142", "PLE1205", "PLE1206",
        "PLE1300", "PLE1307", "PLE1310", "PLE1507", "PLE1519", "PLE1520",
        "PLE1700", "PLE2502", "PLE2510", "PLE2512", "PLE2513", "PLE2514",
        "PLE2515", "PLR0124", "PLR0133", "PLR0206", "PLR0402", "PLR1704",
        "PLR1711", "PLR1716", "PLR1722", "PLR1730", "PLR1733", "PLR1736",
        "PLR2044", "PLW0120", "PLW0127", "PLW0128", "PLW0129", "PLW0131",
        "PLW0133", "PLW0177", "PLW0211", "PLW0245", "PLW0406", "PLW0602",
        "PLW0604", "PLW0642", "PLW0711", "PLW1501", "PLW1507", "PLW1508",
        "PLW1509", "PLW1510", "PLW2101", "PT010", "PT014", "PT020", "PT025",
        "PT026", "PT031", "PTH124", "PTH210", "PYI001", "PYI002", "PYI003",
        "PYI004", "PYI005", "PYI006", "PYI007", "PYI008", "PYI009", "PYI010",
        "PYI012", "PYI013", "PYI015", "PYI016", "PYI017", "PYI018", "PYI019",
        "PYI020", "PYI025", "PYI026", "PYI029", "PYI030", "PYI032", "PYI033",
        "PYI034", "PYI035", "PYI036", "PYI041", "PYI042", "PYI043", "PYI044",
        "PYI045", "PYI046", "PYI047", "PYI048", "PYI049", "PYI050", "PYI052",
        "PYI055", "PYI057", "PYI058", "PYI059", "PYI061", "PYI062", "PYI063",
        "PYI064", "PYI066", "RET501", "RUF007", "RUF008", "RUF009", "RUF010",
        "RUF012", "RUF013", "RUF015", "RUF016", "RUF017", "RUF018", "RUF019",
        "RUF020", "RUF022", "RUF023", "RUF024", "RUF026", "RUF028", "RUF030",
        "RUF032", "RUF033", "RUF034", "RUF040", "RUF041", "RUF046", "RUF048",
        "RUF049", "RUF051", "RUF053", "RUF057", "RUF058", "RUF059", "RUF100",
        "RUF101", "RUF200", "S102", "S110", "S112", "SIM101", "SIM102",
        "SIM103", "SIM107", "SIM113", "SIM114", "SIM115", "SIM117", "SIM118",
        "SIM201", "SIM202", "SIM208", "SIM210", "SIM211", "SIM220", "SIM221",
        "SIM222", "SIM223", "SIM401", "SIM905", "SIM911", "T100", "TC004",
        "TC005", "TC007", "TC010", "TRY002", "TRY004", "TRY201", "TRY203",
        "TRY401", "UP001", "UP003", "UP004", "UP005", "UP006", "UP007", "UP008",
        "UP009", "UP010", "UP011", "UP012", "UP014", "UP017", "UP018", "UP019",
        "UP020", "UP021", "UP022", "UP023", "UP024", "UP025", "UP026", "UP028",
        "UP029", "UP030", "UP031", "UP032", "UP033", "UP034", "UP035", "UP036",
        "UP037", "UP039", "UP040", "UP041", "UP043", "UP044", "UP045", "UP046",
        "UP047", "UP049", "UP050", "W605", "YTT101", "YTT102", "YTT103",
        "YTT201", "YTT202", "YTT203", "YTT204", "YTT301", "YTT302", "YTT303",
    ]
    ignore = []

    # Allow fix for all enabled rules (when `--fix`) is provided.
    fixable = ["ALL"]
    unfixable = []

    # Allow unused variables when underscore-prefixed.
    dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

    [tool.ruff.format]
    # Like Black, use double quotes for strings.
    quote-style = "double"

    # Like Black, indent with spaces, rather than tabs.
    indent-style = "space"

    # Like Black, respect magic trailing commas.
    skip-magic-trailing-comma = false

    # Like Black, automatically detect the appropriate line ending.
    line-ending = "auto"

    # Enable auto-formatting of code examples in docstrings. Markdown,
    # reStructuredText code/literal blocks and doctests are all supported.
    #
    # This is currently disabled by default, but it is planned for this
    # to be opt-out in the future.
    docstring-code-format = false

    # Set the line length limit used when formatting code snippets in
    # docstrings.
    #
    # This only has an effect when the `docstring-code-format` setting is
    # enabled.
    docstring-code-line-length = "dynamic"
    ```

=== "ruff.toml"

    ```toml
    # Exclude a variety of commonly ignored directories.
    exclude = [
        ".bzr",
        ".direnv",
        ".eggs",
        ".git",
        ".git-rewrite",
        ".hg",
        ".ipynb_checkpoints",
        ".mypy_cache",
        ".nox",
        ".pants.d",
        ".pyenv",
        ".pytest_cache",
        ".pytype",
        ".ruff_cache",
        ".svn",
        ".tox",
        ".venv",
        ".vscode",
        "__pypackages__",
        "_build",
        "buck-out",
        "build",
        "dist",
        "node_modules",
        "site-packages",
        "venv",
    ]

    # Same as Black.
    line-length = 88
    indent-width = 4

    # Assume Python 3.10
    target-version = "py310"

    [lint]
    select = [
        "ASYNC100", "ASYNC105", "ASYNC115", "ASYNC116", "ASYNC210", "ASYNC220",
        "ASYNC221", "ASYNC222", "ASYNC230", "ASYNC251", "B002", "B003", "B004",
        "B005", "B006", "B008", "B009", "B010", "B012", "B013", "B014", "B015",
        "B016", "B017", "B018", "B019", "B020", "B021", "B022", "B023", "B025",
        "B026", "B029", "B030", "B031", "B032", "B033", "B035", "B039",
        "BLE001", "C400", "C401", "C402", "C403", "C404", "C405", "C406",
        "C408", "C409", "C410", "C411", "C413", "C414", "C415", "C417", "C418",
        "C419", "D419", "DTZ001", "DTZ002", "DTZ003", "DTZ004", "DTZ005",
        "DTZ006", "DTZ007", "DTZ011", "DTZ012", "DTZ901", "E722", "E902",
        "EXE001", "EXE002", "EXE004", "EXE005", "F401", "F402", "F404", "F407",
        "F501", "F502", "F503", "F504", "F505", "F506", "F507", "F508", "F509",
        "F521", "F522", "F523", "F524", "F525", "F541", "F601", "F602", "F621",
        "F622", "F631", "F632", "F633", "F634", "F701", "F702", "F704", "F706",
        "F707", "F811", "F821", "F822", "F823", "F841", "F842", "F901", "FA100",
        "FA102", "FLY002", "FURB105", "FURB122", "FURB129", "FURB132",
        "FURB136", "FURB157", "FURB161", "FURB162", "FURB163", "FURB166",
        "FURB167", "FURB168", "FURB169", "FURB177", "FURB181", "FURB188",
        "G010", "G101", "G201", "G202", "I001", "INT001", "INT002", "INT003",
        "LOG001", "LOG002", "LOG009", "LOG014", "LOG015", "N999", "PERF101",
        "PERF102", "PERF402", "PGH005", "PIE790", "PIE794", "PIE796", "PIE800",
        "PIE804", "PIE807", "PIE808", "PIE810", "PLC0105", "PLC0131", "PLC0132",
        "PLC0205", "PLC0206", "PLC0208", "PLC0414", "PLC3002", "PLE0100",
        "PLE0101", "PLE0115", "PLE0116", "PLE0117", "PLE0118", "PLE0303",
        "PLE0305", "PLE0307", "PLE0308", "PLE0309", "PLE0604", "PLE0605",
        "PLE0643", "PLE0704", "PLE1132", "PLE1142", "PLE1205", "PLE1206",
        "PLE1300", "PLE1307", "PLE1310", "PLE1507", "PLE1519", "PLE1520",
        "PLE1700", "PLE2502", "PLE2510", "PLE2512", "PLE2513", "PLE2514",
        "PLE2515", "PLR0124", "PLR0133", "PLR0206", "PLR0402", "PLR1704",
        "PLR1711", "PLR1716", "PLR1722", "PLR1730", "PLR1733", "PLR1736",
        "PLR2044", "PLW0120", "PLW0127", "PLW0128", "PLW0129", "PLW0131",
        "PLW0133", "PLW0177", "PLW0211", "PLW0245", "PLW0406", "PLW0602",
        "PLW0604", "PLW0642", "PLW0711", "PLW1501", "PLW1507", "PLW1508",
        "PLW1509", "PLW1510", "PLW2101", "PT010", "PT014", "PT020", "PT025",
        "PT026", "PT031", "PTH124", "PTH210", "PYI001", "PYI002", "PYI003",
        "PYI004", "PYI005", "PYI006", "PYI007", "PYI008", "PYI009", "PYI010",
        "PYI012", "PYI013", "PYI015", "PYI016", "PYI017", "PYI018", "PYI019",
        "PYI020", "PYI025", "PYI026", "PYI029", "PYI030", "PYI032", "PYI033",
        "PYI034", "PYI035", "PYI036", "PYI041", "PYI042", "PYI043", "PYI044",
        "PYI045", "PYI046", "PYI047", "PYI048", "PYI049", "PYI050", "PYI052",
        "PYI055", "PYI057", "PYI058", "PYI059", "PYI061", "PYI062", "PYI063",
        "PYI064", "PYI066", "RET501", "RUF007", "RUF008", "RUF009", "RUF010",
        "RUF012", "RUF013", "RUF015", "RUF016", "RUF017", "RUF018", "RUF019",
        "RUF020", "RUF022", "RUF023", "RUF024", "RUF026", "RUF028", "RUF030",
        "RUF032", "RUF033", "RUF034", "RUF040", "RUF041", "RUF046", "RUF048",
        "RUF049", "RUF051", "RUF053", "RUF057", "RUF058", "RUF059", "RUF100",
        "RUF101", "RUF200", "S102", "S110", "S112", "SIM101", "SIM102",
        "SIM103", "SIM107", "SIM113", "SIM114", "SIM115", "SIM117", "SIM118",
        "SIM201", "SIM202", "SIM208", "SIM210", "SIM211", "SIM220", "SIM221",
        "SIM222", "SIM223", "SIM401", "SIM905", "SIM911", "T100", "TC004",
        "TC005", "TC007", "TC010", "TRY002", "TRY004", "TRY201", "TRY203",
        "TRY401", "UP001", "UP003", "UP004", "UP005", "UP006", "UP007", "UP008",
        "UP009", "UP010", "UP011", "UP012", "UP014", "UP017", "UP018", "UP019",
        "UP020", "UP021", "UP022", "UP023", "UP024", "UP025", "UP026", "UP028",
        "UP029", "UP030", "UP031", "UP032", "UP033", "UP034", "UP035", "UP036",
        "UP037", "UP039", "UP040", "UP041", "UP043", "UP044", "UP045", "UP046",
        "UP047", "UP049", "UP050", "W605", "YTT101", "YTT102", "YTT103",
        "YTT201", "YTT202", "YTT203", "YTT204", "YTT301", "YTT302", "YTT303",
    ]
    ignore = []

    # Allow fix for all enabled rules (when `--fix`) is provided.
    fixable = ["ALL"]
    unfixable = []

    # Allow unused variables when underscore-prefixed.
    dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

    [format]
    # Like Black, use double quotes for strings.
    quote-style = "double"

    # Like Black, indent with spaces, rather than tabs.
    indent-style = "space"

    # Like Black, respect magic trailing commas.
    skip-magic-trailing-comma = false

    # Like Black, automatically detect the appropriate line ending.
    line-ending = "auto"

    # Enable auto-formatting of code examples in docstrings. Markdown,
    # reStructuredText code/literal blocks and doctests are all supported.
    #
    # This is currently disabled by default, but it is planned for this
    # to be opt-out in the future.
    docstring-code-format = false

    # Set the line length limit used when formatting code snippets in
    # docstrings.
    #
    # This only has an effect when the `docstring-code-format` setting is
    # enabled.
    docstring-code-line-length = "dynamic"
    ```

As an example, the following would configure Ruff to:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    # 1. Enable flake8-bugbear (`B`) rules, in addition to the defaults.
    extend-select = ["B"]

    # 2. Avoid enforcing line-length violations (`E501`)
    ignore = ["E501"]

    # 3. Avoid trying to fix flake8-bugbear (`B`) violations.
    unfixable = ["B"]

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in selected subdirectories.
    [tool.ruff.lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [tool.ruff.format]
    # 5. Use single quotes in `ruff format`.
    quote-style = "single"
    ```

=== "ruff.toml"

    ```toml
    [lint]
    # 1. Enable flake8-bugbear (`B`) rules, in addition to the defaults.
    extend-select = ["B"]

    # 2. Avoid enforcing line-length violations (`E501`)
    ignore = ["E501"]

    # 3. Avoid trying to fix flake8-bugbear (`B`) violations.
    unfixable = ["B"]

    # 4. Ignore `E402` (import violations) in all `__init__.py` files, and in selected subdirectories.
    [lint.per-file-ignores]
    "__init__.py" = ["E402"]
    "**/{tests,docs,tools}/*" = ["E402"]

    [format]
    # 5. Use single quotes in `ruff format`.
    quote-style = "single"
    ```

Linter plugin configurations are expressed as subsections, e.g.:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    # Add "Q" to the list of enabled codes.
    extend-select = ["Q"]

    [tool.ruff.lint.flake8-quotes]
    docstring-quotes = "double"
    ```

=== "ruff.toml"

    ```toml
    [lint]
    # Add "Q" to the list of enabled codes.
    extend-select = ["Q"]

    [lint.flake8-quotes]
    docstring-quotes = "double"
    ```

Ruff respects `pyproject.toml`, `ruff.toml`, and `.ruff.toml` files. All three implement an
equivalent schema (though in the `ruff.toml` and `.ruff.toml` versions, the `[tool.ruff]` header and
`tool.ruff` section prefix is omitted).

For a complete enumeration of the available configuration options, see [_Settings_](settings.md).

## Config file discovery

Similar to [ESLint](https://eslint.org/docs/latest/use/configure/configuration-files#cascading-configuration-objects),
Ruff supports hierarchical configuration, such that the "closest" config file in the
directory hierarchy is used for every individual file, with all paths in the config file
(e.g., `exclude` globs, `src` paths) being resolved relative to the directory containing that
config file.

There are a few exceptions to these rules:

1. In locating the "closest" `pyproject.toml` file for a given path, Ruff ignores any
    `pyproject.toml` files that lack a `[tool.ruff]` section.
1. If a configuration file is passed directly via `--config`, those settings are used for _all_
    analyzed files, and any relative paths in that configuration file (like `exclude` globs or
    `src` paths) are resolved relative to the _current_ working directory.
1. If no config file is found in the filesystem hierarchy, Ruff will fall back to using
    a default configuration. If a user-specific configuration file exists
    at `${config_dir}/ruff/pyproject.toml`, that file will be used instead of the default
    configuration, with `${config_dir}` being determined via [`etcetera`'s base strategy](https://docs.rs/etcetera/latest/etcetera/#native-strategy),
    and all relative paths being again resolved relative to the _current working directory_.
1. Any config-file-supported settings that are provided on the command-line (e.g., via
    `--select`) will override the settings in _every_ resolved configuration file.

Unlike [ESLint](https://eslint.org/docs/latest/use/configure/configuration-files#cascading-configuration-objects),
Ruff does not merge settings across configuration files; instead, the "closest" configuration file
is used, and any parent configuration files are ignored. In lieu of this implicit cascade, Ruff
supports an [`extend`](settings.md#extend) field, which allows you to inherit the settings from another
config file, like so:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    # Extend the `pyproject.toml` file in the parent directory...
    extend = "../pyproject.toml"

    # ...but use a different line length.
    line-length = 100
    ```

=== "ruff.toml"

    ```toml
    # Extend the `ruff.toml` file in the parent directory...
    extend = "../ruff.toml"

    # ...but use a different line length.
    line-length = 100
    ```

All of the above rules apply equivalently to `pyproject.toml`, `ruff.toml`, and `.ruff.toml` files.
If Ruff detects multiple configuration files in the same directory, the `.ruff.toml` file will take
precedence over the `ruff.toml` file, and the `ruff.toml` file will take precedence over
the `pyproject.toml` file.

### Inferring the Python version
When no discovered configuration specifies a [`target-version`](settings.md#target-version), Ruff will attempt to fall back to the minimum version compatible with the `requires-python` field in a nearby `pyproject.toml`.
The rules for this behavior are as follows:

1. If a configuration file is passed directly, Ruff does not attempt to infer a missing `target-version`.
1. If a configuration file is found in the filesystem hierarchy, Ruff will infer a missing `target-version` from the `requires-python` field in a `pyproject.toml` file in the same directory as the found configuration.
1. If we are using a user-level configuration from `${config_dir}/ruff/pyproject.toml`, the `requires-python` field in the first `pyproject.toml` file found in an ancestor of the current working directory takes precedence over the `target-version` in the user-level configuration.
1. If no configuration files are found, Ruff will infer the `target-version` from the `requires-python` field in the first `pyproject.toml` file found in an ancestor of the current working directory.

Note that in these last two cases, the behavior of Ruff may differ depending on the working directory from which it is invoked.

## Python file discovery

When passed a path on the command-line, Ruff will automatically discover all Python files in that
path, taking into account the [`exclude`](settings.md#exclude) and [`extend-exclude`](settings.md#extend-exclude)
settings in each directory's configuration file.

Files can also be selectively excluded from linting or formatting by scoping the `exclude` setting
to the tool-specific configuration tables. For example, the following would prevent `ruff` from
formatting `.pyi` files, but would continue to include them in linting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    exclude = ["*.pyi"]
    ```

=== "ruff.toml"

    ```toml
    [format]
    exclude = ["*.pyi"]
    ```

By default, Ruff will also skip any files that are omitted via `.ignore`, `.gitignore`,
`.git/info/exclude`, and global `gitignore` files (see: [`respect-gitignore`](settings.md#respect-gitignore)).

Files that are passed to `ruff` directly are always analyzed, regardless of the above criteria, 
unless [`force-exclude`](settings.md#force-exclude) is also enabled (via CLI or settings file).
For example, without `force-exclude` enabled, `ruff check /path/to/excluded/file.py` will always lint `file.py`.

### Default inclusions

By default, Ruff will discover files matching `*.py`, `*.pyi`, `*.ipynb`, or `pyproject.toml`.
In [preview](preview.md) mode, Ruff will also discover `*.pyw` by default.

To lint or format files with additional file extensions, use the [`extend-include`](settings.md#extend-include) setting.
You can also change the default selection using the [`include`](settings.md#include) setting.


=== "pyproject.toml"

    ```toml
    [tool.ruff]
    include = ["pyproject.toml", "src/**/*.py", "scripts/**/*.py"]
    ```

=== "ruff.toml"

    ```toml
    include = ["pyproject.toml", "src/**/*.py", "scripts/**/*.py"]
    ```

!!! warning
    Paths provided to `include` _must_ match files. For example, `include = ["src"]` will fail since it
    matches a directory.

## Jupyter Notebook discovery

Ruff has built-in support for linting and formatting [Jupyter Notebooks](https://jupyter.org/),
which are linted and formatted by default on version `0.6.0` and higher.

If you'd prefer to either only lint or only format Jupyter Notebook files, you can use the
section-specific `exclude` option to do so. For example, the following would only lint Jupyter
Notebook files and not format them:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    [format]
    exclude = ["*.ipynb"]
    ```

And, conversely, the following would only format Jupyter Notebook files and not lint them:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    exclude = ["*.ipynb"]
    ```

You can completely disable Jupyter Notebook support by updating the
[`extend-exclude`](settings.md#extend-exclude) setting:

=== "pyproject.toml"

    ```toml
    [tool.ruff]
    extend-exclude = ["*.ipynb"]
    ```

=== "ruff.toml"

    ```toml
    extend-exclude = ["*.ipynb"]
    ```

If you'd like to ignore certain rules specifically for Jupyter Notebook files, you can do so by
using the [`per-file-ignores`](settings.md#per-file-ignores) setting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint.per-file-ignores]
    "*.ipynb" = ["T20"]
    ```

=== "ruff.toml"

    ```toml
    [lint.per-file-ignores]
    "*.ipynb" = ["T20"]
    ```

Some rules have different behavior when applied to Jupyter Notebook files. For
example, when applied to `.py` files the
[`module-import-not-at-top-of-file` (`E402`)](rules/module-import-not-at-top-of-file.md)
rule detect imports at the top of a file, but for notebooks it detects imports at the top of a
**cell**. For a given rule, the rule's documentation will always specify if it has different
behavior when applied to Jupyter Notebook files.

## Command-line interface

Some configuration options can be provided or overridden via dedicated flags on the command line.
This includes those related to rule enablement and disablement,
file discovery, logging level, and more:

```console
$ ruff check path/to/code/ --select F401 --select F403 --quiet
```

All other configuration options can be set via the command line
using the `--config` flag, detailed below.

### The `--config` CLI flag

The `--config` flag has two uses. It is most often used to point to the
configuration file that you would like Ruff to use, for example:

```console
$ ruff check path/to/directory --config path/to/ruff.toml
```

However, the `--config` flag can also be used to provide arbitrary
overrides of configuration settings using TOML `<KEY> = <VALUE>` pairs.
This is mostly useful in situations where you wish to override a configuration setting
that does not have a dedicated command-line flag.

In the below example, the `--config` flag is the only way of overriding the
`dummy-variable-rgx` configuration setting from the command line,
since this setting has no dedicated CLI flag. The `per-file-ignores` setting
could also have been overridden via the `--per-file-ignores` dedicated flag,
but using `--config` to override the setting is also fine:

```console
$ ruff check path/to/file --config path/to/ruff.toml --config "lint.dummy-variable-rgx = '__.*'" --config "lint.per-file-ignores = {'some_file.py' = ['F841']}"
```

Configuration options passed to `--config` are parsed in the same way
as configuration options in a `ruff.toml` file.
As such, options specific to the Ruff linter need to be prefixed with `lint.`
(`--config "lint.dummy-variable-rgx = '__.*'"` rather than simply
`--config "dummy-variable-rgx = '__.*'"`), and options specific to the Ruff formatter
need to be prefixed with `format.`.

If a specific configuration option is simultaneously overridden by
a dedicated flag and by the `--config` flag, the dedicated flag
takes priority. In this example, the maximum permitted line length
will be set to 90, not 100:

```console
$ ruff format path/to/file --line-length=90 --config "line-length=100"
```

Specifying `--config "line-length=90"` will override the `line-length`
setting from *all* configuration files detected by Ruff,
including configuration files discovered in subdirectories.
In this respect, specifying `--config "line-length=90"` has
the same effect as specifying `--line-length=90`,
which will similarly override the `line-length` setting from
all configuration files detected by Ruff, regardless of where
a specific configuration file is located.

### Argfile support

Ruff supports reading command-line arguments from a file, which is especially useful when passing a large number of file paths that might exceed your shell's command-line length limit. To use an argfile, prefix the file path with an `@` symbol:

```console
$ ruff check @path/to/args.txt
```

The arguments in the file must all be written on their own line. For example, `args.txt` might contain:

```text
--select
F401
--quiet
path/to/code1/
path/to/code2/
```

### Full command-line interface

See `ruff help` for the full list of Ruff's top-level commands:

<!-- Begin auto-generated command help. -->

```text
Ruff: An extremely fast Python linter and code formatter.

Usage: ruff [OPTIONS] <COMMAND>

Commands:
  check    Run Ruff on the given files or directories
  rule     Explain a rule (or all rules)
  config   List or describe the available configuration options
  linter   List all supported upstream linters
  clean    Clear any caches in the current directory and any subdirectories
  format   Run the Ruff formatter on the given files or directories
  server   Run the language server
  analyze  Run analysis over Python source code
  version  Display Ruff's version
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help (see more with '--help')
  -V, --version  Print version

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)

Global options:
      --config <CONFIG_OPTION>
          Either a path to a TOML configuration file (`pyproject.toml` or
          `ruff.toml`), or a TOML `<KEY> = <VALUE>` pair (such as you might
          find in a `ruff.toml` configuration file) overriding a specific
          configuration option (e.g., `--config "lint.line-length = 100"` or
          `--config "format.quote-style = 'single'"`). Overrides of individual
          settings using this option always take precedence over all
          configuration files, including configuration files that were also
          specified using `--config`
      --isolated
          Ignore all configuration files
      --color <WHEN>
          Control when colored output is used [possible values: auto, always,
          never]

For help with a specific command, see: `ruff help <command>`.
```

<!-- End auto-generated command help. -->

Or `ruff help check` for more on the linting command:

<!-- Begin auto-generated check help. -->

```text
Run Ruff on the given files or directories

Usage: ruff check [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to check, or `-` to read from stdin
              [default: .]

Options:
      --fix
          Apply fixes to resolve lint violations. Use `--no-fix` to disable or
          `--unsafe-fixes` to include unsafe fixes
      --unsafe-fixes
          Include fixes that may not retain the original intent of the code.
          Use `--no-unsafe-fixes` to disable
      --show-fixes
          Show an enumeration of all fixed lint violations. Use
          `--no-show-fixes` to disable
      --diff
          Avoid writing any fixed files back; instead, output a diff for each
          changed file to stdout, and exit 0 if there are no diffs. Implies
          `--fix-only`
  -w, --watch
          Run in watch mode by re-running whenever files change
      --fix-only
          Apply fixes to resolve lint violations, but don't report on, or exit
          non-zero for, leftover violations. Implies `--fix`. Use
          `--no-fix-only` to disable or `--unsafe-fixes` to include unsafe
          fixes
      --ignore-noqa
          Ignore any `# noqa` comments
      --output-format <OUTPUT_FORMAT>
          Output serialization format for violations. The default serialization
          format is "full" [env: RUFF_OUTPUT_FORMAT=] [possible values:
          concise, full, json, json-lines, junit, grouped, github, gitlab,
          pylint, rdjson, azure, sarif]
  -o, --output-file <OUTPUT_FILE>
          Specify file to write the linter output to (default: stdout) [env:
          RUFF_OUTPUT_FILE=]
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values:
          py37, py38, py39, py310, py311, py312, py313, py314, py315]
      --preview
          Enable preview mode; checks will include unstable rules and fixes.
          Use `--no-preview` to disable
      --extension <EXTENSION>
          List of mappings from file extension to language (one of `python`,
          `ipynb`, `pyi`). For example, to treat `.ipy` files as IPython
          notebooks, use `--extension ipy:ipynb`
      --statistics
          Show counts for every rule with at least one violation
      --add-noqa[=<REASON>]
          Enable automatic additions of `noqa` directives to failing lines.
          Optionally provide a reason to append after the codes
      --add-ignore[=<REASON>]
          Enable automatic additions of `ruff:ignore` comments to failing
          lines. Optionally provide a reason to append after the rule names.
          Requires preview mode
      --show-files
          See the files Ruff will be run against with the current settings
      --show-settings
          See the settings Ruff will use to lint a given Python file
  -h, --help
          Print help (see more with '--help')

Rule selection:
      --select <RULE_CODE>
          Comma-separated list of rule codes to enable (or ALL, to enable all
          rules)
      --ignore <RULE_CODE>
          Comma-separated list of rule codes to disable
      --extend-select <RULE_CODE>
          Like --select, but adds additional rule codes on top of those already
          specified
      --per-file-ignores <PER_FILE_IGNORES>
          List of mappings from file pattern to code to exclude
      --extend-per-file-ignores <EXTEND_PER_FILE_IGNORES>
          Like `--per-file-ignores`, but adds additional ignores on top of
          those already specified
      --fixable <RULE_CODE>
          List of rule codes to treat as eligible for fix. Only applicable when
          fix itself is enabled (e.g., via `--fix`)
      --unfixable <RULE_CODE>
          List of rule codes to treat as ineligible for fix. Only applicable
          when fix itself is enabled (e.g., via `--fix`)
      --extend-fixable <RULE_CODE>
          Like --fixable, but adds additional rule codes on top of those
          already specified

File selection:
      --exclude <FILE_PATTERN>
          List of paths, used to omit files and/or directories from analysis
      --extend-exclude <FILE_PATTERN>
          Like --exclude, but adds additional files and directories on top of
          those already excluded
      --respect-gitignore
          Respect file exclusions via `.gitignore` and other standard ignore
          files. Use `--no-respect-gitignore` to disable
      --force-exclude
          Enforce exclusions, even for paths passed to Ruff directly on the
          command-line. Use `--no-force-exclude` to disable

Miscellaneous:
  -n, --no-cache
          Disable cache reads [env: RUFF_NO_CACHE=]
      --cache-dir <CACHE_DIR>
          Path to the cache directory [env: RUFF_CACHE_DIR=]
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
  -e, --exit-zero
          Exit with status code "0", even upon detecting lint violations
      --exit-non-zero-on-fix
          Exit with a non-zero status code if any files were modified via fix,
          even if no lint violations remain

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)

Global options:
      --config <CONFIG_OPTION>
          Either a path to a TOML configuration file (`pyproject.toml` or
          `ruff.toml`), or a TOML `<KEY> = <VALUE>` pair (such as you might
          find in a `ruff.toml` configuration file) overriding a specific
          configuration option (e.g., `--config "lint.line-length = 100"` or
          `--config "format.quote-style = 'single'"`). Overrides of individual
          settings using this option always take precedence over all
          configuration files, including configuration files that were also
          specified using `--config`
      --isolated
          Ignore all configuration files
      --color <WHEN>
          Control when colored output is used [possible values: auto, always,
          never]
```

<!-- End auto-generated check help. -->

Or `ruff help format` for more on the formatting command:

<!-- Begin auto-generated format help. -->

```text
Run the Ruff formatter on the given files or directories

Usage: ruff format [OPTIONS] [FILES]...

Arguments:
  [FILES]...  List of files or directories to format, or `-` to read from stdin
              [default: .]

Options:
      --check
          Avoid writing any formatted files back; instead, exit with a non-zero
          status code if any files would have been modified, and zero otherwise
      --diff
          Avoid writing any formatted files back; instead, exit with a non-zero
          status code and the difference between the current file and how the
          formatted file would look like
      --extension <EXTENSION>
          List of mappings from file extension to language (one of `python`,
          `ipynb`, `pyi`). For example, to treat `.ipy` files as IPython
          notebooks, use `--extension ipy:ipynb`
      --target-version <TARGET_VERSION>
          The minimum Python version that should be supported [possible values:
          py37, py38, py39, py310, py311, py312, py313, py314, py315]
      --preview
          Enable preview mode; enables unstable formatting. Use `--no-preview`
          to disable
      --output-format <OUTPUT_FORMAT>
          Output serialization format for violations, when used with `--check`.
          The default serialization format is "full" [env: RUFF_OUTPUT_FORMAT=]
          [possible values: concise, full, json, json-lines, junit, grouped,
          github, gitlab, pylint, rdjson, azure, sarif]
  -h, --help
          Print help (see more with '--help')

Miscellaneous:
  -n, --no-cache
          Disable cache reads [env: RUFF_NO_CACHE=]
      --cache-dir <CACHE_DIR>
          Path to the cache directory [env: RUFF_CACHE_DIR=]
      --stdin-filename <STDIN_FILENAME>
          The name of the file when passing it through stdin
      --exit-non-zero-on-format
          Exit with a non-zero status code if any files were modified via
          format, even if all files were formatted successfully

File selection:
      --respect-gitignore
          Respect file exclusions via `.gitignore` and other standard ignore
          files. Use `--no-respect-gitignore` to disable
      --exclude <FILE_PATTERN>
          List of paths, used to omit files and/or directories from analysis
      --extend-exclude <FILE_PATTERN>
          Like --exclude, but adds additional files and directories on top of
          those already excluded
      --force-exclude
          Enforce exclusions, even for paths passed to Ruff directly on the
          command-line. Use `--no-force-exclude` to disable

Format configuration:
      --line-length <LINE_LENGTH>  Set the line-length

Editor options:
      --range <RANGE>  When specified, Ruff will try to only format the code in
                       the given range.
                       It might be necessary to extend the start backwards or
                       the end forwards, to fully enclose a logical line.
                       The `<RANGE>` uses the format
                       `<start_line>:<start_column>-<end_line>:<end_column>`.

Log levels:
  -v, --verbose  Enable verbose logging
  -q, --quiet    Print diagnostics, but nothing else
  -s, --silent   Disable all logging (but still exit with status code "1" upon
                 detecting diagnostics)

Global options:
      --config <CONFIG_OPTION>
          Either a path to a TOML configuration file (`pyproject.toml` or
          `ruff.toml`), or a TOML `<KEY> = <VALUE>` pair (such as you might
          find in a `ruff.toml` configuration file) overriding a specific
          configuration option (e.g., `--config "lint.line-length = 100"` or
          `--config "format.quote-style = 'single'"`). Overrides of individual
          settings using this option always take precedence over all
          configuration files, including configuration files that were also
          specified using `--config`
      --isolated
          Ignore all configuration files
      --color <WHEN>
          Control when colored output is used [possible values: auto, always,
          never]
```

<!-- End auto-generated format help. -->

## Shell autocompletion

Ruff supports autocompletion for most shells. A shell-specific completion script can be generated
by `ruff generate-shell-completion <SHELL>`, where `<SHELL>` is one of `bash`, `elvish`, `fig`, `fish`,
`powershell`, or `zsh`.

!!! tip

    You can run `echo $SHELL` to help you determine your shell.

To enable shell autocompletion for Ruff, run one of the following:

=== "Bash"

    ```bash
    echo 'eval "$(ruff generate-shell-completion bash)"' >> ~/.bashrc
    ```

=== "Zsh"

    ```bash
    echo 'eval "$(ruff generate-shell-completion zsh)"' >> ~/.zshrc
    ```

=== "fish"

    ```bash
    echo 'ruff generate-shell-completion fish | source' > ~/.config/fish/completions/ruff.fish
    ```

=== "Elvish"

    ```bash
    echo 'eval (ruff generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "PowerShell / pwsh"

    ```powershell
    if (!(Test-Path -Path $PROFILE)) {
      New-Item -ItemType File -Path $PROFILE -Force
    }
    Add-Content -Path $PROFILE -Value '(& ruff generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

Then restart the shell or source the shell config file.
