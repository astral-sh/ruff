# Deferred annotations

## Deferred annotations in stubs always resolve

`mod.pyi`:

```pyi
def get_foo() -> Foo: ...

class Foo: ...
```

```py
from mod import get_foo

reveal_type(get_foo())  # revealed: Foo
```

## Deferred annotations in regular code fail

In (regular) source files, annotations are *not* deferred. This also tests that imports from
`__future__` that are not `annotations` are ignored.

```py
from __future__ import with_statement as annotations

# error: [unresolved-reference]
def get_foo() -> Foo: ...

class Foo: ...

reveal_type(get_foo())  # revealed: Unknown
```

## Deferred annotations in regular code with `__future__.annotations`

If `__future__.annotations` is imported, annotations *are* deferred.

```py
from __future__ import annotations

def get_foo() -> Foo:
    return Foo()

class Foo: ...

reveal_type(get_foo())  # revealed: Foo
```

## Deferred self-reference annotations in a class definition

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations
from typing import Any

class Foo:
    this: Foo
    # error: [unresolved-reference]
    _ = Foo()
    # error: [unresolved-reference]
    [Foo for _ in range(1)]
    a = int

    def f(self, x: Foo):
        reveal_type(x)  # revealed: Foo

    def g(self) -> Foo:
        _: Foo = self
        return self

    class Bar:
        foo: Foo
        b = int

        def f(self, x: Foo):
            return self
        # error: [unresolved-reference]
        def g(self) -> Bar:
            return self
        # error: [unresolved-reference]
        def h[T: Bar](self):
            pass

        class Baz[T: Foo]:
            pass

        # error: [unresolved-reference] "Name `Foo` used when not defined"
        # error: [unresolved-reference] "Name `Bar` used when not defined"
        class Qux(Foo, Bar, Baz[Any]):
            pass

        # error: [unresolved-reference] "Name `Foo` used when not defined"
        # error: [unresolved-reference] "Name `Bar` used when not defined"
        class Quux[_T](Foo, Bar, Baz[Any]):
            pass

        # error: [unresolved-reference]
        type S = a
        type T = b
        type U = Foo
        # error: [unresolved-reference]
        type V = Bar
        type W = Baz  # error: [missing-type-argument]

    def h[T: Bar]():
        # error: [unresolved-reference]
        return Bar()
    type Baz = Foo
```

## Class bindings shadow types in string annotations

A class attribute with a value shadows an outer type of the same name, including in its own string
annotation. An annotation without a value does not create a class binding, so it can still refer to
the outer type.

```toml
[environment]
python-version = "3.13"
```

```py
class C:
    bytes: "bytes"
    str: "str" = ""  # error: [invalid-type-form]
```

The built-in `type` is an instance of itself, so assigning it to an attribute with this cyclic
annotation is valid:

```py
class C:
    type: "type" = type

reveal_type(C.type)  # revealed: type
```

## Class bindings shadow types with future annotations

With `from __future__ import annotations`, unquoted annotations follow the same name-resolution
rules.

```toml
[environment]
python-version = "3.13"
```

```py
from __future__ import annotations

class C:
    bytes: bytes
    str: str = ""  # error: [invalid-type-form]
```

## Class bindings shadow types with deferred evaluation

Python 3.14 defers annotation evaluation without requiring a future import. The same class binding
rules apply.

```toml
[environment]
python-version = "3.14"
```

```py
class C:
    bytes: bytes
    str: str = ""  # error: [invalid-type-form]
```

## Mutually recursive class annotations with values

These integer and string values are not valid types. We reject both annotations even though
inferring either annotation depends on the other attribute.

```py
class C:
    first: "second" = 1  # error: [invalid-type-form]
    second: "first" = ""  # error: [invalid-type-form]
```

## Non-deferred self-reference annotations in a class definition

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any

class Foo:
    # error: [unresolved-reference]
    this: Foo
    ok: "Foo"
    # error: [unresolved-reference]
    _ = Foo()
    # error: [unresolved-reference]
    [Foo for _ in range(1)]
    a = int

    # error: [unresolved-reference]
    def f(self, x: Foo):
        reveal_type(x)  # revealed: Unknown
    # error: [unresolved-reference]
    def g(self) -> Foo:
        _: Foo = self
        return self

    class Bar:
        # error: [unresolved-reference]
        foo: Foo
        b = int

        # error: [unresolved-reference]
        def f(self, x: Foo):
            return self
        # error: [unresolved-reference]
        def g(self) -> Bar:
            return self
        # error: [unresolved-reference]
        def h[T: Bar](self):
            pass

        class Baz[T: Foo]:
            pass

        # error: [unresolved-reference] "Name `Foo` used when not defined"
        # error: [unresolved-reference] "Name `Bar` used when not defined"
        class Qux(Foo, Bar, Baz[Any]):
            pass

        # error: [unresolved-reference] "Name `Foo` used when not defined"
        # error: [unresolved-reference] "Name `Bar` used when not defined"
        class Quux[_T](Foo, Bar, Baz[Any]):
            pass

        # error: [unresolved-reference]
        type S = a
        type T = b
        type U = Foo
        # error: [unresolved-reference]
        type V = Bar
        type W = Baz  # error: [missing-type-argument]

    def h[T: Bar]():
        # error: [unresolved-reference]
        return Bar()
    type Qux = Foo

def _():
    class C:
        # error: [unresolved-reference]
        def f(self) -> C:
            return self
```

## Base class references

### Not deferred by __future__.annotations

```py
from __future__ import annotations

class A(B):  # error: [unresolved-reference]
    pass

class B:
    pass
```

### Deferred in stub files

```pyi
class A(B): ...
class B: ...
```

## Default argument values

### Not deferred in regular files

```py
# error: [unresolved-reference]
def f(mode: int = ParseMode.test):
    pass

class ParseMode:
    test = 1
```

### Deferred in stub files

Forward references in default argument values are allowed in stub files.

```pyi
def f(mode: int = ParseMode.test): ...

class ParseMode:
    test: int
```

### Undefined names are still errors in stub files

```pyi
# error: [unresolved-reference]
def f(mode: int = NeverDefined.test): ...
```

## Class keyword arguments

### Not deferred in regular files

```py
# error: [unresolved-reference]
class Foo(metaclass=SomeMeta):
    pass

class SomeMeta(type):
    pass
```

### Deferred in stub files

Forward references in class keyword arguments are allowed in stub files.

```pyi
class Foo(metaclass=SomeMeta): ...
class SomeMeta(type): ...
```

### Undefined names are still errors in stub files

```pyi
# error: [unresolved-reference]
class Foo(metaclass=NeverDefined): ...
```

## Lambda default argument values

### Not deferred in regular files

```py
# error: [unresolved-reference]
f = lambda x=Foo(): x

class Foo:
    pass
```

### Deferred in stub files

Forward references in lambda default argument values are allowed in stub files.

```pyi
f = lambda x=Foo(): x

class Foo: ...
```

### Undefined names are still errors in stub files

```pyi
# error: [unresolved-reference]
f = lambda x=NeverDefined(): x
```
