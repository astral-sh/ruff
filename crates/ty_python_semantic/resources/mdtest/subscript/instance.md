# Instance subscript

## `__getitem__` unbound

```py
class NotSubscriptable: ...

a = NotSubscriptable()[0]  # error: "Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"
```

## `__getitem__` not callable

```py
class NotSubscriptable:
    __getitem__ = None

# error: "Method `__getitem__` of type `Unknown | None` is not callable on object of type `NotSubscriptable`"
a = NotSubscriptable()[0]
```

## Valid `__getitem__`

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

reveal_type(Identity()[0])  # revealed: int
```

## `__getitem__` union

```py
def _(flag: bool):
    class Identity:
        if flag:
            def __getitem__(self, index: int) -> int:
                return index
        else:
            def __getitem__(self, index: int) -> str:
                return str(index)

    reveal_type(Identity()[0])  # revealed: int | str
```

## `__setitem__` with no `__getitem__`

```py
class NoGetitem:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = NoGetitem()
a[0] = 0
```