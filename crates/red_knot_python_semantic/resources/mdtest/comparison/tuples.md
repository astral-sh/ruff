# Comparison: Tuples

## Heterogeneous

For tuples like `tuple[int, str, Literal[1]]`

### Value Comparisons

"Value Comparisons" refers to the operators: `==`, `!=`, `<`, `<=`, `>`, `>=`

#### Results without Ambiguity

Cases where the result can be definitively inferred as a `BooleanLiteral`.

```py
a = (1, "test", (3, 13), True)
b = (1, "test", (3, 14), False)

reveal_type(a == a)  # revealed: Literal[True]
reveal_type(a != a)  # revealed: Literal[False]
reveal_type(a < a)  # revealed: Literal[False]
reveal_type(a <= a)  # revealed: Literal[True]
reveal_type(a > a)  # revealed: Literal[False]
reveal_type(a >= a)  # revealed: Literal[True]

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]
```

Even when tuples have different lengths, comparisons should be handled appropriately.

```py
a = (1, 2, 3)
b = (1, 2, 3, 4)

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]

c = ("a", "b", "c", "d")
d = ("a", "b", "c")

reveal_type(c == d)  # revealed: Literal[False]
reveal_type(c != d)  # revealed: Literal[True]
reveal_type(c < d)  # revealed: Literal[False]
reveal_type(c <= d)  # revealed: Literal[False]
reveal_type(c > d)  # revealed: Literal[True]
reveal_type(c >= d)  # revealed: Literal[True]
```

#### Results with Ambiguity

```py
def _(x: bool, y: int):
    a = (x,)
    b = (y,)

    reveal_type(a == a)  # revealed: bool
    reveal_type(a != a)  # revealed: bool
    reveal_type(a < a)  # revealed: bool
    reveal_type(a <= a)  # revealed: bool
    reveal_type(a > a)  # revealed: bool
    reveal_type(a >= a)  # revealed: bool

    reveal_type(a == b)  # revealed: bool
    reveal_type(a != b)  # revealed: bool
    reveal_type(a < b)  # revealed: bool
    reveal_type(a <= b)  # revealed: bool
    reveal_type(a > b)  # revealed: bool
    reveal_type(a >= b)  # revealed: bool
```

#### Comparison Unsupported

If two tuples contain types that do not support comparison, the result may be `Unknown`. However,
`==` and `!=` are exceptions and can still provide definite results.

```py
a = (1, 2)
b = (1, "hello")

# TODO: should be Literal[False], once we implement (in)equality for mismatched literals
reveal_type(a == b)  # revealed: bool

# TODO: should be Literal[True], once we implement (in)equality for mismatched literals
reveal_type(a != b)  # revealed: bool

# error: [unsupported-operator] "Operator `<` is not supported for types `int` and `str`, in comparing `tuple[Literal[1], Literal[2]]` with `tuple[Literal[1], Literal["hello"]]`"
reveal_type(a < b)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<=` is not supported for types `int` and `str`, in comparing `tuple[Literal[1], Literal[2]]` with `tuple[Literal[1], Literal["hello"]]`"
reveal_type(a <= b)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>` is not supported for types `int` and `str`, in comparing `tuple[Literal[1], Literal[2]]` with `tuple[Literal[1], Literal["hello"]]`"
reveal_type(a > b)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>=` is not supported for types `int` and `str`, in comparing `tuple[Literal[1], Literal[2]]` with `tuple[Literal[1], Literal["hello"]]`"
reveal_type(a >= b)  # revealed: Unknown
```

However, if the lexicographic comparison completes without reaching a point where str and int are
compared, Python will still produce a result based on the prior elements.

```py
a = (1, 2)
b = (999999, "hello")

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]
```

#### Matryoshka Tuples

```py
a = (1, True, "Hello")
b = (a, a, a)
c = (b, b, b)

reveal_type(c == c)  # revealed: Literal[True]
reveal_type(c != c)  # revealed: Literal[False]
reveal_type(c < c)  # revealed: Literal[False]
reveal_type(c <= c)  # revealed: Literal[True]
reveal_type(c > c)  # revealed: Literal[False]
reveal_type(c >= c)  # revealed: Literal[True]
```

#### Non Boolean Rich Comparisons

Rich comparison methods defined in a class affect tuple comparisons as well. Proper type inference
should be possible even in cases where these methods return non-boolean types.

Note: Tuples use lexicographic comparisons. If the `==` result for all paired elements in the tuple
is True, the comparison then considers the tuple’s length. Regardless of the return type of the
dunder methods, the final result can still be a boolean value.

(+cpython: For tuples, `==` and `!=` always produce boolean results, regardless of the return type
of the dunder methods.)

```py
from __future__ import annotations

class EqReturnType: ...
class NeReturnType: ...
class LtReturnType: ...
class LeReturnType: ...
class GtReturnType: ...
class GeReturnType: ...

class A:
    def __eq__(self, o: object) -> EqReturnType:
        return EqReturnType()

    def __ne__(self, o: object) -> NeReturnType:
        return NeReturnType()

    def __lt__(self, o: A) -> LtReturnType:
        return LtReturnType()

    def __le__(self, o: A) -> LeReturnType:
        return LeReturnType()

    def __gt__(self, o: A) -> GtReturnType:
        return GtReturnType()

    def __ge__(self, o: A) -> GeReturnType:
        return GeReturnType()

a = (A(), A())

reveal_type(a == a)  # revealed: bool
reveal_type(a != a)  # revealed: bool
reveal_type(a < a)  # revealed: LtReturnType | Literal[False]
reveal_type(a <= a)  # revealed: LeReturnType | Literal[True]
reveal_type(a > a)  # revealed: GtReturnType | Literal[False]
reveal_type(a >= a)  # revealed: GeReturnType | Literal[True]

# If lexicographic comparison is finished before comparing A()
b = ("1_foo", A())
c = ("2_bar", A())

reveal_type(b == c)  # revealed: Literal[False]
reveal_type(b != c)  # revealed: Literal[True]
reveal_type(b < c)  # revealed: Literal[True]
reveal_type(b <= c)  # revealed: Literal[True]
reveal_type(b > c)  # revealed: Literal[False]
reveal_type(b >= c)  # revealed: Literal[False]

class LtReturnTypeOnB: ...

class B:
    def __lt__(self, o: B) -> LtReturnTypeOnB:
        return LtReturnTypeOnB()

reveal_type((A(), B()) < (A(), B()))  # revealed: LtReturnType | LtReturnTypeOnB | Literal[False]
```

#### Special Handling of Eq and NotEq in Lexicographic Comparisons

> Example: `(<int instance>, "foo") == (<int instance>, "bar")`

`Eq` and `NotEq` have unique behavior compared to other operators in lexicographic comparisons.
Specifically, for `Eq`, if any non-equal pair exists within the tuples being compared, we can
immediately conclude that the tuples are not equal. Conversely, for `NotEq`, if any non-equal pair
exists, we can determine that the tuples are unequal.

In contrast, with operators like `<` and `>`, the comparison must consider each pair of elements
sequentially, and the final outcome might remain ambiguous until all pairs are compared.

```py
def _(x: str, y: int):
    reveal_type("foo" == "bar")  # revealed: Literal[False]
    reveal_type(("foo",) == ("bar",))  # revealed: Literal[False]
    reveal_type((4, "foo") == (4, "bar"))  # revealed: Literal[False]
    reveal_type((y, "foo") == (y, "bar"))  # revealed: Literal[False]

    a = (x, y, "foo")

    reveal_type(a == a)  # revealed: bool
    reveal_type(a != a)  # revealed: bool
    reveal_type(a < a)  # revealed: bool
    reveal_type(a <= a)  # revealed: bool
    reveal_type(a > a)  # revealed: bool
    reveal_type(a >= a)  # revealed: bool

    b = (x, y, "bar")

    reveal_type(a == b)  # revealed: Literal[False]
    reveal_type(a != b)  # revealed: Literal[True]
    reveal_type(a < b)  # revealed: bool
    reveal_type(a <= b)  # revealed: bool
    reveal_type(a > b)  # revealed: bool
    reveal_type(a >= b)  # revealed: bool

    c = (x, y, "foo", "different_length")

    reveal_type(a == c)  # revealed: Literal[False]
    reveal_type(a != c)  # revealed: Literal[True]
    reveal_type(a < c)  # revealed: bool
    reveal_type(a <= c)  # revealed: bool
    reveal_type(a > c)  # revealed: bool
    reveal_type(a >= c)  # revealed: bool
```

#### Error Propagation

Errors occurring within a tuple comparison should propagate outward. However, if the tuple
comparison can clearly conclude before encountering an error, the error should not be raised.

```py
def _(n: int, s: str):
    class A: ...
    # error: [unsupported-operator] "Operator `<` is not supported for types `A` and `A`"
    A() < A()
    # error: [unsupported-operator] "Operator `<=` is not supported for types `A` and `A`"
    A() <= A()
    # error: [unsupported-operator] "Operator `>` is not supported for types `A` and `A`"
    A() > A()
    # error: [unsupported-operator] "Operator `>=` is not supported for types `A` and `A`"
    A() >= A()

    a = (0, n, A())

    # error: [unsupported-operator] "Operator `<` is not supported for types `A` and `A`, in comparing `tuple[Literal[0], int, A]` with `tuple[Literal[0], int, A]`"
    reveal_type(a < a)  # revealed: Unknown
    # error: [unsupported-operator] "Operator `<=` is not supported for types `A` and `A`, in comparing `tuple[Literal[0], int, A]` with `tuple[Literal[0], int, A]`"
    reveal_type(a <= a)  # revealed: Unknown
    # error: [unsupported-operator] "Operator `>` is not supported for types `A` and `A`, in comparing `tuple[Literal[0], int, A]` with `tuple[Literal[0], int, A]`"
    reveal_type(a > a)  # revealed: Unknown
    # error: [unsupported-operator] "Operator `>=` is not supported for types `A` and `A`, in comparing `tuple[Literal[0], int, A]` with `tuple[Literal[0], int, A]`"
    reveal_type(a >= a)  # revealed: Unknown

    # Comparison between `a` and `b` should only involve the first elements, `Literal[0]` and `Literal[99999]`,
    # and should terminate immediately.
    b = (99999, n, A())

    reveal_type(a < b)  # revealed: Literal[True]
    reveal_type(a <= b)  # revealed: Literal[True]
    reveal_type(a > b)  # revealed: Literal[False]
    reveal_type(a >= b)  # revealed: Literal[False]
```

### Membership Test Comparisons

"Membership Test Comparisons" refers to the operators `in` and `not in`.

```py
def _(n: int):
    a = (1, 2)
    b = ((3, 4), (1, 2))
    c = ((1, 2, 3), (4, 5, 6))
    d = ((n, n), (n, n))

    reveal_type(a in b)  # revealed: Literal[True]
    reveal_type(a not in b)  # revealed: Literal[False]

    reveal_type(a in c)  # revealed: Literal[False]
    reveal_type(a not in c)  # revealed: Literal[True]

    reveal_type(a in d)  # revealed: bool
    reveal_type(a not in d)  # revealed: bool
```

### Identity Comparisons

"Identity Comparisons" refers to `is` and `is not`.

```py
a = (1, 2)
b = ("a", "b")
c = (1, 2, 3)

reveal_type(a is (1, 2))  # revealed: bool
reveal_type(a is not (1, 2))  # revealed: bool

# TODO should be Literal[False] once we implement comparison of mismatched literal types
reveal_type(a is b)  # revealed: bool
# TODO should be Literal[True] once we implement comparison of mismatched literal types
reveal_type(a is not b)  # revealed: bool

reveal_type(a is c)  # revealed: Literal[False]
reveal_type(a is not c)  # revealed: Literal[True]
```

## Homogeneous

For tuples like `tuple[int, ...]`, `tuple[Any, ...]`

// TODO

## Chained comparisons with elements that incorrectly implement `__bool__`

<!-- snapshot-diagnostics -->

For an operation `A() < A()` to succeed at runtime, the `A.__lt__` method does not necessarily need
to return an object that is convertible to a `bool`. However, the return type _does_ need to be
convertible to a `bool` for the operation `A() < A() < A()` (a _chained_ comparison) to succeed.
This is because `A() < A() < A()` desugars to something like this, which involves several implicit
conversions to `bool`:

```ignore
def compute_chained_comparison():
  a1 = A()
  a2 = A()
  first_comparison = a1 < a2
  return first_comparison and (a2 < A())
```

```py
class NotBoolable:
    __bool__: int = 5

class Comparable:
    def __lt__(self, other) -> NotBoolable:
        return NotBoolable()

    def __gt__(self, other) -> NotBoolable:
        return NotBoolable()

a = (1, Comparable())
b = (1, Comparable())

# error: [unsupported-bool-conversion]
a < b < b

a < b  # fine
```

## Equality with elements that incorrectly implement `__bool__`

<!-- snapshot-diagnostics -->

Python does not generally attempt to coerce the result of `==` and `!=` operations between two
arbitrary objects to a `bool`, but a comparison of tuples will fail if the result of comparing any
pair of elements at equivalent positions cannot be converted to a `bool`:

```py
class NotBoolable:
    __bool__: None = None

class A:
    def __eq__(self, other) -> NotBoolable:
        return NotBoolable()

# error: [unsupported-bool-conversion]
(A(),) == (A(),)
```
