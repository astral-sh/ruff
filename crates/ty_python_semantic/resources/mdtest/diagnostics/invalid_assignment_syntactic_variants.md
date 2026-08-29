# Invalid assignment diagnostics

These tests make sure that we point to the right part of the code when emitting an invalid
assignment diagnostic in various syntactical positions.

## Annotated assignment

```py
x: int = "three"  # snapshot: invalid-assignment
```

Here, we point to the type annotation directly:

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:1:10
  |
1 | x: int = "three"  # snapshot: invalid-assignment
  |    ---   ^^^^^^^ Incompatible value of type `Literal["three"]`
  |    |
  |    Declared type
```

## Unannotated assignment

```py
x: int
x = "three"  # snapshot: invalid-assignment
```

The diagnostic points to the earlier type annotation as well as the incompatible value:

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:2:5
  |
1 | x: int
  |    --- Declared type
2 | x = "three"  # snapshot: invalid-assignment
  |     ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Previously initialized declaration

The original annotation remains the source of the declared type after the variable has been
initialized.

```py
x: int = 1
x = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:2:5
  |
1 | x: int = 1
  |    --- Declared type
2 | x = "three"  # snapshot: invalid-assignment
  |     ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Global declaration

An assignment to a global variable points to the annotation in its defining scope.

```py
x: int

def assign() -> None:
    global x
    x = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:5:9
  |
1 | x: int
  |    --- Declared type
2 |
3 | def assign() -> None:
4 |     global x
5 |     x = "three"  # snapshot: invalid-assignment
  |         ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Annotated parameter

An incompatible assignment to an annotated parameter points to the parameter's type annotation.

```py
def assign(value: int) -> None:
    value = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:2:13
  |
1 | def assign(value: int) -> None:
  |                   --- Declared type
2 |     value = "three"  # snapshot: invalid-assignment
  |             ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Variadic positional parameter

A variadic positional parameter's annotation describes its arguments, while the parameter itself is
a tuple.

```py
def assign(*values: int) -> None:
    values = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `tuple[int, ...]`
 --> src/mdtest_snippet.py:2:14
  |
1 | def assign(*values: int) -> None:
  |                     --- Variadic parameter annotation declares the type as `tuple[int, ...]`
2 |     values = "three"  # snapshot: invalid-assignment
  |              ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Variadic keyword parameter

A variadic keyword parameter's annotation describes its values, while the parameter itself is a
dictionary.

```py
def assign(**values: int) -> None:
    values = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `dict[str, int]`
 --> src/mdtest_snippet.py:2:14
  |
1 | def assign(**values: int) -> None:
  |                      --- Keyword-variadic parameter annotation declares the type as `dict[str, int]`
2 |     values = "three"  # snapshot: invalid-assignment
  |              ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Nonlocal declaration

An assignment to a nonlocal variable points to the annotation in its enclosing scope.

```py
def outer() -> None:
    x: int = 1

    def assign() -> None:
        nonlocal x
        x = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:6:13
  |
2 |     x: int = 1
  |        --- Declared type
3 |
4 |     def assign() -> None:
5 |         nonlocal x
6 |         x = "three"  # snapshot: invalid-assignment
  |             ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## Conflicting declarations

When conflicting annotations contribute to the declared type, the diagnostic does not identify any
one annotation as the declared type.

```py
def assign(flag: bool) -> None:
    if flag:
        x: int
    else:
        x: str

    # error: [conflicting-declarations]
    x = b"three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal[b"three"]` is not assignable to `int | str`
 --> src/mdtest_snippet.py:8:9
  |
8 |     x = b"three"  # snapshot: invalid-assignment
  |     -   ^^^^^^^^ Incompatible value of type `Literal[b"three"]`
  |     |
  |     Declared type `int | str`
```

## Equivalent declarations

When distinct branches declare the same type, neither annotation is the unique source of the
declared type.

```py
def assign(flag: bool) -> None:
    if flag:
        x: int
    else:
        x: int

    x = "three"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:7:9
  |
7 |     x = "three"  # snapshot: invalid-assignment
  |     -   ^^^^^^^ Incompatible value of type `Literal["three"]`
  |     |
  |     Declared type `int`
```

## Named expression

```py
x: int

(x := "three")  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:3:7
  |
1 | x: int
  |    --- Declared type
2 |
3 | (x := "three")  # snapshot: invalid-assignment
  |       ^^^^^^^ Incompatible value of type `Literal["three"]`
```

## For-loop target

```py
value: int

for value in ["three"]:  # snapshot: invalid-assignment
    pass
```

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:3:5
  |
1 | value: int
  |        --- Declared type
2 |
3 | for value in ["three"]:  # snapshot: invalid-assignment
  |     ^^^^^
```

## Context manager target

```py
from contextlib import nullcontext

value: int

with nullcontext("three") as value:  # snapshot: invalid-assignment
    pass
```

```snapshot
error[invalid-assignment]: Object of type `str` is not assignable to `int`
 --> src/mdtest_snippet.py:5:30
  |
3 | value: int
  |        --- Declared type
4 |
5 | with nullcontext("three") as value:  # snapshot: invalid-assignment
  |                              ^^^^^
```

## Augmented assignment

```py
value: int = 1
value += 1.0  # snapshot: invalid-assignment

reveal_type(value)  # revealed: int
```

```snapshot
error[invalid-assignment]: Object of type `float` is not assignable to `int`
 --> src/mdtest_snippet.py:2:1
  |
1 | value: int = 1
  |        --- Declared type
2 | value += 1.0  # snapshot: invalid-assignment
  | ^^^^^ Augmented assignment produces a value of type `float`
```

The concise diagnostic reports the incompatible assignment:

```py
# error: [invalid-assignment] "Object of type `float` is not assignable to `int`"
value += 1.0
```

## Multiline expressions

```py
# fmt: off

# snapshot: invalid-assignment
x: str = (
    1 + 2 + (
        3 + 4 + 5
    )
)
```

```snapshot
error[invalid-assignment]: Object of type `Literal[15]` is not assignable to `str`
 --> src/mdtest_snippet.py:4:10
  |
4 |   x: str = (
  |  ____---___^
  | |    |
  | |    Declared type
5 | |     1 + 2 + (
6 | |         3 + 4 + 5
7 | |     )
8 | | )
  | |_^ Incompatible value of type `Literal[15]`
```

## Multiple targets

```py
x: int
y: str

x, y = ("a", "b")  # snapshot: invalid-assignment

x, y = (0, 0)  # snapshot: invalid-assignment
```

TODO: the right hand side annotation should ideally only point to the `"a"` part of the `("a", "b")`
tuple:

```snapshot
error[invalid-assignment]: Object of type `Literal["a"]` is not assignable to `int`
 --> src/mdtest_snippet.py:4:8
  |
1 | x: int
  |    --- Declared type
2 | y: str
3 |
4 | x, y = ("a", "b")  # snapshot: invalid-assignment
  |        ^^^^^^^^^^ Incompatible value of type `Literal["a"]`


error[invalid-assignment]: Object of type `Literal[0]` is not assignable to `str`
 --> src/mdtest_snippet.py:6:8
  |
2 | y: str
  |    --- Declared type
3 |
4 | x, y = ("a", "b")  # snapshot: invalid-assignment
5 |
6 | x, y = (0, 0)  # snapshot: invalid-assignment
  |        ^^^^^^ Incompatible value of type `Literal[0]`
```

## Shadowing of classes and functions

See [shadowing.md](./shadowing.md).
