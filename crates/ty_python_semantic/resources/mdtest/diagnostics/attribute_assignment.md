# Attribute assignment

This test suite demonstrates various kinds of diagnostics that can be emitted in a
`obj.attr = value` assignment.

## Instance attributes with class-level defaults

These can be set on instances and on class objects.

```py
class C:
    attr: int = 0

instance = C()
instance.attr = 1  # fine

C.attr = 1  # fine
```

But if the type is incorrect, we emit an error:

```py
instance.attr = "wrong"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["wrong"]` is not assignable to attribute `attr` of type `int`
 --> src/mdtest_snippet.py:8:1
  |
8 | instance.attr = "wrong"  # snapshot: invalid-assignment
  | ^^^^^^^^^^^^^
  |
```

And on the class object:

```py
C.attr = "wrong"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal["wrong"]` is not assignable to attribute `attr` of type `int`
 --> src/mdtest_snippet.py:9:1
  |
9 | C.attr = "wrong"  # snapshot: invalid-assignment
  | ^^^^^^
  |
```

## Pure instance attributes

These can only be set on instances.

```py
class C:
    def __init__(self):
        self.attr: int = 0

instance = C()
instance.attr = 1  # fine
instance.attr = "wrong"  # error: [invalid-assignment]
```

When trying to set them on class objects, we generate a useful diagnostic that mentions that the
attribute is only available on instances:

```py
C.attr = 1  # snapshot: invalid-attribute-access
```

```snapshot
error[invalid-attribute-access]: Cannot assign to instance attribute `attr` from the class object `<class 'C'>`
 --> src/mdtest_snippet.py:8:1
  |
8 | C.attr = 1  # snapshot: invalid-attribute-access
  | ^^^^^^
  |
```

## Invalid annotated assignment to attribute

Annotated assignments to attributes on `self` should be validated against their annotation.

```py
class C:
    def __init__(self):
        self.attr: str = None  # snapshot: invalid-assignment
        self.attr2: int = 1  # fine
```

```snapshot
error[invalid-assignment]: Object of type `None` is not assignable to `str`
 --> src/mdtest_snippet.py:3:20
  |
3 |         self.attr: str = None  # snapshot: invalid-assignment
  |                    ---   ^^^^ Incompatible value of type `None`
  |                    |
  |                    Declared type
  |
```

## `ClassVar`s

These can only be set on class objects:

```py
from typing import ClassVar

class C:
    attr: ClassVar[int] = 0

C.attr = 1  # fine
C.attr = "wrong"  # error: [invalid-assignment]
```

When trying to set them on instances, we generate a useful diagnostic that mentions that the
attribute is only available on class objects.

```py
instance = C()
instance.attr = 1  # snapshot: invalid-attribute-access
```

```snapshot
error[invalid-attribute-access]: Cannot assign to ClassVar `attr` from an instance of type `C`
 --> src/mdtest_snippet.py:9:1
  |
9 | instance.attr = 1  # snapshot: invalid-attribute-access
  | ^^^^^^^^^^^^^
  |
```

## Unknown attributes

When trying to set an attribute that is not defined, we also emit errors:

```py
class C: ...

C.non_existent = 1  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Unresolved attribute `non_existent` on type `<class 'C'>`.
 --> src/mdtest_snippet.py:3:1
  |
3 | C.non_existent = 1  # snapshot: unresolved-attribute
  | ^^^^^^^^^^^^^^
  |
```

And on instances:

```py
instance = C()
instance.non_existent = 1  # snapshot: unresolved-attribute
```

```snapshot
error[unresolved-attribute]: Unresolved attribute `non_existent` on type `C`
 --> src/mdtest_snippet.py:5:1
  |
5 | instance.non_existent = 1  # snapshot: unresolved-attribute
  | ^^^^^^^^^^^^^^^^^^^^^
  |
```

## Possibly-missing attributes

When trying to set an attribute that is not defined in all branches, we emit errors:

```py
def _(flag: bool) -> None:
    class C:
        if flag:
            attr: int = 0

    C.attr = 1  # snapshot: possibly-missing-attribute
```

```snapshot
info[possibly-missing-attribute]: Attribute `attr` may be missing on class `C`
 --> src/mdtest_snippet.py:6:5
  |
6 |     C.attr = 1  # snapshot: possibly-missing-attribute
  |     ^^^^^^
  |
```

And on instances:

```py
    instance = C()
    instance.attr = 1  # snapshot: possibly-missing-attribute
```

```snapshot
info[possibly-missing-attribute]: Attribute `attr` may be missing on object of type `C`
 --> src/mdtest_snippet.py:8:5
  |
8 |     instance.attr = 1  # snapshot: possibly-missing-attribute
  |     ^^^^^^^^^^^^^
  |
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
instance.attr = "wrong"  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Invalid assignment to data descriptor attribute `attr` on type `C` with custom `__set__` method
  --> src/mdtest_snippet.py:12:1
   |
12 | instance.attr = "wrong"  # snapshot: invalid-assignment
   | ^^^^^^^^^^^^^
   |
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
instance.attr = 1  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Invalid assignment to data descriptor attribute `attr` on type `C` with custom `__set__` method
  --> src/mdtest_snippet.py:11:1
   |
11 | instance.attr = 1  # snapshot: invalid-assignment
   | ^^^^^^^^^^^^^
   |
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

    C1.attr = 1  # snapshot: invalid-assignment

    class C2:
        if flag:
            attr: int = 0
        else:
            attr: str = ""

    # TODO: This should be an error
    C2.attr = 1
```

TODO: The error message here could be improved to explain *why* the assignment fails.

```snapshot
error[invalid-assignment]: Object of type `Literal[1]` is not assignable to attribute `attr` on type `<class 'mdtest_snippet.<locals of function '_'>.C1 @ src/mdtest_snippet.py:3:15'> | <class 'mdtest_snippet.<locals of function '_'>.C1 @ src/mdtest_snippet.py:7:15'>`
  --> src/mdtest_snippet.py:10:5
   |
10 |     C1.attr = 1  # snapshot: invalid-assignment
   |     ^^^^^^^
   |
```
