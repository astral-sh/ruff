## What it does

Detects decorator applications that replace a function with `Any` or another [dynamic type].

## Why is this bad?

A decorator can replace the function it receives with any object. Type checkers therefore use the
decorator's return type as the type of the decorated function. If the decorator returns `Any` or
`Unknown` (explicitly or implicitly), the original type is lost, along with the type checker's
ability to catch invalid calls and attribute accesses:

```py
from collections.abc import Callable


def untyped_decorator(function: Callable[..., object]):
    return function


# error: "Decorator returns `Unknown`"
@untyped_decorator
def stringify(value: int) -> str:
    return str(value)


# No type error is reported, even though `stringify` expects an integer.
stringify("not an integer")
```

This rule identifies the point where a decorator erases useful type information, before that
imprecision spreads to every use of the decorated function. It can be especially useful in cases
where the decorator is defined in a third-party library. Whereas linter rules such as
[`ANN201`][ann201] and [`ANN202`][ann202] can complain about missing annotations in your
first-party code, they cannot identify instances where unsound types leak into your code due to
missing type annotations in third-party code installed into `site-packages`.

## Examples

`third_party_library.py`:

```py
from collections.abc import Callable


def untyped_decorator(function: Callable[..., object]):
    return function
```

`first_party.py`:

```py
from third_party_library import untyped_decorator


# error: "Decorator returns `Unknown`"
@untyped_decorator
def greet(name: str) -> str:
    return f"Hello, {name}!"
```

If making a PR to the third-party library to improve their annotations is not possible, fixes for
this diagnostic could include writing your own decorator or introducing a type-safe wrapper:

```py
from collections.abc import Callable
from typing import TypeVar

from third_party_library import untyped_decorator


FunctionT = TypeVar("FunctionT", bound=Callable[..., object])


def typed_wrapper(f: FunctionT) -> FunctionT:
    decorated = untyped_decorator(f)
    assert decorated is f
    return decorated


@typed_wrapper
def greet(name: str) -> str:
    return f"Hello, {name}!"
```

## Default level

This rule is disabled by default. It is intended for advanced users wanting additional soundness
checks from their type checker, not for users who have just started to use type checkers on their
Python code.

[ann201]: https://docs.astral.sh/ruff/rules/missing-return-type-undocumented-public-function/
[ann202]: https://docs.astral.sh/ruff/rules/missing-return-type-private-function/
[dynamic type]: https://typing.python.org/en/latest/spec/glossary.html#term-dynamic-type
