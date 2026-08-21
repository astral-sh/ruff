# Tuples containing `Never`

A heterogeneous `tuple[…]` type that contains `Never` remains distinct from `Never`. Although it has
no runtime inhabitants, it retains its tuple length and element types. This prevents it from
becoming assignable to unrelated types such as `str`.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to
from typing_extensions import Never, NoReturn

static_assert(not is_equivalent_to(Never, tuple[Never]))
static_assert(not is_equivalent_to(Never, tuple[Never, int]))
static_assert(not is_equivalent_to(Never, tuple[int, Never]))
static_assert(not is_equivalent_to(Never, tuple[int, Never, str]))
static_assert(not is_equivalent_to(Never, tuple[int, tuple[str, Never]]))
static_assert(not is_equivalent_to(Never, tuple[tuple[str, Never], int]))

def _(x: tuple[Never], y: tuple[int, Never], z: tuple[Never, int]):
    reveal_type(x)  # revealed: tuple[Never]
    reveal_type(y)  # revealed: tuple[int, Never]
    reveal_type(z)  # revealed: tuple[Never, int]
```

The empty `tuple` is *not* equivalent to `Never`!

```py
static_assert(not is_equivalent_to(Never, tuple[()]))
```

`NoReturn` is just a different spelling of `Never`, so these tuple types also retain their shape:

```py
static_assert(not is_equivalent_to(NoReturn, tuple[NoReturn]))
static_assert(not is_equivalent_to(NoReturn, tuple[NoReturn, int]))
static_assert(not is_equivalent_to(NoReturn, tuple[int, NoReturn]))
static_assert(not is_equivalent_to(NoReturn, tuple[int, NoReturn, str]))
static_assert(not is_equivalent_to(NoReturn, tuple[int, tuple[str, NoReturn]]))
static_assert(not is_equivalent_to(NoReturn, tuple[tuple[str, NoReturn], int]))
```
