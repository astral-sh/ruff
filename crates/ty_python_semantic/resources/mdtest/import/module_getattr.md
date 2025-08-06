# Module-level `__getattr__`

## Basic functionality

```py
import module_with_getattr

# Should work: module `__getattr__` returns 'hi'
reveal_type(module_with_getattr.whatever)  # revealed: str
```

`module_with_getattr.py`:

```py
def __getattr__(name: str) -> str:
    if name == "whatever":
        return "hi"
    raise AttributeError(f"module has no attribute '{name}'")
```

## Type annotations on `__getattr__` return type

```py
import typed_getattr_module

reveal_type(typed_getattr_module.dynamic_int)  # revealed: int
```

`typed_getattr_module.py`:

```py
def __getattr__(name: str) -> int:
    if name == "dynamic_int":
        return 42
    raise AttributeError(f"module has no attribute '{name}'")
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
    if name == "dynamic_attr":
        return "dynamic"
    raise AttributeError(f"module has no attribute '{name}'")
```

## `__getattr__` should not be called for existing attributes

```py
import module_no_fallback

# This should not trigger `__getattr__` since the attribute exists
reveal_type(module_no_fallback.real_function)  # revealed: def real_function() -> str
```

`module_no_fallback.py`:

```py
def real_function() -> str:
    return "real"

def __getattr__(name: str) -> str:
    # This should never be called for real_function
    return "fallback"
```
