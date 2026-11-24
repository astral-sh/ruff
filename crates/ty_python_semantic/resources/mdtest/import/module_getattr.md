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

## Invalid `__getattr__` calls

A module-level `__getattr__` must accept the attribute name passed by Python. If the call fails, the
access is invalid, but the function's return type remains available for error recovery.

```py
import invalid_getattr_module

invalid_getattr_module.missing  # snapshot: invalid-attribute-access

# error: [invalid-attribute-access] "Invalid access to attribute `missing` on type `<module 'invalid_getattr_module'>`"
reveal_type(invalid_getattr_module.missing)  # revealed: str

reveal_type(invalid_getattr_module.defined)  # revealed: Literal[1]
```

```snapshot
error[invalid-attribute-access]: Invalid access to attribute `missing` on type `<module 'invalid_getattr_module'>`
 --> src/mdtest_snippet.py:3:1
  |
3 | invalid_getattr_module.missing  # snapshot: invalid-attribute-access
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Too many positional arguments to function `__getattr__`: expected 0, got 1
info: This access implicitly calls `__getattr__`
info: Function signature here
 --> src/invalid_getattr_module.py:3:5
  |
3 | def __getattr__() -> str:
  |     ^^^^^^^^^^^^^^^^^^^^
```

`invalid_getattr_module.py`:

```py
defined = 1

def __getattr__() -> str:
    return "fallback"
```

## Invalid `__getattr__` attribute-name types

An incompatible attribute-name parameter also makes a module-level fallback call invalid.

```py
import invalid_getattr_name

# error: [invalid-attribute-access] "Invalid access to attribute `missing` on type `<module 'invalid_getattr_name'>`"
reveal_type(invalid_getattr_name.missing)  # revealed: bytes
```

`invalid_getattr_name.py`:

```py
def __getattr__(name: int) -> bytes:
    return b"fallback"
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

## Invalid `__getattr__` calls in `from` imports

An invalid module-level `__getattr__` call is reported on `from ... import` statements while
retaining the function's return type for recovery. Since the failed operation is an import, it
receives an `invalid-module-getattr-call` diagnostic instead of an `invalid-attribute-access`
diagnostic.

```py
from invalid_getattr_module import missing  # snapshot: invalid-module-getattr-call

reveal_type(missing)  # revealed: str
```

```snapshot
error[invalid-module-getattr-call]: Cannot import `missing` from module `invalid_getattr_module`
 --> src/mdtest_snippet.py:1:36
  |
1 | from invalid_getattr_module import missing  # snapshot: invalid-module-getattr-call
  |                                    ^^^^^^^ Too many positional arguments to function `__getattr__`: expected 0, got 1
info: This import implicitly calls a module-level `__getattr__` function
info: Function signature here
 --> src/invalid_getattr_module.py:1:5
  |
1 | def __getattr__() -> str:
  |     ^^^^^^^^^^^^^^^^^^^^
```

`invalid_getattr_module.py`:

```py
def __getattr__() -> str:
    return "fallback"
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

## Precedence: submodules vs invalid `__getattr__`

A real submodule takes precedence even when the package's `__getattr__` would reject its name.

`invalid_mod/__init__.py`:

```py
def __getattr__() -> str:
    return "fallback"
```

`invalid_mod/sub.py`:

```py
value = 42
```

```py
from invalid_mod import sub

reveal_type(sub)  # revealed: <module 'invalid_mod.sub'>
```

## Limiting names handled by `__getattr__`

If a module `__getattr__` is annotated to accept only certain string literals, unsupported names
produce an import or attribute-access diagnostic, respectively, while preserving the recovered
return type.

```py
from limited_getattr_module import known_attr

# error: [invalid-module-getattr-call] "Cannot import `unknown_attr` from module `limited_getattr_module`"
from limited_getattr_module import unknown_attr

reveal_type(known_attr)  # revealed: int
reveal_type(unknown_attr)  # revealed: int

import limited_getattr_module

# error: [invalid-attribute-access] "Invalid access to attribute `unknown_attr` on type `<module 'limited_getattr_module'>`"
reveal_type(limited_getattr_module.unknown_attr)  # revealed: int
```

`limited_getattr_module.py`:

```py
from typing import Literal

def __getattr__(name: Literal["known_attr"]) -> int:
    return 3
```
