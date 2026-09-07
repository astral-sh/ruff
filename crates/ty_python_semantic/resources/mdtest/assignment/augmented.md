# Augmented assignment

## Basic

```py
x = 3
x -= 1
reveal_type(x)  # revealed: Literal[2]

x = 1.0
x /= 2
reveal_type(x)  # revealed: float

x = (1, 2)
x += (3, 4)
reveal_type(x)  # revealed: tuple[Literal[1, 2, 3, 4], ...]
```

## Walrus target

```py
def f(xs: list[int | str]) -> None:
    ys = xs
    ys[0] = "s"
    (ys := [1])[0] += 1
```

## Dunder methods

```py
class C:
    def __isub__(self, other: int) -> str:
        return "Hello, world!"

x = C()
x -= 1
reveal_type(x)  # revealed: str

class C:
    def __iadd__(self, other: str) -> int:
        return 1

x = C()
x += "Hello"
reveal_type(x)  # revealed: int
```

## Unsupported types

```py
class C:
    def __isub__(self, other: str) -> int:
        return 42

x = C()
# snapshot: unsupported-operator
x -= 1

reveal_type(x)  # revealed: int
```

```snapshot
error[unsupported-operator]: Unsupported `-=` operation
 --> src/mdtest_snippet.py:7:1
  |
7 | x -= 1
  | -^^^^-
  | |    |
  | |    Has type `Literal[1]`
  | Has type `C`
```

## Method union

```py
def _(flag: bool):
    class Foo:
        if flag:
            def __iadd__(self, other: int) -> str:
                return "Hello, world!"

        else:
            def __iadd__(self, other: int) -> int:
                return 42

    f = Foo()
    f += 12

    reveal_type(f)  # revealed: str | int
```

## Partially bound `__iadd__`

```py
def _(flag: bool):
    class Foo:
        if flag:
            def __iadd__(self, other: str) -> int:
                return 42

    f = Foo()

    # error: [unsupported-operator] "Operator `+=` is not supported between objects of type `Foo` and `Literal["Hello, world!"]`"
    f += "Hello, world!"

    reveal_type(f)  # revealed: int | Unknown
```

## Partially bound with `__add__`

```py
def _(flag: bool):
    class Foo:
        def __add__(self, other: str) -> str:
            return "Hello, world!"
        if flag:
            def __iadd__(self, other: str) -> int:
                return 42

    f = Foo()
    f += "Hello, world!"

    reveal_type(f)  # revealed: int | str
```

## Partially bound target union

```py
def _(flag1: bool, flag2: bool):
    class Foo:
        def __add__(self, other: int) -> str:
            return "Hello, world!"
        if flag1:
            def __iadd__(self, other: int) -> int:
                return 42

    if flag2:
        f = Foo()
    else:
        f = 42.0
    f += 12

    reveal_type(f)  # revealed: float | str
```

## Target union

```py
def _(flag: bool):
    class Foo:
        def __iadd__(self, other: int) -> str:
            return "Hello, world!"

    if flag:
        f = Foo()
    else:
        f = 42
    f += 12

    reveal_type(f)  # revealed: str | Literal[54]
```

## Partially bound target union with `__add__`

```py
def f(flag: bool, flag2: bool):
    class Foo:
        def __add__(self, other: int) -> str:
            return "Hello, world!"
        if flag:
            def __iadd__(self, other: int) -> int:
                return 42

    class Bar:
        def __add__(self, other: int) -> bytes:
            return b"Hello, world!"

        def __iadd__(self, other: int) -> float:
            return 42.0

    if flag2:
        f = Foo()
    else:
        f = Bar()
    f += 12

    reveal_type(f)  # revealed: float | str
```

## Declared attributes with in-place operators

`+=` assigns the value returned by `__iadd__` back to its target. That value must be compatible with
the attribute's declared type.

```py
class Value:
    def __iadd__(self, other: int) -> str:
        return "updated"

class Holder:
    value: Value

holder = Holder()
# error: [invalid-assignment]
holder.value += 1
reveal_type(holder.value)  # revealed: Value
```

## Declared attributes without in-place operators

When an object does not define `__iadd__`, `+=` falls back to `__add__`. Its result must still be
compatible with the attribute's declared type.

```py
class Value:
    def __add__(self, other: int) -> str:
        return "updated"

class Holder:
    value: Value

holder = Holder()
# error: [invalid-assignment]
holder.value += 1
```

## Inferred attributes in loops

An unannotated instance attribute may change type. After its initial `None` value is replaced, an
augmented assignment inside a loop must also contribute its result to the inferred attribute type.

```py
class Counter:
    def update(self) -> None:
        self.value = None
        self.value = 0
        for _ in range(1):
            self.value += 1.0

reveal_type(Counter().value)  # revealed: None | float
```

## Inferred class attributes

An unannotated class attribute still has an inferred type that restricts assignments through an
instance.

```py
class Holder:
    value = 1

holder = Holder()
# error: [invalid-assignment]
holder.value += 0.5
```

## Read-only properties

`+=` writes its result back to the attribute. A property without a setter therefore cannot be the
target of an augmented assignment.

```py
class ReadOnly:
    @property
    def value(self) -> int:
        return 1

read_only = ReadOnly()
# error: [invalid-assignment]
read_only.value += 1
```

## Properties with different getter and setter types

A property can accept a wider type in its setter than it returns from its getter. The result of `/=`
is checked against the setter, while subsequent reads still use the getter's return type.

```py
class Counter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, value: float) -> None:
        pass

counter = Counter()
counter.value /= 2
reveal_type(counter.value)  # revealed: int
```

## Attributes defined by descriptors

When an unannotated class attribute is a data descriptor, its `__set__` method determines which
values may be assigned.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type[object] | None = None) -> int:
        return 1

    def __set__(self, instance: object, value: str) -> None:
        pass

class Holder:
    value = Descriptor()

holder = Holder()
# error: [invalid-assignment]
holder.value += 1
```

## Custom subscript assignments

`/=` first reads an item, then writes the result back through `__setitem__`. The assigned value is
the result of the operation, not the right-hand operand.

```py
class Container:
    def __getitem__(self, key: int) -> int:
        return 1

    def __setitem__(self, key: int, value: int) -> None:
        pass

container = Container()
# error: [invalid-assignment]
container[0] /= 2
reveal_type(container[0])  # revealed: int
```

## Subscript setters with different value types

A collection can accept a wider type in `__setitem__` than `__getitem__` returns. After a valid
assignment, subsequent reads still use the return type of `__getitem__`.

```py
class Container:
    def __getitem__(self, key: int) -> int:
        return 1

    def __setitem__(self, key: int, value: float) -> None:
        pass

container = Container()
container[0] /= 2
reveal_type(container[0])  # revealed: int
```

## Annotated collection entries

An annotation fixes the element type of a list, so `/=` cannot write a `float` into a `list[int]`.

```py
values: list[int] = [1]
# error: [invalid-assignment]
values[0] /= 2
```

The same rule applies to the value type of an annotated dictionary.

```py
mapping: dict[str, int] = {"value": 1}
# error: [invalid-assignment]
mapping["value"] /= 2
```

An annotated collection remains constrained when it is accessed through an attribute.

```py
class Holder:
    values: list[int]

holder = Holder()
# error: [invalid-assignment]
holder.values[0] /= 2
```

## Typed dictionary entries

A `TypedDict` field can only be assigned a value compatible with its declared type.

```py
from typing import TypedDict

class Payload(TypedDict):
    value: int

payload: Payload = {"value": 1}
# error: [invalid-assignment]
payload["value"] /= 2
```

## Read-only subscripts

A readable item cannot be reassigned when its container does not implement `__setitem__`.

```py
values: tuple[int] = (1,)
# error: [invalid-assignment]
values[0] += 1
```

## Missing attributes

If an augmented assignment cannot read its target, it must report that failure only once; no
assignment is attempted.

```py
class Missing: ...

missing = Missing()
# error: [unresolved-attribute]
missing.value += 1
```

The same applies when an attribute is missing from one member of a union.

```py
class Counter:
    count: int

def update(counter: Counter | None) -> None:
    # error: [unresolved-attribute]
    counter.count += 1
```

An augmented assignment cannot define an otherwise missing instance attribute, because it must read
an existing value before writing its result.

```py
class UninitializedCounter:
    def increment(self) -> None:
        # error: [unresolved-attribute]
        self.value += 1

# error: [unresolved-attribute]
reveal_type(UninitializedCounter().value)  # revealed: Unknown
```

## Dynamically provided attributes

A dynamic attribute hook can provide the initial value read by an augmented assignment. Ordinary
attribute lookup preserves the hook's return type, but the resulting assignment is not yet
recognized as establishing instance storage.

```py
class DynamicCounter:
    def __getattr__(self, name: str) -> int:
        return 0

    def increment(self) -> None:
        # TODO: Recognize the instance attribute established after reading from a dynamic hook.
        # error: [unresolved-attribute]
        self.value += 1

reveal_type(DynamicCounter().value)  # revealed: int
```

The same behavior applies when the attribute is provided by `__getattribute__`.

```py
class InterceptedCounter:
    def __getattribute__(self, name: str) -> int:
        return 0

    def increment(self) -> None:
        # TODO: Recognize the instance attribute established after reading from a dynamic hook.
        # error: [unresolved-attribute]
        self.value += 1

reveal_type(InterceptedCounter().value)  # revealed: int
```

## Class-level defaults in diamond inheritance

An overriding class-level default supplies the initial value even when another branch of the
inheritance hierarchy declares a wider instance attribute.

```py
class Base:
    value: int | None = None

class First(Base): ...

class Second(Base):
    value: int | None

class Child(First, Second):
    value: int = 1

    def update(self) -> None:
        self.value |= 2
```

## Invalid subscript reads

An invalid key prevents an item from being read, so the failed assignment must not produce a second
error.

```py
mapping: dict[str, int] = {}
# error: [invalid-argument-type]
mapping[1] += 1
```

A value without `__getitem__` also fails before assignment can be attempted.

```py
value = 1
# error: [not-subscriptable]
value[0] += 1
```

## Right-hand-side errors after failed reads

Even when an attribute cannot be read, the right-hand side must still be checked for unrelated
errors.

```py
class Missing: ...

missing = Missing()
# error: [unresolved-attribute]
# error: [unresolved-reference]
missing.value += missing_attribute_operand
```

The same rule applies when a subscript cannot be read.

```py
mapping: dict[str, int] = {}
# error: [invalid-argument-type]
# error: [unresolved-reference]
mapping[1] += missing_subscript_operand
```

## Failed in-place operations

If `__iadd__` rejects its operand, its return type must not be treated as a value to assign.

```py
class Value:
    def __iadd__(self, other: int) -> str:
        return "updated"

class Holder:
    value: Value

holder = Holder()
# error: [unsupported-operator]
holder.value += "invalid"
```

## Union attribute assignments

When objects in a union have different attribute types, each operator result should be checked
against the attribute from the same object. Ordinary assignments already lose this relationship, so
augmented assignments currently report the same false positive.

```py
class AValue:
    def __iadd__(self, other: int) -> "AValue":
        return self

class BValue:
    def __iadd__(self, other: int) -> "BValue":
        return self

class A:
    value: AValue

class B:
    value: BValue

def update(value: A | B) -> None:
    # TODO: Check each result against the attribute it came from.
    # error: [invalid-assignment]
    value.value += 1
```

## Collections that may be read-only

When a collection could be a writable list or a read-only tuple, an item assignment is invalid
because it cannot be performed on every possible value.

```py
def update(value: list[int] | tuple[int, ...]) -> None:
    # error: [invalid-assignment]
    value[0] += 1
```

## Typed dictionary assignments with multiple possible keys

A key that can select fields with different value types must only be assigned a value accepted by
every possible field.

```py
from typing import Literal, TypedDict

class Payload(TypedDict):
    whole: int
    fractional: float

def update(value: Payload, key: Literal["whole", "fractional"]) -> None:
    # error: [invalid-assignment]
    value[key] /= 2
```

## Inferred collection entries

Augmented assignments are not yet included when inferring the element type of an unannotated
collection.

```py
values = [1]
# TODO: Infer `list[float]` instead of rejecting the assignment.
# error: [invalid-assignment]
values[0] /= 2
```

## Implicit dunder calls on class objects

```py
class Meta(type):
    def __iadd__(cls, other: int) -> str:
        return ""

class C(metaclass=Meta): ...

cls = C
cls += 1

reveal_type(cls)  # revealed: str
```
