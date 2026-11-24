# `redundant-numeric-union` (`PYI041`)

```toml
target-version = "py311"

[lint]
select = ["PYI041"]
```

## Ordinary parameter annotations

Numeric unions are redundant when they are used only for static typing.

```py
def function(value: int | float) -> None: ...  # error: [redundant-numeric-union]
```

## Single-dispatch registrations

The first annotated parameter determines the concrete types registered at runtime, so its numeric
union is not redundant.

```py
import functools

@functools.singledispatch
def dispatch(value: object) -> None: ...

@dispatch.register
def _(value: int | float) -> None: ...
```

## Generic single-dispatch functions

The generic function's annotation does not register concrete types, so its numeric union remains
redundant even when a registered implementation needs the same union.

```py
import functools

@functools.singledispatch
def dispatch(value: int | float) -> None: ...  # error: [redundant-numeric-union]

@dispatch.register
def _(value: int | float) -> None: ...
```

## Other parameters of registered functions

Numeric unions remain redundant for parameters that do not determine dispatch registration.

```py
import functools

@functools.singledispatch
def dispatch(value: object) -> None: ...

@dispatch.register
def _(value: float | complex, other: int | float) -> None: ...  # snapshot: redundant-numeric-union
```

```snapshot
error[PYI041]: Use `float` instead of `int | float`
 --> src/mdtest_snippet.py:7:38
  |
7 | def _(value: float | complex, other: int | float) -> None: ...  # snapshot: redundant-numeric-union
  |                                      ^^^^^^^^^^^
help: Remove redundant type
  |
6 | @dispatch.register
  - def _(value: float | complex, other: int | float) -> None: ...  # snapshot: redundant-numeric-union
7 + def _(value: float | complex, other: float) -> None: ...  # snapshot: redundant-numeric-union
  |
```

## Single-dispatch method registrations

The dispatch parameter comes after the unannotated instance parameter.

```py
import functools

class Dispatch:
    @functools.singledispatchmethod
    def dispatch(self, value: object) -> None: ...

    @dispatch.register
    def _(self, value: int | float) -> None: ...
```

## Explicit single-dispatch registrations

An explicit registration does not inspect the implementation's parameter annotation.

```py
import functools

@functools.singledispatch
def dispatch(value: object) -> None: ...

@dispatch.register(int | float)
def _(value: int | float) -> None: ...  # error: [redundant-numeric-union]
```

## Stub annotations

Registered dispatch implementations retain their numeric unions in stub files as well.

```pyi
import functools

@functools.singledispatch
def dispatch(value: object) -> None: ...

@dispatch.register
def _(value: int | float) -> None: ...
```
