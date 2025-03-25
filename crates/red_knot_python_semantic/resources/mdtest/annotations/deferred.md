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

```py
from __future__ import annotations

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

        # error: [unresolved-reference]
        type S = a
        type T = b

    def h[T: Bar]():
        # error: [unresolved-reference]
        return Bar()
    type Baz = Foo
```

## Non-deferred self-reference annotations in a class definition

```py
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

        # error: [unresolved-reference]
        type S = a
        type T = b

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
