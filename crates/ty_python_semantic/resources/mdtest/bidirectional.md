# Bidirectional Type Inference

ty partially supports bidirectional type inference. This is a mechanism for inferring the type of an
expression "from the outside in". Normally, type inference proceeds "from the inside out". That is,
in order to infer the type of an expression, the types of all sub-expressions must first be
inferred. There is no reverse dependency. However, when performing complex type inference, such as
when generics are involved, the type of an outer expression can sometimes be useful in inferring
inner expressions. Bidirectional type inference is a mechanism that propagates such "expected types"
to the inference of inner expressions.

## Propagating target type annotation

```toml
[environment]
python-version = "3.12"
```

```py
def list1[T](x: T) -> list[T]:
    return [x]

l1 = list1(1)
reveal_type(l1)  # revealed: list[Literal[1]]
l2: list[int] = list1(1)
reveal_type(l2)  # revealed: list[int]

# `list[Literal[1]]` and `list[int]` are incompatible, since `list[T]` is invariant in `T`.
# error: [invalid-assignment] "Object of type `list[Literal[1]]` is not assignable to `list[int]`"
l2 = l1

intermediate = list1(1)
# TODO: the error will not occur if we can infer the type of `intermediate` to be `list[int]`
# error: [invalid-assignment] "Object of type `list[Literal[1]]` is not assignable to `list[int]`"
l3: list[int] = intermediate
# TODO: it would be nice if this were `list[int]`
reveal_type(intermediate)  # revealed: list[Literal[1]]
reveal_type(l3)  # revealed: list[int]
```

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

d1 = {"x": 1}
d2: TD = {"x": 1}
d3: dict[str, int] = {"x": 1}

reveal_type(d1)  # revealed: dict[@Todo(dict literal key type), @Todo(dict literal value type)]
reveal_type(d2)  # revealed: TD
# TODO: should be `dict[str, int]`
reveal_type(d3)  # revealed: dict[@Todo(dict literal key type), @Todo(dict literal value type)]
```

## Propagating return type annotation

```toml
[environment]
python-version = "3.12"
```

```py
def list1[T](x: T) -> list[T]:
    return [x]

def get_data() -> dict | None:
    return {}

def wrap_data() -> list[dict]:
    if not (res := get_data()):
        return list1({})
    reveal_type(list1(res))  # revealed: list[dict[Unknown, Unknown] & ~AlwaysFalsy]
    # `list[dict[Unknown, Unknown] & ~AlwaysFalsy]` and `list[dict[Unknown, Unknown]]` are incompatible,
    # but the return type check passes here because the inferred return type is widened
    # by bidirectional type inference.
    return list1(res)
```
