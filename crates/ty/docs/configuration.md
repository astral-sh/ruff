<!-- WARNING: This file is auto-generated (cargo dev generate-all). Update the doc comments on the 'Options' struct in 'crates/ty_project/src/metadata/options.rs' if you want to change anything here. -->

# Configuration
#### `respect-ignore-files`

Whether to automatically exclude files that are ignored by `.ignore`,
`.gitignore`, `.git/info/exclude`, and global `gitignore` files.
Enabled by default.

**Default value**: `true`

**Type**: `bool`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty]
respect-ignore-files = false
```

---

#### `rules`

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

#### `extra-paths`

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

#### `python`

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

#### `python-platform`

Specifies the target platform that will be used to analyze the source code.
If specified, ty will understand conditions based on comparisons with `sys.platform`, such
as are commonly found in typeshed to reflect the differing contents of the standard library across platforms.

If no platform is specified, ty will use the current platform:
- `win32` for Windows
- `darwin` for macOS
- `android` for Android
- `ios` for iOS
- `linux` for everything else

**Default value**: `<current-platform>`

**Type**: `"win32" | "darwin" | "android" | "ios" | "linux" | str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
# Tailor type stubs and conditionalized type definitions to windows.
python-platform = "win32"
```

---

#### `python-version`

Specifies the version of Python that will be used to analyze the source code.
The version should be specified as a string in the format `M.m` where `M` is the major version
and `m` is the minor (e.g. `"3.0"` or `"3.6"`).
If a version is provided, ty will generate errors if the source code makes use of language features
that are not supported in that version.
It will also understand conditionals based on comparisons with `sys.version_info`, such
as are commonly found in typeshed to reflect the differing contents of the standard
library across Python versions.

**Default value**: `"3.13"`

**Type**: `"3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | <major>.<minor>`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.environment]
python-version = "3.12"
```

---

#### `typeshed`

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

## `src`

#### `root`

The root of the project, used for finding first-party modules.

If left unspecified, ty will try to detect common project layouts and initialize `src.root` accordingly:

* if a `./src` directory exists, include `.` and `./src` in the first party search path (src layout or flat)
* if a `./<project-name>/<project-name>` directory exists, include `.` and `./<project-name>` in the first party search path
* otherwise, default to `.` (flat layout)

**Default value**: `null`

**Type**: `str`

**Example usage** (`pyproject.toml`):

```toml
[tool.ty.src]
root = "./app"
```

---

## `terminal`

#### `error-on-warning`

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

#### `output-format`

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

