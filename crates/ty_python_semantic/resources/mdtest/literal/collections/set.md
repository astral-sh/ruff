# Sets

## Basic set

```py
reveal_type({1, 2})  # revealed: set[int]
```

## Set of tuples

```py
reveal_type({(1, 2), (3, 4)})  # revealed: set[tuple[int, int]]
```

## Set of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = {a, b}
reveal_type(x)  # revealed: set[(_: int) -> int]
```

## Mixed set

```py
# revealed: set[int | tuple[int, ...]]
reveal_type({1, (1, 2), (1, 2, 3)})
```

## Set comprehensions

```py
reveal_type({x for x in range(42)})  # revealed: set[int]
```

## Custom set with an additional type parameter

The literal supplies the element type. The additional parameter falls back to `Unknown`.

```toml
[environment]
python-version = "3.12"
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class type: ...
class tuple: ...
class int: ...
class set[T, U]: ...
```

```py
def f(value: int):
    wrong: int = {value}  # error: [invalid-assignment] "Object of type `set[int, Unknown]` is not assignable to `int`"
```
