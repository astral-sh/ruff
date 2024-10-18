# Class subscript

## Class getitem unbound

```py
class NotSubscriptable: pass
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

class Identity:
    if flag:
        def __class_getitem__(cls, item: int) -> str:
            return item
    else:
        def __class_getitem__(cls, item: int) -> int:
            return item

reveal_type(Identity[0])  # revealed: str | int
```

## Class getitem with class union

```py
flag = True

class Identity1:
    def __class_getitem__(cls, item: int) -> str:
        return item

class Identity2:
    def __class_getitem__(cls, item: int) -> int:
        return item

if flag:
    a = Identity1
else:
    a = Identity2

reveal_type(a)  # revealed: Literal[Identity1, Identity2]
reveal_type(a[0])  # revealed: str | int
```

## Class getitem with unbound method union

```py
flag = True

if flag:
    class Identity:
        def __class_getitem__(self, x: int) -> str:
            pass
else:
    class Identity: pass

a = Identity[42]  # error: [call-non-callable] "Method `__class_getitem__` of type `Literal[__class_getitem__] | Unbound` is not callable on object of type `Literal[Identity, Identity]`" 
reveal_type(a)  # revealed: str | Unknown 
```

## TODO: Class getitem non-class union

```py
flag = True

if flag:
    class Identity:
        def __class_getitem__(self, x: int) -> str:
            pass
else:
    Identity = 1

a = Identity[42]  # error: "Cannot subscript object of type `Literal[Identity] | Literal[1]` with no `__getitem__` method"
# TODO: should _probably_ emit `str | Unknown` 
reveal_type(a)  # revealed: Unknown 
```
