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

## Annotated name targets

An augmented assignment to an annotated name must validate its result against the declaration. An
unannotated name can instead change type.

```py
class Value:
    def __add__(self, other: int) -> object:
        return other

annotated: Value = Value()
# error: [invalid-assignment]
annotated += 1
reveal_type(annotated)  # revealed: Value

inferred = Value()
inferred += 1
reveal_type(inferred)  # revealed: object
```

## Attribute targets

The result must satisfy the attribute's write contract, whether the operation uses `__iadd__` or
falls back to `__add__`.

```py
class AddValue:
    def __add__(self, other: int) -> object:
        return other

class InplaceValue:
    def __iadd__(self, other: int) -> object:
        return other

class Holder:
    add: AddValue
    inplace: InplaceValue

holder = Holder()
# error: [invalid-assignment]
holder.add += 1
reveal_type(holder.add)  # revealed: AddValue

# error: [invalid-assignment]
holder.inplace += 1
reveal_type(holder.inplace)  # revealed: InplaceValue
```

## Inferred attribute targets in loops

An inferred attribute can change type across assignments. Its initial value must not become a
declaration that pollutes the loop-carried type after the attribute has been reassigned.

```py
class Counter:
    def update(self, increment: float) -> None:
        self.value = None
        self.value = 0
        for _ in range(1):
            self.value += increment

reveal_type(Counter().value)  # revealed: None | float
```

## Inferred public attribute targets

An inferred class attribute has the same public write contract for augmented and ordinary
assignments.

```py
class Holder:
    value = 1

holder = Holder()
# error: [invalid-assignment]
holder.value += 0.5
```

## Attribute descriptors

Even an in-place operation writes its result back, so a read-only property rejects augmented
assignment.

```py
class ReadOnly:
    @property
    def value(self) -> int:
        return 1

read_only = ReadOnly()
# error: [invalid-assignment]
read_only.value += 1
reveal_type(read_only.value)  # revealed: int
```

A property's setter can accept a different type than its getter returns. The operation's result must
satisfy the setter, while later reads continue to use the getter type.

```py
class ReadValue:
    def __iadd__(self, other: int) -> str:
        return "updated"

class Writable:
    @property
    def value(self) -> ReadValue:
        return ReadValue()

    @value.setter
    def value(self, value: str) -> None:
        pass

writable = Writable()
writable.value += 1
reveal_type(writable.value)  # revealed: ReadValue
```

Unannotated data descriptors still impose the write contract declared by their setter.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type[object] | None = None) -> int:
        return 1

    def __set__(self, instance: object, value: str) -> None:
        pass

class Custom:
    value = Descriptor()

custom = Custom()
# error: [invalid-assignment]
custom.value += 1
```

## Subscript targets

An augmented subscript assignment must pass its result, not the operator's right-hand operand, to
the target's `__setitem__` method.

```py
class Value:
    def __iadd__(self, other: int) -> object:
        return other

class Container:
    def __getitem__(self, key: int) -> Value:
        return Value()

    def __setitem__(self, key: int, value: Value) -> None:
        pass

container = Container()
# error: [invalid-assignment]
container[0] += 1
reveal_type(container[0])  # revealed: Value
```

A custom setter may accept a broader type than its getter returns.

```py
class PermissiveContainer:
    def __getitem__(self, key: int) -> Value:
        return Value()

    def __setitem__(self, key: int, value: object) -> None:
        pass

permissive = PermissiveContainer()
permissive[0] += 1
reveal_type(permissive[0])  # revealed: Value
```

Explicitly annotated lists and dictionaries retain their write contracts.

```py
items: list[Value] = [Value()]
# error: [invalid-assignment]
items[0] += 1
reveal_type(items[0])  # revealed: Value

mapping: dict[str, Value] = {"value": Value()}
# error: [invalid-assignment]
mapping["value"] += 1
reveal_type(mapping["value"])  # revealed: Value
```

Declared collection-valued attributes also retain their write contracts.

```py
class Holder:
    values: list[Value]

holder = Holder()
# error: [invalid-assignment]
holder.values[0] += 1
```

Typed dictionary entries validate the value written back to their declared fields.

```py
from typing import TypedDict

class Payload(TypedDict):
    value: Value

payload: Payload = {"value": Value()}
# error: [invalid-assignment]
payload["value"] += 1
reveal_type(payload["value"])  # revealed: Value
```

## Read-only subscripts

A readable subscript is not necessarily writable.

```py
values: tuple[int] = (1,)
# error: [invalid-assignment]
values[0] += 1
```

## Failed attribute and subscript loads

Both the load and the store are checked, just as they are for an ordinary assignment whose value
reads the same target.

```py
class Missing: ...

missing = Missing()
# error: [unresolved-attribute]
# error: [unresolved-attribute]
missing.value += 1

mapping: dict[str, int] = {}
# error: [invalid-argument-type]
# error: [invalid-assignment]
mapping[1] += 1
```

## Failed augmented operations

An operation that cannot run does not perform a store.

```py
class Value:
    def __iadd__(self, other: int) -> object:
        return other

class Holder:
    value: Value

holder = Holder()
# error: [unsupported-operator]
holder.value += "invalid"
```

## Correlated union targets

The result of an operation on one union member must not be checked against another member's write
contract.

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
    # TODO: Preserve receiver correlation, which is also lost in ordinary assignments.
    # error: [invalid-assignment]
    value.value += 1
```

## Union subscript targets

An augmented assignment must reject a union alternative that does not support the write.

```py
def update(value: list[int] | tuple[int, ...]) -> None:
    # error: [invalid-assignment]
    value[0] += 1
```

## Union subscript keys

Each possible typed-dictionary key must accept the value written by the augmented assignment.

```py
from typing import Literal, TypedDict

class Payload(TypedDict):
    first: int
    second: int

def update(value: Payload, key: Literal["first", "second"]) -> None:
    value[key] += 1

    # error: [invalid-assignment]
    # error: [invalid-assignment]
    value[key] /= 2
```

## Inferred collection targets

Augmented assignments do not yet participate in full-scope collection inference.

```py
values = [1]
# TODO: This should widen the inferred element type without reporting an error.
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
