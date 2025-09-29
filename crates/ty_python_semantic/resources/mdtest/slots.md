# Tests for improved `__slots__` support

## Basic slot access

```py
class C:
    __slots__ = ("foo", "bar")

    def __init__(self, foo: int, bar: str):
        self.foo = foo  # OK
        self.bar = bar  # OK

c = C(1, "hello")
c.foo = 2  # OK
c.bar = "world"  # OK
c.baz = 3  # error: [unresolved-attribute] "Unresolved attribute `baz` on type `C`."
```

## Accessing undefined attributes on slotted class

```py
class F:
    __slots__ = ("x",)

f = F()
print(f.y)  # error: [unresolved-attribute] "Type `F` has no attribute `y`"
```

## __dict__ and __weakref__ handling

```py
class G:
    __slots__ = ("foo",)

# Note: __dict__ and __weakref__ handling needs further work
g = G()
```

## Inheritance with __slots__

```py
class Parent:
    __slots__ = ("x",)

class Child(Parent):
    __slots__ = ("y",)

c = Child()
c.z = 3  # error: [unresolved-attribute] "Unresolved attribute `z` on type `Child`."
```

## Empty slots

```py
class L:
    __slots__ = ()

l = L()
l.x = 1  # error: [unresolved-attribute] "Unresolved attribute `x` on type `L`."
```

## Slots as string

```py
# Single character string slots work
class SingleChar:
    __slots__ = "x"

sc = SingleChar()
sc.other = 2  # error: [unresolved-attribute] "Unresolved attribute `other` on type `SingleChar`."

# Multi-character string expands to individual character slots
class MultiChar:
    __slots__ = "value"  # Creates slots "v", "a", "l", "u", "e"

mc = MultiChar()
mc.value = 3  # error: [unresolved-attribute] "Unresolved attribute `value` on type `MultiChar`."
```

## Attrs-like usage pattern

```py
class AttrsLike:
    """Simulates attrs-generated class"""

    __slots__ = ("name", "value")

    def __init__(self, name: str, value: int):
        self.name = name
        self.value = value

# Valid usage
obj = AttrsLike("test", 42)
obj.name = "updated"  # OK
obj.value = 100  # OK

# Invalid usage
obj.invalid = 1  # error: [unresolved-attribute] "Unresolved attribute `invalid` on type `AttrsLike`."
```
