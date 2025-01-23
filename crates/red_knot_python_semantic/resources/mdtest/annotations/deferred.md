# Deferred annotations

## Deferred annotations in stubs always resolve

```pyi path=mod.pyi
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

def get_foo() -> Foo: ...

class Foo: ...

reveal_type(get_foo())  # revealed: Foo
```
