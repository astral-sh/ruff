# Invalid match pattern

## Too many positional subpatterns

The diagnostic is emitted when the positional limit comes from a direct, unconditional, unannotated
tuple-literal assignment in the matched class body.

```py
class Point:
    __match_args__ = ("x", "y")

class Empty:
    __match_args__ = ()

def describe(point: Point, empty: Empty) -> None:
    match point:
        case Point(_, _):
            pass

    match point:
        # error: [invalid-match-pattern] "Too many positional subpatterns for `<class 'Point'>`: expected 2, got 3"
        case Point(_, _, _):
            pass

    match empty:
        case Empty(_):  # error: [invalid-match-pattern] "expected 0, got 1"
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
declarations. Inherited, metaclass, or synthesized values, declarations, conditional bindings,
unions, variadic tuples, tuple subclasses, malformed or absent `__match_args__` values, and
assignments whose value is not a tuple literal are deliberately left undiagnosed.

```toml
[environment]
python-version = "3.11"
```

```py
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

flag: bool = bool()

class Missing: ...

class Base:
    __match_args__ = ("x",)

class Inherited(Base): ...

class Meta(type):
    __match_args__ = ("x",)

class FromMeta(metaclass=Meta): ...

@dataclass
class Synthesized:
    x: int

class AnnotationOnly:
    __match_args__: tuple[Literal["x"]]

class AnnotatedBinding:
    __match_args__: tuple[Literal["x"]] = ("x",)

class AssignedName:
    args: tuple[Literal["x"]] = ("x",)
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

class TupleSubclass(tuple[str, ...]): ...

class SubclassValue:
    __match_args__ = TupleSubclass(("x",))

class VariadicPrefix:
    __match_args__: tuple[Literal[1], *tuple[str, ...]]

class UnionValue:
    __match_args__: tuple[Literal["x"]] | tuple[Literal["x"], Literal["y"]]

class UnionNames:
    __match_args__: tuple[Literal["x", "y"], Literal[1]]

@dataclass
class ConditionalDataclass:
    x: int
    if flag:
        __match_args__ = ()

def describe(
    missing: Missing,
    inherited: Inherited,
    from_meta: FromMeta,
    synthesized: Synthesized,
    annotation_only: AnnotationOnly,
    annotated_binding: AnnotatedBinding,
    assigned_name: AssignedName,
    conditional: Conditional,
    type_checking_only: TypeCheckingOnly,
    direct_type_checking_only: DirectTypeCheckingOnly,
    runtime_only: RuntimeOnly,
    invalid_type: InvalidType,
    subclass_value: SubclassValue,
    variadic_prefix: VariadicPrefix,
    union_value: UnionValue,
    union_names: UnionNames,
    conditional_dataclass: ConditionalDataclass,
) -> None:
    match missing:
        case Missing(_):
            pass
    match inherited:
        case Inherited(_, _):
            pass
    match from_meta:
        case FromMeta(_, _):
            pass
    match synthesized:
        case Synthesized(_, _):
            pass
    match annotation_only:
        case AnnotationOnly(_, _):
            pass
    match annotated_binding:
        case AnnotatedBinding(_, _):
            pass
    match assigned_name:
        case AssignedName(_, _):
            pass
    match conditional:
        case Conditional(_, _):
            pass
    match type_checking_only:
        case TypeCheckingOnly(_, _):
            pass
    match direct_type_checking_only:
        case DirectTypeCheckingOnly(_, _):
            pass
    match runtime_only:
        case RuntimeOnly(_, _):
            pass
    match invalid_type:
        case InvalidType(_):
            pass
    match subclass_value:
        case SubclassValue(_, _):
            pass
    match variadic_prefix:
        case VariadicPrefix(_):
            pass
    match union_value:
        case UnionValue(_, _, _):
            pass
    match union_names:
        case UnionNames(_, _):
            pass
    match conditional_dataclass:
        case ConditionalDataclass(_):
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
