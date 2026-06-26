# Invalid match pattern

## Too many positional subpatterns

The diagnostic is emitted when the positional limit comes from a direct, unconditional tuple literal
in the body of a plain class without decorators, explicit bases, or an explicit metaclass. A
statically missing `__match_args__` has a limit of zero; match-self builtins such as `int` have a
limit of one.

```py
from typing import Literal

class Point:
    __match_args__ = ("x", "y")

class Missing: ...

class Empty:
    __match_args__ = ()

class Reassigned:
    __match_args__ = ("x",)
    __match_args__ = ("x", "y")

class AnnotationOnly:
    __match_args__: tuple[str]

class AnnotatedBinding:
    __match_args__: tuple[Literal["x"]] = ("x",)

def describe(
    point: Point,
    missing: Missing,
    empty: Empty,
    reassigned: Reassigned,
    annotation_only: AnnotationOnly,
    annotated_binding: AnnotatedBinding,
    integer: int,
) -> None:
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

    match annotation_only:
        case AnnotationOnly(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass

    match annotated_binding:
        case AnnotatedBinding(_, _):  # error: [invalid-match-pattern] "expected 1, got 2"
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

## Invalid `__match_args__` type

A direct, unconditional list or string literal is invalid whenever a positional subpattern is used.

```py
from typing_extensions import LiteralString

class ListMatchArgs:
    __match_args__ = ["x"]

class StringMatchArgs:
    __match_args__: LiteralString = "x"

class PlainStringMatchArgs:
    __match_args__ = "x"

def describe(
    list_value: ListMatchArgs,
    string_value: StringMatchArgs,
    plain_string_value: PlainStringMatchArgs,
) -> None:
    match list_value:
        # error: [invalid-match-pattern] "must be an exact tuple, not `list[str]`"
        case ListMatchArgs(_):
            pass

    match string_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case StringMatchArgs(_):
            pass

    match plain_string_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case PlainStringMatchArgs(_):
            pass
```

## Dataclass without `__match_args__`

```py
from dataclasses import dataclass

@dataclass(match_args=False)
class NoMatchArgs:
    value: int

def describe(value: NoMatchArgs) -> None:
    match value:
        case NoMatchArgs(_):  # error: [invalid-match-pattern] "expected 0, got 1"
            pass
```

## Descriptor-provided `__match_args__`

Descriptor lookup can produce a valid tuple even when the raw binding is not a tuple.

```py
from typing import Literal, final

@final
class MatchArgsDescriptor:
    def __get__(self, instance: object | None, owner: type[object]) -> tuple[Literal["x"]]:
        return ("x",)

class Model:
    __match_args__ = MatchArgsDescriptor()

def describe(value: Model) -> None:
    match value:
        case Model(_):
            pass
```

## Missing `__match_args__` in a stub

An omitted stub member does not prove that the runtime class lacks `__match_args__`.

```py
from lib import Declared, Point

def describe(point: Point, declared: Declared) -> None:
    match point:
        case Point(_):
            pass
    match declared:
        case Declared(_):
            pass
```

`lib.pyi`:

```pyi
from typing import Literal

class Point:
    x: int

class Declared:
    __match_args__: tuple[Literal["x"]]
    x: int
```

`lib.py`:

```py
class Point:
    __match_args__ = ("x",)
    x = 1

class Declared:
    __match_args__ = ("x",)
    x = 1
```

## Type-checking-only pattern classes

The class selected during type checking may be replaced by a different runtime class.

```py
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    class Model: ...

    @dataclass(match_args=False)
    class DataModel:
        value: int

else:
    class Model:
        __match_args__ = ("value",)

    @dataclass
    class DataModel:
        value: int

def describe(model: Model, data_model: DataModel) -> None:
    match model:
        case Model(_):
            pass
    match data_model:
        case DataModel(_):
            pass
```

## Type-checking-only pattern imports

An imported pattern class may resolve to a different class at runtime.

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from static_model import Model
    from builtins import int as BuiltinModel
else:
    from runtime_model import Model
    from runtime_model import Model as BuiltinModel

def describe(model: Model, builtin_model: BuiltinModel) -> None:
    match model:
        case Model(_):
            pass
    match builtin_model:
        case BuiltinModel(_, _):
            pass
```

`static_model.py`:

```py
class Model: ...
```

`runtime_model.py`:

```py
class Model:
    __match_args__ = ("value", "other")
```

## Mutable `__match_args__` declaration

A wider class-variable declaration permits legal writes after the class body, so its initializer
does not establish an exact runtime limit.

```py
class Model:
    __match_args__: tuple[str, ...] = ()

Model.__match_args__ = ("value",)

def describe(model: Model) -> None:
    match model:
        case Model(_):
            pass
```

## Deliberately conservative cases

The diagnostic does not attempt to model alternate runtime states or infer exact runtime values from
declarations. Decorated classes, classes with explicit bases or metaclasses, inherited, metaclass,
or synthesized values, indirect or conditional bindings, pattern aliases, unions that may contain
tuples, variadic tuples, tuple subclasses, and invalid tuple elements are deliberately left
undiagnosed.

```toml
[environment]
python-version = "3.11"
```

```py
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, TypeVar

flag: bool = bool()

T = TypeVar("T")

def identity(cls: type[T]) -> type[T]:
    return cls

@identity
class Decorated:
    __match_args__ = ("x",)

@identity
@dataclass(match_args=False)
class DecoratedDataclass:
    value: int

class ExplicitBase(object):
    __match_args__ = ("x",)

class ExplicitMetaclass(metaclass=type):
    __match_args__ = ("x",)

class MatchSelfOverride(int):
    __match_args__ = ("real", "imag")

class MatchSelfSubclass(int): ...

class SeparateAnnotationAndBinding:
    __match_args__: tuple[Literal["x"]]
    __match_args__ = ("x",)

args = ("x",)

class AssignedName:
    __match_args__ = args

def make_args() -> tuple[Literal["x"]]:
    return ("x",)

class FromCall:
    __match_args__ = make_args()

class FixedTupleAnnotation:
    __match_args__: tuple[Literal["x"]] = make_args()

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

class InvalidElementValue:
    __match_args__ = (1,)

if TYPE_CHECKING:
    runtime_args = ()
else:
    runtime_args = ("x",)

class RuntimeValue:
    __match_args__ = runtime_args

class StaticModel: ...

class RuntimeModel:
    __match_args__ = ("value",)

if TYPE_CHECKING:
    PatternModel = StaticModel
else:
    PatternModel = RuntimeModel

class Slotted:
    __slots__ = ("__match_args__",)

def describe(subject: object) -> None:
    match subject:
        case Decorated(_, _):
            pass
        case DecoratedDataclass(_):
            pass
        case ExplicitBase(_, _):
            pass
        case ExplicitMetaclass(_, _):
            pass
        case MatchSelfOverride(_, _):
            pass
        case MatchSelfSubclass(_, _):
            pass
        case SeparateAnnotationAndBinding(_, _):
            pass
        case AssignedName(_, _):
            pass
        case FromCall(_, _):
            pass
        case FixedTupleAnnotation(_, _):
            pass
        case Conditional(_, _):
            pass
        case TypeCheckingOnly(_, _):
            pass
        case DirectTypeCheckingOnly(_, _):
            pass
        case RuntimeOnly(_, _):
            pass
        case InvalidElementValue(_):
            pass
        case RuntimeValue(_):
            pass
        case PatternModel(_):
            pass
        case Slotted(_):
            pass
```

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
