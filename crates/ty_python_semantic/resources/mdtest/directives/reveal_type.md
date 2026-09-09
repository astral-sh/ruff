# `reveal_type`

`reveal_type` is used to inspect the type of an expression at a given point in the code. It is often
used for debugging and understanding how types are inferred by the type checker.

```toml
[environment]
python-version = "3.11"
```

## Basic usage

```py
from typing_extensions import reveal_type

reveal_type(1)  # revealed: Literal[1]
```

This also works with the fully qualified name:

```py
import typing_extensions

typing_extensions.reveal_type(1)  # revealed: Literal[1]
```

The return type of `reveal_type` is the type of the argument:

```py
from typing_extensions import assert_type

def _(x: int):
    y = reveal_type(x)  # revealed: int
    assert_type(y, int)
```

## Without importing it

For convenience, we also allow `reveal_type` to be used without importing it, even if that would
fail at runtime. The diagnostic offers a fix to import it from `typing`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/mdtest_snippet.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
help: Import `reveal_type` from `typing`
  |
1 | # snapshot: undefined-reveal
2 + from typing import reveal_type
3 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  |
note: This is an unsafe fix and may change runtime behavior
```

## With a shadowed alias

An imported alias can be shadowed by a function parameter. The diagnostic does not offer to replace
`reveal_type` with that alias.

```py
from typing import reveal_type as reveal

def f(reveal: int):
    # snapshot: undefined-reveal
    reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/mdtest_snippet.py:5:5
  |
5 |     reveal_type(1)  # error: [revealed-type] "Literal[1]"
  |     ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

## On Python 3.10 without dependency metadata

On Python versions before 3.11, `reveal_type` requires `typing_extensions`. Without dependency
metadata, we cannot establish that the project declares a direct dependency on `typing_extensions`,
so the diagnostic does not offer an import fix.

```toml
[environment]
python-version = "3.10"
```

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/mdtest_snippet.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

## On Python 3.10 with a direct `typing_extensions` dependency

Dependency metadata can establish that `typing_extensions` is a direct dependency. The diagnostic
offers to import `reveal_type` only if the installed runtime module also exports it. (Typeshed's
stub for `typing_extensions` is not a trustworthy source of information here: the bundled stub
falsely states that `typing_extensions.reveal_type` has always existed.)

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["extensions"] }]

[dependency-metadata.distributions]
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
typing_extensions = ["extensions"]
```

### Available runtime function

This installation of `typing_extensions` provides `reveal_type`, so the diagnostic offers an import
fix.

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def reveal_type(value):
    return value
```

`main.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/main.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
help: Import `reveal_type` from `typing_extensions`
  |
1 | # snapshot: undefined-reveal
2 + from typing_extensions import reveal_type
3 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  |
note: This is an unsafe fix and may change runtime behavior
```

### Older runtime module

An installation of `typing_extensions` without `reveal_type` cannot provide the runtime import, even
if it is declared as a direct dependency and typeshed's bundled stub includes the function. As such,
no import fix is offered.

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
```

`main.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/main.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

### Missing runtime module

A declared dependency on `typing_extensions` does not establish that the library is installed. When
only typeshed's stub is available and no runtime source is available in `site-packages`, no import
fix is offered.

`/.venv/<path-to-site-packages>/unrelated.py`:

```py
```

`main.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/main.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

## On Python 3.10 with an indirect `typing_extensions` dependency

An installation of `typing_exetnsions` does not justify adding an import if the containing project
does not declare a dependency on the library. A parent project's dependency declaration does not
apply to a nested project.

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[dependency-metadata]
projects = [
    { path = "/src", dependencies = ["extensions"] },
    { path = "/src/member", dependencies = [] },
]

[dependency-metadata.distributions]
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
typing_extensions = ["extensions"]
```

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def reveal_type(value):
    return value
```

`member/main.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/member/main.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

## On Python 3.10 with `typing_extensions` in a dependency group

Files outside the installed package can use direct dependencies from dependency groups. Package code
cannot rely on those groups, however, so only the test file receives an import fix.

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[dependency-metadata]
projects = [{ path = "/src", distribution = "app", group-dependencies = ["extensions"] }]

[dependency-metadata.distributions]
app = { name = "app", editable-path = "/src" }
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
app = ["app"]
typing_extensions = ["extensions"]
```

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def reveal_type(value):
    return value
```

`app/__init__.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/app/__init__.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

`tests/test_app.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/tests/test_app.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
help: Import `reveal_type` from `typing_extensions`
  |
1 | # snapshot: undefined-reveal
2 + from typing_extensions import reveal_type
3 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  |
note: This is an unsafe fix and may change runtime behavior
```

## On Python 3.10 with `typing_extensions` shadowed

A local runtime module can shadow an installed dependency even when type checking uses the bundled
stub. Declaring `typing_extensions` as a dependency does not establish that the import reaches it,
so no import fix is offered.

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["extensions"] }]

[dependency-metadata.distributions]
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
typing_extensions = ["extensions"]
```

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def reveal_type(value):
    return value
```

`typing_extensions.py`:

```py
```

`main.py`:

```py
# snapshot: undefined-reveal
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

```snapshot
warning[undefined-reveal]: `reveal_type` used without importing it
 --> src/main.py:2:1
  |
2 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  | ^^^^^^^^^^^
info: This is allowed for debugging convenience but will fail at runtime
```

## In type-checking blocks

An unimported `reveal_type` cannot fail at runtime inside a `TYPE_CHECKING` block because that code
is never executed at runtime.

Note that this test uses `# error: [revealed-type]` assertions instead of the more common
`# revealed` assertions that we use elsewhere for `reveal_type` calls. `# revealed` assertions
swallow `undefined-reveal` errors as well as asserting the revealed type, but
`# error: [revealed-type]` assertions do not also match `undefined-reveal`. This means that an
unexpected so an unexpected `undefined-reveal` warning would cause these tests to fail.

```py
from typing import TYPE_CHECKING
import typing

if TYPE_CHECKING:
    reveal_type(1)  # error: [revealed-type] "Literal[1]"

    def nested() -> None:
        reveal_type("nested")  # error: [revealed-type] "nested"

if typing.TYPE_CHECKING:
    reveal_type(True)  # error: [revealed-type] "Literal[True]"
```

## In stub files

An unimported `reveal_type` also cannot fail at runtime in a stub file because stub files are never
executed.

As in the previous section, this test uses `# error: [revealed-type]` rather than `revealed:`
assertions to ensure that an unexpected `undefined-reveal` warning is not silently matched.

```pyi
reveal_type(1)  # error: [revealed-type] "Literal[1]"
```

## In unreachable code

Make sure that `reveal_type` works even in unreachable code.

### When importing it

```py
from typing_extensions import reveal_type
import typing_extensions

if False:
    reveal_type(1)  # revealed: Literal[1]
    typing_extensions.reveal_type(1)  # revealed: Literal[1]

if 1 + 1 != 2:
    reveal_type(1)  # revealed: Literal[1]
    typing_extensions.reveal_type(1)  # revealed: Literal[1]
```

### Without importing it

```py
if False:
    reveal_type(1)  # revealed: Literal[1]

if 1 + 1 != 2:
    reveal_type(1)  # revealed: Literal[1]
```
