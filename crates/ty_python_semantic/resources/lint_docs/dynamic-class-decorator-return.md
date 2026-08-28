## What it does

Detects decorator applications that replace a class with `Any` or another [dynamic type], or with a
gradual `type[]` type whose type argument is dynamic, such as `type[Any]`.

## Why is this bad?

A decorator can replace the class it receives with any object. Type checkers therefore use the
decorator's return type as the type of the decorated class. If the decorator returns `Any` or
`type[Any]`, the original type is lost, along with the type checker's ability to catch invalid calls
and attribute accesses:

```py
from typing import Any


def dynamic_decorator(cls: type) -> Any:
    return cls


# error: "Decorator returns `Any`"
@dynamic_decorator
class Stringify:
    def __init__(self, value: int) -> None:
        self.value = str(value)


# no longer inferred as being a class!
reveal_type(Stringify)  # revealed: Any

# No type error is reported, even though `Stringify` expects an integer.
Stringify("not an integer")
```

This rule identifies the point where a decorator erases useful type information, before that
imprecision spreads to every use of the decorated class. It can be especially useful in cases where
the decorator is defined in a third-party library. Whereas linter rules such as [`ANN401`][ann401]
can complain about dynamic annotations in your first-party code, they cannot identify instances
where unsound types leak into your code due to imprecise type annotations in third-party code
installed into `site-packages`.

Unlike `dynamic-function-decorator-return`, ty's equivalent rule for function decorators,
`dynamic-class-decorator-return` does not flag class decorators that are inferred as returning
`Unknown` or `type[Unknown]`. This is because ty already applies pragmatic special casing for these
decorators. Since the vast majority of class decorators return the original class more-or-less
unchanged, ty assumes that this is the case for a class decorator inferred as returning `Unknown`
(usually this is caused by a missing return-type annotation, an invalid type annotation, or an
unresolved import). On encountering a class decorated with a decorator returning `Unknown` or
`type[Unknown]`, ty simply retains the original type of the class rather than replacing the inferred
type of the symbol with `Unknown`:

```py
def unannotated_decorator(cls):
    return cls


class Foo:
    pass


# When called directly, ty infers `unannotated_decorator` as returning `Unknown`:
reveal_type(unannotated_decorator(Foo))  # revealed: Unknown


# But no diagnostic here...
@unannotated_decorator
class Bar:
    pass


# ...because as a decorator, ty applies special casing to ensure that the stored type
# of the `Bar` symbol is not replaced with `Unknown` here, which means that there
# is no `dynamic-class-decorator-return` diagnostic on the class declaration above:
reveal_type(Bar)  # revealed: <class 'Bar'>
```

## Examples

`third_party_library.py`:

```py
from typing import Any


def dynamic_decorator(cls: type) -> Any:
    return cls
```

`first_party.py`:

```py
from third_party_library import dynamic_decorator


# error: "Decorator returns `Any`"
@dynamic_decorator
class Greeting:
    def __init__(self, name: str) -> None:
        self.message = f"Hello, {name}!"
```

If making a PR to the third-party library to improve their annotations is not possible, fixes for
this diagnostic could include writing your own decorator or introducing a type-safe wrapper:

```py
from typing import TypeVar

from third_party_library import dynamic_decorator


ClassT = TypeVar("ClassT", bound=type)


def typed_wrapper(cls: ClassT) -> ClassT:
    decorated = dynamic_decorator(cls)
    assert decorated is cls
    return decorated


@typed_wrapper
class Greeting:
    def __init__(self, name: str) -> None:
        self.message = f"Hello, {name}!"
```

## Default level

This rule is disabled by default. It is intended for advanced users wanting additional soundness
checks from their type checker, not for users who have just started to use type checkers on their
Python code.

## See also

- `dynamic-function-decorator-return` is a similar rule that triggers on function decorators rather
    than class decorators

[ann401]: https://docs.astral.sh/ruff/rules/any-type/
[dynamic type]: https://typing.python.org/en/latest/spec/glossary.html#term-dynamic-type
