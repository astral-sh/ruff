# Class subscript

## Class getitem unbound

```py
class NotSubscriptable: ...

# error: "Cannot subscript object of type `<class 'NotSubscriptable'>` with no `__class_getitem__` method"
a = NotSubscriptable[0]
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

## Class getitem with unbound method union

```py
def _(flag: bool):
    if flag:
        class Spam:
            def __class_getitem__(self, x: int) -> str:
                return "foo"

    else:
        class Spam: ...
    # error: [not-subscriptable] "Cannot subscript object of type `<class 'Spam'>` with no `__class_getitem__` method"
    # revealed: str | Unknown
    reveal_type(Spam[42])
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
