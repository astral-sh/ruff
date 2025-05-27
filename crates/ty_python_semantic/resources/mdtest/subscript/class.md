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
    # error: [possibly-unbound-implicit-call] "Method `__class_getitem__` of type `<class 'Spam'> | <class 'Spam'>` is possibly unbound"
    # revealed: str
    reveal_type(Spam[42])
```

## TODO: Class getitem non-class union

```py
def _(flag: bool):
    if flag:
        class Eggs:
            def __class_getitem__(self, x: int) -> str:
                return "foo"

    else:
        Eggs = 1

    a = Eggs[42]  # error: "Cannot subscript object of type `<class 'Eggs'> | Literal[1]` with no `__getitem__` method"

    # TODO: should _probably_ emit `str | Unknown`
    reveal_type(a)  # revealed: Unknown
```
