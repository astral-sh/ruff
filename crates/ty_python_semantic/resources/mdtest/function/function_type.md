# Generic function types

## Assignability

`types.FunctionType` describes Python functions with a particular parameter list and return type.

```py
from types import FunctionType

def stringify(value: int) -> str:
    return str(value)

matching: FunctionType[[int], str] = stringify  # no diagnostic
wrong_parameter: FunctionType[[str], str] = stringify  # error: [invalid-assignment]
wrong_return: FunctionType[[int], int] = stringify  # error: [invalid-assignment]
wrong_arity: FunctionType[[], str] = stringify  # error: [invalid-assignment]
```

A callable instance is not a Python function, even if its call signature matches.

```py
class Stringifier:
    def __call__(self, value: int) -> str:
        return str(value)

not_a_function: FunctionType[[int], str] = Stringifier()  # error: [invalid-assignment]
```

## Calls and attributes

A specialized function type supports calls with its declared signature, as well as attributes of
Python functions.

```py
from collections.abc import Callable
from types import FunctionType

def check(function: FunctionType[[int], str]) -> None:
    reveal_type(function(1))  # revealed: str
    reveal_type(function.__name__)  # revealed: str
    function("one")  # error: [invalid-argument-type]
    function()  # error: [missing-argument]
    function(1, 2)  # error: [too-many-positional-arguments]
    callback: Callable[[int], str] = function  # no diagnostic
```

## Inferred collection elements

Inferring the element type of a list preserves the functions' signatures and attributes.

```py
def stringify(value: int, /, label: str = "value", *, uppercase: bool = False) -> str:
    return str(value)

functions = [stringify]
# revealed: list[FunctionType[(value: int, /, label: str = "value", *, uppercase: bool = False), str]]
reveal_type(functions)
reveal_type(functions[0](1))  # revealed: str
reveal_type(functions[0](1, label="item", uppercase=True))  # revealed: str
reveal_type(functions[0].__name__)  # revealed: str
functions[0]("one")  # error: [invalid-argument-type]
functions[0]()  # error: [missing-argument]
functions[0](1, "item", True)  # error: [too-many-positional-arguments]
functions[0](1, value=2)  # error: [positional-only-parameter-as-kwarg]
```

## Method binding

A `FunctionType` stored on a class binds the instance as the first argument. Access through the
class preserves the unbound signature.

```py
from types import FunctionType
from typing import ClassVar

class C:
    method: ClassVar[FunctionType[["C", int], str]]

def check(function: FunctionType[[C, int], str]) -> None:
    C.method = function  # no diagnostic
    reveal_type(C().method(1))  # revealed: str
    reveal_type(C.method(C(), 1))  # revealed: str
    C().method("one")  # error: [invalid-argument-type]
    C().method()  # error: [missing-argument]
    C.method(C(), "one")  # error: [invalid-argument-type]
```

## Bound method types

`MethodType` describes the signature after binding the instance.

```py
from types import MethodType

class C:
    def method(self, value: int) -> str:
        return str(value)

matching: MethodType[[int], str] = C().method  # no diagnostic
wrong_parameter: MethodType[[str], str] = C().method  # error: [invalid-assignment]
wrong_return: MethodType[[int], int] = C().method  # error: [invalid-assignment]
wrong_arity: MethodType[[], str] = C().method  # error: [invalid-assignment]
```

Constructing a method explicitly also removes the first parameter and preserves the remaining
signature.

```py
method = MethodType(C.method, C())
reveal_type(method(1))  # revealed: str
reveal_type(method(value=1))  # revealed: str
method("one")  # error: [invalid-argument-type]
method()  # error: [missing-argument]
```

## Variance

Function parameters are contravariant, and return types are covariant.

```py
from types import FunctionType

def parameters(broad: FunctionType[[object], str], narrow: FunctionType[[int], str]) -> None:
    accepts_int: FunctionType[[int], str] = broad  # no diagnostic
    accepts_object: FunctionType[[object], str] = narrow  # error: [invalid-assignment]

def returns(broad: FunctionType[[int], object], narrow: FunctionType[[int], str]) -> None:
    returns_object: FunctionType[[int], object] = narrow  # no diagnostic
    returns_str: FunctionType[[int], str] = broad  # error: [invalid-assignment]
```

## Overloaded functions

An overloaded function is compatible with each signature that its overloads support.

```py
from types import FunctionType
from typing import overload

@overload
def identity(value: int) -> int: ...
@overload
def identity(value: str) -> str: ...
def identity(value: int | str) -> int | str:
    return value

integer: FunctionType[[int], int] = identity  # no diagnostic
string: FunctionType[[str], str] = identity  # no diagnostic
wrong_parameter: FunctionType[[float], int] = identity  # error: [invalid-assignment]
wrong_return: FunctionType[[int], str] = identity  # error: [invalid-assignment]

# Collection inference preserves the correlation between each parameter and return type.
functions = [identity]
reveal_type(functions)  # revealed: list[Overload[(value: int) -> int, (value: str) -> str]]

def check(value: int, label: str) -> None:
    reveal_type(functions[0](value))  # revealed: int
    reveal_type(functions[0](label))  # revealed: str
```
