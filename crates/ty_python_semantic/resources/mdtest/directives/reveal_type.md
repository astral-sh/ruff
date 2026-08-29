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

## On Python 3.10

On Python versions before 3.11, the fix imports `reveal_type` from `typing_extensions` because
`typing.reveal_type` is not available.

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
help: Import `reveal_type` from `typing_extensions`
  |
1 | # snapshot: undefined-reveal
2 + from typing_extensions import reveal_type
3 | reveal_type(1)  # error: [revealed-type] "Literal[1]"
  |
note: This is an unsafe fix and may change runtime behavior
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
