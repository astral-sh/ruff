# `@no_type_check`

> If a type checker supports the `no_type_check` decorator for functions, it should suppress all
> type errors for the def statement and its body including any nested functions or classes. It
> should also ignore all parameter and return type annotations and treat the function as if it were
> unannotated. [source](https://typing.readthedocs.io/en/latest/spec/directives.html#no-type-check)

## Error in the function body

```py
from typing import no_type_check

@no_type_check
def test() -> int:
    return a + 5
```

## Error in nested function

```py
from typing import no_type_check

@no_type_check
def test() -> int:
    def nested():
        return a + 5
```

## Error in nested class

```py
from typing import no_type_check

@no_type_check
def test() -> int:
    class Nested:
        def inner(self):
            return a + 5
```

## Error in preceding decorator

Don't suppress diagnostics for decorators appearing before the `no_type_check` decorator.

```py
from typing import no_type_check

@unknown_decorator  # error: [unresolved-reference]
@no_type_check
def test() -> int:
    # TODO: this should not be an error
    # error: [unresolved-reference]
    return a + 5
```

## Error in following decorator

Unlike Pyright and mypy, suppress diagnostics appearing after the `no_type_check` decorator. We do
this because it more closely matches Python's runtime semantics of decorators. For more details, see
the discussion on the
[PR adding `@no_type_check` support](https://github.com/astral-sh/ruff/pull/15122#discussion_r1896869411).

```py
from typing import no_type_check

@no_type_check
@unknown_decorator
def test() -> int:
    # TODO: this should not be an error
    # error: [unresolved-reference]
    return a + 5
```

## Error in default value

```py
from typing import no_type_check

@no_type_check
def test(a: int = "test"):
    return x + 5
```

## Error in return value position

```py
from typing import no_type_check

@no_type_check
def test() -> Undefined:
    return x + 5
```

## `no_type_check` on classes isn't supported

Red Knot does not support decorating classes with `no_type_check`. The behaviour of `no_type_check`
when applied to classes is
[not specified currently](https://typing.readthedocs.io/en/latest/spec/directives.html#no-type-check),
and is not supported by Pyright or mypy.

A future improvement might be to emit a diagnostic if a `no_type_check` annotation is applied to a
class.

```py
from typing import no_type_check

@no_type_check
class Test:
    def test(self):
        return a + 5  # error: [unresolved-reference]
```

## `type: ignore` comments in `@no_type_check` blocks

```py
from typing import no_type_check

@no_type_check
def test():
    # error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'unresolved-reference'"
    return x + 5  # knot: ignore[unresolved-reference]
```
