# PEP 681 `dataclass_transform` (`RUF008`, `RUF009`)

```toml
[lint]
select = ["RUF008", "RUF009"]
```

`RUF008` and `RUF009` recognize classes built with a custom decorator marked via [PEP
681's `dataclass_transform`](https://peps.python.org/pep-0681/), in addition to the stdlib
`dataclasses.dataclass` and `attrs` decorators. Only decorators defined in the same file being
linted are recognized, since Ruff's semantic model does not perform cross-module type inference.

## Field specifiers

A `dataclass_transform`-marked decorator may declare `field_specifiers`: callables that behave
like `dataclasses.field()`. Calls to a declared field specifier are exempt from both rules, and a
`ClassVar` annotation is still excluded, just as with a stdlib dataclass:

```py
from collections.abc import Callable
from typing import Any, ClassVar, TypeVar

from typing_extensions import dataclass_transform

_T = TypeVar("_T")


def default_function() -> list[int]:
    return []


def model_field(*, default: Any = ..., resolver: Callable[[], Any] | None = None) -> Any:
    return default


@dataclass_transform(kw_only_default=True, field_specifiers=(model_field,))
def create_model(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@create_model(init=False)
class CustomerModel:
    # A mutable literal default is flagged, mirroring a stdlib dataclass.
    mutable_default: list[int] = []  # snapshot: mutable-dataclass-default

    # A call to a declared field specifier is fine, mirroring `dataclasses.field()`.
    fine_resolver: int = model_field(resolver=lambda: 0)

    # A call that isn't a declared field specifier is still flagged.
    hidden_mutable_default: list[int] = default_function()  # snapshot: function-call-in-dataclass-default-argument

    # `ClassVar`-annotated attributes are excluded, just as for a stdlib dataclass.
    class_variable: ClassVar[list[int]] = []
```

```snapshot
error[RUF008]: Do not use mutable default values for dataclass attributes
  --> src/mdtest_snippet.py:28:34
   |
28 |     mutable_default: list[int] = []  # snapshot: mutable-dataclass-default
   |                                  ^^


error[RUF009]: Do not perform function call `default_function` in dataclass defaults
  --> src/mdtest_snippet.py:34:41
   |
34 |     hidden_mutable_default: list[int] = default_function()  # snapshot: function-call-in-dataclass-default-argument
   |                                         ^^^^^^^^^^^^^^^^^^
```

## No field specifiers

A `dataclass_transform` decorator with no `field_specifiers` treats every call as a plain,
non-exempt default, so it's flagged by `RUF009` just like a call anywhere else in a dataclass body:

```py
from collections.abc import Callable
from typing import TypeVar

from typing_extensions import dataclass_transform

_T = TypeVar("_T")


def default_function() -> list[int]:
    return []


@dataclass_transform()
def create_model_no_specifiers(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@create_model_no_specifiers()
class NoFieldSpecifiers:
    hidden_mutable_default: list[int] = default_function()  # snapshot: function-call-in-dataclass-default-argument
```

```snapshot
error[RUF009]: Do not perform function call `default_function` in dataclass defaults
  --> src/mdtest_snippet.py:23:41
   |
23 |     hidden_mutable_default: list[int] = default_function()  # snapshot: function-call-in-dataclass-default-argument
   |                                         ^^^^^^^^^^^^^^^^^^
```

## Not a `dataclass_transform`

A class decorated by a function that is not itself marked with `dataclass_transform` is not
treated as a dataclass by either rule:

```py
from collections.abc import Callable
from typing import TypeVar

_T = TypeVar("_T")


def default_function() -> list[int]:
    return []


def not_a_transform(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@not_a_transform()
class NotATransform:
    fine_default: list[int] = default_function()
```
