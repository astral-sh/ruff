# Instance subscript

## `__getitem__` unbound

<!-- snapshot-diagnostics -->

```py
class NotSubscriptable: ...

a = NotSubscriptable()[0]  # error: [not-subscriptable]
```

## `__getitem__` not callable

```py
class NotSubscriptable:
    __getitem__ = None

# TODO: this would be more user-friendly if the `call-non-callable` diagnostic was
# transformed into a `not-subscriptable` diagnostic with a subdiagnostic explaining
# that this was because `__getitem__` was possibly not callable
#
# error: [call-non-callable] "Method `__getitem__` of type `Unknown | None` may not be callable on object of type `NotSubscriptable`"
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

## `__getitem__` with invalid index argument

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

a = Identity()
# error: [invalid-argument-type] "Method `__getitem__` of type `bound method Identity.__getitem__(index: int) -> int` cannot be called with key of type `Literal["a"]` on object of type `Identity`"
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
a[0] = 0  # error: "Cannot assign to a subscript on an object of type `NoSetitem`"
```

## `__setitem__` not callable

```py
class NoSetitem:
    __setitem__ = None

a = NoSetitem()
a[0] = 0  # error: "Method `__setitem__` of type `Unknown | None` may not be callable on object of type `NoSetitem`"
```

## Valid `__setitem__` method

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
a[0] = 0
```

## `__setitem__` with invalid index argument

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
# error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal["a"]` and value of type `Literal[0]` on object of type `Identity`"
a["a"] = 0
```
