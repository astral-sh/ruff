# Narrowing for `in` conditionals

## `in` for tuples

### `in` for tuples of `int`

```py
def _(x: int):
    if x in (1, 2, 3):
        reveal_type(x)  # revealed: Literal[1, 2, 3]
    else:
        # TODO should be `int & ~Literal[1, 2, 3]`
        reveal_type(x)  # revealed: int
```

### `in` for tuples of `object`

```py
class A: ...
class B: ...

def _(x: object):
    if x in (A(), B()):
        reveal_type(x)  # revealed: A | B
    else:
        # TODO should be `object & ~(A | B)`
        reveal_type(x)  # revealed: object
```

### `in` for tuples of `str`

```py
def _(x: str):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        # TODO should be `str & ~Literal["a", "b", "c"]`
        reveal_type(x)  # revealed: str
```
