# Ellipsis

## Function and Methods

The ellipsis literal `...` can be used as a placeholder default value for a function parameter,
 in a stub file only, regardless of the type of the parameter.

```py path=test.pyi
def f(x: int = ...) -> None:
    reveal_type(x)  # revealed: int
def f2(x: str = ...) -> None:
    reveal_type(x)  # revealed: str
```

## Class and Module Level Attributes

The ellipsis literal can be assigned to a class or module attribute, regardless of its type, in a
stub file only.

```py path=test.pyi
y: float = ...

class Foo:
    y: int = ...
```

## Ellipsis Usage In Non Stub File

Ellipsis can only be used in assignment if it's actually assignable to the type it's being assigned
to.

```py
# error: [invalid-parameter-default] "Default value of type `EllipsisType | ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = ...) -> None: ...

# error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `int`"
a: int = ...
b = ...
reveal_type(b)  # revealed: EllipsisType | ellipsis
```

## Use of Ellipsis Symbol

When the ellipsis symbol is used as default value the assignment is checked.

```py path=test.pyi
# error: [invalid-parameter-default] "Default value of type `EllipsisType | ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = Ellipsis) -> None: ...
```
