# ParamSpec Equivalence

Test for <https://github.com/astral-sh/ty/issues/1995>

```toml
[environment]
python-version = "3.13"
```

## ParamSpec equivalence in assert_type

When a generic class has a `ParamSpec` type variable, `assert_type` should correctly validate the
ParamSpec portion of the type, not just ignore it.

```py
from typing import Callable, assert_type, reveal_type

class C[T, **P]:
    def __init__(self, fn: Callable[P, T]) -> None:
        self.fn = fn

def f0(x: int, /) -> int:
    return x

def f1(x: str, /) -> int:
    return 0

# correctly passes - both T and P match (positional-only int parameter)
assert_type((x := C(f0)), C[int, [int]])
reveal_type(x)  # revealed: C[int, (x: int, /)]

# correctly fails - T doesn't match (str vs int return type)
# error: [type-assertion-failure]
assert_type((y := C(f0)), C[str, [int]])
reveal_type(y)  # revealed: C[int, (x: int, /)]

# This should fail because P doesn't match ([str] vs [int])
# error: [type-assertion-failure]
assert_type((z := C(f0)), C[int, [str]])
reveal_type(z)  # revealed: C[int, (x: int, /)]

# This should also fail - P doesn't match the return type function
# error: [type-assertion-failure]
assert_type((w := C(f1)), C[int, [int]])
reveal_type(w)  # revealed: C[int, (x: str, /)]
```
