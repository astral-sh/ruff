# Assignment with annotations

## Annotation only transparent to local inference

```py
x = 1
x: int
y = x

reveal_type(y)  # revealed: Literal[1]
```

## Violates own annotation

```py
x: int = "foo"  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
```

## Violates previous annotation

```py
x: int
x = "foo"  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
```

## PEP-604 annotations are supported

```py
def f() -> str | int | None:
    return None

reveal_type(f())  # revealed: str | int | None
```
