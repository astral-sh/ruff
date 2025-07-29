# Instance subscript

## Getitem unbound

```py
class NotSubscriptable: ...

a = NotSubscriptable()[0]  # error: "Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"
```

## Getitem not callable

```py
class NotSubscriptable:
    __getitem__ = None

# error: "Method `__getitem__` of type `Unknown | None` is not callable on object of type `NotSubscriptable`"
a = NotSubscriptable()[0]
```

## Valid getitem

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

reveal_type(Identity()[0])  # revealed: int
```

## Getitem union

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

## `__getitem__` with invalid index argument

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

a = Identity()
# error: [call-non-callable] "Method `__getitem__` of type `bound method Identity.__getitem__(index: int) -> int` is not callable on object of type `Identity`"
a["a"]
```

## `__setitem__` with no `__getitem__`

```py
class NoGetitem:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = NoGetitem()
a[0] = 0
```

## Subscript store with no `__setitem__`

```py
class NoSetitem: ...

a = NoSetitem()
a[0] = 0  # error: "Cannot assign to object of type `NoSetitem` with no `__setitem__` method"
```

## Setitem not callable

```py
class NoSetitem:
    __setitem__ = None

a = NoSetitem()
a[0] = 0  # error: "Method `__setitem__` of type `Unknown | None` is not callable on object of type `NoSetitem`"
```

## Valid setitem

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
a[0] = 0
```

## Invalid index

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
# error: [call-non-callable] "Method `__setitem__` of type `bound method Identity.__setitem__(index: int, value: int) -> None` is not callable on object of type `Identity`"
a["a"] = 0
```
