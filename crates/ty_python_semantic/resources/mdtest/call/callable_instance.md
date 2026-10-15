# Callable instance

## Dunder call

```py
class Multiplier:
    def __init__(self, factor: int):
        self.factor = factor

    def __call__(self, number: int) -> int:
        return number * self.factor

a = Multiplier(2)(3)
reveal_type(a)  # revealed: int

class Unit: ...

b = Unit()(3.0)  # error: "Object of type `Unit` is not callable"
reveal_type(b)  # revealed: Unknown
```

## Possibly missing `__call__` method

```py
def _(flag: bool):
    class PossiblyNotCallable:
        if flag:
            def __call__(self) -> int:
                return 1

    a = PossiblyNotCallable()
    result = a()  # error: "Object of type `PossiblyNotCallable` is not callable (possibly missing `__call__` method)"
    reveal_type(result)  # revealed: int
```

## Possibly unbound callable

```py
def _(flag: bool):
    if flag:
        class PossiblyUnbound:
            def __call__(self) -> int:
                return 1

    # error: [possibly-unresolved-reference]
    a = PossiblyUnbound()
    reveal_type(a())  # revealed: int
```

## Non-callable `__call__`

```py
class NonCallable:
    __call__ = 1

a = NonCallable()
# error: [call-non-callable] "Object of type `NonCallable` is not callable"
reveal_type(a())  # revealed: Unknown
```

## Possibly non-callable `__call__`

```py
def _(flag: bool):
    class NonCallable:
        if flag:
            __call__ = 1
        else:
            def __call__(self) -> int:
                return 1

    a = NonCallable()
    # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    reveal_type(a())  # revealed: Unknown | int
```

## Call binding errors

### Wrong argument type

```py
class C:
    def __call__(self, x: int) -> int:
        return 1

c = C()

# error: 15 [invalid-argument-type] "Argument to bound method `C.__call__` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(c("foo"))  # revealed: int
```

### Wrong argument type on `self`

```py
class C:
    # TODO this definition should also be an error; `C` must be assignable to type of `self`
    def __call__(self: int) -> int:
        return 1

c = C()

# error: 13 [invalid-argument-type] "Argument to bound method `C.__call__` is incorrect: Expected `int`, found `C`"
reveal_type(c())  # revealed: int
```

## Union over callables

### Possibly missing `__call__`

```py
def outer(cond1: bool):
    class Test:
        if cond1:
            def __call__(self): ...

    class Other:
        def __call__(self): ...

    def inner(cond2: bool):
        if cond2:
            a = Test()
        else:
            a = Other()

        # error: [call-non-callable] "Object of type `Test` is not callable (possibly missing `__call__` method)"
        a()
```

## Recursive TypedDict values

The `child` field refers to the types of all values in `Node`, including the callable in `leaf`:

```py
from typing import cast
from typing_extensions import TypedDict
from ty_extensions._internal import TypeOf

class Payload:
    def __call__(self, value: int) -> str:
        return str(value)

class Node(TypedDict, closed=True):
    leaf: Payload
    child: TypeOf[next(iter(cast("Node", {}).values()))]
```

Calling a value still requires an `int` argument and returns `str`:

```py
def check(node: Node):
    value = next(iter(node.values()))
    reveal_type(value(1))  # revealed: str
    value("wrong")  # error: [invalid-argument-type]
```
