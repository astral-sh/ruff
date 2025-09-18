# Import conventions

This document describes the conventions for importing symbols.

Reference:

- <https://typing.python.org/en/latest/spec/distributing.html#import-conventions>

## Builtins scope

When looking up for a name, ty will fallback to using the builtins scope if the name is not found in
the global scope. The `builtins.pyi` file, that will be used to resolve any symbol in the builtins
scope, contains multiple symbols from other modules (e.g., `typing`) but those are not re-exported.

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

In this case the symbols shouldn't be available as imports or attributes.

```py
from a import b

# error: [unresolved-attribute] "no attribute `Any`"
reveal_type(b.Any)  # revealed: Unknown
# error: [unresolved-attribute] "no attribute `Literal`"
reveal_type(b.Literal)  # revealed: Unknown
# error: [unresolved-attribute] "no attribute `foo`"
reveal_type(b.foo)  # revealed: Unknown
# error: [unresolved-attribute] "no attribute `bar`"
reveal_type(b.bar)  # revealed: Unknown

# error: [unresolved-import] "Module `a.b` has no member `foo`"
# error: [unresolved-import] "Module `a.b` has no member `bar`"
# error: [unresolved-import] "Module `a.b` has no member `Any`"
# error: [unresolved-import] "Module `a.b` has no member `Literal`"
from a.b import foo, bar, Any, Literal

reveal_type(Any)  # revealed: Unknown
reveal_type(Literal)  # revealed: Unknown
reveal_type(foo)  # revealed: Unknown
reveal_type(bar)  # revealed: Unknown
```

`a/__init__.pyi`:

```pyi
```

`a/b.pyi`:

```pyi
import a.foo
from . import bar
from typing import Any, Literal
```

`a/foo.pyi`:

```pyi

```

`a/bar.pyi`:

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

reveal_type(Foo)  # revealed: <class 'AnyFoo'>
```

`b.pyi`:

```pyi
class AnyFoo: ...
```

## Exported using `__all__`

Here, the symbol is re-exported using the `__all__` variable.

```py
from a import Foo

reveal_type(Foo)  # revealed: <class 'Foo'>
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

## Re-exports with `__all__`

If a symbol is re-exported via redundant alias but is not included in `__all__`, it shouldn't raise
an error when using named import.

`named_import.py`:

```py
from a import Foo

reveal_type(Foo)  # revealed: <class 'Foo'>
```

`a.pyi`:

```pyi
from b import Foo as Foo

__all__ = []
```

`b.pyi`:

```pyi
class Foo: ...
```

However, a star import _would_ raise an error.

`star_import.py`:

```py
from a import *

# error: [unresolved-reference] "Name `Foo` used when not defined"
reveal_type(Foo)  # revealed: Unknown
```

## Re-exports in `__init__.pyi`

Within `__init__.pyi` relative imports (`from . import xyz` or `from .pub import xyz`) are also
treated as a re-exports.

We check the both the members of the module and the imports of the module as you _should_ be able to
do `from a import priv` but the attribute `a.priv` _should not_ exist.

The most subtle detail here is whether `from .semipriv import Pub` should make the `a.semipriv`
attribute exist or not. We do not currently do this, although perhaps we should.

```py
import a

reveal_type(a.Pub)  # revealed: <class 'Pub'>
# error: [unresolved-attribute]
reveal_type(a.Priv)  # revealed: Unknown
reveal_type(a.pub)  # revealed: <module 'a.pub'>
# error: [unresolved-attribute]
reveal_type(a.priv)  # revealed: Unknown
# error: [unresolved-attribute]
reveal_type(a.semipriv)  # revealed: Unknown
# error: [unresolved-attribute]
reveal_type(a.sub)  # revealed: Unknown
reveal_type(a.subpub)  # revealed: <module 'a.sub.subpub'>
# error: [unresolved-attribute]
reveal_type(a.subpriv)  # revealed: Unknown

# error: [unresolved-import] "Priv"
from a import Pub, Priv

# error: [unresolved-import] "subpriv"
from a import pub, priv, semipriv, sub, subpub, subpriv

reveal_type(Pub)  # revealed: <class 'Pub'>
reveal_type(Priv)  # revealed: Unknown
reveal_type(pub)  # revealed: <module 'a.pub'>
reveal_type(priv)  # revealed: <module 'a.priv'>
reveal_type(semipriv)  # revealed: <module 'a.semipriv'>
reveal_type(sub)  # revealed: <module 'a.sub'>
reveal_type(subpub)  # revealed: <module 'a.sub.subpub'>
reveal_type(subpriv)  # revealed: Unknown
```

`a/__init__.pyi`:

```pyi
# re-exported because they're relative
from .sub import subpub
from .semipriv import Pub
from . import pub

# not re-exported because they're absolute
from a.sub import subpriv
from a.semipriv import Priv
from a import priv
```

`a/pub.pyi`:

```pyi
```

`a/priv.pyi`:

```pyi
```

`a/semipriv.pyi`:

```pyi
class Pub: ...

class Priv: ...
```

`a/sub/__init__.pyi`:

```pyi

```

`a/sub/subpub.pyi`:

```pyi

```

`a/sub/subpriv.pyi`:

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

reveal_type(Foo)  # revealed: <class 'Foo'> | str
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

reveal_type(Foo)  # revealed: <class 'Foo'>
```

`a.pyi`:

```pyi
def coinflip() -> bool: ...

if coinflip():
    from b import Foo
else:
    from b import Foo as Foo

reveal_type(Foo)  # revealed: <class 'Foo'>
```

`b.pyi`:

```pyi
class Foo: ...
```

### Re-export in one branch

```py
# error: "Member `Foo` of module `a` is possibly unbound"
from a import Foo

reveal_type(Foo)  # revealed: <class 'Foo'>
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
