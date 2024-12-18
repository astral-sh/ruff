# Callable instance

## Dunder call

```py
class Multiplier:
    def __init__(self, factor: float):
        self.factor = factor

    def __call__(self, number: float) -> float:
        return number * self.factor

a = Multiplier(2.0)(3.0)
reveal_type(a)  # revealed: float

class Unit: ...

b = Unit()(3.0)  # error: "Object of type `Unit` is not callable"
reveal_type(b)  # revealed: Unknown
```

## Possibly unbound `__call__` method

```py
def _(flag: bool):
    class PossiblyNotCallable:
        if flag:
            def __call__(self) -> int: ...

    a = PossiblyNotCallable()
    result = a()  # error: "Object of type `PossiblyNotCallable` is not callable (possibly unbound `__call__` method)"
    reveal_type(result)  # revealed: int
```

## Possibly unbound callable

```py
def _(flag: bool):
    if flag:
        class PossiblyUnbound:
            def __call__(self) -> int: ...

    # error: [possibly-unresolved-reference]
    a = PossiblyUnbound()
    reveal_type(a())  # revealed: int
```

## Non-callable `__call__`

```py
class NonCallable:
    __call__ = 1

a = NonCallable()
# error: "Object of type `NonCallable` is not callable"
reveal_type(a())  # revealed: Unknown
```

## Possibly non-callable `__call__`

```py
def _(flag: bool):
    class NonCallable:
        if flag:
            __call__ = 1
        else:
            def __call__(self) -> int: ...

    a = NonCallable()
    # error: "Object of type `Literal[__call__] | Literal[1]` is not callable (due to union element `Literal[1]`)"
    reveal_type(a())  # revealed: int | Unknown
```
