# Attribute assignment

<!-- snapshot-diagnostics -->

This test suite demonstrates various kinds of diagnostics that can be emitted in a
`obj.attr = value` assignment.

## Instance attributes with class-level defaults

These can be set on instances and on class objects.

```py
class C:
    attr: int = 0

instance = C()
instance.attr = 1  # fine
instance.attr = "wrong"  # error: [invalid-assignment]

C.attr = 1  # fine
C.attr = "wrong"  # error: [invalid-assignment]
```

## Pure instance attributes

These can only be set on instances. When trying to set them on class objects, we generate a useful
diagnostic that mentions that the attribute is only available on instances.

```py
class C:
    def __init__(self):
        self.attr: int = 0

instance = C()
instance.attr = 1  # fine
instance.attr = "wrong"  # error: [invalid-assignment]

C.attr = 1  # error: [invalid-attribute-access]
```

## `ClassVar`s

These can only be set on class objects. When trying to set them on instances, we generate a useful
diagnostic that mentions that the attribute is only available on class objects.

```py
from typing import ClassVar

class C:
    attr: ClassVar[int] = 0

C.attr = 1  # fine
C.attr = "wrong"  # error: [invalid-assignment]

instance = C()
instance.attr = 1  # error: [invalid-attribute-access]
```

## Unknown attributes

When trying to set an attribute that is not defined, we also emit errors:

```py
class C: ...

C.non_existent = 1  # error: [unresolved-attribute]

instance = C()
instance.non_existent = 1  # error: [unresolved-attribute]
```

## Possibly-unbound attributes

When trying to set an attribute that is not defined in all branches, we emit errors:

```py
def _(flag: bool) -> None:
    class C:
        if flag:
            attr: int = 0

    C.attr = 1  # error: [possibly-unbound-attribute]

    instance = C()
    instance.attr = 1  # error: [possibly-unbound-attribute]
```

## Data descriptors

When assigning to a data descriptor attribute, we implicitly call the descriptor's `__set__` method.
This can lead to various kinds of diagnostics.

### Invalid argument type

```py
class Descriptor:
    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    attr: Descriptor = Descriptor()

instance = C()
instance.attr = 1  # fine

# TODO: ideally, we would mention why this is an invalid assignment (wrong argument type for `value` parameter)
instance.attr = "wrong"  # error: [invalid-assignment]
```

### Invalid `__set__` method signature

```py
class WrongDescriptor:
    def __set__(self, instance: object, value: int, extra: int) -> None:
        pass

class C:
    attr: WrongDescriptor = WrongDescriptor()

instance = C()

# TODO: ideally, we would mention why this is an invalid assignment (wrong number of arguments for `__set__`)
instance.attr = 1  # error: [invalid-assignment]
```

## Setting attributes on union types

```py
def _(flag: bool) -> None:
    if flag:
        class C1:
            attr: int = 0

    else:
        class C1:
            attr: str = ""

    # TODO: The error message here could be improved to explain why the assignment fails.
    C1.attr = 1  # error: [invalid-assignment]

    class C2:
        if flag:
            attr: int = 0
        else:
            attr: str = ""

    # TODO: This should be an error
    C2.attr = 1
```
