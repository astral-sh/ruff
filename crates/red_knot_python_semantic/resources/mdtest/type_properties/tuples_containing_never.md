# Tuples containing `Never`

A heterogeneous `tuple[…]` type that contains `Never` as a type argument simplifies to `Never`. One
way to think about this is the following: in order to construct a tuple, you need to have an object
of every element type. But since there is no object of type `Never`, you cannot construct the tuple.
Such a tuple type is therefore uninhabited and equivalent to `Never`.

In the language of algebraic data types, a tuple type is a product type and `Never` acts like the
zero element in multiplication, similar to how a Cartesian product with the empty set is the empty
set.

```py
from knot_extensions import static_assert, is_equivalent_to
from typing_extensions import Never, NoReturn

static_assert(is_equivalent_to(Never, tuple[Never]))
static_assert(is_equivalent_to(Never, tuple[Never, int]))
static_assert(is_equivalent_to(Never, tuple[int, Never]))
static_assert(is_equivalent_to(Never, tuple[int, Never, str]))
static_assert(is_equivalent_to(Never, tuple[int, tuple[str, Never]]))
static_assert(is_equivalent_to(Never, tuple[tuple[str, Never], int]))

def _(x: tuple[Never], y: tuple[int, Never], z: tuple[Never, int]):
    reveal_type(x)  # revealed: Never
    reveal_type(y)  # revealed: Never
    reveal_type(z)  # revealed: Never
```

The empty `tuple` is *not* equivalent to `Never`!

```py
static_assert(not is_equivalent_to(Never, tuple[()]))
```

`NoReturn` is just a different spelling of `Never`, so the same is true for `NoReturn`:

```py
static_assert(is_equivalent_to(NoReturn, tuple[NoReturn]))
static_assert(is_equivalent_to(NoReturn, tuple[NoReturn, int]))
static_assert(is_equivalent_to(NoReturn, tuple[int, NoReturn]))
static_assert(is_equivalent_to(NoReturn, tuple[int, NoReturn, str]))
static_assert(is_equivalent_to(NoReturn, tuple[int, tuple[str, NoReturn]]))
static_assert(is_equivalent_to(NoReturn, tuple[tuple[str, NoReturn], int]))
```
