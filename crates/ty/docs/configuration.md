### Top-level
The options for the project.

##### [`rules`](#rules) {: #rules }

Configures the enabled lints and their severity.

**Default value**: `{...}`

**Type**: `dict[str, ignore | warn | error]`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.rules]
    possibly-unresolved-reference = "warn"
    division-by-zero = "ignore"
    ```

---

#### `environment`

##### [`extra-paths`](#environment_extra-paths) {: #environment_extra-paths }
<span id="extra-paths"></span>

List of user-provided paths that should take first priority in the module resolution.
Examples in other type checkers are mypy's `MYPYPATH` environment variable,
or pyright's `stubPath` configuration setting.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.environment]
    extra-paths = ["~/shared/my-search-path"]
    ```

---

##### [`python`](#environment_python) {: #environment_python }
<span id="python"></span>

Path to the Python installation from which ty resolves type information and third-party dependencies.

ty will search in the path's `site-packages` directories for type information and
third-party imports.

This option is commonly used to specify the path to a virtual environment.

**Default value**: `null`

**Type**: `str`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.environment]
    python = "./.venv"
    ```

---

##### [`python-platform`](#environment_python-platform) {: #environment_python-platform }
<span id="python-platform"></span>

Specifies the target platform that will be used to analyze the source code.
If specified, ty will tailor its use of type stub files,
which conditionalize type definitions based on the platform.

If no platform is specified, ty will use the current platform:
- `win32` for Windows
- `darwin` for macOS
- `android` for Android
- `ios` for iOS
- `linux` for everything else

**Default value**: `<current-platform>`

**Type**: `"win32" | "darwin" | "android" | "ios" | "linux" | str`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.environment]
    # Tailor type stubs and conditionalized type definitions to windows.
    python-platform = "win32"
    ```

---

##### [`python-version`](#environment_python-version) {: #environment_python-version }
<span id="python-version"></span>

Specifies the version of Python that will be used to analyze the source code.
The version should be specified as a string in the format `M.m` where `M` is the major version
and `m` is the minor (e.g. `"3.0"` or `"3.6"`).
If a version is provided, ty will generate errors if the source code makes use of language features
that are not supported in that version.
It will also tailor its use of type stub files, which conditionalizes type definitions based on the version.

**Default value**: `"3.13"`

**Type**: `"3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | <major>.<minor>`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.environment]
    python-version = "3.12"
    ```

---

##### [`typeshed`](#environment_typeshed) {: #environment_typeshed }
<span id="typeshed"></span>

Optional path to a "typeshed" directory on disk for us to use for standard-library types.
If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
bundled as a zip file in the binary

**Default value**: `null`

**Type**: `str`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.environment]
    typeshed = "/path/to/custom/typeshed"
    ```

---

#### `src`

##### [`root`](#src_root) {: #src_root }
<span id="root"></span>

The root of the project, used for finding first-party modules.

**Default value**: `[".", "./src"]`

**Type**: `list[str]`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.src]
    root = ["./app"]
    ```

---

#### `terminal`

##### [`error-on-warning`](#terminal_error-on-warning) {: #terminal_error-on-warning }
<span id="error-on-warning"></span>

Use exit code 1 if there are any warning-level diagnostics.

Defaults to `false`.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.terminal]
    # Error if ty emits any warning-level diagnostics.
    error-on-warning = true
    ```

---

##### [`output-format`](#terminal_output-format) {: #terminal_output-format }
<span id="output-format"></span>

The format to use for printing diagnostic messages.

Defaults to `full`.

**Default value**: `full`

**Type**: `full | concise`

**Example usage**:

**pyproject.toml**

    ```toml
    [tool.ty.terminal]
    output-format = "concise"
    ```

---

