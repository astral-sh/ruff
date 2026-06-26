# Invalid match pattern

## Too many positional subpatterns

The diagnostic is emitted when the positional limit comes from a direct, unconditional, unannotated
tuple-literal assignment in the body of a plain class without decorators, explicit bases, or an
explicit metaclass. A missing `__match_args__` has a limit of zero; match-self builtins such as
`int` have a limit of one.

```py
class Point:
    __match_args__ = ("x", "y")

class Missing: ...

class Empty:
    __match_args__ = ()

class Reassigned:
    __match_args__ = ("x",)
    __match_args__ = ("x", "y")

def describe(point: Point, missing: Missing, empty: Empty, reassigned: Reassigned, integer: int) -> None:
    match point:
        case Point(_, _):
            pass

    match missing:
        case Missing(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass

    match point:
        # error: [invalid-match-pattern] "Too many positional subpatterns for `<class 'Point'>`: expected 2, got 3"
        case Point(_, _, _):
            pass

    match empty:
        case Empty(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass

    match reassigned:
        case Reassigned(_, _, _):  # error: [invalid-match-pattern] "expected 2, got 3"
            pass

    match integer:
        case int(_):
            pass

    match integer:
        case int(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass
```

Overflow is checked before the contents of the tuple because this is also the order used at runtime.

```py
class InvalidElement:
    __match_args__ = (1,)

def describe(value: InvalidElement) -> None:
    match value:
        # error: [invalid-match-pattern] "expected 1, got 2"
        case InvalidElement(_, _):
            pass
```

## Deliberately conservative cases

The diagnostic does not attempt to model alternate runtime states or infer exact runtime values from
declarations. Decorated classes, classes with explicit bases or metaclasses, inherited, metaclass,
or synthesized values, declarations, conditional bindings, unions, variadic tuples, tuple
subclasses, malformed `__match_args__` values, and assignments whose value is not a tuple literal
are deliberately left undiagnosed.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import TYPE_CHECKING, Literal, TypeVar

flag: bool = bool()

T = TypeVar("T")

def identity(cls: type[T]) -> type[T]:
    return cls

@identity
class Decorated:
    __match_args__ = ("x",)

class ExplicitBase(object):
    __match_args__ = ("x",)

class ExplicitMetaclass(metaclass=type):
    __match_args__ = ("x",)

class MatchSelfOverride(int):
    __match_args__ = ("real", "imag")

class MatchSelfSubclass(int): ...

class AnnotationOnly:
    __match_args__: tuple[Literal["x"]]

class AnnotatedBinding:
    __match_args__: tuple[Literal["x"]] = ("x",)

class AssignedName:
    args = ("x",)
    __match_args__ = args

class Conditional:
    if flag:
        __match_args__ = ("x",)

class TypeCheckingOnly:
    if TYPE_CHECKING is True:
        __match_args__ = ("x",)

class DirectTypeCheckingOnly:
    if TYPE_CHECKING:
        __match_args__ = ("x",)

class RuntimeOnly:
    if not TYPE_CHECKING:
        __match_args__ = ("x",)

class InvalidType:
    __match_args__ = ["x"]

class InvalidElementValue:
    __match_args__ = (1,)

def describe(subject: object) -> None:
    match subject:
        case Decorated(_, _):
            pass
        case ExplicitBase(_, _):
            pass
        case ExplicitMetaclass(_, _):
            pass
        case MatchSelfOverride(_, _):
            pass
        case MatchSelfSubclass(_, _):
            pass
        case AnnotationOnly(_, _):
            pass
        case AnnotatedBinding(_, _):
            pass
        case AssignedName(_, _):
            pass
        case Conditional(_, _):
            pass
        case TypeCheckingOnly(_, _):
            pass
        case DirectTypeCheckingOnly(_, _):
            pass
        case RuntimeOnly(_, _):
            pass
        case InvalidType(_):
            pass
        case InvalidElementValue(_):
            pass
```

Patterns without positional subpatterns do not inspect `__match_args__`.

```py
class Model:
    __match_args__ = ("value",)
    value: int = 0

def describe(value: Model) -> None:
    match value:
        case Model():
            pass
    match value:
        case Model(value=_):
            pass
```

## Invalid pattern classes do not cascade

The existing invalid-class diagnostic takes precedence over positional validation.

```py
from typing import Protocol, TypedDict

class Payload(TypedDict):
    value: int

class HasValue(Protocol):
    value: int

def describe(value: object) -> None:
    match value:
        # error: [isinstance-against-typed-dict]
        case Payload(_):
            pass

    match value:
        # error: [isinstance-against-protocol]
        case HasValue(_):
            pass
```
