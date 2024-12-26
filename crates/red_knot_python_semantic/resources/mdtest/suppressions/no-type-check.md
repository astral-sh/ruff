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
    return a + 5
```

## Error in following decorator

Suppress diagnostics for decorators appearing after the `no_type_check` decorator.

```py
from typing import no_type_check

@no_type_check
@unknown_decorator
def test() -> int:
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

Red Knot does not support `no_type_check` annotations on classes currently. The behaviour of
`no_type_check` when applied to classes is
[not specified currently](https://typing.readthedocs.io/en/latest/spec/directives.html#no-type-check),
and applying the decorator to classes is not supported by Pyright or mypy.

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
    # error: [unused-ignore-comment]
    return x + 5  # knot: ignore[unresolved-reference]
```
