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
 --> src/mdtest_snippet.py:1:4
  |
1 | x: int = "three"  # snapshot: invalid-assignment
  |    ---   ^^^^^^^ Incompatible value of type `Literal["three"]`
  |    |
  |    Declared type
  |
```

## Unannotated assignment

```py
x: int
x = "three"  # snapshot: invalid-assignment
```

Here, we could ideally point to the annotation as well, but for now, we just call out the declared
type in an annotation on the variable name:

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:2:1
  |
2 | x = "three"  # snapshot: invalid-assignment
  | -   ^^^^^^^ Incompatible value of type `Literal["three"]`
  | |
  | Declared type `int`
  |
```

## Named expression

```py
x: int

(x := "three")  # snapshot: invalid-assignment
```

Similar here, we could ideally point to the type annotation:

```snapshot
error[invalid-assignment]: Object of type `Literal["three"]` is not assignable to `int`
 --> src/mdtest_snippet.py:3:2
  |
3 | (x := "three")  # snapshot: invalid-assignment
  |  -    ^^^^^^^ Incompatible value of type `Literal["three"]`
  |  |
  |  Declared type `int`
  |
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
 --> src/mdtest_snippet.py:4:4
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
  |
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
 --> src/mdtest_snippet.py:4:1
  |
4 | x, y = ("a", "b")  # snapshot: invalid-assignment
  | -      ^^^^^^^^^^ Incompatible value of type `Literal["a"]`
  | |
  | Declared type `int`
  |


error[invalid-assignment]: Object of type `Literal[0]` is not assignable to `str`
 --> src/mdtest_snippet.py:6:4
  |
6 | x, y = (0, 0)  # snapshot: invalid-assignment
  |    -   ^^^^^^ Incompatible value of type `Literal[0]`
  |    |
  |    Declared type `str`
  |
```

## Shadowing of classes and functions

See [shadowing.md](./shadowing.md).
