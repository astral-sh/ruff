# Import conventions

This document describes the conventions for importing symbols.

Reference:

- <https://typing.readthedocs.io/en/latest/spec/distributing.html#import-conventions>

## Builtins scope

When looking up for a name, red knot will fallback to using the builtins scope if the name is not
found in the global scope. The `builtins.pyi` file, that will be used to resolve any symbol in the
builtins scope, contains multiple symbols from other modules (e.g., `typing`) but those are not
re-exported.

```py
# These symbols are being imported in `builtins.pyi` but shouldn't be considered as being
# available in the builtins scope.

# error: "Name `Literal` used when not defined"
reveal_type(Literal)  # revealed: Unknown

# error: "Name `sys` used when not defined"
reveal_type(sys)  # revealed: Unknown
```

## Builtins import

Similarly, trying to import the symbols from the builtins module which aren't re-exported should
also raise an error.

```py
# error: "Module `builtins` has no member `Literal`"
# error: "Module `builtins` has no member `sys`"
from builtins import Literal, sys

reveal_type(Literal)  # revealed: Unknown
reveal_type(sys)  # revealed: Unknown

# error: "Module `math` has no member `Iterable`"
from math import Iterable

reveal_type(Iterable)  # revealed: Unknown
```

## Re-exported symbols in stub files

When a symbol is re-exported, importing it should not raise an error. This tests both `import ...`
and `from ... import ...` forms.

Note: Submodule imports in `import ...` form doesn't work because it's a syntax error. For example,
in `import os.path as os.path` the `os.path` is not a valid identifier.

```py
from b import Any, Literal, foo

reveal_type(Any)  # revealed: typing.Any
reveal_type(Literal)  # revealed: typing.Literal
reveal_type(foo)  # revealed: <module 'foo'>
```

`b.pyi`:

```pyi
import foo as foo
from typing import Any as Any, Literal as Literal
```

`foo.py`:

```py
```

## Non-exported symbols in stub files

Here, none of the symbols are being re-exported in the stub file.

```py
# error: 15 [unresolved-import] "Module `b` has no member `foo`"
# error: 20 [unresolved-import] "Module `b` has no member `Any`"
# error: 25 [unresolved-import] "Module `b` has no member `Literal`"
from b import foo, Any, Literal

reveal_type(Any)  # revealed: Unknown
reveal_type(Literal)  # revealed: Unknown
reveal_type(foo)  # revealed: Unknown
```

`b.pyi`:

```pyi
import foo
from typing import Any, Literal
```

`foo.pyi`:

```pyi
```

## Nested non-exports

Here, a chain of modules all don't re-export an import.

```py
# error: "Module `a` has no member `Any`"
from a import Any

reveal_type(Any)  # revealed: Unknown
```

`a.pyi`:

```pyi
# error: "Module `b` has no member `Any`"
from b import Any

reveal_type(Any)  # revealed: Unknown
```

`b.pyi`:

```pyi
# error: "Module `c` has no member `Any`"
from c import Any

reveal_type(Any)  # revealed: Unknown
```

`c.pyi`:

```pyi
from typing import Any

reveal_type(Any)  # revealed: typing.Any
```

## Nested mixed re-export and not

But, if the symbol is being re-exported explicitly in one of the modules in the chain, it should not
raise an error at that step in the chain.

```py
# error: "Module `a` has no member `Any`"
from a import Any

reveal_type(Any)  # revealed: Unknown
```

`a.pyi`:

```pyi
from b import Any

reveal_type(Any)  # revealed: Unknown
```

`b.pyi`:

```pyi
# error: "Module `c` has no member `Any`"
from c import Any as Any

reveal_type(Any)  # revealed: Unknown
```

`c.pyi`:

```pyi
from typing import Any

reveal_type(Any)  # revealed: typing.Any
```

## Exported as different name

The re-export convention only works when the aliased name is exactly the same as the original name.

```py
# error: "Module `a` has no member `Foo`"
from a import Foo

reveal_type(Foo)  # revealed: Unknown
```

`a.pyi`:

```pyi
from b import AnyFoo as Foo

reveal_type(Foo)  # revealed: Literal[AnyFoo]
```

`b.pyi`:

```pyi
class AnyFoo: ...
```

## Exported using `__all__`

Here, the symbol is re-exported using the `__all__` variable.

```py
# TODO: This should *not* be an error but we don't understand `__all__` yet.
# error: "Module `a` has no member `Foo`"
from a import Foo
```

`a.pyi`:

```pyi
from b import Foo

__all__ = ['Foo']
```

`b.pyi`:

```pyi
class Foo: ...
```

## Re-exports in `__init__.pyi`

Similarly, for an `__init__.pyi` (stub) file, importing a non-exported name should raise an error
but the inference would be `Unknown`.

```py
# error: 15 "Module `a` has no member `Foo`"
# error: 20 "Module `a` has no member `c`"
from a import Foo, c, foo

reveal_type(Foo)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(foo)  # revealed: <module 'a.foo'>
```

`a/__init__.pyi`:

```pyi
from .b import c
from .foo import Foo
```

`a/foo.pyi`:

```pyi
class Foo: ...
```

`a/b/__init__.pyi`:

```pyi
```

`a/b/c.pyi`:

```pyi
```

## Conditional re-export in stub file

The following scenarios are when a re-export happens conditionally in a stub file.

### Global import

```py
# error: "Member `Foo` of module `a` is possibly unbound"
from a import Foo

reveal_type(Foo)  # revealed: str
```

`a.pyi`:

```pyi
from b import Foo

def coinflip() -> bool: ...

if coinflip():
    Foo: str = ...

reveal_type(Foo)  # revealed: Literal[Foo] | str
```

`b.pyi`:

```pyi
class Foo: ...
```

### Both branch is an import

Here, both the branches of the condition are import statements where one of them re-exports while
the other does not.

```py
# error: "Member `Foo` of module `a` is possibly unbound"
from a import Foo

reveal_type(Foo)  # revealed: Literal[Foo]
```

`a.pyi`:

```pyi
def coinflip() -> bool: ...

if coinflip():
    from b import Foo
else:
    from b import Foo as Foo

reveal_type(Foo)  # revealed: Literal[Foo]
```

`b.pyi`:

```pyi
class Foo: ...
```

### Re-export in one branch

```py
# error: "Member `Foo` of module `a` is possibly unbound"
from a import Foo

reveal_type(Foo)  # revealed: Literal[Foo]
```

`a.pyi`:

```pyi
def coinflip() -> bool: ...

if coinflip():
    from b import Foo as Foo
```

`b.pyi`:

```pyi
class Foo: ...
```

### Non-export in one branch

```py
# error: "Module `a` has no member `Foo`"
from a import Foo

reveal_type(Foo)  # revealed: Unknown
```

`a.pyi`:

```pyi
def coinflip() -> bool: ...

if coinflip():
    from b import Foo
```

`b.pyi`:

```pyi
class Foo: ...
```
