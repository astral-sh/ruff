# Instance subscript

## Getitem unbound

```py
class NotSubscriptable: pass
a = NotSubscriptable()[0]  # error: "Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"
```

## Getitem not callable

```py
class NotSubscriptable:
    __getitem__ = None

a = NotSubscriptable()[0]  # error: "Method `__getitem__` of type `None` is not callable on object of type `NotSubscriptable`"
```

## Valid getitem

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

a = Identity()[0]  
reveal_type(a) # revealed: int
```

## Getitem union

```py
flag = True

class Identity:
    if flag:
        def __getitem__(self, index: int) -> int:
            return index
    else:
        def __getitem__(self, index: int) -> str:
            return str(index)

a = Identity()[0]  
reveal_type(a) # revealed: int | str
```
