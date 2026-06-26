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

class AnnotationOnly:
    __match_args__: tuple[str]

class AnnotatedBinding:
    __match_args__: tuple[Literal["x"]] = ("x",)

def describe(
    point: Point,
    missing: Missing,
    empty: Empty,
    annotation_only: AnnotationOnly,
    annotated_binding: AnnotatedBinding,
    integer: int,
    complex_number: complex,
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

    match complex_number:
        case complex(_):  # error: [invalid-match-pattern] "expected 0, got 1"
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

A direct, unconditional non-tuple literal is invalid whenever a positional subpattern is used.

```py
from typing_extensions import LiteralString

class ListMatchArgs:
    __match_args__ = ["x"]

class StringMatchArgs:
    __match_args__: LiteralString = "x"

class PlainStringMatchArgs:
    __match_args__ = "x"

class BytesMatchArgs:
    __match_args__ = b"x"

class NumberMatchArgs:
    __match_args__ = 1

class BooleanMatchArgs:
    __match_args__ = True

class NoneMatchArgs:
    __match_args__ = None

class EllipsisMatchArgs:
    __match_args__ = ...

class DictMatchArgs:
    __match_args__ = {}

class SetMatchArgs:
    __match_args__ = {1}

class FStringMatchArgs:
    __match_args__ = f"x"

def describe(
    list_value: ListMatchArgs,
    string_value: StringMatchArgs,
    plain_string_value: PlainStringMatchArgs,
    bytes_value: BytesMatchArgs,
    number_value: NumberMatchArgs,
    boolean_value: BooleanMatchArgs,
    none_value: NoneMatchArgs,
    ellipsis_value: EllipsisMatchArgs,
    dict_value: DictMatchArgs,
    set_value: SetMatchArgs,
    f_string_value: FStringMatchArgs,
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

    match bytes_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case BytesMatchArgs(_):
            pass

    match number_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case NumberMatchArgs(_):
            pass

    match boolean_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case BooleanMatchArgs(_):
            pass

    match none_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case NoneMatchArgs(_):
            pass

    match ellipsis_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case EllipsisMatchArgs(_):
            pass

    match dict_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case DictMatchArgs(_):
            pass

    match set_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case SetMatchArgs(_):
            pass

    match f_string_value:
        # error: [invalid-match-pattern] "must be an exact tuple"
        case FStringMatchArgs(_):
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

## Type-checking-only dataclass decorator

The decorator selected during type checking may be replaced by a runtime decorator that supplies
`__match_args__`.

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from dataclasses import dataclass
else:
    from runtime_dataclass import dataclass

@dataclass(match_args=False)
class Model:
    value: int

def describe(model: Model) -> None:
    match model:
        case Model(_):
            pass
```

`runtime_dataclass.py`:

```py
from typing import Any

def dataclass(**kwargs: Any) -> Any:
    def decorate(cls: Any) -> Any:
        cls.__match_args__ = ("value",)
        return cls

    return decorate
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

## Runtime-only rebindings

A runtime-only assignment can replace either the pattern class or its `__match_args__` value. The
diagnostic does not rely on the type-checking control-flow graph when either symbol has another
binding.

```py
from typing import TYPE_CHECKING

class Model: ...

class RuntimeModel:
    __match_args__ = ("value",)

if not TYPE_CHECKING:
    Model = RuntimeModel

class RuntimeMatchArgs:
    __match_args__ = ()

    if not TYPE_CHECKING:
        __match_args__ = ("value",)

def describe(model: Model, runtime_match_args: RuntimeMatchArgs) -> None:
    match model:
        case Model(_):
            pass
    match runtime_match_args:
        case RuntimeMatchArgs(_):
            pass
```

## Keyword-unpacked metaclass

An unpacked class keyword may select a metaclass that supplies `__match_args__`, so the class is not
treated as plain.

```py
class Meta(type):
    __match_args__ = ("value",)

class Model(**{"metaclass": Meta}): ...

def describe(model: Model) -> None:
    match model:
        case Model(_):
            pass
```

## Synthesized slot descriptors

A local `__slots__` declaration may synthesize a `__match_args__` descriptor. The diagnostic does
not interpret the possible runtime iterables accepted by `__slots__`.

```py
class TupleSlots:
    __slots__ = ("__match_args__",)

class ListSlots:
    __slots__ = ["__match_args__"]

class SetSlots:
    __slots__ = {"__match_args__"}

class DictSlots:
    __slots__ = {"__match_args__": "match arguments"}

def describe(subject: object) -> None:
    match subject:
        case TupleSlots(_):
            pass
        case ListSlots(_):
            pass
        case SetSlots(_):
            pass
        case DictSlots(_):
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

class Reassigned:
    __match_args__ = ("x",)
    __match_args__ = ("x", "y")

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
        case Reassigned(_, _, _):
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
