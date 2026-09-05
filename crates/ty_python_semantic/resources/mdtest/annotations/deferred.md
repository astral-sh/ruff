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

## Deferred annotations exclude superseded bindings

A deferred annotation can refer to the binding visible at its definition or a later binding, but not
a binding that was already shadowed.

```py
from __future__ import annotations

Target = bytes
Target = int

def use(value: Target) -> None: ...

Target = str

reveal_type(use)  # revealed: def use(value: int | str) -> None
```

## Deferred attribute annotations fall back after deleting a local root

Deleting a class-local root allows a deferred attribute annotation to resolve a whole-place binding
from the enclosing function instead of treating the deleted local binding as visible.

```py
from __future__ import annotations

class Namespace:
    Value: type[int] | type[str]

namespace = Namespace

def outer() -> None:
    namespace.Value = str

    class Holder:
        namespace = Namespace
        del namespace
        value: namespace.Value

    reveal_type(Holder.value)  # revealed: str
```

## Deferred annotations preserve conditional current bindings

A deferred annotation includes every binding visible after a conditional and any later binding, but
still excludes a binding shadowed by both branches.

```py
from __future__ import annotations

def condition() -> bool:
    return True

Target = bytes

if condition():
    Target = int
else:
    Target = str

def use(value: Target) -> None: ...

Target = bytearray

reveal_type(use)  # revealed: def use(value: int | str | bytearray) -> None
```

## Declaration-only deferred annotations exclude superseded bindings

A deferred annotation without an accompanying assignment also ignores an already shadowed type.

```py
from __future__ import annotations

Target = bytes
Target = int
value: Target

Target = str

# error: [invalid-assignment]
value = b"wrong"
```

## Deferred annotations resolve bindings after a deletion

Deleting a name before a deferred annotation does not prevent the annotation from resolving a later
binding.

```py
from __future__ import annotations

Target = bytes
del Target

def use(value: Target) -> None: ...

Target = str

reveal_type(use)  # revealed: def use(value: str) -> None
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
