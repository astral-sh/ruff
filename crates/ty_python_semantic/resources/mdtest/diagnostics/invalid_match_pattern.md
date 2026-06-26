# Invalid match pattern

## Too many positional subpatterns

The number of positional subpatterns must not exceed the length of a statically known fixed-length
`__match_args__` tuple. The tuple type can come from an inferred value, an annotation, or another
expression.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

class Point:
    __match_args__ = ("x", "y")

one_arg = ("value",)

class FromVariable:
    __match_args__ = one_arg

def make_args() -> tuple[Literal["value"]]:
    return ("value",)

class FromCall:
    __match_args__ = make_args()

class Annotated:
    __match_args__: tuple[Literal["value"]] = make_args()

type MatchArgs = tuple[Literal["value"]]

class Aliased:
    __match_args__: MatchArgs = ("value",)

def describe(
    point: Point,
    from_variable: FromVariable,
    from_call: FromCall,
    annotated: Annotated,
    aliased: Aliased,
) -> None:
    match point:
        case Point(_, _):
            pass

    match point:
        # error: [invalid-match-pattern] "Too many positional subpatterns for `<class 'Point'>`: expected 2, got 3"
        case Point(_, _, _):
            pass

    match from_variable:
        case FromVariable(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match from_call:
        case FromCall(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match annotated:
        case Annotated(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match aliased:
        case Aliased(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass
```

An absent `__match_args__` accepts no positional subpatterns for source classes. Builtins with
match-self behavior accept one; other known builtins accept none.

```py
class Missing: ...

def describe(missing: Missing, integer: int, complex_number: complex) -> None:
    match missing:
        case Missing(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass

    match integer:
        case int(_):
            pass

    match integer:
        case int(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match complex_number:
        case complex(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass
```

## Generated match arguments

Dataclasses and named tuples synthesize `__match_args__`. Keyword-only dataclass fields are omitted.

```py
from dataclasses import dataclass, field
from typing import NamedTuple

@dataclass
class Point:
    x: int
    y: int

@dataclass(match_args=False)
class NoMatch:
    x: int
    y: int

@dataclass(kw_only=True)
class KeywordOnly:
    x: int

@dataclass
class PartlyKeywordOnly:
    x: int
    y: int = field(kw_only=True)

class NamedPoint(NamedTuple):
    x: int
    y: int

def describe(
    point: Point,
    no_match: NoMatch,
    keyword_only: KeywordOnly,
    partly_keyword_only: PartlyKeywordOnly,
    named_point: NamedPoint,
) -> None:
    match point:
        case Point(_, _, _):  # error: [invalid-match-pattern] "expected 2, got 3"
            pass

    match no_match:
        case NoMatch(_, _):  # error: [invalid-match-pattern] "expected 0, got 2"
            pass

    match keyword_only:
        case KeywordOnly(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass

    match partly_keyword_only:
        case PartlyKeywordOnly(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match named_point:
        case NamedPoint(_, _, _):  # error: [invalid-match-pattern] "expected 2, got 3"
            pass
```

## Invalid `__match_args__` type

A definitely non-tuple `__match_args__` type cannot support positional subpatterns.

```py
from typing_extensions import LiteralString

bad_args = ["value"]

class FromVariable:
    __match_args__ = bad_args

def make_args() -> str:
    return "value"

class FromCall:
    __match_args__ = make_args()

class Annotated:
    __match_args__: int = 1

class Position:
    __match_args__: LiteralString = "field"

def describe(
    from_variable: FromVariable,
    from_call: FromCall,
    annotated: Annotated,
    position: Position,
) -> None:
    match from_variable:
        # error: [invalid-match-pattern] "must be an exact tuple, not `list[str]`"
        case FromVariable(_):
            pass

    match from_call:
        # error: [invalid-match-pattern] "must be an exact tuple, not `str`"
        case FromCall(_):
            pass

    match annotated:
        # error: [invalid-match-pattern] "must be an exact tuple, not `int`"
        case Annotated(_):
            pass

    match position:
        # error: [invalid-match-pattern] "must be an exact tuple, not `LiteralString`"
        case Position(_):
            pass
```

## Semantic member lookup

The check uses the resolved member type, including inherited and descriptor-provided values.

```py
from typing import Literal, final

class Base:
    __match_args__ = ("value",)

class Derived(Base): ...

@final
class MatchArgsDescriptor:
    def __get__(self, instance: object | None, owner: type[object]) -> tuple[Literal["value"]]:
        return ("value",)

class Descriptor:
    __match_args__ = MatchArgsDescriptor()

def describe(derived: Derived, descriptor: Descriptor) -> None:
    match derived:
        case Derived(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass

    match descriptor:
        case Descriptor(_):
            pass

    match descriptor:
        case Descriptor(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
            pass
```

## Unknown limits

No diagnostic is emitted when the member type does not establish a fixed tuple length or a definite
non-tuple value.

```py
from typing import Literal

class Variadic:
    __match_args__: tuple[str, ...] = ()

class Mixed:
    __match_args__: tuple[Literal["value"]] | list[str] = ("value",)

class TupleSubclass(tuple[str]): ...

class SubclassValue:
    __match_args__: TupleSubclass

def describe(
    variadic: Variadic,
    mixed: Mixed,
    subclass_value: SubclassValue,
) -> None:
    match variadic:
        case Variadic(_, _):
            pass

    match mixed:
        case Mixed(_, _):
            pass

    match subclass_value:
        case SubclassValue(_):
            pass
```

## Missing stub member

An omitted stub member does not establish a runtime limit.

```py
from lib import Model

def describe(model: Model) -> None:
    match model:
        case Model(_):
            pass
```

`lib.pyi`:

```pyi
class Model: ...
```

## Patterns without positional subpatterns

Patterns without positional subpatterns do not inspect `__match_args__`.

```py
class Model:
    __match_args__ = ["value"]
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

The invalid-class diagnostic takes precedence over positional validation.

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
