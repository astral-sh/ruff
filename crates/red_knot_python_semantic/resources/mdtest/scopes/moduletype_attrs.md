# Implicit globals from `types.ModuleType`

## Implicit `ModuleType` globals

All modules are instances of `types.ModuleType`.
If a name can't be found in any local or global scope, we look it up
as an attribute on `types.ModuleType` in typeshed
before deciding that the name is unbound.

```py
reveal_type(__name__)  # revealed: str

# TODO: infer types from PEP-604 union annotations
reveal_type(__file__)  # revealed: @Todo
reveal_type(__loader__)  # revealed: @Todo
reveal_type(__package__)  # revealed: @Todo
reveal_type(__spec__)  # revealed: @Todo

# TODO: generics
reveal_type(__path__)  # revealed: @Todo

# TODO: this should probably be added to typeshed; not sure why it isn't?
reveal_type(__doc__)  # revealed: Unbound

class X:
    reveal_type(__name__)  # revealed: str

def foo():
    reveal_type(__name__)  # revealed: str
```

However, three attributes on `types.ModuleType` are not present as implicit
module globals; these are excluded:

```py path=unbound_dunders.py
reveal_type(__getattr__)  # revealed: Unbound
reveal_type(__dict__)  # revealed: Unbound
reveal_type(__init__)  # revealed: Unbound
```

## Conditionally global or `ModuleType` attribute

Attributes overridden in the module namespace take priority.
If a builtin name is conditionally defined as a global, however,
a name lookup should union the `ModuleType` type with the conditionally defined type:

```py
__file__ = 42

def returns_bool() -> bool:
    return True

if returns_bool():
    __name__ = 1

reveal_type(__file__)  # revealed: Literal[42]
reveal_type(__name__)  # revealed: str | Literal[1]
```

## Conditionally global or `ModuleType` attribute, with annotation

The same is true if the name is annotated:

```py
__file__: int = 42

def returns_bool() -> bool:
    return True

if returns_bool():
    __name__: int = 1

reveal_type(__file__)  # revealed: Literal[42]
reveal_type(__name__)  # revealed: str | Literal[1]
```
