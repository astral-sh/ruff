# Class subscript

## Class getitem unbound

```py
class NotSubscriptable: ...

# error: [not-subscriptable] "Cannot subscript object of type `<class 'NotSubscriptable'>` with no `__class_getitem__` method"
NotSubscriptable[0]

# snapshot: not-subscriptable
a = NotSubscriptable[0]
```

```snapshot
error[not-subscriptable]: Cannot subscript object of type `<class 'NotSubscriptable'>` with no `__class_getitem__` method
 --> src/mdtest_snippet.py:7:5
  |
7 | a = NotSubscriptable[0]
  |     ^^^^^^^^^^^^^^^^^^^
  |
```

## Class getitem not callable

```py
class NotSubscriptable:
    __class_getitem__ = None

# snapshot: not-subscriptable
a = NotSubscriptable[0]
```

```snapshot
error[not-subscriptable]: Invalid subscript read
 --> src/mdtest_snippet.py:5:5
  |
5 | a = NotSubscriptable[0]
  |     ----------------^^^
  |     |                |
  |     |                Method `__class_getitem__` has type `None | Unknown`
  |     |                An object of type `None | Unknown` may not be callable
  |     Has type `<class 'NotSubscriptable'>`
  |
info: `__class_getitem__` is implicitly called due to this subscript expression
```

## Class getitem

```py
class Identity:
    def __class_getitem__(cls, item: int) -> str:
        return str(item)

reveal_type(Identity[0])  # revealed: str
```

`__class_getitem__` is implicitly a classmethod, so it can be called like this:

```py
reveal_type(Identity.__class_getitem__(0))  # revealed: str
```

## Class getitem union

```py
def _(flag: bool):
    class UnionClassGetItem:
        if flag:
            def __class_getitem__(cls, item: int) -> str:
                return str(item)

        else:
            def __class_getitem__(cls, item: int) -> int:
                return item

    reveal_type(UnionClassGetItem[0])  # revealed: str | int
```

## Class getitem with too many parameters

```py
class Foo:
    def __class_getitem__(cls, x, y): ...

# error: [missing-argument] "No argument provided for required parameter `y` of bound method `Foo.__class_getitem__`"
Foo["x"]

Foo["x"]  # snapshot: missing-argument
```

```snapshot
error[missing-argument]: No argument provided for required parameter `y` of bound method `Foo.__class_getitem__`
 --> src/mdtest_snippet.py:7:1
  |
7 | Foo["x"]  # snapshot: missing-argument
  | ^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:2:35
  |
2 |     def __class_getitem__(cls, x, y): ...
  |                                   ^
  |
```

## Class getitem with too few parameters

```py
class Foo:
    def __class_getitem__(cls): ...

Foo["x"]  # snapshot: too-many-positional-arguments
```

```snapshot
error[too-many-positional-arguments]: Too many positional arguments to bound method `Foo.__class_getitem__`: expected 1, got 2
 --> src/mdtest_snippet.py:4:1
  |
4 | Foo["x"]  # snapshot: too-many-positional-arguments
  | ^^^^^^^^
  |
info: Method signature here
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __class_getitem__(cls): ...
  |         ^^^^^^^^^^^^^^^^^^^^^^
  |
```

## Overloaded bad class getitem

```py
from typing import overload

class Foo:
    @overload
    def __class_getitem__(cls, x: int): ...
    @overload
    def __class_getitem__(cls, x: str, y): ...
    def __class_getitem__(cls, x, y=None): ...

Foo["foo"]  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `Foo.__class_getitem__` is incorrect
  --> src/mdtest_snippet.py:10:1
   |
10 | Foo["foo"]  # snapshot: invalid-argument-type
   | ^^^^^^^^^^ Expected `int`, found `Literal["foo"]`
   |
info: Matching overload defined here
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __class_getitem__(cls, x: int): ...
  |         ^^^^^^^^^^^^^^^^^      ------ Parameter declared here
  |
info: Non-matching overloads for bound method `__class_getitem__`:
info:   (cls, x: str, y) -> Unknown
```

## Class getitem with invalid index argument

```py
class Identity:
    def __class_getitem__(cls, index: int) -> int:
        return index

Identity["a"]  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
 --> src/mdtest_snippet.py:5:1
  |
5 | Identity["a"]  # snapshot: invalid-argument-type
  | --------^^^^^
  | |        |
  | |        Expected `int`, got object of type `Literal["a"]`
  | Has type `<class 'Identity'>`
  |
info: This subscript expression implicitly calls `<class 'Identity'>.__class_getitem__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __class_getitem__(cls, index: int) -> int:
  |         ^^^^^^^^^^^^^^^^^ Method defined here
  |
```

## Class getitem with class union

```py
def _(flag: bool):
    class A:
        def __class_getitem__(cls, item: int) -> str:
            return str(item)

    class B:
        def __class_getitem__(cls, item: int) -> int:
            return item

    x = A if flag else B

    reveal_type(x)  # revealed: <class 'A'> | <class 'B'>
    reveal_type(x[0])  # revealed: str | int
```

## Union of bad class getitem methods

```py
class Foo:
    def __class_getitem__(cls, x: str): ...

class Bar:
    def __class_getitem__(cls, x: str): ...

def test(flag: bool):
    x = Foo if flag else Bar

    # error: [invalid-argument-type] "Cannot subscript an object of type `<class 'Bar'>` with a key of type `Literal[42]` (expected `str`)"
    # error: [invalid-argument-type] "Cannot subscript an object of type `<class 'Foo'>` with a key of type `Literal[42]` (expected `str`)"
    x[42]

    # snapshot: invalid-argument-type
    # snapshot: invalid-argument-type
    x[42]
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
  --> src/mdtest_snippet.py:16:5
   |
16 |     x[42]
   |     -^^^^
   |     | |
   |     | Expected `str`, got object of type `Literal[42]`
   |     Has type `<class 'Foo'> | <class 'Bar'>`
   |
info: This subscript expression implicitly calls `<class 'Bar'>.__class_getitem__`
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __class_getitem__(cls, x: str): ...
  |         ^^^^^^^^^^^^^^^^^ Method defined here
  |


error[invalid-argument-type]: Invalid subscript read
  --> src/mdtest_snippet.py:16:5
   |
16 |     x[42]
   |     -^^^^
   |     | |
   |     | Expected `str`, got object of type `Literal[42]`
   |     Has type `<class 'Foo'> | <class 'Bar'>`
   |
info: This subscript expression implicitly calls `<class 'Foo'>.__class_getitem__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __class_getitem__(cls, x: str): ...
  |         ^^^^^^^^^^^^^^^^^ Method defined here
  |
```

## Class getitem with unbound method union

```py
def _(flag: bool):
    if flag:
        class Spam:
            def __class_getitem__(self, x: int) -> str:
                return "foo"

    else:
        class Spam: ...

    # snapshot: not-subscriptable
    # revealed: str | Unknown
    reveal_type(Spam[42])
```

```snapshot
error[not-subscriptable]: Cannot subscript object of type `<class 'Spam'>` with no `__class_getitem__` method
  --> src/mdtest_snippet.py:12:17
   |
12 |     reveal_type(Spam[42])
   |                 ^^^^^^^^
   |
```

## Class getitem non-class union

```py
def _(flag: bool):
    if flag:
        class Eggs:
            def __class_getitem__(self, x: int) -> str:
                return "foo"

    else:
        Eggs = 1

    a = Eggs[42]  # error: "Cannot subscript object of type `Literal[1]` with no `__getitem__` method"

    reveal_type(a)  # revealed: str | Unknown
```

## Class getitem on metaclass

`__class_getitem__` is also looked up on the metaclass.

```py
class Meta(type):
    def __class_getitem__(cls, item: int) -> str:
        return str(item)

class WithMeta(metaclass=Meta): ...

reveal_type(WithMeta[0])  # revealed: str
```

## Conflicting class and metaclass getitem

`__class_getitem__` on the class takes precedence over the one on the metaclass.

```py
class Meta(type):
    def __class_getitem__(cls, item: int) -> str:
        return str(item)

class WithMetaAndClassGetItem(metaclass=Meta):
    def __class_getitem__(cls, item: int) -> int:
        return item

reveal_type(WithMetaAndClassGetItem[0])  # revealed: int
```

## Class getitem with getitem on metaclass

`__getitem__` on the metaclass takes precedence over `__class_getitem__` on the class.

```py
class Meta(type):
    def __getitem__(cls, item: int) -> str:
        return str(item)

class WithMetaGetItem(metaclass=Meta):
    def __class_getitem__(cls, item: int) -> int:
        return item

reveal_type(WithMetaGetItem[0])  # revealed: str
```

## Intersection of nominal-instance types

If a subscript operation could succeed for *any* positive element of an intersection, no diagnostic
should be reported even if it would not succeed for some other element of the intersection.

```py
class Foo: ...

class Bar:
    def __getitem__(self, key: str) -> int:
        return 42

def f(x: Foo):
    if isinstance(x, Bar):
        reveal_type(x["whatever"])  # revealed: int
```

When a subscript operation fails due to an invalid argument type, we should report only that error
(not a spurious `not-subscriptable` error for elements that lack `__getitem__`), and the return type
should be inferred from the element that does have `__getitem__`.

```py
class Baz:
    def __getitem__(self, key: int) -> str:
        return ""

class Qux: ...

def g(x: Baz):
    if isinstance(x, Qux):
        # x is Baz & Qux. Baz has __getitem__(int) -> str, Qux has no __getitem__.
        # The intersection IS subscriptable (via Baz), but with wrong argument type.
        # error: [invalid-argument-type]
        reveal_type(x["hello"])  # revealed: str
```

When both elements have `__getitem__` but both fail, we report errors for each element.

```py
class A:
    def __getitem__(self, key: int) -> str:
        return ""

class B:
    def __getitem__(self, key: str) -> int:
        return 0

def h(x: A):
    if isinstance(x, B):
        # x is A & B. A expects int, B expects str. A list satisfies neither.
        # error: [invalid-argument-type]
        # error: [invalid-argument-type]
        reveal_type(x[[]])  # revealed: Never
```

When we have an intersection with only negative elements (e.g., `~int & ~str`), the type is
implicitly `object & ~int & ~str`. Subscripting such a type should use `object` as the positive
element, which is not subscriptable.

```py
def i(x: object):
    if not isinstance(x, (int, str)):
        # x is object & ~int & ~str, which simplifies to ~int & ~str with implicit object
        # error: [not-subscriptable]
        reveal_type(x[0])  # revealed: Unknown
```

When both the value and the slice are intersections, we distribute over both. For `(A & B)[X & Y]`,
we get `(A[X] & A[Y]) & (B[X] & B[Y])`.

```py
from ty_extensions import Intersection

class ResultA: ...
class ResultB: ...

class ContainerA:
    def __getitem__(self, key: int) -> ResultA:
        return ResultA()

class ContainerB:
    def __getitem__(self, key: int) -> ResultB:
        return ResultB()

class IndexA(int): ...
class IndexB(int): ...

def j(
    container_a: ContainerA,
    container_b: ContainerB,
    container_ab: Intersection[ContainerA, ContainerB],
    index_a: IndexA,
    index_ab: Intersection[IndexA, IndexB],
):
    # Single container, single index
    reveal_type(container_a[index_a])  # revealed: ResultA

    # Single container, intersection index distributes over the index:
    # ContainerA[IndexA] & ContainerA[IndexB] = ResultA & ResultA = ResultA
    reveal_type(container_a[index_ab])  # revealed: ResultA

    # Intersection container, single index distributes over the container:
    # ContainerA[IndexA] & ContainerB[IndexA] = ResultA & ResultB
    reveal_type(container_ab[index_a])  # revealed: ResultA & ResultB

    # Both are intersections: distributes over both
    # (ContainerA[IndexA] & ContainerA[IndexB]) & (ContainerB[IndexA] & ContainerB[IndexB])
    # = (ResultA & ResultA) & (ResultB & ResultB) = ResultA & ResultB
    reveal_type(container_ab[index_ab])  # revealed: ResultA & ResultB
```
