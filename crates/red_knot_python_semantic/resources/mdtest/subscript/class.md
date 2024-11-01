# Class subscript

## Class getitem unbound

```py
class NotSubscriptable: ...

a = NotSubscriptable[0]  # error: "Cannot subscript object of type `Literal[NotSubscriptable]` with no `__class_getitem__` method"
```

## Class getitem

```py
class Identity:
    def __class_getitem__(cls, item: int) -> str:
        return item

reveal_type(Identity[0])  # revealed: str
```

## Class getitem union

```py
flag = True

class UnionClassGetItem:
    if flag:

        def __class_getitem__(cls, item: int) -> str:
            return item
    else:

        def __class_getitem__(cls, item: int) -> int:
            return item

reveal_type(UnionClassGetItem[0])  # revealed: str | int
```

## Class getitem with class union

```py
def bool_instance() -> bool:
    return True

class A:
    def __class_getitem__(cls, item: int) -> str:
        return item

class B:
    def __class_getitem__(cls, item: int) -> int:
        return item

x = A if bool_instance() else B

reveal_type(x)  # revealed: Literal[A, B]
reveal_type(x[0])  # revealed: str | int
```

## Class getitem with unbound method union

```py
flag = True

if flag:
    class Spam:
        def __class_getitem__(self, x: int) -> str:
            return "foo"

else:
    class Spam: ...

# error: [call-possibly-unbound-method] "Method `__class_getitem__` of type `Literal[Spam, Spam]` is possibly unbound"
# revealed: str
reveal_type(Spam[42])
```

## TODO: Class getitem non-class union

```py
flag = True

if flag:
    class Eggs:
        def __class_getitem__(self, x: int) -> str:
            return "foo"

else:
    Eggs = 1

a = Eggs[42]  # error: "Cannot subscript object of type `Literal[Eggs] | Literal[1]` with no `__getitem__` method"

# TODO: should _probably_ emit `str | Unknown`
reveal_type(a)  # revealed: Unknown
```
