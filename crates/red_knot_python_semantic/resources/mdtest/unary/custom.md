# Custom unary operations

## Class instances

```py
class Yes:
    def __pos__(self) -> bool:
        return False

    def __neg__(self) -> str:
        return "negative"

    def __invert__(self) -> int:
        return 17

class Sub(Yes): ...
class No: ...

reveal_type(+Yes())  # revealed: bool
reveal_type(-Yes())  # revealed: str
reveal_type(~Yes())  # revealed: int

reveal_type(+Sub())  # revealed: bool
reveal_type(-Sub())  # revealed: str
reveal_type(~Sub())  # revealed: int

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `No`"
reveal_type(+No())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `No`"
reveal_type(-No())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `No`"
reveal_type(~No())  # revealed: Unknown
```

## Classes

Dunder methods defined in a class are available to instances of that class, but not to the class
itself. (For these operators to work on the class itself, they would have to be defined on the
class's type, i.e. `type`.)

```py
class Yes:
    def __pos__(self) -> bool:
        return False

    def __neg__(self) -> str:
        return "negative"

    def __invert__(self) -> int:
        return 17

class Sub(Yes): ...
class No: ...

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `Literal[Yes]`"
reveal_type(+Yes)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `Literal[Yes]`"
reveal_type(-Yes)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `Literal[Yes]`"
reveal_type(~Yes)  # revealed: Unknown

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `Literal[Sub]`"
reveal_type(+Sub)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `Literal[Sub]`"
reveal_type(-Sub)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `Literal[Sub]`"
reveal_type(~Sub)  # revealed: Unknown

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `Literal[No]`"
reveal_type(+No)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `Literal[No]`"
reveal_type(-No)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `Literal[No]`"
reveal_type(~No)  # revealed: Unknown
```

## Function literals

```py
def f():
    pass

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `Literal[f]`"
reveal_type(+f)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `Literal[f]`"
reveal_type(-f)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `Literal[f]`"
reveal_type(~f)  # revealed: Unknown
```

## Subclass

```py
class Yes:
    def __pos__(self) -> bool:
        return False

    def __neg__(self) -> str:
        return "negative"

    def __invert__(self) -> int:
        return 17

class Sub(Yes): ...
class No: ...

def yes() -> type[Yes]:
    return Yes

def sub() -> type[Sub]:
    return Sub

def no() -> type[No]:
    return No

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `type[Yes]`"
reveal_type(+yes())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `type[Yes]`"
reveal_type(-yes())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `type[Yes]`"
reveal_type(~yes())  # revealed: Unknown

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `type[Sub]`"
reveal_type(+sub())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `type[Sub]`"
reveal_type(-sub())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `type[Sub]`"
reveal_type(~sub())  # revealed: Unknown

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `type[No]`"
reveal_type(+no())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `type[No]`"
reveal_type(-no())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `type[No]`"
reveal_type(~no())  # revealed: Unknown
```

## Metaclass

```py
class Meta(type):
    def __pos__(self) -> bool:
        return False

    def __neg__(self) -> str:
        return "negative"

    def __invert__(self) -> int:
        return 17

class Yes(metaclass=Meta): ...
class Sub(Yes): ...
class No: ...

reveal_type(+Yes)  # revealed: bool
reveal_type(-Yes)  # revealed: str
reveal_type(~Yes)  # revealed: int

reveal_type(+Sub)  # revealed: bool
reveal_type(-Sub)  # revealed: str
reveal_type(~Sub)  # revealed: int

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `Literal[No]`"
reveal_type(+No)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `Literal[No]`"
reveal_type(-No)  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `Literal[No]`"
reveal_type(~No)  # revealed: Unknown
```
