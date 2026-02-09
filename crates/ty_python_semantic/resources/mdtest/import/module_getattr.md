# Module-level `__getattr__`

## Basic functionality

```py
import module_with_getattr

# Should work: module `__getattr__` returns `str`
reveal_type(module_with_getattr.whatever)  # revealed: str
```

`module_with_getattr.py`:

```py
def __getattr__(name: str) -> str:
    return "hi"
```

## `from import` with `__getattr__`

At runtime, if `module` has a `__getattr__` implementation, you can do `from module import whatever`
and it will exercise the `__getattr__` when `whatever` is not found as a normal attribute.

```py
from module_with_getattr import nonexistent_attr

reveal_type(nonexistent_attr)  # revealed: int
```

`module_with_getattr.py`:

```py
def __getattr__(name: str) -> int:
    return 42
```

## Precedence: explicit attributes take priority over `__getattr__`

```py
import mixed_module

# Explicit attribute should take precedence
reveal_type(mixed_module.explicit_attr)  # revealed: Literal["explicit"]

# `__getattr__` should handle unknown attributes
reveal_type(mixed_module.dynamic_attr)  # revealed: str
```

`mixed_module.py`:

```py
explicit_attr = "explicit"

def __getattr__(name: str) -> str:
    return "dynamic"
```

## Precedence: submodules vs `__getattr__`

If a package's `__init__.py` (e.g. `mod/__init__.py`) defines a `__getattr__` function, and there is
also a submodule file present (e.g. `mod/sub.py`), then:

`mod/__init__.py`:

```py
def __getattr__(name: str) -> str:
    return "from_getattr"
```

`mod/sub.py`:

```py
value = 42
```

If you `import mod` (without importing the submodule directly), accessing `mod.sub` will call
`mod.__getattr__('sub')`, so `reveal_type(mod.sub)` will show the return type of `__getattr__`.

`test_import_mod.py`:

```py
import mod

reveal_type(mod.sub)  # revealed: str
```

If you `import mod.sub` (importing the submodule directly), then `mod.sub` refers to the actual
submodule, so `reveal_type(mod.sub)` will show the type of the submodule itself.

`test_import_mod_sub.py`:

```py
import mod.sub

reveal_type(mod.sub)  # revealed: <module 'mod.sub'>
```

If you `from mod import sub`, at runtime `sub` will be the value returned by the module
`__getattr__`, but other type checkers do not model the precedence this way. They will always prefer
a submodule over a package `__getattr__`, and thus this is the current expectation in the ecosystem.
Effectively, this assumes that a well-implemented package `__getattr__` will always raise
`AttributeError` for a name that also exists as a submodule (and in fact this is the case for many
module `__getattr__` in the ecosystem.)

`test_from_import.py`:

```py
from mod import sub

reveal_type(sub)  # revealed: <module 'mod.sub'>
```

## Limiting names handled by `__getattr__`

If a module `__getattr__` is annotated to only accept certain string literals, then the module
`__getattr__` will be ignored for other names. (In principle this could be a more explicit way to
handle the precedence issues discussed above, but it's not currently used in the ecosystem.)

```py
from limited_getattr_module import known_attr

# error: [unresolved-import]
from limited_getattr_module import unknown_attr

reveal_type(known_attr)  # revealed: int
reveal_type(unknown_attr)  # revealed: Unknown
```

`limited_getattr_module.py`:

```py
from typing import Literal

def __getattr__(name: Literal["known_attr"]) -> int:
    return 3
```
