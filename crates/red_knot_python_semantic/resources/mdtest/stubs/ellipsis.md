# Ellipsis

## Function and methods

The ellipsis literal `...` can be used as a placeholder default value for a function parameter, in a
stub file only, regardless of the type of the parameter.

```pyi
def f(x: int = ...) -> None:
    reveal_type(x)  # revealed: int

def f2(x: str = ...) -> None:
    reveal_type(x)  # revealed: str
```

## Class and module symbols

The ellipsis literal can be assigned to a class or module symbol, regardless of its declared type,
in a stub file only.

```pyi
y: bytes = ...
reveal_type(y)  # revealed: bytes
x = ...
reveal_type(x)  # revealed: Unknown

class Foo:
    y: int = ...

reveal_type(Foo.y)  # revealed: int
```

## Unpacking ellipsis literal in assignment

No diagnostic is emitted if an ellipsis literal is "unpacked" in a stub file as part of an
assignment statement:

```pyi
x, y = ...
reveal_type(x)  # revealed: Unknown
reveal_type(y)  # revealed: Unknown
```

## Unpacking ellipsis literal in for loops

Iterating over an ellipsis literal as part of a `for` loop in a stub is invalid, however, and
results in a diagnostic:

```pyi
# error: [not-iterable] "Object of type `ellipsis` is not iterable"
for a, b in ...:
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

## Ellipsis usage in non stub file

In a non-stub file, there's no special treatment of ellipsis literals. An ellipsis literal can only
be assigned if `EllipsisType` is actually assignable to the annotated type.

```py
# error: 7 [invalid-parameter-default] "Default value of type `ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = ...) -> None: ...

# error: 1 [invalid-assignment] "Object of type `ellipsis` is not assignable to `int`"
a: int = ...
b = ...
reveal_type(b)  # revealed: ellipsis
```

## Use of `Ellipsis` symbol

There is no special treatment of the builtin name `Ellipsis` in stubs, only of `...` literals.

```pyi
# error: 7 [invalid-parameter-default] "Default value of type `ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = Ellipsis) -> None: ...
```
