# Comparison: Unions

## Union on one side of the comparison

Comparisons on union types need to consider all possible cases:

```py
from typing import Literal

def _(
    one_or_two: Literal[1, 2],
    a_or_ab: Literal["a", "ab"],
    one_or_none: Literal[1] | None,
):
    reveal_type(one_or_two <= 2)  # revealed: Literal[True]
    reveal_type(one_or_two <= 1)  # revealed: bool
    reveal_type(one_or_two <= 0)  # revealed: Literal[False]

    reveal_type(2 >= one_or_two)  # revealed: Literal[True]
    reveal_type(1 >= one_or_two)  # revealed: bool
    reveal_type(0 >= one_or_two)  # revealed: Literal[False]

    reveal_type(one_or_two < 1)  # revealed: Literal[False]
    reveal_type(one_or_two < 2)  # revealed: bool
    reveal_type(one_or_two < 3)  # revealed: Literal[True]

    reveal_type(one_or_two > 0)  # revealed: Literal[True]
    reveal_type(one_or_two > 1)  # revealed: bool
    reveal_type(one_or_two > 2)  # revealed: Literal[False]

    reveal_type(one_or_two == 3)  # revealed: Literal[False]
    reveal_type(one_or_two == 1)  # revealed: bool

    reveal_type(one_or_two != 3)  # revealed: Literal[True]
    reveal_type(one_or_two != 1)  # revealed: bool

    reveal_type(a_or_ab in "ab")  # revealed: Literal[True]
    reveal_type("a" in a_or_ab)  # revealed: Literal[True]

    reveal_type("c" not in a_or_ab)  # revealed: Literal[True]
    reveal_type("a" not in a_or_ab)  # revealed: Literal[False]

    reveal_type("b" in a_or_ab)  # revealed: bool
    reveal_type("b" not in a_or_ab)  # revealed: bool

    reveal_type(one_or_none is None)  # revealed: bool
    reveal_type(one_or_none is not None)  # revealed: bool
```

## Union on both sides of the comparison

With unions on both sides, we need to consider the full cross product of options when building the
resulting (union) type:

```py
from typing import Literal

def _(small: Literal[1, 2], large: Literal[2, 3]):
    reveal_type(small <= large)  # revealed: Literal[True]
    reveal_type(small >= large)  # revealed: bool

    reveal_type(small < large)  # revealed: bool
    reveal_type(small > large)  # revealed: Literal[False]
```

## Unsupported operations

Make sure we emit a diagnostic if *any* of the possible comparisons is unsupported. For now, we fall
back to `bool` for the result type instead of trying to infer something more precise from the other
(supported) variants:

```py
from typing import Literal

def _(
    x: list[int] | Literal[1],
    y: list[int] | Literal[1],
    aa: tuple[int],
    bb: tuple[int] | tuple[int, int],
    cc: tuple[str] | tuple[str, str],
):
    result = 1 in x  # snapshot
    reveal_type(result)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:10:14
   |
10 |     result = 1 in x  # snapshot
   |              -^^^^-
   |              |    |
   |              |    Has type `list[int] | Literal[1]`
   |              Has type `Literal[1]`
info: Operation fails because operator `in` is not supported between two objects of type `Literal[1]`
```

```py
    result2 = y in x  # snapshot: unsupported-operator
    reveal_type(result)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:12:15
   |
12 |     result2 = y in x  # snapshot: unsupported-operator
   |               -^^^^-
   |               |
   |               Both operands have type `list[int] | Literal[1]`
info: Operation fails because operator `in` is not supported between objects of type `list[int]` and `Literal[1]`
```

```py
    result3 = aa < cc  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:14:15
   |
14 |     result3 = aa < cc  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[str] | tuple[str, str]`
   |               Has type `tuple[int]`
info: Operation fails because operator `<` is not supported between objects of type `int` and `str`
```

```py
    result4 = cc < aa  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:15:15
   |
15 |     result4 = cc < aa  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[int]`
   |               Has type `tuple[str] | tuple[str, str]`
info: Operation fails because operator `<` is not supported between objects of type `str` and `int`
```

```py
    result5 = bb < cc  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:16:15
   |
16 |     result5 = bb < cc  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[str] | tuple[str, str]`
   |               Has type `tuple[int] | tuple[int, int]`
info: Operation fails because operator `<` is not supported between objects of type `int` and `str`
```

## Equality narrowing with many union alternatives

```toml
[environment]
python-version = "3.12"
```

Excluding a union of intersections can expand into exponentially many alternatives.

```py
from typing import Any, Literal
from ty_extensions import Intersection

class A[T]: ...
class B[T]: ...

type Case[T] = Intersection[A[T], B[T]]
```

With two cases, the exact exclusions fit within the budget:

```py
def two_cases(value: Any | Case[Literal[1]] | Case[Literal[2]]) -> None:
    if value == "example":
        # revealed: (Any & ~A[Literal[1]] & ~A[Literal[2]]) | (Any & ~B[Literal[1]] & ~A[Literal[2]]) | (Any & ~A[Literal[1]] & ~B[Literal[2]]) | (Any & ~B[Literal[1]] & ~B[Literal[2]])
        reveal_type(value)
```

When the expansion exceeds the intersection budget, equality narrowing keeps the surviving `Any`
without the exclusions:

```py
def three_cases(value: Any | Case[Literal[1]] | Case[Literal[2]] | Case[Literal[3]]) -> None:
    if value == "example":
        reveal_type(value)  # revealed: Any
```

The same fallback prevents excessive memory use with a larger union:

```py
type Cases = (
    Case[Literal[1]]
    | Case[Literal[2]]
    | Case[Literal[3]]
    | Case[Literal[4]]
    | Case[Literal[5]]
    | Case[Literal[6]]
    | Case[Literal[7]]
    | Case[Literal[8]]
    | Case[Literal[9]]
    | Case[Literal[10]]
    | Case[Literal[11]]
    | Case[Literal[12]]
    | Case[Literal[13]]
    | Case[Literal[14]]
    | Case[Literal[15]]
    | Case[Literal[16]]
    | Case[Literal[17]]
    | Case[Literal[18]]
    | Case[Literal[19]]
    | Case[Literal[20]]
)

def large(value: Any | Cases) -> None:
    if value == "example":
        reveal_type(value)  # revealed: Any
```
