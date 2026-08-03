<!-- WARNING: This file is auto-generated (cargo dev generate-all). Update the doc comments on the 'Options' struct in 'crates/ty_project/src/metadata/options.rs' if you want to change anything here. -->

# Configuration
## `rules`

Configures the enabled rules and their severity.

The keys are either rule names or `all` to set a default severity for all rules.
See [the rules documentation](https://ty.dev/rules) for a list of all available rules.

Valid severities are:

* `ignore`: Disable the rule.
* `warn`: Enable the rule and create a warning diagnostic.
* `error`: Enable the rule and create an error diagnostic.

By default, ty exits with code 1 if it emits any warning or error diagnostics.
Set `terminal.error-on-warning` to `false` to exit with code 0 if all diagnostics have `warning` severity.

**Default value**: `{...}`

**Type**: `dict[RuleName | "all", "ignore" | "warn" | "error"]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.rules]
    possibly-unresolved-reference = "warn"
    division-by-zero = "ignore"
    ```

=== "ty.toml"

    ```toml
    [rules]
    possibly-unresolved-reference = "warn"
    division-by-zero = "ignore"
    ```

---

## `analysis`

### `allowed-unresolved-imports`

A list of module glob patterns for which `unresolved-import` diagnostics should be suppressed.

Details on supported glob patterns:
- `*` matches zero or more characters except `.`. For example, `foo.*` matches `foo.bar` but
  not `foo.bar.baz`; `foo*` matches `foo` and `foobar` but not `foo.bar` or `barfoo`; and `*foo`
  matches `foo` and `barfoo` but not `foo.bar` or `foobar`.
- `**` matches any number of module components (e.g., `foo.**` matches `foo`, `foo.bar`, etc.)
- Prefix a pattern with `!` to exclude matching modules

When multiple patterns match, later entries take precedence.

Glob patterns can be used in combinations with each other. For example, to suppress errors for
any module where the first component contains the substring `test`, use `*test*.**`.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.analysis]
    # Suppress errors for all `test` modules except `test.foo`
    allowed-unresolved-imports = ["test.**", "!test.foo"]
    ```

=== "ty.toml"

    ```toml
    [analysis]
    # Suppress errors for all `test` modules except `test.foo`
    allowed-unresolved-imports = ["test.**", "!test.foo"]
    ```

---

### `replace-imports-with-any`

A list of module glob patterns whose imports should be replaced with `typing.Any`.

Unlike `allowed-unresolved-imports`, this setting replaces the module's type information
with `typing.Any` even if the module can be resolved. Import diagnostics are
unconditionally suppressed for matching modules.

- Prefix a pattern with `!` to exclude matching modules

When multiple patterns match, later entries take precedence.

Glob patterns can be used in combinations with each other. For example, to suppress errors for
any module where the first component contains the substring `test`, use `*test*.**`.

When multiple patterns match, later entries take precedence.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.analysis]
    # Replace all pandas and numpy imports with Any
    replace-imports-with-any = ["pandas.**", "numpy.**"]
    ```

=== "ty.toml"

    ```toml
    [analysis]
    # Replace all pandas and numpy imports with Any
    replace-imports-with-any = ["pandas.**", "numpy.**"]
    ```

---

### `respect-type-ignore-comments`

Whether ty should respect `type: ignore` comments.

When set to `false`, `type: ignore` comments are treated like any other normal
comment and can't be used to suppress ty errors (you have to use `ty: ignore` instead).

Setting this option can be useful when using ty alongside other type checkers or when
you prefer using `ty: ignore` over `type: ignore`.

Defaults to `true`.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.analysis]
    # Disable support for `type: ignore` comments
    respect-type-ignore-comments = false
    ```

=== "ty.toml"

    ```toml
    [analysis]
    # Disable support for `type: ignore` comments
    respect-type-ignore-comments = false
    ```

---

### `strict-equality-semantics`

Configure ty's behavior regarding type inference and narrowing of equality
checks. Defaults to `false`.

By default, ty makes various assumptions about equality checks that match the
intuitions of most Python programmers, but may not be fully sound in all situations.
Enabling this option makes ty more conservative about these assumptions, making it
less likely to infer `Literal[True]` or `Literal[False]` as the result of an
equality check. This has various effects on type checking, including fewer type
narrowing opportunities and more conservative assumptions regarding control flow.

One way in which ty will by default make unsound assumptions is by narrowing an
object `x` of type `str` to `Literal["a"]` after an `if x == "a"` check. This is
unsound because a subclass of `str` with value `"a"` will (by default) compare equal
to `"a"`, but will not be of type `Literal["a"]`:

```pycon
>>> # `Literal["a"]` can only be inhabited by instances of exactly `str`, not
>>> # subclasses, but str subclasses compare equal by default:
>>> class StringSubclass(str): ...
...
>>> StringSubclass("a") == "a"
True
>>>
>>> # This also applies to `StrEnum`s:
>>> from enum import StrEnum
>>> class MyEnum(StrEnum):
...     A = "a"
...
>>> MyEnum.A == "a"
True
```

Enabling this option prevents the unsound narrowing of `x` to `Literal["a"]`,
and instead keeps it as `str`:

```python
from typing import Literal

def parse(value: str) -> Literal["a"] | None:
    # with `strict-equality-semantics = true`, no narrowing will occur here,
    # and an error will be emitted on the `return` statement.
    if value == "a":
        return value
    return None
```

Another assumption ty makes by default is that subclasses will never override `__eq__` or
`__ne__`. This allows ty to narrow the following union based on an equality check, despite
the fact that an instance of a subclass of `Foo` could compare equal to `None`, and it's
perfectly valid to pass an instance of a subclass into the `x` parameter of this function:

```python
def narrow(x: Foo | None, other: Foo) -> None:
    if x == other:
        # with this option enabled, `x` will still have type `Foo | None` here,
        # since it is legal to subclass `Foo` and override its `__eq__` method.
        reveal_type(x)
```

Many operations in Python implicitly call `__eq__` under the hood; enabling this option
will also impact those operations. For example, this option will also impact narrowing from
`in` checks, and narrowing in `match` statements that use value patterns:

```python
def narrow_in(x: Foo | None, other: list[Foo]) -> None:
    if x in other:
        # with this option enabled, `x` will still have type `Foo | None` here,
        # since the `in` operator implicitly calls `__eq__` on each element of `other`.
        reveal_type(x)


def narrow_match(x: str) -> None:
    match x:
        case "a":
            # with this option enabled, `x` will still have type `str` here,
            # since this `case` branch will be taken by any object that compares
            # equal to `"a"`, including subclasses of `str`.
            reveal_type(x)
```

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.analysis]
    # Preserve broad builtin types instead of narrowing them to literals
    strict-equality-semantics = true
    ```

=== "ty.toml"

    ```toml
    [analysis]
    # Preserve broad builtin types instead of narrowing them to literals
    strict-equality-semantics = true
    ```

---

## `environment`

### `extra-paths`

User-provided paths that should take first priority in module resolution.

This is an advanced option that should usually only be used for first-party or third-party
modules that are not installed into your Python environment in a conventional way.
Use the `python` option to specify the location of your Python environment.

This option is similar to mypy's `MYPYPATH` environment variable and pyright's `stubPath`
configuration setting.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    extra-paths = ["./shared/my-search-path"]
    ```

=== "ty.toml"

    ```toml
    [environment]
    extra-paths = ["./shared/my-search-path"]
    ```

---

### `python`

Path to your project's Python environment or interpreter.

ty uses the `site-packages` directory of your project's Python environment
to resolve third-party (and, in some cases, first-party) imports in your code.

This can be a path to:

- A Python interpreter, e.g. `.venv/bin/python3`
- A virtual environment directory, e.g. `.venv`
- A system Python [`sys.prefix`] directory, e.g. `/usr`

If you're using a project management tool such as uv, you should not generally need to
specify this option, as commands such as `uv run` will set the `VIRTUAL_ENV` environment
variable to point to your project's virtual environment. ty can also infer the location of
your environment from an activated Conda environment, and will look for a `.venv` directory
in the project root if none of the above apply. Failing that, ty will look for a `python3`
or `python` binary available in `PATH`.

[`sys.prefix`]: https://docs.python.org/3/library/sys.html#sys.prefix

**Default value**: `null`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    python = "./custom-venv-location/.venv"
    ```

=== "ty.toml"

    ```toml
    [environment]
    python = "./custom-venv-location/.venv"
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    # Tailor type stubs and conditionalized type definitions to windows.
    python-platform = "win32"
    ```

=== "ty.toml"

    ```toml
    [environment]
    # Tailor type stubs and conditionalized type definitions to windows.
    python-platform = "win32"
    ```

---

### `python-version`

Specifies the version of Python that will be used to analyze the source code.
The version should be specified as a string in the format `M.m` where `M` is the major version
and `m` is the minor (e.g. `"3.7"` or `"3.12"`).
If a version is provided, ty will generate errors if the source code makes use of language features
that are not supported in that version.

ty officially supports type checking code that targets Python 3.10 and later. Python 3.7
through 3.9 can still be selected, but ty may produce false positives or false negatives for
standard-library APIs because its bundled stubs do not fully describe those versions.

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

**Default value**: `"3.14"`

**Type**: `"3.7" | "3.8" | "3.9" | "3.10" | "3.11" | "3.12" | "3.13" | "3.14" | "3.15"`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    python-version = "3.12"
    ```

=== "ty.toml"

    ```toml
    [environment]
    python-version = "3.12"
    ```

---

### `root`

The root paths of the project, used for finding first-party modules.

Accepts a list of directory paths searched in priority order (first has highest priority).

If left unspecified, ty will try to detect common project layouts and initialize `root` accordingly.
The project root (`.`) is always included. Additionally, the following directories are included
if they exist and are not packages (i.e. they do not contain `__init__.py` or `__init__.pyi` files):

* `./src`
* `./<project-name>` (if a `./<project-name>/<project-name>` directory exists)
* `./python`

**Default value**: `null`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    # Multiple directories (priority order)
    root = ["./src", "./lib", "./vendor"]
    ```

=== "ty.toml"

    ```toml
    [environment]
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.environment]
    typeshed = "/path/to/custom/typeshed"
    ```

=== "ty.toml"

    ```toml
    [environment]
    typeshed = "/path/to/custom/typeshed"
    ```

---

## `overrides`

Configuration override that applies to specific files based on glob patterns.

An override allows you to apply different rule configurations to specific
files or directories. Multiple overrides can match the same file, with
later overrides take precedence. Override rules take precedence over global
rules for matching files.

For example, to relax enforcement of rules in test files:

```toml
[[tool.ty.overrides]]
include = ["tests/**", "**/test_*.py"]

[tool.ty.overrides.rules]
possibly-unresolved-reference = "warn"
```

Or, to ignore a rule in generated files but retain enforcement in an important file:

```toml
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [[tool.ty.overrides]]
    exclude = [
        "generated",
        "*.proto",
        "tests/fixtures/**",
        "!tests/fixtures/important.py"  # Include this one file
    ]
    ```

=== "ty.toml"

    ```toml
    [[overrides]]
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [[tool.ty.overrides]]
    include = [
        "src",
        "tests",
    ]
    ```

=== "ty.toml"

    ```toml
    [[overrides]]
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

**Type**: `dict[RuleName | "all", "ignore" | "warn" | "error"]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [[tool.ty.overrides]]
    include = ["src"]

    [tool.ty.overrides.rules]
    possibly-unresolved-reference = "ignore"
    ```

=== "ty.toml"

    ```toml
    [[overrides]]
    include = ["src"]

    [overrides.rules]
    possibly-unresolved-reference = "ignore"
    ```

---

## `overrides.analysis`

#### `allowed-unresolved-imports`

A list of module glob patterns for which `unresolved-import` diagnostics should be suppressed.

Details on supported glob patterns:
- `*` matches zero or more characters except `.`. For example, `foo.*` matches `foo.bar` but
  not `foo.bar.baz`; `foo*` matches `foo` and `foobar` but not `foo.bar` or `barfoo`; and `*foo`
  matches `foo` and `barfoo` but not `foo.bar` or `foobar`.
- `**` matches any number of module components (e.g., `foo.**` matches `foo`, `foo.bar`, etc.)
- Prefix a pattern with `!` to exclude matching modules

When multiple patterns match, later entries take precedence.

Glob patterns can be used in combinations with each other. For example, to suppress errors for
any module where the first component contains the substring `test`, use `*test*.**`.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.overrides.analysis]
    # Suppress errors for all `test` modules except `test.foo`
    allowed-unresolved-imports = ["test.**", "!test.foo"]
    ```

=== "ty.toml"

    ```toml
    [overrides.analysis]
    # Suppress errors for all `test` modules except `test.foo`
    allowed-unresolved-imports = ["test.**", "!test.foo"]
    ```

---

#### `replace-imports-with-any`

A list of module glob patterns whose imports should be replaced with `typing.Any`.

Unlike `allowed-unresolved-imports`, this setting replaces the module's type information
with `typing.Any` even if the module can be resolved. Import diagnostics are
unconditionally suppressed for matching modules.

- Prefix a pattern with `!` to exclude matching modules

When multiple patterns match, later entries take precedence.

Glob patterns can be used in combinations with each other. For example, to suppress errors for
any module where the first component contains the substring `test`, use `*test*.**`.

When multiple patterns match, later entries take precedence.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.overrides.analysis]
    # Replace all pandas and numpy imports with Any
    replace-imports-with-any = ["pandas.**", "numpy.**"]
    ```

=== "ty.toml"

    ```toml
    [overrides.analysis]
    # Replace all pandas and numpy imports with Any
    replace-imports-with-any = ["pandas.**", "numpy.**"]
    ```

---

#### `respect-type-ignore-comments`

Whether ty should respect `type: ignore` comments.

When set to `false`, `type: ignore` comments are treated like any other normal
comment and can't be used to suppress ty errors (you have to use `ty: ignore` instead).

Setting this option can be useful when using ty alongside other type checkers or when
you prefer using `ty: ignore` over `type: ignore`.

Defaults to `true`.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.overrides.analysis]
    # Disable support for `type: ignore` comments
    respect-type-ignore-comments = false
    ```

=== "ty.toml"

    ```toml
    [overrides.analysis]
    # Disable support for `type: ignore` comments
    respect-type-ignore-comments = false
    ```

---

#### `strict-equality-semantics`

Configure ty's behavior regarding type inference and narrowing of equality
checks. Defaults to `false`.

By default, ty makes various assumptions about equality checks that match the
intuitions of most Python programmers, but may not be fully sound in all situations.
Enabling this option makes ty more conservative about these assumptions, making it
less likely to infer `Literal[True]` or `Literal[False]` as the result of an
equality check. This has various effects on type checking, including fewer type
narrowing opportunities and more conservative assumptions regarding control flow.

One way in which ty will by default make unsound assumptions is by narrowing an
object `x` of type `str` to `Literal["a"]` after an `if x == "a"` check. This is
unsound because a subclass of `str` with value `"a"` will (by default) compare equal
to `"a"`, but will not be of type `Literal["a"]`:

```pycon
>>> # `Literal["a"]` can only be inhabited by instances of exactly `str`, not
>>> # subclasses, but str subclasses compare equal by default:
>>> class StringSubclass(str): ...
...
>>> StringSubclass("a") == "a"
True
>>>
>>> # This also applies to `StrEnum`s:
>>> from enum import StrEnum
>>> class MyEnum(StrEnum):
...     A = "a"
...
>>> MyEnum.A == "a"
True
```

Enabling this option prevents the unsound narrowing of `x` to `Literal["a"]`,
and instead keeps it as `str`:

```python
from typing import Literal

def parse(value: str) -> Literal["a"] | None:
    # with `strict-equality-semantics = true`, no narrowing will occur here,
    # and an error will be emitted on the `return` statement.
    if value == "a":
        return value
    return None
```

Another assumption ty makes by default is that subclasses will never override `__eq__` or
`__ne__`. This allows ty to narrow the following union based on an equality check, despite
the fact that an instance of a subclass of `Foo` could compare equal to `None`, and it's
perfectly valid to pass an instance of a subclass into the `x` parameter of this function:

```python
def narrow(x: Foo | None, other: Foo) -> None:
    if x == other:
        # with this option enabled, `x` will still have type `Foo | None` here,
        # since it is legal to subclass `Foo` and override its `__eq__` method.
        reveal_type(x)
```

Many operations in Python implicitly call `__eq__` under the hood; enabling this option
will also impact those operations. For example, this option will also impact narrowing from
`in` checks, and narrowing in `match` statements that use value patterns:

```python
def narrow_in(x: Foo | None, other: list[Foo]) -> None:
    if x in other:
        # with this option enabled, `x` will still have type `Foo | None` here,
        # since the `in` operator implicitly calls `__eq__` on each element of `other`.
        reveal_type(x)


def narrow_match(x: str) -> None:
    match x:
        case "a":
            # with this option enabled, `x` will still have type `str` here,
            # since this `case` branch will be taken by any object that compares
            # equal to `"a"`, including subclasses of `str`.
            reveal_type(x)
```

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.overrides.analysis]
    # Preserve broad builtin types instead of narrowing them to literals
    strict-equality-semantics = true
    ```

=== "ty.toml"

    ```toml
    [overrides.analysis]
    # Preserve broad builtin types instead of narrowing them to literals
    strict-equality-semantics = true
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.src]
    exclude = [
        "generated",
        "*.proto",
        "tests/fixtures/**",
        "!tests/fixtures/important.py"  # Include this one file
    ]
    ```

=== "ty.toml"

    ```toml
    [src]
    exclude = [
        "generated",
        "*.proto",
        "tests/fixtures/**",
        "!tests/fixtures/important.py"  # Include this one file
    ]
    ```

---

### `exclude-scripts`

Whether to exclude files containing PEP 723 inline script metadata unless they are
explicitly passed on the command line.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.src]
    exclude-scripts = true
    ```

=== "ty.toml"

    ```toml
    [src]
    exclude-scripts = true
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.src]
    include = [
        "src",
        "tests",
    ]
    ```

=== "ty.toml"

    ```toml
    [src]
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

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.src]
    respect-ignore-files = false
    ```

=== "ty.toml"

    ```toml
    [src]
    respect-ignore-files = false
    ```

---

### `root`

!!! warning "Deprecated"
    This option has been deprecated. Use `environment.root` instead.

The root of the project, used for finding first-party modules.

If left unspecified, ty will try to detect common project layouts and initialize `src.root` accordingly.
The project root (`.`) is always included. Additionally, the following directories are included
if they exist and are not packages (i.e. they do not contain `__init__.py` or `__init__.pyi` files):

* `./src`
* `./<project-name>` (if a `./<project-name>/<project-name>` directory exists)
* `./python`

**Default value**: `null`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.src]
    root = "./app"
    ```

=== "ty.toml"

    ```toml
    [src]
    root = "./app"
    ```

---

## `terminal`

### `error-on-warning`

Use exit code 1, even if all diagnostics only had `warning` severity.

Defaults to `true`.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.terminal]
    # Exit with code 0 if all diagnostics had `warning` severity.
    error-on-warning = false
    ```

=== "ty.toml"

    ```toml
    [terminal]
    # Exit with code 0 if all diagnostics had `warning` severity.
    error-on-warning = false
    ```

---

### `output-format`

The format to use for printing diagnostic messages.

Defaults to `full`.

**Default value**: `full`

**Type**: `full | concise | github | gitlab | junit`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.ty.terminal]
    output-format = "concise"
    ```

=== "ty.toml"

    ```toml
    [terminal]
    output-format = "concise"
    ```

---

