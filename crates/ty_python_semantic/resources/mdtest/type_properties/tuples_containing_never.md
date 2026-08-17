# Tuples containing `Never`

A heterogeneous `tuple[…]` type that contains `Never` as a type argument is equivalent to `Never`,
but retains its shape for display. One way to think about this is the following: in order to
construct a tuple, you need to have an object of every element type. But since there is no object of
type `Never`, you cannot construct the tuple. Such a tuple type is therefore uninhabited and
equivalent to `Never`.

In the language of algebraic data types, a tuple type is a product type and `Never` acts like the
zero element in multiplication, similar to how a Cartesian product with the empty set is the empty
set.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to
from typing_extensions import Never, NoReturn

static_assert(is_equivalent_to(Never, tuple[Never]))
static_assert(is_equivalent_to(Never, tuple[Never, int]))
static_assert(is_equivalent_to(Never, tuple[int, Never]))
static_assert(is_equivalent_to(Never, tuple[int, Never, str]))
static_assert(is_equivalent_to(Never, tuple[int, tuple[str, Never]]))
static_assert(is_equivalent_to(Never, tuple[tuple[str, Never], int]))

def one_element(x: tuple[Never]) -> None:
    reveal_type(x)  # revealed: tuple[Never]

def never_last(y: tuple[int, Never]) -> None:
    reveal_type(y)  # revealed: tuple[int, Never]

def never_first(z: tuple[Never, int]) -> None:
    reveal_type(z)  # revealed: tuple[Never, int]
```

A type alias cannot make an uninhabited tuple element inhabitable.

```py
from typing_extensions import TypeAlias

Bottom: TypeAlias = Never

static_assert(is_equivalent_to(Never, tuple[Bottom]))
static_assert(is_equivalent_to(Never, tuple[int, tuple[Bottom]]))
```

A subclass of an uninhabited tuple type is also uninhabited, including through indirect inheritance.

```py
class UninhabitedTupleSubclass(tuple[int, Never]): ...
class IndirectUninhabitedTupleSubclass(UninhabitedTupleSubclass): ...

static_assert(is_equivalent_to(UninhabitedTupleSubclass, Never))
static_assert(is_equivalent_to(IndirectUninhabitedTupleSubclass, Never))
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
