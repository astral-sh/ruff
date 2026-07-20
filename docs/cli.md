# Command-line interface

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

Or `ruff help analyze` for more on the analysis command:

<!-- Begin auto-generated analyze help. -->

```text
Run analysis over Python source code

Usage: ruff analyze [OPTIONS] <COMMAND>

Commands:
  graph  Generate a map of Python file dependencies or dependents
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help (see more with '--help')

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

<!-- End auto-generated analyze help. -->

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
