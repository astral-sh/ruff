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
        # TODO: should be `int`
        reveal_type(x["whatever"])  # revealed: @Todo(Subscript expressions on intersections)
```
