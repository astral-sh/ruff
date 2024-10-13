# Classes

## Unbound class variable

In Python, class variables can reference global variables unless overridden within the class scope.

```py
x = 1
class C:
    y = x
    if flag:
        x = 2

reveal_type(C.x) # revealed: Unbound | Literal[2]
reveal_type(C.y) # revealed: Literal[1]
```

## Cyclical class definition

Python supports classes that can reference themselves in their base class definitions. Although it may seem unusual, such a structure is not uncommon, particularly in type hinting systems like `typeshed`, where base classes can be self-referential: `class str(Sequence[str]): ...`.

```py path=a.pyi
class C(C): ...
reveal_type(C)  # revealed: Literal[C]
```

## Union of attributes

```py
if flag:
    class C:
        x = 1
else:
    class C:
        x = 2

y = C.x
reveal_type(y)  # revealed: Literal[1, 2]
```

## Shadowing

### Implicit error

```py
class C: pass
C = 1 # error: "Implicit shadowing of class `C`; annotate to make it explicit if this is intentional"
```

### Explicit

This test ensures that no diagnostic is raised in the case of explicit shadowing:

```py
class C: pass
C: int = 1
```

## Subscript

### Getitem unbound

```py
class NotSubscriptable: pass
a = NotSubscriptable()[0]  # error: "Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"
```

### Class getitem unbound

```py
class NotSubscriptable: pass
a = NotSubscriptable[0]  # error: "Cannot subscript object of type `Literal[NotSubscriptable]` with no `__class_getitem__` method"
```

### Getitem not callable

```py
class NotSubscriptable:
    __getitem__ = None

a = NotSubscriptable()[0]  # error: "Method `__getitem__` of type `None` is not callable on object of type `NotSubscriptable`"
```

### Valid getitem

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

a = Identity()[0]  
reveal_type(a) # revealed: int
```

### Class getitem

```py
class Identity:
    def __class_getitem__(cls, item: int) -> str:
        return item

a = Identity[0]  
reveal_type(a) # revealed: str
```

### Getitem union

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

### Class getitem union

```py
flag = True

class Identity:
    if flag:
        def __class_getitem__(cls, item: int) -> str:
            return item
    else:
        def __class_getitem__(cls, item: int) -> int:
            return item

a = Identity[0]  
reveal_type(a) # revealed: str | int
```

### Class getitem with class union

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

b = a[0]
reveal_type(a) # revealed: Literal[Identity1, Identity2]
reveal_type(b) # revealed: str | int
```

### Class getitem with unbound method union

```py
flag = True

if flag:
    class Identity:
        def __class_getitem__(self, x: int) -> str:
            pass
else:
    class Identity: pass

a = Identity[42] # error: [call-non-callable] "Method `__class_getitem__` of type `Literal[__class_getitem__] | Unbound` is not callable on object of type `Literal[Identity, Identity]`" 
reveal_type(a) # revealed: str | Unknown 
```

### TODO: Class getitem non-class union

`a = Identity[42]` should _probably_ emit `str | Unknown` instead of `Unknown`.

```py
flag = True

if flag:
    class Identity:
        def __class_getitem__(self, x: int) -> str:
            pass
else:
    Identity = 1

a = Identity[42] # error: "Cannot subscript object of type `Literal[Identity] | Literal[1]` with no `__getitem__` method"
reveal_type(a) # revealed: Unknown 
```

## Dunder call

```py
class Multiplier:
    def __init__(self, factor: float):
        self.factor = factor

    def __call__(self, number: float) -> float:
        return number * self.factor

a = Multiplier(2.0)(3.0)

class Unit: ...

b = Unit()(3.0) # error: "Object of type `Unit` is not callable"

reveal_type(a) # revealed: float
reveal_type(b) # revealed: Unknown
```
