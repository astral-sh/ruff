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
y: bytes = ...
reveal_type(y)  # revealed: bytes
x = ...
reveal_type(x)  # revealed: Unknown

class Foo:
    y: int = ...

reveal_type(Foo.y)  # revealed: int
```

## Ellipsis Usage In Non Stub File

In a non-stub file, there's no special treatment of ellipsis literals. An ellipsis literal can only be assigned if
`EllipsisType` is actually assignable to the annotated type.

```py
# error: [invalid-parameter-default] "Default value of type `EllipsisType | ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = ...) -> None: ...

# error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `int`"
a: int = ...
b = ...
reveal_type(b)  # revealed: EllipsisType | ellipsis
```

## Use of Ellipsis Symbol

There is no special treatment of the builtin name `Ellipsis`, only of `...` literals.

```py path=test.pyi
# error: [invalid-parameter-default] "Default value of type `EllipsisType | ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = Ellipsis) -> None: ...
```
