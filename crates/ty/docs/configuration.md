<!-- WARNING: This file is auto-generated (cargo dev generate-all). Update the doc comments on the 'Options' struct in 'crates/ty_project/src/metadata/options.rs' if you want to change anything here. -->

# Configuration
## `rules`

Configures the enabled rules and their severity.

See [the rules documentation](https://ty.dev/rules) for a list of all available rules.

Valid severities are:

* `ignore`: Disable the rule.
* `warn`: Enable the rule and create a warning diagnostic.
* `error`: Enable the rule and create an error diagnostic.
  ty will exit with a non-zero code if any error diagnostics are emitted.

**Default value**: `{...}`

**Type**: `dict[RuleName, "ignore" | "warn" | "error"]`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.rules]
possibly-unresolved-reference = "warn"
division-by-zero = "ignore"
```

---

## `environment`

### `extra-paths`

List of user-provided paths that should take first priority in the module resolution.
Examples in other type checkers are mypy's `MYPYPATH` environment variable,
or pyright's `stubPath` configuration setting.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
extra-paths = ["~/shared/my-search-path"]
```

---

### `python`

Path to the Python installation from which ty resolves type information and third-party dependencies.

ty will search in the path's `site-packages` directories for type information and
third-party imports.

This option is commonly used to specify the path to a virtual environment.

**Default value**: `null`

**Type**: `str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
python = "./.venv"
```

---

### `python-platform`

Specifies the target platform that will be used to analyze the source code.
If specified, ty will understand conditions based on comparisons with `sys.platform`, such
as are commonly found in typeshed to reflect the differing contents of the standard library across platforms.
If `all` is specified, ty will assume that the source code can run on any platform.

If no platform is specified, ty will use the current platform:
- `win32` for Windows
- `darwin` for macOS
- `android` for Android
- `ios` for iOS
- `linux` for everything else

**Default value**: `<current-platform>`

**Type**: `"win32" | "darwin" | "android" | "ios" | "linux" | "all" | str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
# Tailor type stubs and conditionalized type definitions to windows.
python-platform = "win32"
```

---

### `python-version`

Specifies the version of Python that will be used to analyze the source code.
The version should be specified as a string in the format `M.m` where `M` is the major version
and `m` is the minor (e.g. `"3.0"` or `"3.6"`).
If a version is provided, ty will generate errors if the source code makes use of language features
that are not supported in that version.

If a version is not specified, ty will try the following techniques in order of preference
to determine a value:
1. Check for the `project.requires-python` setting in a `pyproject.toml` file
   and use the minimum version from the specified range
2. Check for an activated or configured Python environment
   and attempt to infer the Python version of that environment
3. Fall back to the default value (see below)

For some language features, ty can also understand conditionals based on comparisons
with `sys.version_info`. These are commonly found in typeshed, for example,
to reflect the differing contents of the standard library across Python versions.

**Default value**: `"3.13"`

**Type**: `"3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | <major>.<minor>`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
python-version = "3.12"
```

---

### `root`

The root paths of the project, used for finding first-party modules.

Accepts a list of directory paths searched in priority order (first has highest priority).

If left unspecified, ty will try to detect common project layouts and initialize `root` accordingly:

* if a `./src` directory exists, include `.` and `./src` in the first party search path (src layout or flat)
* if a `./<project-name>/<project-name>` directory exists, include `.` and `./<project-name>` in the first party search path
* otherwise, default to `.` (flat layout)

Besides, if a `./tests` directory exists and is not a package (i.e. it does not contain an `__init__.py` file),
it will also be included in the first party search path.

**Default value**: `null`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
# Multiple directories (priority order)
root = ["./src", "./lib", "./vendor"]
```

---

### `typeshed`

Optional path to a "typeshed" directory on disk for us to use for standard-library types.
If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
bundled as a zip file in the binary

**Default value**: `null`

**Type**: `str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
typeshed = "/path/to/custom/typeshed"
```

---

## `overrides`

Configuration override that applies to specific files based on glob patterns.

An override allows you to apply different rule configurations to specific
files or directories. Multiple overrides can match the same file, with
later overrides take precedence.

### Precedence

- Later overrides in the array take precedence over earlier ones
- Override rules take precedence over global rules for matching files

### Examples

```toml
# Relax rules for test files
[[tool.ty.overrides]]
include = ["tests/**", "**/test_*.py"]

[tool.ty.overrides.rules]
possibly-unresolved-reference = "warn"

# Ignore generated files but still check important ones
[[tool.ty.overrides]]
include = ["generated/**"]
exclude = ["generated/important.py"]

[tool.ty.overrides.rules]
possibly-unresolved-reference = "ignore"
```


### `exclude`

A list of file and directory patterns to exclude from this override.

Patterns follow a syntax similar to `.gitignore`.
Exclude patterns take precedence over include patterns within the same override.

If not specified, defaults to `[]` (excludes no files).

**Default value**: `null`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[[tool.ty.overrides]]
exclude = [
    "generated",
    "*.proto",
    "tests/fixtures/**",
    "!tests/fixtures/important.py"  # Include this one file
]
```

---

### `include`

A list of file and directory patterns to include for this override.

The `include` option follows a similar syntax to `.gitignore` but reversed:
Including a file or directory will make it so that it (and its contents)
are affected by this override.

If not specified, defaults to `["**"]` (matches all files).

**Default value**: `null`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[[tool.ty.overrides]]
include = [
    "src",
    "tests",
]
```

---

### `rules`

Rule overrides for files matching the include/exclude patterns.

These rules will be merged with the global rules, with override rules
taking precedence for matching files. You can set rules to different
severity levels or disable them entirely.

**Default value**: `{...}`

**Type**: `dict[RuleName, "ignore" | "warn" | "error"]`

**Example usage** (`pyproject.toml`):

```toml
[[tool.ty.overrides]]
include = ["src"]

[tool.ty.overrides.rules]
possibly-unresolved-reference = "ignore"
```

---

## `src`

### `exclude`

A list of file and directory patterns to exclude from type checking.

Patterns follow a syntax similar to `.gitignore`:

- `./src/` matches only a directory
- `./src` matches both files and directories
- `src` matches files or directories named `src`
- `*` matches any (possibly empty) sequence of characters (except `/`).
- `**` matches zero or more path components.
  This sequence **must** form a single path component, so both `**a` and `b**` are invalid and will result in an error.
  A sequence of more than two consecutive `*` characters is also invalid.
- `?` matches any single character except `/`
- `[abc]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as ordered by Unicode,
  so e.g. `[0-9]` specifies any character between `0` and `9` inclusive. An unclosed bracket is invalid.
- `!pattern` negates a pattern (undoes the exclusion of files that would otherwise be excluded)

All paths are anchored relative to the project root (`src` only
matches `<project_root>/src` and not `<project_root>/test/src`).
To exclude any directory or file named `src`, use `**/src` instead.

By default, ty excludes commonly ignored directories:

- `**/.bzr/`
- `**/.direnv/`
- `**/.eggs/`
- `**/.git/`
- `**/.git-rewrite/`
- `**/.hg/`
- `**/.mypy_cache/`
- `**/.nox/`
- `**/.pants.d/`
- `**/.pytype/`
- `**/.ruff_cache/`
- `**/.svn/`
- `**/.tox/`
- `**/.venv/`
- `**/__pypackages__/`
- `**/_build/`
- `**/buck-out/`
- `**/dist/`
- `**/node_modules/`
- `**/venv/`

You can override any default exclude by using a negated pattern. For example,
to re-include `dist` use `exclude = ["!dist"]`

**Default value**: `null`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.src]
exclude = [
    "generated",
    "*.proto",
    "tests/fixtures/**",
    "!tests/fixtures/important.py"  # Include this one file
]
```

---

### `include`

A list of files and directories to check. The `include` option
follows a similar syntax to `.gitignore` but reversed:
Including a file or directory will make it so that it (and its contents)
are type checked.

- `./src/` matches only a directory
- `./src` matches both files and directories
- `src` matches a file or directory named `src`
- `*` matches any (possibly empty) sequence of characters (except `/`).
- `**` matches zero or more path components.
  This sequence **must** form a single path component, so both `**a` and `b**` are invalid and will result in an error.
  A sequence of more than two consecutive `*` characters is also invalid.
- `?` matches any single character except `/`
- `[abc]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as ordered by Unicode,
  so e.g. `[0-9]` specifies any character between `0` and `9` inclusive. An unclosed bracket is invalid.

All paths are anchored relative to the project root (`src` only
matches `<project_root>/src` and not `<project_root>/test/src`).

`exclude` takes precedence over `include`.

**Default value**: `null`

**Type**: `list[str]`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.src]
include = [
    "src",
    "tests",
]
```

---

### `respect-ignore-files`

Whether to automatically exclude files that are ignored by `.ignore`,
`.gitignore`, `.git/info/exclude`, and global `gitignore` files.
Enabled by default.

**Default value**: `true`

**Type**: `bool`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.src]
respect-ignore-files = false
```

---

### `root`

> [!WARN] "Deprecated"
> This option has been deprecated. Use `environment.root` instead.

The root of the project, used for finding first-party modules.

If left unspecified, ty will try to detect common project layouts and initialize `src.root` accordingly:

* if a `./src` directory exists, include `.` and `./src` in the first party search path (src layout or flat)
* if a `./<project-name>/<project-name>` directory exists, include `.` and `./<project-name>` in the first party search path
* otherwise, default to `.` (flat layout)

Besides, if a `./tests` directory exists and is not a package (i.e. it does not contain an `__init__.py` file),
it will also be included in the first party search path.

**Default value**: `null`

**Type**: `str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.src]
root = "./app"
```

---

## `terminal`

### `error-on-warning`

Use exit code 1 if there are any warning-level diagnostics.

Defaults to `false`.

**Default value**: `false`

**Type**: `bool`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.terminal]
# Error if ty emits any warning-level diagnostics.
error-on-warning = true
```

---

### `output-format`

The format to use for printing diagnostic messages.

Defaults to `full`.

**Default value**: `full`

**Type**: `full | concise`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.terminal]
output-format = "concise"
```

---

