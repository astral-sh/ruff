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

## Precedence: explicit attributes take priority over `__getattr__`

```py
import mixed_module

# Explicit attribute should take precedence
reveal_type(mixed_module.explicit_attr)  # revealed: Unknown | Literal["explicit"]

# `__getattr__` should handle unknown attributes
reveal_type(mixed_module.dynamic_attr)  # revealed: str
```

`mixed_module.py`:

```py
explicit_attr = "explicit"

def __getattr__(name: str) -> str:
    return "dynamic"
```

## Precedence: submodules vs `__getattr__` - Case 1

```py
# if `mod/__init__.py` has a `__getattr__`, and there also exists a `mod/sub.py`
# if we have `import mod;` `reveal_type(mod.sub)` returns the type of `__getattr__`.
import mod

reveal_type(mod.sub)  # revealed: str
```

`mod/__init__.py`:

```py
def __getattr__(name: str) -> str:
    return "from_getattr"
```

`mod/sub.py`:

```py
value = 42
```

## Precedence: submodules vs `__getattr__` - Case 2

```py
# if `mod/__init__.py` has a `__getattr__`, and there also exists a `mod/sub.py`
# if we have `import mod.sub;` `reveal_type(mod.sub)` returns the type of the submodule.
import mod.sub

reveal_type(mod.sub)  # revealed: <module 'mod.sub'>
```

`mod/__init__.py`:

```py
def __getattr__(name: str) -> str:
    return "from_getattr"
```

`mod/sub.py`:

```py
value = 42
```
