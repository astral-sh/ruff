# Public type of undeclared symbols

## Summary

A strict application of the [gradual guarantee] would suggest that all assignments to an unannotated
attribute should be allowed; this could be implemented by unioning all such attributes' inferred
types with `Unknown`. However, in practice this requires too many annotations to achieve sound
typing, and we can heuristically pick the "right" type for unannotated attributes most of the time.

## Promotion

We promote the inferred type of an unannotated attribute to our best guess of its intended public
type. For example, we promote literal types to their nominal supertype, because it is unlikely the
author intended the `value` attribute to always hold the literal `0`:

```py
class Counter:
    def __init__(self) -> None:
        self.value = 0

reveal_type(Counter().value)  # revealed: int
```

Class literals stored in inferred attributes are widened when accessed through an instance. A
subclass can override an undeclared class attribute, so a method that accesses the attribute through
`self` cannot assume that it still holds the original class object. Direct access on a specific
class object remains exact:

```py
from typing import Final, NewType, TypeVar, final
from typing_extensions import assert_never

class Response: ...
class HtmlResponse(Response): ...

class TestResponse:
    response_class = Response
    response_classes = (Response,)

    def check(self) -> None:
        reveal_type(self.response_class)  # revealed: type[Response]
        reveal_type(self.response_classes)  # revealed: tuple[type[Response]]
        reveal_type(self.response_class == Response)  # revealed: bool

        if self.response_class == Response:
            true_branch: int = "not an int"  # error: [invalid-assignment]
        else:
            false_branch: int = "not an int"  # error: [invalid-assignment]

class TestHtmlResponse(TestResponse):
    response_class = HtmlResponse

reveal_type(TestResponse.response_class)  # revealed: <class 'Response'>
reveal_type(TestResponse.response_classes)  # revealed: tuple[<class 'Response'>]

def check_type(response: type[TestResponse]) -> None:
    reveal_type(response.response_class)  # revealed: type[Response]

T = TypeVar("T", bound=TestResponse)

def check_typevar(response: T) -> None:
    reveal_type(response.response_class)  # revealed: type[Response]

TClass = TypeVar("TClass", bound=type[TestResponse])

def check_typevar_class(response: TClass) -> None:
    reveal_type(response.response_class)  # revealed: type[Response]

@final
class FinalTestResponse:
    response_class = Response

    @classmethod
    def check(cls) -> None:
        if cls.response_class is not Response:
            assert_never(cls.response_class)

TFinal = TypeVar("TFinal", bound=FinalTestResponse)

def check_final_typevar(response: type[TFinal]) -> None:
    reveal_type(response.response_class)  # revealed: <class 'Response'>

NewTestResponse = NewType("NewTestResponse", TestResponse)

def check_newtype(response: NewTestResponse) -> None:
    reveal_type(response.response_class)  # revealed: type[Response]

class ResponseMeta(type):
    response_class = Response

NewResponseMeta = NewType("NewResponseMeta", ResponseMeta)

def check_newtype_class(response: NewResponseMeta) -> None:
    reveal_type(response.response_class)  # revealed: type[Response]

class AnnotatedResponse:
    response_class: type[Response] = Response

    def check(self) -> None:
        reveal_type(self.response_class)  # revealed: type[Response]
        reveal_type(self.response_class == Response)  # revealed: bool

class FixedResponse:
    response_class: Final = Response

    def check(self) -> None:
        reveal_type(self.response_class)  # revealed: <class 'Response'>
        reveal_type(self.response_class == Response)  # revealed: Literal[True]
```

The same widening applies to undeclared instance attributes assigned in methods:

```py
class InstanceResponse: ...

class Wrapper:
    def __init__(self) -> None:
        self.response_class = InstanceResponse

reveal_type(Wrapper().response_class)  # revealed: type[InstanceResponse]
```

Widening distributes over unions of class literals:

```py
class UnionA: ...
class UnionB: ...

def get_flag() -> bool:
    return bool()

class EitherClass:
    value = UnionA if get_flag() else UnionB

reveal_type(EitherClass().value)  # revealed: type[UnionA | UnionB]
```

Module-level variables keep their narrow inferred type. In particular, class literals in an
invariant collection remain precise enough for exhaustive equality checks:

```py
class OffsetA: ...
class OffsetB: ...

classes = {"a": OffsetA, "b": OffsetB}

def choose(name: str) -> None:
    class_value = classes[name]
    if class_value == OffsetA:
        expected = 1
    elif class_value == OffsetB:
        expected = 2
    reveal_type(expected)  # revealed: Literal[1, 2]
```

## Widening of non-literal singleton types

It's similarly unlikely that an unannotated attribute initialized to a singleton type (like `None`)
is intended to always and only hold the value `None`. But unlike literal types, `None` doesn't have
an obvious candidate super-type to widen to. In this case, we do widen by unioning with `Unknown`:

```py
class Wrapper:
    value = None

wrapper = Wrapper()

reveal_type(wrapper.value)  # revealed: None | Unknown

wrapper.value = 1
```

In this example, the public type is `None | Unknown`, so we also catch uses that are incompatible
with `None`:

```py
def accepts_int(i: int) -> None:
    pass

def f(w: Wrapper) -> None:
    # This is fine
    v: int | None = w.value

    # This function call is incorrect, because `w.value` could be `None`. We therefore emit the following
    # error: "Argument to function `accepts_int` is incorrect: Expected `int`, found `None | Unknown`"
    c = accepts_int(w.value)
```

The same widening also applies to undeclared instance attributes that are only assigned inside
`__init__`:

```py
class InstanceWrapper:
    def __init__(self) -> None:
        self.value = None

reveal_type(InstanceWrapper().value)  # revealed: None | Unknown
```

## Declaring a wider type

Users can always opt in to a wider public type by adding annotations. For the `Wrapper` class, this
could be:

```py
class Wrapper:
    value: int | None = None

w = Wrapper()

# The following public type is now
# revealed: int | None
reveal_type(w.value)

# Incompatible assignments are now caught:
# error: "Object of type `Literal["a"]` is not assignable to attribute `value` of type `int | None`"
w.value = "a"
```

## Declaring a narrower type to avoid promotion

It's also possible to declare a narrower type to avoid promotion. For example, if we know that an
attribute will always hold one of two literal values, we may want to avoid promotion of the literal:

```py
from typing import Literal

class Constant:
    value: Literal[0, 1] = 0

# We would have promoted this to `int` without the explicit annotation:
reveal_type(Constant().value)  # revealed: Literal[0, 1]
```

This also works to avoid widening of singleton types, if for some reason you want an attribute that
can only ever hold that one singleton value:

```py
class NoneWrapper:
    value: None = None

reveal_type(NoneWrapper().value)  # revealed: None
```

## What is meant by 'public' type?

We apply different semantics depending on whether a symbol is accessed from the same scope in which
it was originally defined, or whether it is accessed from an external scope. External scopes will
see the symbol's "public type", which has been discussed above. But within the same scope the symbol
was defined in, we can often use a narrower literal type before promotion. For example:

```py
class Wrapper:
    value = 10

    # Type as seen from the same scope:
    reveal_type(value)  # revealed: Literal[10]

# Type as seen from another scope:
reveal_type(Wrapper.value)  # revealed: int
```

[gradual guarantee]: https://typing.python.org/en/latest/spec/concepts.html#the-gradual-guarantee
