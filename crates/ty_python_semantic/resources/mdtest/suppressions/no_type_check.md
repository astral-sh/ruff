# `@no_type_check`

> If a type checker supports the `no_type_check` decorator for functions, it should suppress all
> type errors for the def statement and its body including any nested functions or classes. It
> should also ignore all parameter and return type annotations and treat the function as if it were
> unannotated. [source](https://typing.python.org/en/latest/spec/directives.html#no-type-check)

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

## Errors in decorator applications

We currently suppress all decorator-application errors on a function decorated with `no_type_check`,
regardless of whether those errors occur in applying decorators appearing before or after
`@no_type_check`. TODO: it would be more intuitive and consistent with our behavior for
decorator-expression errors (see below) if we only suppressed these for decorators located after
`@no_type_check` in source order.

```py
from typing import no_type_check

def takes_int(value: int) -> int:
    return value

# TODO this should be an error:
@takes_int
@no_type_check
def before() -> None: ...

# no error, swallowed by `no_type_check`:
@no_type_check
@takes_int
def after() -> None: ...

# error: [invalid-argument-type]
@takes_int
def checked() -> None: ...
```

## Error in following decorator expression

Unlike Pyright and mypy, we also suppress diagnostics in decorator expressions appearing after the
`no_type_check` decorator. We do this because it more closely matches Python's runtime semantics of
decorators. For more details, see the discussion on the
[PR adding `@no_type_check` support](https://github.com/astral-sh/ruff/pull/15122#discussion_r1896869411).

```py
from typing import no_type_check

@no_type_check
@unknown_decorator
def test() -> int:
    return a + 5
```

## Error in preceding decorator expression

We don't suppress diagnostics for decorator expressions appearing before the `no_type_check`
decorator.

```py
from typing import no_type_check

@unknown_decorator  # error: [unresolved-reference]
@no_type_check
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

## Errors in function declarations

Post-inference checks on the function declaration are also suppressed.

```py
from typing import no_type_check

@no_type_check
def positional(x: int, __y: str): ...
```

## `no_type_check` on classes isn't supported

ty does not support decorating classes with `no_type_check`. The behavior of `no_type_check` when
applied to classes is
[not specified currently](https://typing.python.org/en/latest/spec/directives.html#no-type-check),
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
    # error: [unused-ignore-comment] "Unused `ty: ignore` directive"
    return x + 5  # ty: ignore[unresolved-reference]
```
