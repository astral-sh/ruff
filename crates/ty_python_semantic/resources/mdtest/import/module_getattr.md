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

- If you do `import mod` (without importing the submodule directly), accessing `mod.sub` will call
    `mod.__getattr__('sub')`, so `reveal_type(mod.sub)` will show the return type of `__getattr__`.
- If you do `import mod.sub` (importing the submodule directly), then `mod.sub` refers to the actual
    submodule, so `reveal_type(mod.sub)` will show the type of the submodule itself.

`mod/__init__.py`:

```py
def __getattr__(name: str) -> str:
    return "from_getattr"
```

`mod/sub.py`:

```py
value = 42
```

`test_import_mod.py`:

```py
import mod

reveal_type(mod.sub)  # revealed: str
```

`test_import_mod_sub.py`:

```py
import mod.sub

reveal_type(mod.sub)  # revealed: <module 'mod.sub'>
```
