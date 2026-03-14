# Invalid assignment diagnostics

<!-- snapshot-diagnostics -->

## Annotated assignment

```py
x: int = "three"  # error: [invalid-assignment]
```

## Unannotated assignment

```py
x: int
x = "three"  # error: [invalid-assignment]
```

## Named expression

```py
x: int

(x := "three")  # error: [invalid-assignment]
```

## Multiline expressions

```py
# fmt: off

# error: [invalid-assignment]
x: str = (
    1 + 2 + (
        3 + 4 + 5
    )
)
```

## Multiple targets

```py
x: int
y: str

x, y = ("a", "b")  # error: [invalid-assignment]

x, y = (0, 0)  # error: [invalid-assignment]
```

## Shadowing of classes and functions

See [shadowing.md](./shadowing.md).
